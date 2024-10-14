mod aizel {
    include!(concat!(env!("OUT_DIR"), "/aizel.rs"));
}
use super::aizel::gate_service_client::GateServiceClient;
use super::aizel::inference_server::Inference;
use super::aizel::{InferenceRequest, InferenceResponse};
use super::aizel::UploadOutputRequest;
use super::config::{AIZEL_CONFIG, DEFAULT_CHANNEL_SIZE, INPUT_BUCKET, TRANSFER_AGENT_ID};
use super::model_client::{ChatClient, TransferAgentClient, MlClient};
use super::model_server::MlServer;
use crate::chains::contract::Contract;
use crate::chains::contract::ModelInfo;
use crate::chains::ethereum::pubkey_to_address;
use crate::crypto::digest::Digest;
use crate::crypto::elgamal::{Ciphertext, Elgamal};
use crate::crypto::secret::Secret;
use crate::node::model_server::LlamaServer;
use crate::s3_minio::client::MinioClient;
use crate::tee::attestation::AttestationAgent;
use common::error::Error;
use ethers::core::{
    abi::{self, Token},
    utils,
};
use log::{error, info};
use secp256k1::{PublicKey, SecretKey};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{channel, Sender};
use tonic::{Request, Response, Status};
pub struct AizelInference {
    pub secret: Secret,
    sender: Sender<InferenceRequest>,
}

type Hash = [u8; 32];

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct ModelServiceResponse {
    pub code: u16,
    pub msg: String,
    pub data: bool,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct InferenceOutput {
    pub output: String,
    pub report: String,
}

#[derive(Deserialize, Serialize, Debug)]
struct TransferInfo {
    to: String,
    token: String,
    amount: u64,
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
    pub async fn new(secret: Secret, default_model_info: ModelInfo) -> Self {
        let (tx, mut rx) = channel::<InferenceRequest>(DEFAULT_CHANNEL_SIZE);

        let aizel_inference: AizelInference = Self {
            secret: secret.clone(),
            sender: tx,
        };
        let mut llama_cpp_server = LlamaServer::new(&default_model_info).await.unwrap();
        let mut ml_server = MlServer::new(&None).await.unwrap();

        tokio::spawn(async move {
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
                        if req.req_type == aizel::InferenceType::AizelModel as i32 {
                            match ml_server.run(&model_info).await {
                                Err(e) => {
                                    error!("failed to run model {}", e.to_string());
                                    continue;
                                }
                                Ok(()) => {}
                            }
                        } else {
                            match llama_cpp_server.run(&model_info).await {
                                Err(e) => {
                                    error!("failed to run model {}", e.to_string());
                                    continue;
                                }
                                Ok(()) => {}
                            }
                        }
                        match AizelInference::process_inference(&req, secret.clone(), &agent, &model_info).await
                        {
                            Ok(output) => {
                                info!("successfully processed the request {}", req.request_id);
                                // submit output to gate server
                                let (output_hash, report_hash) =
                                    AizelInference::submit_output(output.output, output.report)
                                        .await
                                        .unwrap();
                                let _ = Contract::submit_inference(
                                    req.request_id,
                                    output_hash,
                                    report_hash,
                                )
                                .await;
                            }
                            Err(e) => {
                                error!(
                                    "failed to process the request {}: {}",
                                    req.request_id,
                                    e.to_string()
                                );
                                AizelInference::handle_error(&req, e, &agent).await;
                            }
                        };
                    }
                    Err(e) => {
                        error!("failed to query model from contract {}", e.to_string());
                        AizelInference::handle_error(&req, e, &agent).await;
                    }
                }
            }
        });

        aizel_inference
    }

    async fn submit_output(output: String, report: String) -> Result<(Hash, Hash), Error> {
        let mut client = GateServiceClient::connect(AIZEL_CONFIG.gate_url.clone())
            .await
            .map_err(|_| Error::InferenceError {
                message: "failed to connect to gate server".to_string(),
            })
            .unwrap();
        let response = client
            .upload_output(UploadOutputRequest { output, report })
            .await
            .unwrap();
        let mut resp: crate::node::aizel::UploadOutputResponse = response.into_inner();
        let output_hash = hex::decode(resp.output_hash.split_off(2))
            .unwrap()
            .try_into()
            .unwrap();
        let report_hash = hex::decode(resp.report_hash.split_off(2))
            .unwrap()
            .try_into()
            .unwrap();
        Ok((output_hash, report_hash))
    }

    async fn handle_error(req: &InferenceRequest, e: Error, agent: &AttestationAgent) {
        let output = e.to_string();
        let encrypted_output: String = match AizelInference::encrypt(&output, &req.user_pk) {
            Ok(s) => s,
            Err(_) => {
                error!("failed to encrypt error msg");
                return;
            }
        };
        let output_hash: Digest = AizelInference::hash(&encrypted_output);
        let report = if AIZEL_CONFIG.within_tee {
            agent
                .get_attestation_report(output_hash.to_string())
                .await
                .unwrap()
        } else {
            "mock report".to_string()
        };

        let report_hash: Digest = AizelInference::hash(&report);
        let _ = AizelInference::submit_output(encrypted_output, report).await;
        let _ = Contract::submit_inference(req.request_id, output_hash.0, report_hash.0).await;
    }

    async fn process_inference(
        req: &InferenceRequest,
        secret: Secret,
        agent: &AttestationAgent,
        model_info: &ModelInfo
    ) -> Result<InferenceOutput, Error> {
        let client: std::sync::Arc<MinioClient> = MinioClient::get_public_client().await;
        let user_input = client.get_inputs(INPUT_BUCKET, &req.input).await?;
        let decrypted_input = AizelInference::decrypt(&secret, &user_input.input)?;

        let output = if req.req_type == aizel::InferenceType::AizelModel as i32 {
            MlClient::request(decrypted_input, model_info.name.clone()).await?
        } else {
            if req.model_id == TRANSFER_AGENT_ID {
                let from = pubkey_to_address(&req.user_pk).unwrap();
                TransferAgentClient::transfer(req.request_id, decrypted_input, from).await?
            } else {
                ChatClient::request(decrypted_input).await?
            }
        };

        let encrypted_output: String = AizelInference::encrypt(&output, &req.user_pk)?;
        let output_hash: Digest = AizelInference::hash(&encrypted_output);
        // upload the report to minio bucket
        let report = if AIZEL_CONFIG.within_tee {
            agent
                .get_attestation_report(output_hash.to_string())
                .await?
        } else {
            "mock report".to_string()
        };

        Ok(InferenceOutput {
            output: encrypted_output,
            report,
        })
    }

    fn hash(message: &str) -> Digest {
        let token_hash = abi::encode_packed(&[Token::String(message.to_string())]).unwrap();
        Digest(utils::keccak256(token_hash))
    }

    pub fn decrypt(secret: &Secret, ciphertext: &str) -> Result<String, Error> {
        let ciphertext = hex::decode(&ciphertext).map_err(|e| Error::InvalidArgumentError {
            argument: ciphertext.to_string(),
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

    fn encrypt(plaintext: &str, user_pk: &str) -> Result<String, Error> {
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
