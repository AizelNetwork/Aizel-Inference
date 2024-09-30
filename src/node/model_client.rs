use super::config::{COIN_ADDRESS_MAPPING, AIZEL_MODEL_SERVICE};
use crate::chains::contract::Contract;
use common::error::Error;
use ethers::core::utils::{parse_units, ParseUnits};
use ethers::types::U256;
use log::{error, info};
use openai_api_rs::v1::api::OpenAIClient;
use openai_api_rs::v1::chat_completion::{self, ChatCompletionRequest};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tonic::async_trait;

#[async_trait]
pub trait ModelClient: Send + Sync {
    async fn request(input: String) -> Result<String, Error>;
}
pub struct ChatClient {}

impl ChatClient {
    pub async fn request(input: String) -> Result<String, Error> {
        std::env::set_var("OPENAI_API_BASE", "http://localhost:8888/v1");
        let client = OpenAIClient::new(String::new());

        let req = ChatCompletionRequest::new(
            String::new(),
            vec![chat_completion::ChatCompletionMessage {
                role: chat_completion::MessageRole::user,
                content: chat_completion::Content::Text(input),
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
            Some(c) => Ok(c.clone()),
            None => {
                return Err(Error::InferenceError {
                    message: "response is empty from local llama server".to_string(),
                });
            }
        }
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub struct TransferInfo {
    pub to: String,
    pub token: String,
    pub amount: f64,
}

pub struct TransferAgentClient {}

impl TransferAgentClient {
    pub async fn transfer(request_id: u64, input: String, from: String) -> Result<String, Error> {
        let transfer_info = TransferAgentClient::request(input).await?;
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
        info!("transfer agent output {}", output);
        Contract::transfer(
            request_id,
            token_address.unwrap().clone(),
            from,
            transfer_info.to,
            amount,
        )
        .await?;
        Ok(output)
    }

    pub async fn request(input: String) -> Result<TransferInfo, Error> {
        std::env::set_var("OPENAI_API_BASE", "http://localhost:8888/v1");
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
                content: chat_completion::Content::Text(input),
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
}

pub struct MlEnergeClient {}

impl MlEnergeClient {
    pub async fn request(input: String) -> Result<String, Error> {
        let client = reqwest::Client::new();
        let res = client.post(format!("{}/{}", AIZEL_MODEL_SERVICE, "peaq/predict"))
            .header("Content-Type", "application/json")
            .body(input)
            .send()
        .await.map_err(|e| {
            Error::InferenceError { message: format!("failed to request backend server {}", e.to_string()) }
        })?;
        let output = res.text().await.map_err(|e| {
            Error::InferenceError { message: format!("failed to get result {}", e.to_string()) }
        })?;
        return Ok(output)
    }
}

#[tokio::test]
async fn request_ml_model() {
    let output = MlEnergeClient::request("{ \"data\": [ { \"cet_cest_timestamp\": \"2024-08-26 10:29:00\", \"grid_import\": 0.0, \"pv\": 0.1049999999995634 }, { \"cet_cest_timestamp\": \"2024-08-26 10:31:00\", \"grid_import\": 0.0, \"pv\": 0.100999999999658 }, { \"cet_cest_timestamp\": \"2024-08-26 10:33:00\", \"grid_import\": 0.0, \"pv\": 0.1000000000003638 }, { \"cet_cest_timestamp\": \"2024-08-26 10:34:00\", \"grid_import\": 0.0, \"pv\": 0.0999999999994543 }, { \"cet_cest_timestamp\": \"2024-08-26 10:36:00\", \"grid_import\": 0.0, \"pv\": 0.100999999999658 }, { \"cet_cest_timestamp\": \"2024-08-26 10:39:00\", \"grid_import\": 0.0, \"pv\": 0.1039999999993597 }, { \"cet_cest_timestamp\": \"2024-08-26 10:41:00\", \"grid_import\": 0.0, \"pv\": 0.100999999999658 }, { \"cet_cest_timestamp\": \"2024-08-26 10:42:00\", \"grid_import\": 0.0, \"pv\": 0.1050000000004729 }, { \"cet_cest_timestamp\": \"2024-08-26 10:44:00\", \"grid_import\": 0.0, \"pv\": 0.09900000000016 }, { \"cet_cest_timestamp\": \"2024-08-26 10:46:00\", \"grid_import\": 0.0, \"pv\": 0.1050000000004729 }, { \"cet_cest_timestamp\": \"2024-08-26 10:49:00\", \"grid_import\": 0.0, \"pv\": 0.1059999999997671 }, { \"cet_cest_timestamp\": \"2024-08-26 10:51:00\", \"grid_import\": 0.0, \"pv\": 0.1049999999995634 }, { \"cet_cest_timestamp\": \"2024-08-26 10:51:00\", \"grid_import\": 0.0, \"pv\": 0.1049999999995634 }, { \"cet_cest_timestamp\": \"2024-08-26 10:52:00\", \"grid_import\": 0.0, \"pv\": 0.1040000000002692 }, { \"cet_cest_timestamp\": \"2024-08-26 10:54:00\", \"grid_import\": 0.0, \"pv\": 0.1050000000004729 }, { \"cet_cest_timestamp\": \"2024-08-26 10:56:00\", \"grid_import\": 0.0, \"pv\": 0.1040000000002692 }, { \"cet_cest_timestamp\": \"2024-08-26 10:58:00\", \"grid_import\": 0.0, \"pv\": 0.1050000000004729 }, { \"cet_cest_timestamp\": \"2024-08-26 11:00:00\", \"grid_import\": 0.0, \"pv\": 0.1059999999997671 }, { \"cet_cest_timestamp\": \"2024-08-26 11:02:00\", \"grid_import\": 0.0, \"pv\": 0.1049999999995634 }, { \"cet_cest_timestamp\": \"2024-08-26 11:04:00\", \"grid_import\": 0.0, \"pv\": 0.1109999999998763 }, { \"cet_cest_timestamp\": \"2024-08-26 11:05:00\", \"grid_import\": 0.0, \"pv\": 0.1050000000004729 }, { \"cet_cest_timestamp\": \"2024-08-26 11:08:00\", \"grid_import\": 0.0, \"pv\": 0.1049999999995634 }, { \"cet_cest_timestamp\": \"2024-08-26 11:10:00\", \"grid_import\": 0.0, \"pv\": 0.1089999999994688 }, { \"cet_cest_timestamp\": \"2024-08-26 11:12:00\", \"grid_import\": 0.0, \"pv\": 0.1050000000004729 }, { \"cet_cest_timestamp\": \"2024-08-26 11:13:00\", \"grid_import\": 0.0, \"pv\": 0.1049999999995634 }, { \"cet_cest_timestamp\": \"2024-08-26 11:15:00\", \"grid_import\": 0.0, \"pv\": 0.1099999999996725 }, { \"cet_cest_timestamp\": \"2024-08-26 11:18:00\", \"grid_import\": 0.0, \"pv\": 0.1109999999998763 }, { \"cet_cest_timestamp\": \"2024-08-26 11:20:00\", \"grid_import\": 0.0, \"pv\": 0.1089999999994688 }, { \"cet_cest_timestamp\": \"2024-08-26 11:21:00\", \"grid_import\": 0.0, \"pv\": 0.1059999999997671 }, { \"cet_cest_timestamp\": \"2024-08-26 11:23:00\", \"grid_import\": 0.0, \"pv\": 0.1099999999996725 }, { \"cet_cest_timestamp\": \"2024-08-26 11:25:00\", \"grid_import\": 0.0, \"pv\": 0.1099999999996725 }, { \"cet_cest_timestamp\": \"2024-08-26 11:27:00\", \"grid_import\": 0.0, \"pv\": 0.1090000000003783 }, { \"cet_cest_timestamp\": \"2024-08-26 11:28:00\", \"grid_import\": 0.0, \"pv\": 0.1109999999998763 }, { \"cet_cest_timestamp\": \"2024-08-26 11:30:00\", \"grid_import\": 0.0, \"pv\": 0.1040000000002692 }, { \"cet_cest_timestamp\": \"2024-08-26 11:33:00\", \"grid_import\": 0.0, \"pv\": 0.1099999999996725 }, { \"cet_cest_timestamp\": \"2024-08-26 11:35:00\", \"grid_import\": 0.0, \"pv\": 0.1290000000008149 }, { \"cet_cest_timestamp\": \"2024-08-26 11:37:00\", \"grid_import\": 0.0, \"pv\": 0.1290000000008149 }, { \"cet_cest_timestamp\": \"2024-08-26 11:39:00\", \"grid_import\": 0.0, \"pv\": 0.1099999999996725 }, { \"cet_cest_timestamp\": \"2024-08-26 11:40:00\", \"grid_import\": 0.0, \"pv\": 0.1090000000003783 }, { \"cet_cest_timestamp\": \"2024-08-26 11:42:00\", \"grid_import\": 0.0, \"pv\": 0.1159999999999854 }, { \"cet_cest_timestamp\": \"2024-08-26 11:44:00\", \"grid_import\": 0.0, \"pv\": 0.1059999999997671 }, { \"cet_cest_timestamp\": \"2024-08-26 11:45:00\", \"grid_import\": 0.0, \"pv\": 0.1099999999996725 }, { \"cet_cest_timestamp\": \"2024-08-26 11:47:00\", \"grid_import\": 0.0, \"pv\": 0.1109999999998763 }, { \"cet_cest_timestamp\": \"2024-08-26 11:49:00\", \"grid_import\": 0.0, \"pv\": 0.1089999999994688 }, { \"cet_cest_timestamp\": \"2024-08-26 11:51:00\", \"grid_import\": 0.0, \"pv\": 0.1149999999997817 }, { \"cet_cest_timestamp\": \"2024-08-26 11:51:00\", \"grid_import\": 0.0, \"pv\": 0.1149999999997817 }, { \"cet_cest_timestamp\": \"2024-08-26 11:51:00\", \"grid_import\": 0.0, \"pv\": 0.1149999999997817 }, { \"cet_cest_timestamp\": \"2024-08-26 11:51:00\", \"grid_import\": 0.0, \"pv\": 0.1149999999997817 }, { \"cet_cest_timestamp\": \"2024-08-26 11:51:00\", \"grid_import\": 0.0, \"pv\": 0.1149999999997817 }, { \"cet_cest_timestamp\": \"2024-08-26 11:51:00\", \"grid_import\": 0.0, \"pv\": 0.1149999999997817 }, { \"cet_cest_timestamp\": \"2024-08-26 11:52:00\", \"grid_import\": 0.0, \"pv\": 0.1090000000003783 }, { \"cet_cest_timestamp\": \"2024-08-26 11:54:00\", \"grid_import\": 0.0, \"pv\": 0.1099999999996725 }, { \"cet_cest_timestamp\": \"2024-08-26 11:56:00\", \"grid_import\": 0.0, \"pv\": 0.1109999999998763 }, { \"cet_cest_timestamp\": \"2024-08-26 11:57:00\", \"grid_import\": 0.0, \"pv\": 0.110000000000582 }, { \"cet_cest_timestamp\": \"2024-08-26 11:59:00\", \"grid_import\": 0.0, \"pv\": 0.1149999999997817 }, { \"cet_cest_timestamp\": \"2024-08-26 12:01:00\", \"grid_import\": 0.0, \"pv\": 0.1090000000003783 }, { \"cet_cest_timestamp\": \"2024-08-26 12:03:00\", \"grid_import\": 0.0, \"pv\": 0.1099999999996725 }, { \"cet_cest_timestamp\": \"2024-08-26 12:04:00\", \"grid_import\": 0.0, \"pv\": 0.1090000000003783 }, { \"cet_cest_timestamp\": \"2024-08-26 12:06:00\", \"grid_import\": 0.0, \"pv\": 0.1149999999997817 }, { \"cet_cest_timestamp\": \"2024-08-26 12:08:00\", \"grid_import\": 0.0, \"pv\": 0.1099999999996725 } ] }".to_string()).await.unwrap();
    println!("{}", output);
}
