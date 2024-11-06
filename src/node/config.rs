use common::error::Error;
use ethers::types::H160;
use lazy_static::lazy_static;
use serde::de::DeserializeOwned;
use serde_derive::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::sync::OnceCell;

pub const DEFAULT_BASE_PORT: u16 = 8080;

pub const NODE_KEY_FILENAME: &str = "node_key.json";
pub const DEFAULT_ROOT_DIR: &str = "aizel";
pub const DEFAULT_MODEL_DIR: &str = "models";
pub const DEFAULT_LOG_DIR: &str = "logs";
pub const DEFAULT_AIZEL_CONFIG: &str = "aizel_config.yml";
pub const DEFAULT_NETWORK_CONFIG: &str = "config.json";
pub const ML_DIR: &str = "aizel-face-recognition";
pub const ML_MODEL_DIR: &str = "conf";
pub const ML_MODEL_CONFIG: &str = "models.json";
pub const DEFAULT_MODEL: &str = "llama2_7b_chat.Q4_0.gguf-1.0";

pub const INPUT_BUCKET: &str = "users-input";
pub const MODEL_BUCKET: &str = "models";
pub const OUTPUT_BUCKET: &str = "users-output";
pub const REPORT_BUCKET: &str = "inference-report";

pub const DEFAULT_CHANNEL_SIZE: usize = 1_000;

pub const LLAMA_SERVER_PORT: u16 = 8888;
pub const ML_SERVER_PORT: u16 = 9888;

pub const TRANSFER_AGENT_ID: u64 = 2;   
lazy_static! {
    pub static ref COIN_ADDRESS_MAPPING: HashMap<String, HashMap<String, String>> = {
        let mut res = HashMap::new();
        {
            let mut coin_address_mapping = HashMap::new();
            coin_address_mapping.insert(
                "USDT".to_string(),
                "0x411A42fE3F187b778e8D2dAE41E062D3F417929a".to_string(),
            );
            res.insert("aizel".to_string(), coin_address_mapping);
        }
        {
            let mut coin_address_mapping = HashMap::new();
            coin_address_mapping.insert(
                "USDT".to_string(),
                "0xd312378bceFc9C05CB92bb019fbbFB9D737BE521".to_string(),
            );
            res.insert("avax".to_string(), coin_address_mapping);
        }
        {
            let mut coin_address_mapping = HashMap::new();
            coin_address_mapping.insert(
                "USDT".to_string(),
                "0x792DA88707f8802e2575E7913D2a87601C55E717".to_string(),
            );
            res.insert("reddio".to_string(), coin_address_mapping);
        }

        res
    };
}

