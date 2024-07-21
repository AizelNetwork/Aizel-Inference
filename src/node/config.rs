use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::env;
use std::fs;
use lazy_static::lazy_static;
use common::error::Error;

pub const DEFAULT_BASE_PORT: u16 = 8080;
pub const NODE_KEY_FILENAME: &str = "node_key.json";

pub const DEFAULT_ROOT_DIR: &str = "aizel";
pub const DEFAULT_MODEL_DIR: &str = "models";
pub const DEFAULT_MODEL: &str = "llama2_7b_chat.Q4_0.gguf-1.0";
pub const WALLET_SK_FILE: &str = "wallet-sk";
pub const ALICLOUD_CONFIG: &str = "config";
/// socket_address: self server listen socket address
/// root_path:
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct NodeConfig {
    /// self socket address
    pub socket_address: SocketAddr,
    pub root_path: PathBuf,
    pub gate_address: SocketAddr,
    pub data_address: SocketAddr,
}

#[derive(Deserialize)]
struct Config {
    #[serde(rename = "CHAIN_ID")]
    chain_id: String,
    #[serde(rename = "ENDPOINT")]
    endpoint: String,
    #[serde(rename = "CONTRACT_ADDRESS")]
    contraact_address: String,
    #[serde(rename = "DATA_ADDRESS")]
    data_address: String,
    #[serde(rename = "GATE_ADDRESS")]
    gate_address: String,
}

lazy_static! {
    pub static ref DATA_ADDRESS: SocketAddr = env::var("DATA_ADDRESS").unwrap().parse().unwrap();
    pub static ref GATE_ADDRESS: SocketAddr = env::var("GATE_ADDRESS").unwrap().parse().unwrap();

}

pub async fn prepare_config() -> Result<(), Error>{
    // If on AliCloud, export configs to environment variable
    let client = reqwest::Client::new();
    let response = client.get("http://100.100.100.200/latest/meta-data").send().await.map_err(|e| Error::UnkownTEETypeERROR {
        message: e.to_string(),
    })?;
    if response.status().is_success() {
        let config = fs::read_to_string(
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(ALICLOUD_CONFIG),
        )
        .map_err(|e| Error::FileError {
            path: ALICLOUD_CONFIG.into(),
            message: e.to_string(),
        }).unwrap();
        let config: Config = serde_json::from_str(&config).unwrap();
        env::set_var("CHAIN_ID", &config.chain_id);
        env::set_var("ENDPOINT", &config.endpoint);
        env::set_var("CONTRACT_ADDRESS", &config.contraact_address);
        env::set_var("DATA_ADDRESS", &config.data_address);
        env::set_var("GATE_ADDRESS", &config.gate_address);
    }
    return Ok(());
}
