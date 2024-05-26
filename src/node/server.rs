mod aizel {
    include!(concat!(env!("OUT_DIR"), "/aizel.rs"));
}
use super::aizel::inference_server::Inference;
use super::aizel::{InferenceRequest, InferenceResponse};
use super::config::{NodeConfig, DEFAULT_MODEL_DIR};
use common::error::Error;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::ggml_time_us;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::{AddBos, Special};
use llama_cpp_2::token::data_array::LlamaTokenDataArray;
use log::{info, warn};
use std::fs;
use std::num::NonZeroU32;
use std::time::Duration;
use tonic::{Request, Response, Status};

#[derive(Debug)]
pub struct AizelInference {
    pub config: NodeConfig,
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

        // model inference
        self.model_inference(model, req.input).await.map_err(|e| {
            Status::internal(e.to_string())
        })?;
        Ok(Response::new(InferenceResponse {}))
    }
}

impl AizelInference {
    async fn check_model_exist(&self, model: String) -> Result<bool, Error> {
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

    async fn download_model(&self, model: String) -> Result<(), Error> {
        Ok(())
    }

    async fn model_inference(&self, model_name: String, prompt: String) -> Result<Vec<String>, Error> {
        let model_path = self.config.root_path.join(DEFAULT_MODEL_DIR).join(model_name);
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

                let output_bytes = model.token_to_bytes(new_token_id, Special::Tokenize).map_err(|e| {
                    Error::InferenceError {
                        message: format!("failed to parse token {}", e.to_string()),
                    }
                })?;
                // use `Decoder.decode_to_string()` to avoid the intermediate buffer
                let mut output_string = String::with_capacity(32);
                let _decode_result =
                    decoder.decode_to_string(&output_bytes, &mut output_string, false);
                info!("{}", output_string);
                res.push(output_string);
                batch.clear();
                batch.add(new_token_id, n_cur, &[0], true).map_err(|e| {
                    Error::InferenceError {
                        message: format!("failed to add batch {}", e.to_string()),
                    }
                })?;
            }

            n_cur += 1;

            ctx.decode(&mut batch).map_err(|e| {
                Error::InferenceError {
                    message: format!("failed to decode {}", e.to_string()),
                }
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

        Ok(res)
    }
}
