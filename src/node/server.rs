mod aizel {
    include!(concat!(env!("OUT_DIR"), "/aizel.rs"));
}
use super::aizel::inference_server::Inference;
use super::aizel::{InferenceRequest, InferenceResponse};
use super::config::{NodeConfig, DEFAULT_MODEL_DIR};
use crate::crypto::elgamal::{Ciphertext, Elgamal};
use crate::crypto::secret::Secret;
use common::error::Error;
use ethers::{
    contract::abigen,
    middleware::SignerMiddleware,
    providers::{Http, Provider},
    signers::{LocalWallet, Signer},
    types::Address,
};
use lazy_static::lazy_static;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::ggml_time_us;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::{AddBos, Special};
use llama_cpp_2::token::data_array::LlamaTokenDataArray;
use log::{info, warn};
use minio::s3::{
    args::{BucketExistsArgs, DownloadObjectArgs},
    client::Client,
    creds::StaticProvider,
    http::BaseUrl,
};
use secp256k1::{SecretKey, PublicKey};
use std::fs;
use std::num::NonZeroU32;
use std::time::{Duration, Instant};
use std::{env, sync::Arc};
use tonic::{Request, Response, Status};
abigen!(
    InferenceContract,
    r#"[
        function submitInference(uint256 requestId,string memory output) external onlyOwner
    ]"#,
);

pub struct AizelInference {
    pub config: NodeConfig,
    pub secret: Secret,
}

lazy_static! {
    static ref INFERENCE_CONTRACT: InferenceContract<SignerMiddleware<Provider<Http>, LocalWallet>> = {
        let provider = Provider::<Http>::try_from(env::var("ENDPOINT").unwrap()).unwrap();

        let chain_id: u64 = env::var("CHAIN_ID").unwrap().parse().unwrap();
        let wallet = env::var("PRIVATE_KEY")
            .unwrap()
            .parse::<LocalWallet>()
            .unwrap()
            .with_chain_id(chain_id);

        let signer = Arc::new(SignerMiddleware::new(provider, wallet));
        let contract_address: String = env::var("CONTRACT_ADDRESS").unwrap().parse().unwrap();
        InferenceContract::new(contract_address.parse::<Address>().unwrap(), signer)
    };
}

#[tonic::async_trait]
impl Inference for AizelInference {
    async fn llama_inference(
        &self,
        request: Request<InferenceRequest>,
    ) -> Result<Response<InferenceResponse>, Status> {
        let req = request.into_inner();
        let model = req.model.clone();
        if !self
            .check_model_exist(model.clone())
            .await
            .map_err(|e| Status::internal(e.to_string()))?
        {
            info!("download models from data node {}", model);
            // model doesn't exist, download model from data node
            self.download_model(model.clone())
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
        }

        // decrypt input
        let decrypted_input = {
            let ciphertext = hex::decode(req.input).map_err(|e| {
                Status::internal(format!("failed decode ciphertext {}", e.to_string()))
            })?;
            let ct = Ciphertext::from_bytes(ciphertext.as_slice());
            let rng = rand::thread_rng();
            let mut elgamal = Elgamal::new(rng);
            let plain = elgamal
                .decrypt(&ct, &SecretKey::from_slice(&self.secret.secret.0).unwrap())
                .map_err(|e| {
                    Status::internal(format!("failed to decrypt input {}", e.to_string()))
                })?;
            String::from_utf8(plain)
        }
        .map_err(|e| Status::internal(format!("failde to get plain input {}", e.to_string())))?;

        // model inference
        let output = self
            .model_inference(model, decrypted_input)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        let request_id = req.request_id;

        if output.len() == 0 {
            return Err(Status::internal("failed to generate output"));
        }
        // encrypt output
        // send model output to smart contract
        let encrypted_output = {
            let rng = rand::thread_rng();
            let mut elgamal = Elgamal::new(rng);
            let ct = elgamal.encrypt(output.as_bytes(), &PublicKey::from_slice(&hex::decode(req.user_pk).unwrap()).unwrap() ).map_err(|e| {
                Status::internal(e.to_string())
            })?;
            hex::encode(ct.to_bytes())
        };

        let tx = &INFERENCE_CONTRACT.submit_inference(request_id.into(), encrypted_output.clone());
        let _pending_tx = tx
            .send()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        
        Ok(Response::new(InferenceResponse {output: encrypted_output}))
    }
}

