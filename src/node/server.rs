mod aizel {
    include!(concat!(env!("OUT_DIR"), "/aizel.rs"));
}

use super::aizel::inference_server::Inference;
use super::aizel::{InferenceRequest, InferenceResponse};
use super::config::{
    models_dir, root_dir, AIZEL_CONFIG, DEFAULT_CHANNEL_SIZE, DEFAULT_MODEL, INPUT_BUCKET,
    MODEL_BUCKET, OUTPUT_BUCKET, REPORT_BUCKET, LLAMA_SERVER_PORT
};
use crate::chains::{
    contract::{Contract, WALLET},
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
use log::{error, info};
use openai_api_rs::v1::api::OpenAIClient;
use openai_api_rs::v1::chat_completion::{self, ChatCompletionRequest};
use secp256k1::{PublicKey, SecretKey};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::{fs, str::FromStr};
use tokio::sync::mpsc::{channel, Sender};
use tonic::{Request, Response, Status};

pub struct AizelInference {
    pub secret: Secret,
    sender: Sender<InferenceRequest>,
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
    pub fn new(secret: Secret) -> Self {
        let (tx, mut rx) = channel::<InferenceRequest>(DEFAULT_CHANNEL_SIZE);

        let aizel_inference: AizelInference = Self {
            secret: secret.clone(),
            sender: tx,
        };
        tokio::spawn(async move {
            let mut child =
                AizelInference::run_llama_server(&models_dir().join(DEFAULT_MODEL)).unwrap();
            let mut current_model = DEFAULT_MODEL.to_string();
            let agent = AttestationAgent::new()
                .await
                .map_err(|e| {
                    error!("failed to create attestation agent {}", e);
                    e
                })
                .unwrap();
            std::env::set_var("OPENAI_API_BASE", "http://localhost:8888/v1");
            while let Some(req) = rx.recv().await {
                if req.model != current_model {
                    info!("change model from {} to {}", current_model, req.model);
                    match child.kill() {
                        Err(e) => error!("failed to kill llama server {}", e.to_string()),
                        Ok(()) => {
                            child.wait().unwrap();
                            child =
                                AizelInference::run_llama_server(&models_dir().join(&req.model))
                                    .unwrap();
                            current_model = req.model.clone();
                        }
                    }
                }
                match AizelInference::process_inference(&req, secret.clone(), &agent).await {
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
        });

        aizel_inference
    }

    async fn process_inference(
        req: &InferenceRequest,
        secret: Secret,
        agent: &AttestationAgent,
    ) -> Result<(), Error> {
        let model = req.model.clone();
        let client = MinioClient::get();
        let user_input = client.get_inputs(INPUT_BUCKET, &req.input).await?;
        let decrypted_input = AizelInference::decrypt(&secret, user_input.input)?;

        let output = match model.as_str() {
            "Agent-1.0" => {
                let transfer_info: Vec<&str> = decrypted_input.split(' ').collect();
                if transfer_info.len() != 5 {
                    return Err(Error::InferenceError {
                        message: "failed to parse the instruction".to_string(),
                    });
                }
                let from = pubkey_to_address(&req.user_pk).unwrap();
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
            _ => {
                // process llama model
                if !AizelInference::check_model_exist(&models_dir(), &model).await? {
                    info!("download models from data node {}", model);
                    let _ = client
                        .download_model(MODEL_BUCKET, &model, &models_dir().join(&model))
                        .await?;
                }
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
            }
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
        let child: Child = Command::new("python3")
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
            .arg(LLAMA_SERVER_PORT.into())
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
