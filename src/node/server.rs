mod aizel {
    include!(concat!(env!("OUT_DIR"), "/aizel.rs"));
}
use super::aizel::gate_service_client::GateServiceClient;
use super::aizel::inference_server::Inference;
use super::aizel::{InferenceRequest, InferenceResponse};
use super::aizel::{UploadOutputRequest, UploadOutputResponse};
use super::config::{
    models_dir, root_dir, AIZEL_CONFIG, COIN_ADDRESS_MAPPING, DEFAULT_CHANNEL_SIZE, DEFAULT_MODEL,
    INPUT_BUCKET, LLAMA_SERVER_PORT, MODEL_BUCKET, TRANSFER_AGENT_ID,
};
use crate::chains::contract::Contract;
use crate::chains::contract::ModelInfo;
use crate::chains::ethereum::pubkey_to_address;
use crate::crypto::digest::Digest;
use crate::crypto::elgamal::{Ciphertext, Elgamal};
use crate::crypto::secret::Secret;
use crate::s3_minio::client::MinioClient;
use crate::tee::attestation::AttestationAgent;
use common::error::Error;
use ethers::core::{
    abi::{self, Token},
    utils,
    utils::{parse_units, ParseUnits},
};
use ethers::types::U256;
use log::{error, info};
use openai_api_rs::v1::api::OpenAIClient;
use openai_api_rs::v1::chat_completion::{self, ChatCompletionRequest};
use secp256k1::{PublicKey, SecretKey};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
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
    pub fn new(secret: Secret, default_model_info: Option<ModelInfo>) -> Self {
        let (tx, mut rx) = channel::<InferenceRequest>(DEFAULT_CHANNEL_SIZE);

        let aizel_inference: AizelInference = Self {
            secret: secret.clone(),
            sender: tx,
        };
        tokio::spawn(async move {
            let (mut child, mut current_model) = match default_model_info {
                Some(m) => (
                    AizelInference::run_llama_server(&models_dir().join(&m.name), m.id).unwrap(),
                    m.id,
                ),
                None => (
                    AizelInference::run_llama_server(&models_dir().join(DEFAULT_MODEL), 0).unwrap(),
                    0,
                ),
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
                        let model_id = model_info.id;
                        let model_name = model_info.name;
                        let model_cid = model_info.cid;
                        if model_id != current_model {
                            // process llama model
                            if !AizelInference::check_model_exist(&models_dir(), &model_name)
                                .await
                                .unwrap()
                            {
                                let client = MinioClient::get_data_client().await;
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
                                        AizelInference::handle_error(&req, e, &agent).await;
                                    }
                                }
                            }
                            info!("change model from {} to {}", current_model, model_name);
                            match child.kill() {
                                Err(e) => {
                                    error!("failed to kill llama server {}", e.to_string());
                                    AizelInference::handle_error(
                                        &req,
                                        Error::InferenceError {
                                            message: "failed to change model".to_string(),
                                        },
                                        &agent,
                                    )
                                    .await;
                                }
                                Ok(()) => {
                                    child.wait().unwrap();
                                    child = AizelInference::run_llama_server(
                                        &models_dir().join(&model_name),
                                        model_id,
                                    )
                                    .unwrap();
                                    current_model = model_id;
                                }
                            }
                        }

                        match AizelInference::process_inference(&req, secret.clone(), &agent).await
                        {
                            Ok(output) => {
                                info!("successfully processed the request {}", req.request_id);
                                // submit output to gate server
                                let mut resp: UploadOutputResponse =
                                    AizelInference::submit_output(output.output, output.report)
                                        .await
                                        .unwrap();
                                let output_hash = hex::decode(resp.output_hash.split_off(2))
                                    .unwrap()
                                    .try_into()
                                    .unwrap();
                                let report_hash = hex::decode(resp.report_hash.split_off(2))
                                    .unwrap()
                                    .try_into()
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

    async fn submit_output(output: String, report: String) -> Result<UploadOutputResponse, Error> {
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
        let resp: crate::node::aizel::UploadOutputResponse = response.into_inner();
        Ok(resp)
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

    async fn transfer_agent(content: String) -> Result<TransferInfo, Error> {
        let mut properties = HashMap::new();
        properties.insert(
            "to".to_string(),
            Box::new(chat_completion::JSONSchemaDefine {
                schema_type: Some(chat_completion::JSONSchemaType::String),
                description: Some("The wallet address of the receiver".to_string()),
                ..Default::default()
            }),
        );
        properties.insert(
            "amount".to_string(),
            Box::new(chat_completion::JSONSchemaDefine {
                schema_type: Some(chat_completion::JSONSchemaType::Number),
                description: Some("The amount of the token to be transferred".to_string()),
                ..Default::default()
            }),
        );
        properties.insert(
            "token".to_string(),
            Box::new(chat_completion::JSONSchemaDefine {
                schema_type: Some(chat_completion::JSONSchemaType::String),
                description: Some("The name of the token to be transferred".to_string()),
                ..Default::default()
            }),
        );
        let client = OpenAIClient::new(String::new());
        let req = ChatCompletionRequest::new(
            String::new(),
            vec![chat_completion::ChatCompletionMessage {
                role: chat_completion::MessageRole::system,
                content: chat_completion::Content::Text(String::from("You are a helpful customer support assistant. Use the supplied tools to assist the user.")),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            },
                chat_completion::ChatCompletionMessage {
                role: chat_completion::MessageRole::user,
                content: chat_completion::Content::Text(content),
                name: None,
                tool_calls: None,
                tool_call_id: None,
            }],
        )
        .tools(vec![chat_completion::Tool {
            r#type: chat_completion::ToolType::Function,
            function: chat_completion::Function {
                name: String::from("TransferAgent"),
                description: Some(String::from("Proxy Transfer or angent Transfer in web3, transfer some tokens to another address.")),
                parameters: chat_completion::FunctionParameters {
                    schema_type: chat_completion::JSONSchemaType::Object,
                    properties: Some(properties),
                    required: Some(vec![String::from("token"), String::from("amount"), String::from("to")]),
                },
            },
        }])
        .tool_choice(chat_completion::ToolChoiceType::Auto);

        let result = client.chat_completion(req).await.unwrap();
        match result.choices[0].finish_reason {
            Some(chat_completion::FinishReason::tool_calls) => {
                let tool_calls = result.choices[0].message.tool_calls.as_ref().unwrap();
                if tool_calls.is_empty() {
                    return Err(Error::InferenceError {
                        message: "function calling failed".to_string(),
                    });
                }
                let tool_call = tool_calls[0].clone();
                let name = tool_call.function.name.clone().unwrap();
                let arguments = tool_call.function.arguments.clone().unwrap();
                let t: TransferInfo = serde_json::from_str(&arguments).unwrap();
                if name == "TransferAgent" {
                    info!("transfer agent result {:?}", t);
                }
                Ok(t)
            }
            _ => {
                error!("function call failed");
                Err(Error::InferenceError {
                    message: "function calling failed".to_string(),
                })
            }
        }
    }

    async fn process_inference(
        req: &InferenceRequest,
        secret: Secret,
        agent: &AttestationAgent,
    ) -> Result<InferenceOutput, Error> {
        let client: std::sync::Arc<MinioClient> = MinioClient::get_public_client().await;
        let user_input = client.get_inputs(INPUT_BUCKET, &req.input).await?;
        let decrypted_input = AizelInference::decrypt(&secret, &user_input.input)?;

        let output = if req.model_id == TRANSFER_AGENT_ID {
            let transfer_info = AizelInference::transfer_agent(decrypted_input).await?;
            let from = pubkey_to_address(&req.user_pk).unwrap();
            let token_address = COIN_ADDRESS_MAPPING.get(&transfer_info.token);
            if token_address.is_none() {
                return Err(Error::InferenceError {
                    message: "failed to transfer, token address is unkown".to_string(),
                });
            }
            let pu: ParseUnits = parse_units(transfer_info.amount, 18).unwrap();
            let amount = U256::from(pu);
            let output = format!(
                "token {}, transfer {} from {} to {}",
                token_address.unwrap(),
                amount,
                from,
                transfer_info.to
            );
            info!("auto transfer output {}", output);
            Contract::transfer(
                req.request_id,
                token_address.unwrap().clone(),
                from,
                transfer_info.to,
                amount,
            )
            .await?;
            output
        } else {
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
            let result = client
                .chat_completion(req)
                .await
                .map_err(|e| Error::InferenceError {
                    message: format!("failed to request local llama sevrer {}", e.to_string()),
                })?;
            match &result.choices[0].message.content {
                Some(c) => c.clone(),
                None => {
                    return Err(Error::InferenceError {
                        message: "response is empty from local llama server".to_string(),
                    });
                }
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

        Ok(InferenceOutput { output, report })
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

    pub fn run_llama_server(model_path: &PathBuf, model_id: u64) -> Result<Child, Error> {
        let llama_server_output = fs::File::create(root_dir().join("llama_stdout.txt")).unwrap();
        let llama_server_error = fs::File::create(root_dir().join("llama_stderr.txt")).unwrap();
        info!("llama server model path {}", model_path.to_str().unwrap());
        // let child: Child =
        let mut command = Command::new("python3");
        let mut command = command
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
            .stderr(Stdio::from(llama_server_error));

        if model_id == TRANSFER_AGENT_ID {
            command = command.arg("--chat_format").arg("chatml-function-calling");
        }
        let child = command.spawn().expect("Failed to start Python script");
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
async fn test_openai_function_tool() {
    use openai_api_rs::v1::api::OpenAIClient;
    use openai_api_rs::v1::chat_completion::{self, ChatCompletionRequest};
    use std::collections::HashMap;

    let mut properties = HashMap::new();
    properties.insert(
        "to".to_string(),
        Box::new(chat_completion::JSONSchemaDefine {
            schema_type: Some(chat_completion::JSONSchemaType::String),
            description: Some("The wallet address of the receiver".to_string()),
            ..Default::default()
        }),
    );
    properties.insert(
        "amount".to_string(),
        Box::new(chat_completion::JSONSchemaDefine {
            schema_type: Some(chat_completion::JSONSchemaType::Number),
            description: Some("The amount of the token to be transferred".to_string()),
            ..Default::default()
        }),
    );
    properties.insert(
        "token".to_string(),
        Box::new(chat_completion::JSONSchemaDefine {
            schema_type: Some(chat_completion::JSONSchemaType::String),
            description: Some("The name of the token to be transferred".to_string()),
            ..Default::default()
        }),
    );

    std::env::set_var("OPENAI_API_BASE", "http://localhost:8000/v1");
    let client = OpenAIClient::new(String::new());
    let req = ChatCompletionRequest::new(
        String::new(),
        vec![chat_completion::ChatCompletionMessage {
            role: chat_completion::MessageRole::system,
            content: chat_completion::Content::Text(String::from("You are a helpful customer support assistant. Use the supplied tools to assist the user.")),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        },
            chat_completion::ChatCompletionMessage {
            role: chat_completion::MessageRole::user,
            content: chat_completion::Content::Text(String::from("Transfer 10 USDT to the address 0xC68884D8bE3D37E2fD61837cB65bc72Aa5a4EBcf")),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }],
    )
    .tools(vec![chat_completion::Tool {
        r#type: chat_completion::ToolType::Function,
        function: chat_completion::Function {
            name: String::from("TransferAgent"),
            description: Some(String::from("Proxy Transfer or angent Transfer in web3, transfer some tokens to another address.")),
            parameters: chat_completion::FunctionParameters {
                schema_type: chat_completion::JSONSchemaType::Object,
                properties: Some(properties),
                required: Some(vec![String::from("token"), String::from("amount"), String::from("to")]),
            },
        },
    }])
    .tool_choice(chat_completion::ToolChoiceType::Auto);

    let result = client.chat_completion(req).await.unwrap();
    match result.choices[0].finish_reason {
        None => {
            println!("No finish_reason");
            println!("{:?}", result.choices[0].message.content);
        }
        Some(chat_completion::FinishReason::stop) => {
            println!("Stop");
            println!("{:?}", result.choices[0].message.content);
        }
        Some(chat_completion::FinishReason::length) => {
            println!("Length");
        }
        Some(chat_completion::FinishReason::tool_calls) => {
            println!("ToolCalls");
            #[derive(Deserialize, Serialize, Debug)]
            struct TransferInfo {
                to: String,
                token: String,
                amount: u64,
            }
            let tool_calls = result.choices[0].message.tool_calls.as_ref().unwrap();
            for tool_call in tool_calls {
                let name = tool_call.function.name.clone().unwrap();
                let arguments = tool_call.function.arguments.clone().unwrap();
                let t: TransferInfo = serde_json::from_str(&arguments).unwrap();
                if name == "TransferAgent" {
                    println!("result {:?}", t);
                }
            }
        }
        Some(chat_completion::FinishReason::content_filter) => {
            println!("ContentFilter");
        }
        Some(chat_completion::FinishReason::null) => {
            println!("Null");
        }
    }
}

#[tokio::test]
async fn test_load_llama_cpp_server() {
    let client = MinioClient::get_data_client().await;
    // let model_path = "/home/jiangyi/aizel/models/llama2_7b_chat.Q4_0.gguf-1.0";
    let model_path = "llama2_7b_chat.Q4_0.gguf-1.0";
    let input = client
        .download_model("models", model_path, &PathBuf::from(model_path))
        .await;
    println!("finished {:?}", input.unwrap());

    let mut child = AizelInference::run_llama_server(&PathBuf::from(model_path), 4).unwrap();
    child.wait();
}