impl AizelInference {
   pub async fn check_model_exist(&self, model: String) -> Result<bool, Error> {
        let model_path = self.config.root_path.join(DEFAULT_MODEL_DIR);
        for entry in fs::read_dir(&model_path).map_err(|e| Error::FileError {
            path: model_path.clone(),
            message: e.to_string(),
        })? {
            let entry = entry.map_err(|e| Error::FileError {
                path: model_path.clone(),
                message: e.to_string(),
            })?;
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_file() && entry.file_name().to_string_lossy() == model {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    pub async fn download_model(&self, model: String) -> Result<(), Error> {
        let base_url = format!("http://{}", self.config.data_address)
            .parse::<BaseUrl>()
            .map_err(|e| Error::DownloadingModelError {
                model: model.clone(),
                message: e.to_string(),
            })?;
        info!("Trying to connect to MinIO at: `{:?}`", base_url);
        let static_provider = StaticProvider::new("aizel_test", "aizel_test_pwd", None);
        let client =
            Client::new(base_url, Some(Box::new(static_provider)), None, None).map_err(|e| {
                Error::DownloadingModelError {
                    model: model.clone(),
                    message: format!("failed to connect to data node {}", e.to_string()),
                }
            })?;
        let bucket_name: &str = "models";
        let object_name: &str = &model;
        if client
            .bucket_exists(&BucketExistsArgs::new(&bucket_name).unwrap())
            .await
            .map_err(|e| Error::DownloadingModelError {
                model: model.clone(),
                message: format!("failed to get bucket {}", e.to_string()),
            })?
        {
            let time_start = Instant::now();
            let model_path = self.config.root_path.join(DEFAULT_MODEL_DIR).join(&model);
            let args: DownloadObjectArgs =
                DownloadObjectArgs::new(bucket_name, object_name, &model_path.to_str().unwrap())
                    .unwrap();
            let _ =
                client
                    .download_object(&args)
                    .await
                    .map_err(|e| Error::DownloadingModelError {
                        model: model.clone(),
                        message: format!("failed to download model {}", e.to_string()),
                    })?;
            let duration = time_start.elapsed();
            info!("downloading model time cost: {:?}", duration);
        } else {
            return Err(Error::DownloadingModelError {
                model: model.clone(),
                message: format!("bucket doesn't exist"),
            });
        }
        Ok(())
    }

    async fn model_inference(
        &self,
        model_name: String,
        prompt: String,
    ) -> Result<String, Error> {
        let model_path = self
            .config
            .root_path
            .join(DEFAULT_MODEL_DIR)
            .join(model_name);
        // LLM parameters
        // TODO: update
        let ctx_size = NonZeroU32::new(2048).unwrap();
        let seed = 1234;
        let threads = num_cpus::get();
        let n_len: i32 = 32;
        let mut res = Vec::new();
        // init LLM
        let backend = LlamaBackend::init().map_err(|e| Error::InferenceError {
            message: e.to_string(),
        })?;
        // offload all layers to the gpu
        let model_params = {
            #[cfg(feature = "cublas")]
            if !disable_gpu {
                LlamaModelParams::default().with_n_gpu_layers(1000)
            } else {
                LlamaModelParams::default()
            }
            #[cfg(not(feature = "cublas"))]
            LlamaModelParams::default()
        };
        let model =
            LlamaModel::load_from_file(&backend, model_path, &model_params).map_err(|e| {
                Error::InferenceError {
                    message: e.to_string(),
                }
            })?;

        let mut ctx_params = LlamaContextParams::default()
            .with_n_ctx(Some(ctx_size))
            .with_seed(seed);
        ctx_params = ctx_params.with_n_threads(threads as u32);
        let mut ctx =
            model
                .new_context(&backend, ctx_params)
                .map_err(|e| Error::InferenceError {
                    message: format!("failed to create context {}", e.to_string()),
                })?;

        let tokens_list =
            model
                .str_to_token(&prompt, AddBos::Always)
                .map_err(|e| Error::InferenceError {
                    message: format!("failed to tokenize prompt {}", e.to_string()),
                })?;
        let n_cxt = ctx.n_ctx() as i32;
        let n_kv_req = tokens_list.len() as i32 + (n_len - tokens_list.len() as i32);
        info!("n_len = {n_len}, n_ctx = {n_cxt}, k_kv_req = {n_kv_req}");
        // make sure the KV cache is big enough to hold all the prompt and generated tokens
        if n_kv_req > n_cxt {
            warn!(
                "n_kv_req > n_ctx, the required kv cache size is not big enough
    either reduce n_len or increase n_ctx"
            )
        }
        if tokens_list.len() >= usize::try_from(n_len).unwrap() {
            warn!("the prompt is too long, it has more tokens than n_len")
        }

        for token in &tokens_list {
            info!(
                "{}",
                model.token_to_str(*token, Special::Tokenize).map_err(|e| {
                    Error::InferenceError {
                        message: format!("failed to convert token to string {}", e.to_string()),
                    }
                })?
            );
        }

        // create a llama_batch with size 512
        // we use this object to submit token data for decoding
        let mut batch = LlamaBatch::new(512, 1);
        let last_index: i32 = (tokens_list.len() - 1) as i32;
        for (i, token) in (0_i32..).zip(tokens_list.into_iter()) {
            // llama_decode will output logits only for the last token of the prompt
            let is_last = i == last_index;
            batch
                .add(token, i, &[0], is_last)
                .map_err(|e| Error::InferenceError {
                    message: format!("failed to add to batch {}", e.to_string()),
                })?;
        }

        ctx.decode(&mut batch).map_err(|e| Error::InferenceError {
            message: format!("failed to decode batch {}", e.to_string()),
        })?;

        // main loop

        let mut n_cur = batch.n_tokens();
        let mut n_decode = 0;

        let t_main_start = ggml_time_us();

        // The `Decoder`
        let mut decoder = encoding_rs::UTF_8.new_decoder();

        while n_cur <= n_len {
            // sample the next token
            {
                let candidates = ctx.candidates_ith(batch.n_tokens() - 1);

                let candidates_p = LlamaTokenDataArray::from_iter(candidates, false);

                // sample the most likely token
                let new_token_id = ctx.sample_token_greedy(candidates_p);

                // is it an end of stream?
                if new_token_id == model.token_eos() {
                    eprintln!();
                    break;
                }

                let output_bytes = model
                    .token_to_bytes(new_token_id, Special::Tokenize)
                    .map_err(|e| Error::InferenceError {
                        message: format!("failed to parse token {}", e.to_string()),
                    })?;
                // use `Decoder.decode_to_string()` to avoid the intermediate buffer
                let mut output_string = String::with_capacity(32);
                let _decode_result =
                    decoder.decode_to_string(&output_bytes, &mut output_string, false);
                info!("{}", output_string);
                res.push(output_string);
                batch.clear();
                batch
                    .add(new_token_id, n_cur, &[0], true)
                    .map_err(|e| Error::InferenceError {
                        message: format!("failed to add batch {}", e.to_string()),
                    })?;
            }

            n_cur += 1;

            ctx.decode(&mut batch).map_err(|e| Error::InferenceError {
                message: format!("failed to decode {}", e.to_string()),
            })?;

            n_decode += 1;
        }

        let t_main_end = ggml_time_us();

        let duration = Duration::from_micros((t_main_end - t_main_start) as u64);

        info!(
            "decoded {} tokens in {:.2} s, speed {:.2} t/s\n",
            n_decode,
            duration.as_secs_f32(),
            n_decode as f32 / duration.as_secs_f32()
        );
        
        info!("{}", ctx.timings());
        
        Ok(res.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::path::PathBuf;
    use std::io::Write;
    use env_logger::Env;
    use chrono::Local;
    #[tokio::test]
    async fn test_model_inference() {
        let _logger = env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .format(|buf, record| {
            let level = { buf.default_level_style(record.level()) };
            writeln!(
                buf,
                "{} {} [{}:{}] {}",
                Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                format_args!("{:>5}", level),
                record.module_path().unwrap_or("<unnamed>"),
                record.line().unwrap_or(0),
                &record.args()
            )
        })
        .init();
        let base_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("aizel");
        let config = NodeConfig {
            socket_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127,0,0,1)), 8080),
            root_path: base_dir,
            gate_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127,0,0,1)), 8080),
            data_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127,0,0,1)), 8080),
            contract_address: "".to_string(),
        };
        let inference = AizelInference {
            config,
            secret: Secret::new()
        };
        let res = inference.model_inference("llama2-7b-chat.Q4_0.gguf".to_string(), "What is the capital of the United States?".to_string()).await.unwrap();
        println!("{:?}", res);

    }

    #[tokio::test]
    async fn test_contract() {
        let tx = &INFERENCE_CONTRACT.submit_inference(1.into(), "mock output".to_string());
        let _pending_tx = tx
            .send()
            .await.unwrap();
    }
}
