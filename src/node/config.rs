use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

pub const DEFAULT_BASE_PORT: u16 = 8080;
pub const NODE_KEY_FILENAME: &str = "node_key.json";
/// socket_address: self server listen socket address
/// root_path:
#[derive(Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub socket_address: SocketAddr,
    pub root_path: PathBuf,
    pub gate_address: SocketAddr,
}
