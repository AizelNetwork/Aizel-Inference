use super::config::COIN_ADDRESS_MAPPING;
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
    pub amount: u64,
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
