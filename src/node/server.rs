mod aizel {
    include!(concat!(env!("OUT_DIR"), "/aizel.rs"));
}
use super::aizel::inference_server::Inference;
use super::aizel::{InferenceRequest, InferenceResponse};
use super::config::{
    NodeConfig, DEFAULT_MODEL_DIR, INPUT_BUCKET, MODEL_BUCKET, OUTPUT_BUCKET, REPORT_BUCKET,
};
use crate::chains::{
    contract::{INFERENCE_CONTRACT, WALLET},
    ethereum::pubkey_to_address,
};
use crate::crypto::digest::Digest;
use crate::crypto::elgamal::{Ciphertext, Elgamal};
use crate::crypto::secret::Secret;
use crate::s3_minio::client::MinioClient;
use crate::tee::attestation::AttestationAgent;
use common::error::Error;
use ethers::{
    core::{
        abi::{self, Token},
        utils,
    },
    signers::Signer,
    types::{H160, U256},
};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::ggml_time_us;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::{AddBos, Special};
use llama_cpp_2::token::data_array::LlamaTokenDataArray;
use log::error;
use log::{info, warn};
use rand::Rng;
use secp256k1::{PublicKey, SecretKey};
use std::time::Duration;
use std::{fs, num::NonZeroU32, str::FromStr};
use tonic::{Request, Response, Status};

pub struct AizelInference {
    pub config: NodeConfig,
    pub secret: Secret,
}

fn generate_random(length: usize) -> String {
    let mut rng = rand::thread_rng();
    let mut dest = vec![0; length];
    rng.fill(&mut dest[..]);
    hex::encode(dest)
}

fn calc_hash(message: &str) -> Digest {
    let token_hash = abi::encode_packed(&[Token::String(message.to_string())]).unwrap();
    Digest(utils::keccak256(token_hash))
}

#[tonic::async_trait]
impl Inference for AizelInference {
    async fn llama_inference(
        &self,
        request: Request<InferenceRequest>,
    ) -> Result<Response<InferenceResponse>, Status> {
        let req = request.into_inner();
        let model = req.model.clone();

        let client = MinioClient::get();
        let user_input = client
            .get_inputs(INPUT_BUCKET.to_string(), req.input.clone())
            .await
            .unwrap();

        let decrypted_input = self.decrypt(user_input.input)?;
        let output = match model.as_str() {
            "Agent-1.0" => {
                let transfer_info: Vec<&str> = decrypted_input.split(' ').collect();
                if transfer_info.len() != 5 {
                    return Err(Status::invalid_argument(
                        "failed to parse the instruction".to_string(),
                    ));
                }
                let from = pubkey_to_address(req.user_pk.clone()).unwrap();
                let encoded_data = [
                    abi::encode_packed(&[
                        Token::Address(H160::from_str(&from).unwrap()),
                        Token::Address(H160::from_str(transfer_info[4]).unwrap()),
                    ])
                    .unwrap(),
                    abi::encode(&[Token::Uint(U256::from_dec_str(transfer_info[1]).unwrap())]),
                    abi::encode_packed(&[Token::Address(
                        H160::from_str(transfer_info[2]).unwrap(),
                    )])
                    .unwrap(),
                ]
                .concat();
                let message = utils::keccak256(&encoded_data);
                let signature = WALLET.sign_message(message).await.unwrap();
                signature
                    .verify(message.as_ref(), WALLET.address())
                    .unwrap();
                format!(
                    "{:} from {:} signature 0x{:}",
                    decrypted_input, from, signature
                )
            }
            "Auth-1.0" => {
                let cid = generate_random(24);
                let from = pubkey_to_address(req.user_pk.clone()).unwrap();
                let encoded_data = abi::encode_packed(&[
                    Token::Address(H160::from_str(&from).unwrap()),
                    Token::String(cid.clone()),
                ])
                .unwrap();
                let message = utils::keccak256(&encoded_data);
                let signature = WALLET.sign_message(message).await.unwrap();
                signature
                    .verify(message.as_ref(), WALLET.address())
                    .unwrap();
                format!("user {:} cid {:} signature {:}", from, cid, signature)
            }
            _ => {
                // model inference
                if !self
                    .check_model_exist(model.clone())
                    .await
                    .map_err(|e| Status::internal(e.to_string()))?
                {
                    info!("download models from data node {}", model);
                    let _ = client
                        .download_model(
                            MODEL_BUCKET.to_string(),
                            model.clone(),
                            self.config.root_path.join(DEFAULT_MODEL_DIR).join(&model),
                        )
                        .await
                        .map_err(|e| Status::internal(e.to_string()))?;
                }

                self.model_inference(model, decrypted_input)
                    .await
                    .map_err(|e| Status::internal(e.to_string()))?
            }
        };

        if output.len() == 0 {
            return Err(Status::internal("failed to generate output"));
        }

        // encrypt output
        let encrypted_output = self.encrypt(output, req.user_pk)?;
        let output_hash = calc_hash(&encrypted_output);

        // upload the output to the minio bucket
        let _ = client
            .upload(
                OUTPUT_BUCKET.to_string(),
                output_hash.to_string().clone(),
                encrypted_output.as_bytes(),
            )
            .await
            .map_err(|e| {
                error!("failed to upload output {}", e);
                Status::internal("failed to upload output")
            })?;

        // upload the report to minio bucket
        // {
        //     let agent = AttestationAgent::new().await.map_err(|e| {
        //         error!("failed to create attestation agent {}", e);
        //         Status::internal("failed to create attestation agent")
        //     })?;
        //     let report = agent
        //         .get_attestation_report(encrypted_output)
        //         .await
        //         .map_err(|e| {
        //             error!("failed to get attestation report {}", e);
        //             Status::internal("failed to get attestation report")
        //         })?;
        //     let report_hash = calc_hash(&report);
        //     client
        //         .upload(REPORT_BUCKET.to_string(), report_hash, report.as_bytes())
        //         .await
        //         .map_err(|e| {
        //             error!("failed to upload output {}", e);
        //             Status::internal("failed to upload output")
        //         })?;
        // }
        let mock_report = "mock report";
        let report_hash = calc_hash(&mock_report);
        client
            .upload(
                REPORT_BUCKET.to_string(),
                report_hash.to_string().clone(),
                mock_report.as_bytes(),
            )
            .await
            .map_err(|e| {
                error!("failed to upload output {}", e);
                Status::internal("failed to upload output")
            })?;
        // send output hash to the contract
        let tx = &INFERENCE_CONTRACT.submit_inference(
            req.request_id.into(),
            output_hash.0,
            report_hash.0,
        );

        let _pending_tx = tx
            .send()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(InferenceResponse {
            output: output_hash.to_stirng(),
        }))
    }
}

