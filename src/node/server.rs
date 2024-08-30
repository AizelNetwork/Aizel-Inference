mod aizel {
    include!(concat!(env!("OUT_DIR"), "/aizel.rs"));
}

use super::aizel::inference_server::Inference;
use super::aizel::{InferenceRequest, InferenceResponse, InferenceType};
use super::config::{
    models_dir, root_dir, AIZEL_CONFIG, DEFAULT_CHANNEL_SIZE, DEFAULT_MODEL, FACE_MODEL_SERVICE,
    INPUT_BUCKET, LLAMA_SERVER_PORT, MODEL_BUCKET, OUTPUT_BUCKET, REPORT_BUCKET,
};
use crate::chains::contract::ModelInfo;
use crate::chains::{
    contract::Contract,
    ethereum::pubkey_to_address,
};
use crate::crypto::digest::Digest;
use crate::crypto::elgamal::{Ciphertext, Elgamal};
use crate::crypto::secret::Secret;
use crate::s3_minio::client::MinioClient;
use crate::tee::attestation::AttestationAgent;
use base64::{engine::general_purpose::STANDARD, Engine};
use common::error::Error;
use ethers::{
    core::{
        abi::{self, Token},
        utils,
    }
};
use log::{error, info};
use openai_api_rs::v1::api::OpenAIClient;
use openai_api_rs::v1::chat_completion::{self, ChatCompletionRequest};
use reqwest::multipart::{Form, Part};
use secp256k1::{PublicKey, SecretKey};
use serde::Deserialize;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::{fs, str::FromStr};
use tokio::sync::mpsc::{channel, Sender};
use tonic::{Request, Response, Status};
pub struct AizelInference {
    pub secret: Secret,
    sender: Sender<InferenceRequest>,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct ModelServiceResponse {
    pub code: u16,
    pub msg: String,
    pub data: bool,
}

#[tonic::async_trait]
impl Inference for AizelInference {
    async fn llama_inference(
        &self,
        request: Request<InferenceRequest>,
    ) -> Result<Response<InferenceResponse>, Status> {
        let req = request.into_inner();
        self.sender.send(req).await.map_err(|e| {
            Status::internal(format!("failed to process request {}", e.to_string()))
        })?;
        Ok(Response::new(InferenceResponse {
            output: String::new(),
        }))
    }
}

impl AizelInference {
    pub fn new(secret: Secret, default_model_info: Option<ModelInfo>) -> Self {
        let (tx, mut rx) = channel::<InferenceRequest>(DEFAULT_CHANNEL_SIZE);

        let aizel_inference: AizelInference = Self {
            secret: secret.clone(),
            sender: tx,
        };
        tokio::spawn(async move {
            let (mut child, mut current_model) = match default_model_info {
                Some(m) => {
                    (AizelInference::run_llama_server(&models_dir().join(&m.name)).unwrap(), m.name)
                },
                None => {
                    (AizelInference::run_llama_server(&models_dir().join(DEFAULT_MODEL)).unwrap(), DEFAULT_MODEL.to_string())
                }
            };
            let agent = AttestationAgent::new()
                .await
                .map_err(|e| {
                    error!("failed to create attestation agent {}", e);
                    e
                })
                .unwrap();
            while let Some(req) = rx.recv().await {
                let model_info = Contract::query_model(req.model_id).await;
                match model_info {
                    Ok(model_info) => {
                        let model_name = model_info.name;
                        let model_cid = model_info.cid;
                        match InferenceType::try_from(req.inference_type) {
                            Ok(InferenceType::Llama) => {
                                if model_name != current_model {
                                    // process llama model
                                    if !AizelInference::check_model_exist(&models_dir(), &model_name)
                                        .await
                                        .unwrap()
                                    {
                                        let client = MinioClient::get().await;
                                        match client
                                            .download_model(
                                                MODEL_BUCKET,
                                                &model_cid,
                                                &models_dir().join(&model_name),
                                            )
                                            .await
                                        {
                                            Ok(_) => {
                                                info!("download model from data node {}", model_name);
                                            }
                                            Err(e) => {
                                                error!("failed to downlaod model: {}", e.to_string());
                                            }
                                        }
                                    }
                                    info!("change model from {} to {}", current_model, model_name);
                                    match child.kill() {
                                        Err(e) => {
                                            error!("failed to kill llama server {}", e.to_string())
                                        }
                                        Ok(()) => {
                                            child.wait().unwrap();
                                            child = AizelInference::run_llama_server(
                                                &models_dir().join(&model_name),
                                            )
                                            .unwrap();
                                            current_model = model_name.clone();
                                        }
                                    }
                                }
                            },
                            Err(e) => {
                                error!("failed to decode inference type {}", e.to_string());
                                continue ;
                            },
                
                            _ => { }
                        }
                        match AizelInference::process_inference(&req, secret.clone(), &agent).await
                        {
                            Ok(_) => {
                                info!("successfully processed the request {}", req.request_id);
                            }
                            Err(e) => {
                                error!(
                                    "failed to process the request {}: {}",
                                    req.request_id,
                                    e.to_string()
                                );
                            }
                        };
                    }
                    Err(e) => {
                        error!("failed to query model from contract {}", e.to_string())
                    }
                }
            }
        });

        aizel_inference
    }

