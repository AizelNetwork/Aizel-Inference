use super::aizel::inference_server::InferenceServer;
use super::{
    aizel_server::AizelInference,
    config::{models_dir, node_key_path, root_dir, AIZEL_CONFIG, initialize_network_configs},
};
use crate::chains::contract::{Contract, NONCE_MANAGERS};
use crate::node::config::{logs_dir, NETWORK_CONFIGS};
use crate::{
    crypto::secret::{Export, Secret},
    tee::attestation::AttestationAgent,
};
use futures::future::join_all;
use common::error::Error;
use log::{error, info};
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use tonic::transport::Server;

pub struct Node {
    pub address: SocketAddr,
    pub secret: Secret,
    pub agent: AttestationAgent,
}

impl Node {
    pub async fn new(address: SocketAddr) -> Result<Node, Error> {
        NETWORK_CONFIGS.set(initialize_network_configs().await?).unwrap();
        assert_eq!(AIZEL_CONFIG.data_nodes.len(), AIZEL_CONFIG.networks.len());
        fs::create_dir_all(root_dir()).unwrap();
        AIZEL_CONFIG.networks.iter().for_each(|network| {
            fs::create_dir_all(models_dir(network)).unwrap();
            fs::create_dir_all(logs_dir(network)).unwrap();
        });

        let secret = match &AIZEL_CONFIG.node_secret {
            Some(s) => {
                if s.len() != 64 || s.len() != 66 {
                    error!("the secret length is not 64 or 66, please input correct node secret in your aizel_config.yml file");
                    return Err(Error::InvalidArgumentError { argument: "node secret".to_string(), message: "the secret length is not 64 or 66, please input correct node secret in your aizel_config.yml file".to_string() });
                }
                if s.len() == 66 {
                    Secret::from_str(&s[2..])
                } else {
                    Secret::from_str(s)
                }
            }
            None => open_or_create_secret(node_key_path())?,
        };
        Ok(Node {
            address,
            secret,
            agent: AttestationAgent::new().await?,
        })
    }

    pub async fn register(&self) -> Result<(), Error> {
        let f: Vec<_> = NONCE_MANAGERS.iter().map(|(_, n)| async move{
            n.initialize_nonce(None).await
        }).collect();
        let _ = join_all(f);
        let tee_type = self.agent.get_tee_type().unwrap();
        if AIZEL_CONFIG.within_tee {
            info!(
                "attestation report {}",
                self.agent
                    .get_attestation_report(self.secret.name.encode())
                    .await?
            );
        }

        let address = format!("http://{}", self.address.to_string());
        for (network_id, network) in AIZEL_CONFIG.networks.iter().enumerate() {
            if !Contract::query_public_key_exist(self.secret.name.encode(), network).await? {
                Contract::register(
                    AIZEL_CONFIG.node_name.clone(),
                    AIZEL_CONFIG.node_bio.clone(),
                    address.clone(),
                    self.secret.name.encode(),
                    AIZEL_CONFIG.data_nodes[network_id],
                    tee_type as u32,
                    AIZEL_CONFIG.initial_stake,
                    network
                )
                .await?;
                info!("successfully registered on network {}", network);
            } else {
                info!("already registerd")
            }
        }
        Ok(())
    }

    pub async fn run_server(&self) -> Result<(), Error> {
        std::env::set_var("OPENAI_API_BASE", "http://localhost:8888/v1");
        let aizel_inference_service =
            AizelInference::new(self.secret.clone()).await;
        self.register().await?;

        let mut listen_addr = self.address.clone();
        listen_addr.set_ip("0.0.0.0".parse().unwrap());
        Server::builder()
            .add_service(InferenceServer::new(aizel_inference_service))
            .serve(listen_addr)
            .await
            .map_err(|e| Error::ServerError {
                message: format!("failed to listen: {}", e.to_string()),
            })?;
        Ok(())
    }
}

pub fn open_or_create_secret(path: PathBuf) -> Result<Secret, Error> {
    if path.exists() {
        info!("secret already exist {:?}", path);
        Secret::read(path.to_str().unwrap())
    } else {
        let secret = Secret::new();
        secret.write(path.to_str().unwrap())?;
        Ok(secret)
    }
}