#[derive(Deserialize, Debug)]
pub struct AizelConfig {
    // model data node configuration
    pub minio_account: String,
    pub minio_password: String,
    pub data_nodes: Vec<u64>,
    pub networks: Vec<String>,
    // public data node url 
    pub public_data_node_url: String,
    // gate url
    pub gate_url: String,
    // config server url 
    pub config_server_url: String,
    // contract configuration
    pub wallet_sk: String,
    // inference node configuration
    pub node_name: String,
    pub node_bio: String,
    pub initial_stake: u64,
    pub within_tee: bool,
    pub node_secret: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct NetworkConfig {
    pub network_id: u64,
    #[serde(rename = "network_name")]
    pub network: String, 
    #[serde(rename = "evm_chain_id")]
    pub chain_id: u64,
    pub rpc_url: String,
    pub contracts: Vec<ContractConfig>
}

#[derive(Deserialize, Debug)]
pub struct ContractConfig {
    #[serde(rename = "smart_contract_name")]
    pub name: String,
    #[serde(rename = "smart_contract_address")]
    pub address: H160
}

pub trait FromFile<T: DeserializeOwned> {
    fn from_file<P: AsRef<Path>>(path: P) -> Result<T, String> {
        let content: String =
            std::fs::read_to_string(path).map_err(|e| format!("can't read file: {:?}", e))?;
        serde_yaml::from_str(&content).map_err(|e| format!("can't deserialize file {:?}", e))
    }
}

impl FromFile<AizelConfig> for AizelConfig {}

pub static NETWORK_CONFIGS: OnceCell<Vec<NetworkConfig>> = OnceCell::const_new();

lazy_static! {
    pub static ref AIZEL_CONFIG: AizelConfig = prepare_config().unwrap();
}

pub fn root_dir() -> PathBuf {
    dirs::home_dir().unwrap().join(DEFAULT_ROOT_DIR)
}

pub fn config_path() -> PathBuf {
    root_dir().join(DEFAULT_AIZEL_CONFIG)
}

pub fn network_config_path() -> PathBuf {
    root_dir().join(DEFAULT_NETWORK_CONFIG)
}

pub fn models_dir(network: &str) -> PathBuf {
    root_dir().join(DEFAULT_MODEL_DIR).join(network)
}

pub fn logs_dir(network: &str) -> PathBuf {
    root_dir().join(DEFAULT_LOG_DIR).join(network)
}

pub fn node_key_path() -> PathBuf {
    root_dir().join(NODE_KEY_FILENAME)
}

pub fn source_ml_models_dir() -> PathBuf {
    root_dir().join(ML_DIR).join(ML_MODEL_DIR)
}

pub fn ml_dir(network: &str) -> PathBuf {
    root_dir().join("ml_models").join(network)
}

pub fn ml_models_dir(network: &str) -> PathBuf {
    ml_dir(network).join(ML_MODEL_DIR)
}

pub fn ml_model_config(network: &str) -> PathBuf {
    ml_models_dir(network).join(ML_MODEL_CONFIG)
}

pub fn ml_model_config_with_id(network: &str, id: u64) -> PathBuf {
    ml_models_dir(network).join(format!("{}_{}", ML_MODEL_CONFIG, id))
}

pub fn ml_models_start_script() -> PathBuf {
    root_dir().join(ML_DIR).join("bin").join("start.sh")
}

pub fn prepare_config() -> Result<AizelConfig, Error> {
    Ok(
        AizelConfig::from_file(config_path()).map_err(|e| Error::SerDeError {
            message: format!("failed to parse config: {}", e.to_string()),
        })?,
    )
}

pub fn data_node_id(network: &str) -> Result<u64, Error> {
    let network_id = AIZEL_CONFIG.networks.iter().position(|x| {
        *x == network
    }).ok_or(Error::NetworkConfigNotFoundError { network: network.to_string() })?;
    Ok(AIZEL_CONFIG.data_nodes[network_id])
}

pub fn llama_server_port(network: &str) -> Result<u16, Error> {
    let network_id = AIZEL_CONFIG.networks.iter().position(|x| {
        x == network
    }).ok_or(Error::NetworkConfigNotFoundError { network: network.to_string() })? as u16;
    Ok(LLAMA_SERVER_PORT + network_id)
}

pub fn ml_server_port(network: &str) -> Result<u16, Error> {
    let network_id = AIZEL_CONFIG.networks.iter().position(|x| {
        x == network
    }).ok_or(Error::NetworkConfigNotFoundError { network: network.to_string() })? as u16;
    Ok(ML_SERVER_PORT + network_id)
}

pub async fn initialize_network_configs_by_network() -> Result<Vec<NetworkConfig>, Error> {
    let client = reqwest::Client::new();
    let res = client.get(format!("{}/{}", AIZEL_CONFIG.config_server_url, "api/v1/networks")).send().await.map_err(|e| {
        Error::InferenceError { message: format!("failed to request config server {}", e.to_string()) }
    })?;
    let output = res.text().await.map_err(|e| {
        Error::InferenceError { message: format!("failed to get config {}", e.to_string()) }
    })?;
    Ok(serde_json::from_str(&output).map_err(|_| {
        Error::InferenceError { message: "failed to parse network config".to_string() }
    })?)
}

pub fn initialize_network_configs_by_file() -> Result<Vec<NetworkConfig>, Error> {
    let config = fs::read_to_string(network_config_path()).map_err(|_| Error::FileError { path: network_config_path(), message: "failed to open config file".to_string() })?;
    Ok(serde_json::from_str(&config).unwrap())
}

pub async fn initialize_network_configs() -> Result<Vec<NetworkConfig>, Error> {
    match  initialize_network_configs_by_network().await {
        Ok(r) => Ok(r),
        Err(_) => initialize_network_configs_by_file()
    }
}

#[test]
fn test_aizel_config() {
    println!(
        "{:?}",
        AizelConfig::from_file("/home/jiangyi/aizel/aizel_config.yml").unwrap()
    );
}

#[tokio::test]
async fn test_query_config() {
    let networks: Vec<NetworkConfig> = initialize_network_configs().await.unwrap();
    println!("{:?}", networks);
}