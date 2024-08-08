use crate::chains::contract::DATA_REGISTRY_CONTRACT;
use common::error::Error;
use lazy_static::lazy_static;
use log::info;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
pub const DEFAULT_BASE_PORT: u16 = 8080;
pub const NODE_KEY_FILENAME: &str = "node_key.json";

pub const DEFAULT_ROOT_DIR: &str = "aizel";
pub const DEFAULT_MODEL_DIR: &str = "models";
pub const DEFAULT_MODEL: &str = "llama2_7b_chat.Q4_0.gguf-1.0";
pub const WALLET_SK_FILE: &str = "wallet-sk";
pub const MINIO_USER_FILE: &str = "minio-user";
pub const MINIO_PWD_FILE: &str = "minio-pwd";
pub const ALICLOUD_CONFIG: &str = "config";

pub const INPUT_BUCKET: &str = "users-input";
pub const MODEL_BUCKET: &str = "models";
pub const OUTPUT_BUCKET: &str = "users-output";
pub const REPORT_BUCKET: &str = "inference-report";

/// socket_address: self server listen socket address
/// root_path:
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct NodeConfig {
    /// self socket address
    pub socket_address: SocketAddr,
    pub root_path: PathBuf,
    pub data_id: u64,
}

#[derive(Deserialize)]
struct Config {
    #[serde(rename = "CHAIN_ID")]
    chain_id: String,
    #[serde(rename = "ENDPOINT")]
    endpoint: String,
    #[serde(rename = "INFERENCE_CONTRACT")]
    inference_contract: String,
    #[serde(rename = "INFERENCE_REGISTRY_CONTRACT")]
    inference_registry_contract: String,
    #[serde(rename = "DATA_REGISTRY_CONTRACT")]
    data_registry_contract: String,
}

lazy_static! {
    pub static ref DATA_ID: u64 = env::var("DATA_ID").unwrap().parse().unwrap();
    pub static ref INITAIL_STAKE_AMOUNT: u64 =
        env::var("INITAIL_STAKE_AMOUNT").unwrap().parse().unwrap();
}

pub async fn prepare_config() -> Result<u64, Error> {
    // If on AliCloud, export configs to environment variable
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .map_err(|e| Error::UnkownTEETypeERROR {
            message: e.to_string(),
        })?;
    let response = client
        .get("http://100.100.100.200/latest/meta-data")
        .send()
        .await;

    match response {
        Ok(resp) => {
            if resp.status().is_success() {
                let config = fs::read_to_string(
                    dirs::home_dir()
                        .unwrap_or_else(|| PathBuf::from("."))
                        .join(ALICLOUD_CONFIG),
                )
                .map_err(|e| Error::FileError {
                    path: ALICLOUD_CONFIG.into(),
                    message: e.to_string(),
                })
                .unwrap();
                let config: Config = serde_json::from_str(&config).unwrap();
                env::set_var("CHAIN_ID", &config.chain_id);
                env::set_var("ENDPOINT", &config.endpoint);
                env::set_var("INFERENCE_CONTRACT", &config.inference_contract);
                env::set_var(
                    "INFERENCE_REGISTRY_CONTRACT",
                    &config.inference_registry_contract,
                );
                env::set_var("DATA_REGISTRY_CONTRACT", &config.data_registry_contract);
            }
        }
        Err(_) => {
            info!("not on alicloud");
        }
    }
    // according to the data node id, query the url of the data node
    let data_id: u64 = env::var("DATA_NODE_ID").unwrap().parse().unwrap();
    let data_node_url: String = DATA_REGISTRY_CONTRACT
        .get_url(data_id.into())
        .call()
        .await
        .unwrap();
    info!("data node url {}", data_node_url);
    env::set_var("DATA_ADDRESS", &data_node_url);
    return Ok(data_id);
}
