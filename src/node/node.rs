use super::config::{NodeConfig, NODE_KEY_FILENAME};
use crate::crypto::secret::{Export, Secret};
use common::error::Error;
use log::info;
use std::path::{Path, PathBuf};
pub struct Node {
    pub config: NodeConfig,
    pub secret: Secret,
}

impl Node {
    pub async fn new(config: NodeConfig) -> Result<Node, Error> {
        let secret_path = config.root_path.join(NODE_KEY_FILENAME);
        let secret = open_or_create_secret(secret_path)?;
        Ok(Node { config, secret })
    }

    // pub async fn
}

pub fn open_or_create_secret(path: PathBuf) -> Result<Secret, Error> {
    if path.exists() {
        info!("{:?}", path);
        Secret::read(path.to_str().unwrap())
    } else {
        let secret = Secret::new();
        secret.write(path.to_str().unwrap())?;
        Ok(secret)
    }
}
