use super::aizel::inference_server::InferenceServer;
use super::{
    config::{models_dir, node_key_path, root_dir, AIZEL_CONFIG, DEFAULT_MODEL, MODEL_BUCKET},
    server::AizelInference,
};
use crate::chains::contract::Contract;
use crate::s3_minio::client::MinioClient;
use crate::{
    crypto::secret::{Export, Secret},
    tee::attestation::AttestationAgent,
};
use common::error::Error;
use log::{info, error};
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
        fs::create_dir_all(root_dir()).unwrap();
        fs::create_dir_all(models_dir()).unwrap();
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
        if !Contract::query_public_key_exist(self.secret.name.encode()).await? {
            Contract::register(
                AIZEL_CONFIG.node_name.clone(),
                AIZEL_CONFIG.node_bio.clone(),
                address,
                self.secret.name.encode(),
                AIZEL_CONFIG.data_node_id,
                tee_type as u32,
                AIZEL_CONFIG.initial_stake,
            )
            .await?;
        }

        info!("successfully registered");
        Ok(())
    }

    pub async fn run_server(&self) -> Result<(), Error> {
        std::env::set_var("OPENAI_API_BASE", "http://localhost:8888/v1");
        if !AizelInference::check_model_exist(&models_dir(), DEFAULT_MODEL).await? {
            let client = MinioClient::get().await;
            client
                .download_model(
                    MODEL_BUCKET,
                    DEFAULT_MODEL,
                    &models_dir().join(DEFAULT_MODEL),
                )
                .await?;
        }
        let aizel_inference_service = AizelInference::new(self.secret.clone());
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