impl AizelInference {
    fn decrypt(&self, ciphertext: String) -> Result<String, Status> {
        let ciphertext = hex::decode(ciphertext)
            .map_err(|e| Status::internal(format!("failed decode ciphertext {}", e.to_string())))?;
        let ct = Ciphertext::from_bytes(ciphertext.as_slice());
        let rng = rand::thread_rng();
        let mut elgamal = Elgamal::new(rng);
        let plain = elgamal
            .decrypt(&ct, &SecretKey::from_slice(&self.secret.secret.0).unwrap())
            .map_err(|e| Status::internal(format!("failed to decrypt input {}", e.to_string())))?;
        Ok(String::from_utf8(plain).map_err(|e| {
            Status::internal(format!("failde to get plain input {}", e.to_string()))
        })?)
    }

    fn encrypt(&self, plaintext: String, user_pk: String) -> Result<String, Status> {
        let rng = rand::thread_rng();
        let mut elgamal = Elgamal::new(rng);
        let ct = elgamal
            .encrypt(
                plaintext.as_bytes(),
                &PublicKey::from_slice(&hex::decode(user_pk).unwrap()).unwrap(),
            )
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(hex::encode(ct.to_bytes()))
    }

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

    async fn model_inference(&self, model_name: String, prompt: String) -> Result<String, Error> {
        let model_path = self
            .config
            .root_path
            .join(DEFAULT_MODEL_DIR)
            .join(model_name);
        // LLM parameters
        // TODO: update
        let ctx_size = NonZeroU32::new(64).unwrap();
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
    use chrono::Local;
    use env_logger::Env;
    use std::io::Write;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::path::PathBuf;
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
            socket_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
            root_path: base_dir,
            data_id: 1,
        };
        let inference = AizelInference {
            config,
            secret: Secret::new(),
        };
        let res = inference
            .model_inference(
                "llama2-7b-chat.Q4_0.gguf".to_string(),
                "What is the capital of the United States?".to_string(),
            )
            .await
            .unwrap();
        println!("{:?}", res);
    }
}