    async fn process_inference(
        req: &InferenceRequest,
        secret: Secret,
        agent: &AttestationAgent,
    ) -> Result<(), Error> {
        let client: std::sync::Arc<MinioClient> = MinioClient::get().await;
        let user_input = client.get_inputs(INPUT_BUCKET, &req.input).await?;
        let decrypted_input = AizelInference::decrypt(&secret, user_input.input)?;

        let output: String = match InferenceType::try_from(req.inference_type) {
            Ok(InferenceType::Llama) => {
                let client = OpenAIClient::new(String::new());
                let req = ChatCompletionRequest::new(
                    String::new(),
                    vec![chat_completion::ChatCompletionMessage {
                        role: chat_completion::MessageRole::user,
                        content: chat_completion::Content::Text(decrypted_input),
                        name: None,
                        tool_calls: None,
                        tool_call_id: None,
                    }],
                );
                let result =
                    client
                        .chat_completion(req)
                        .await
                        .map_err(|e| Error::InferenceError {
                            message: format!(
                                "failed to request local llama sevrer {}",
                                e.to_string()
                            ),
                        })?;
                match &result.choices[0].message.content {
                    Some(c) => c.clone(),
                    None => {
                        return Err(Error::InferenceError {
                            message: "response is empty from local llama server".to_string(),
                        });
                    }
                }
            },
            Ok(InferenceType::FaceValidate) => {
                let file = decrypted_input.clone();
                let base64_encoded = file.split(',').last().unwrap_or_default();
                let image_data = STANDARD.decode(base64_encoded).unwrap();
                let user_id = pubkey_to_address(&req.user_pk).unwrap();

                let file_part = Part::bytes(image_data).mime_str("image/png").map_err(|_| {
                    Error::InferenceError {
                        message: "image format is not png".to_string(),
                    }
                })?;

                let form = Form::new().part("files", file_part).text("userId", user_id);

                let client = reqwest::Client::new();
                let response = client
                    .post(FACE_MODEL_SERVICE)
                    .multipart(form)
                    .send()
                    .await
                    .map_err(|e| Error::InferenceError {
                        message: format!("failed to validate face image {}", e.to_string()),
                    })?;
                let resp: ModelServiceResponse =
                    response.json().await.map_err(|e| Error::InferenceError {
                        message: format!("failed to parse response {}", e.to_string()),
                    })?;
                if resp.data {
                    "true".to_string()
                } else {
                    "false".to_string()
                }
            },
            Err(e) => {
                error!("failed to query model from contract {}", e.to_string());
                e.to_string()
            },
            _ => {
                "TO BE IMPLEMENTED".to_string()
            },
        };

        let encrypted_output: String = AizelInference::encrypt(output, &req.user_pk)?;
        let output_hash: Digest = AizelInference::hash(&encrypted_output);
        let _ = client
            .upload(
                OUTPUT_BUCKET,
                &output_hash.to_string(),
                encrypted_output.as_bytes(),
            )
            .await?;
        // upload the report to minio bucket
        if AIZEL_CONFIG.within_tee {
            let report = agent
                .get_attestation_report(output_hash.to_string())
                .await?;
            let report_hash = AizelInference::hash(&report);
            let _ = client
                .upload(REPORT_BUCKET, &report_hash.to_string(), report.as_bytes())
                .await?;
            let _ =
                Contract::submit_inference(req.request_id, output_hash.0, report_hash.0).await?;
        } else {
            let report = "mock report".to_string();
            let report_hash = AizelInference::hash(&report);
            let _ = client
                .upload(REPORT_BUCKET, &report_hash.to_string(), report.as_bytes())
                .await?;
            let _ =
                Contract::submit_inference(req.request_id, output_hash.0, report_hash.0).await?;
        }
        Ok(())
    }

