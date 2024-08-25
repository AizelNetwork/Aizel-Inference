use common::error::Error;
use lazy_static::lazy_static;
use serde::de::DeserializeOwned;
use serde_derive::Deserialize;
use std::path::{Path, PathBuf};

pub const DEFAULT_BASE_PORT: u16 = 8080;

pub const NODE_KEY_FILENAME: &str = "node_key.json";
pub const DEFAULT_ROOT_DIR: &str = "aizel";
pub const DEFAULT_MODEL_DIR: &str = "models";
pub const DEFAULT_AIZEL_CONFIG: &str = "aizel_config.yml";

pub const DEFAULT_MODEL: &str = "llama2_7b_chat.Q4_0.gguf-1.0";

pub const INPUT_BUCKET: &str = "users-input";
pub const MODEL_BUCKET: &str = "models";
pub const OUTPUT_BUCKET: &str = "users-output";
pub const REPORT_BUCKET: &str = "inference-report";

pub const DEFAULT_CHANNEL_SIZE: usize = 1_000;

pub const LLAMA_SERVER_PORT: u16 = 8888;

pub const FACE_MODEL_SERVICE: &str = "http://localhost:9081/aizel/face/validate";

#[derive(Deserialize, Debug)]
pub struct AizelConfig {
    // contract configuration
    pub chain_id: u64,
    pub endpoint: String,
    pub inference_contract: String,
    pub inference_registry_contract: String,
    pub data_registry_contract: String,
    pub wallet_sk: String,
    // data node configuration
    pub minio_account: String,
    pub minio_password: String,
    pub data_node_id: u64,
    // inference node configuration
    pub node_name: String,
    pub node_bio: String,
    pub initial_stake: u64,
    pub within_tee: bool,
    pub node_secret: Option<String>,
}
pub trait FromFile<T: DeserializeOwned> {
    fn from_file<P: AsRef<Path>>(path: P) -> Result<T, String> {
        let content: String =
            std::fs::read_to_string(path).map_err(|e| format!("can't read file: {:?}", e))?;
        serde_yaml::from_str(&content).map_err(|e| format!("can't deserialize file {:?}", e))
    }
}

impl FromFile<AizelConfig> for AizelConfig {}

lazy_static! {
    pub static ref AIZEL_CONFIG: AizelConfig = prepare_config().unwrap();
}

pub fn root_dir() -> PathBuf {
    dirs::home_dir().unwrap().join(DEFAULT_ROOT_DIR)
}

pub fn config_path() -> PathBuf {
    root_dir().join(DEFAULT_AIZEL_CONFIG)
}

pub fn models_dir() -> PathBuf {
    root_dir().join(DEFAULT_MODEL_DIR)
}

pub fn node_key_path() -> PathBuf {
    root_dir().join(NODE_KEY_FILENAME)
}

pub fn prepare_config() -> Result<AizelConfig, Error> {
    Ok(
        AizelConfig::from_file(config_path()).map_err(|e| Error::SerDeError {
            message: format!("failed to parse config: {}", e.to_string()),
        })?,
    )
}

#[test]
fn test_aizel_config() {
    println!(
        "{:?}",
        AizelConfig::from_file("/home/jiangyi/aizel/aizel_config.yml").unwrap()
    );
}
