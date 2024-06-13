use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;

pub const DEFAULT_BASE_PORT: u16 = 8080;
pub const NODE_KEY_FILENAME: &str = "node_key.json";

pub const DEFAULT_ROOT_DIR: &str = "aizel";
pub const DEFAULT_MODEL_DIR: &str = "models";
pub const DEFAULT_MODEL: &str = "llama2_7b_chat.Q4_0.gguf-1.0";
/// socket_address: self server listen socket address
/// root_path:
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct NodeConfig {
    /// self socket address
    pub socket_address: SocketAddr,
    pub root_path: PathBuf,
    pub gate_address: SocketAddr,
    pub data_address: SocketAddr,
    pub contract_address: String,
}