    fn hash(message: &str) -> Digest {
        let token_hash = abi::encode_packed(&[Token::String(message.to_string())]).unwrap();
        Digest(utils::keccak256(token_hash))
    }

    pub fn decrypt(secret: &Secret, ciphertext: String) -> Result<String, Error> {
        let ciphertext = hex::decode(&ciphertext).map_err(|e| Error::InvalidArgumentError {
            argument: ciphertext,
            message: format!("failed to decode hex string {}", e.to_string()),
        })?;
        let ct = Ciphertext::from_bytes(ciphertext.as_slice());
        let rng = rand::thread_rng();
        let mut elgamal = Elgamal::new(rng);
        let plain = elgamal
            .decrypt(&ct, &SecretKey::from_slice(&secret.secret.0).unwrap())
            .map_err(|e| Error::InferenceError {
                message: format!("failed to decrypt input: {}", e.to_string()),
            })?;
        Ok(String::from_utf8(plain).unwrap())
    }

    fn encrypt(plaintext: String, user_pk: &str) -> Result<String, Error> {
        let rng = rand::thread_rng();
        let mut elgamal = Elgamal::new(rng);
        let ct = elgamal
            .encrypt(
                plaintext.as_bytes(),
                &PublicKey::from_slice(&hex::decode(user_pk).unwrap()).unwrap(),
            )
            .map_err(|e| Error::InferenceError {
                message: format!("failed to encrypt input {}", e.to_string()),
            })?;
        Ok(hex::encode(ct.to_bytes()))
    }

    pub async fn check_model_exist(model_path: &PathBuf, model: &str) -> Result<bool, Error> {
        for entry in fs::read_dir(&model_path).map_err(|e| Error::FileError {
            path: model_path.clone(),
            message: e.to_string(),
        })? {
            let entry = entry.map_err(|e| Error::FileError {
                path: model_path.clone(),
                message: e.to_string(),
            })?;
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_file() && entry.file_name().to_string_lossy() == *model {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    pub fn run_llama_server(model_path: &PathBuf) -> Result<Child, Error> {
        let llama_server_output = fs::File::create(root_dir().join("llama_stdout.txt")).unwrap();
        let llama_server_error = fs::File::create(root_dir().join("llama_stderr.txt")).unwrap();
        info!("llama server model path {}", model_path.to_str().unwrap());
        let child: Child = Command::new("/home/jiangyi/aizel-python/bin/python")
            .arg("-m")
            .arg("llama_cpp.server")
            .arg("--model")
            .arg(model_path.to_str().unwrap())
            .arg("--seed")
            .arg("-1")
            .arg("--n_threads")
            .arg("-1")
            .arg("--n_threads_batch")
            .arg("-1")
            .arg("--port")
            .arg::<String>(format!("{}", LLAMA_SERVER_PORT))
            .stdout(Stdio::from(llama_server_output))
            .stderr(Stdio::from(llama_server_error))
            .spawn()
            .expect("Failed to start Python script");
        Ok(child)
    }
}

#[tokio::test]
async fn test_openai_client() {
    use openai_api_rs::v1::api::OpenAIClient;
    use openai_api_rs::v1::chat_completion::{self, ChatCompletionRequest};
    std::env::set_var("OPENAI_API_BASE", "http://localhost:8000/v1");
    let client = OpenAIClient::new(String::new());

    let req = ChatCompletionRequest::new(
        String::new(),
        vec![chat_completion::ChatCompletionMessage {
            role: chat_completion::MessageRole::user,
            content: chat_completion::Content::Text(String::from("What is bitcoin?")),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }],
    );
    let result = client.chat_completion(req).await.unwrap();
    println!("Content: {:?}", result.choices[0].message.content);
    println!("Response Headers: {:?}", result.headers);
}

#[tokio::test]
async fn test_load_llama_cpp_server() {
    let client = MinioClient::get().await;
    // let model_path = "/home/jiangyi/aizel/models/llama2_7b_chat.Q4_0.gguf-1.0";
    let model_path = "llama2_7b_chat.Q4_0.gguf-1.0";
    let input = client
        .download_model("models", model_path, &PathBuf::from(model_path))
        .await;
    println!("finished {:?}", input.unwrap());

    let mut child = AizelInference::run_llama_server(&PathBuf::from(model_path)).unwrap();
    child.wait();
}
