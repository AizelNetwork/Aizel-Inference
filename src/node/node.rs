use super::aizel::gate_service_client::GateServiceClient;
use super::aizel::{inference_server::InferenceServer, NodeRegistrationRequest};
use super::{
    config::{
        NodeConfig, DEFAULT_MODEL, DEFAULT_MODEL_DIR, NODE_KEY_FILENAME,
    },
    server::AizelInference,
};
use crate::{
    crypto::secret::{Export, Secret},
    tee::attestation::AttestationAgent,
};
use common::error::Error;
use log::{error, info};
use std::fs;
use std::path::PathBuf;
use tonic::transport::Server;
pub struct Node {
    pub config: NodeConfig,
    pub secret: Secret,
    pub agent: AttestationAgent,
}

impl Node {
    pub async fn new(config: NodeConfig) -> Result<Node, Error> {
        fs::create_dir_all(&config.root_path).map_err(|e| Error::FileError {
            path: config.root_path.clone(),
            message: e.to_string(),
        })?;
        fs::create_dir_all(&config.root_path.join(DEFAULT_MODEL_DIR)).map_err(|e| {
            Error::FileError {
                path: config.root_path.clone(),
                message: e.to_string(),
            }
        })?;
        let secret_path = config.root_path.clone().join(NODE_KEY_FILENAME);
        let secret = open_or_create_secret(secret_path)?;

        Ok(Node {
            config: config,
            secret,
            agent: AttestationAgent::new().await?,
        })
    }

    pub async fn register(&self) -> Result<(), Error> {
        let gate_address = self.config.gate_address.to_string();
        let url = format!("http://{}", gate_address);
        let mut client =
            GateServiceClient::connect(url)
                .await
                .map_err(|e| Error::NetworkError {
                    address: gate_address.clone(),
                    message: e.to_string(),
                })?;
        let report: String = self
            .agent
            .get_attestation_report(hex::encode(self.secret.name.0))
            .await?;
        let request = tonic::Request::new(NodeRegistrationRequest {
            ip: format!("http://{}", self.config.socket_address.to_string()),
            pub_key: hex::encode(self.secret.name.0),
            tee_type: self.agent.get_tee_type()?,
            report: report,
        });
        let _ = client
            .node_registration(request)
            .await
            .map_err(|e| {
                error!("failed to register node: {:#?}", e.message().to_string());
                Error::RegistrationError {
                    message: e.message().to_string(),
                }
            })?
            .into_inner();
        info!("successfully registered");
        Ok(())
    }

    pub async fn run_server(&self) -> Result<(), Error> {
        let aizel_inference_service = AizelInference {
            config: self.config.clone(),
            secret: self.secret.clone(),
        };
        if !aizel_inference_service
            .check_model_exist(DEFAULT_MODEL.to_string())
            .await?
        {
            aizel_inference_service
                .download_model(DEFAULT_MODEL.to_string())
                .await?;
        }
        // self.register().await?;
        let mut listen_addr = self.config.socket_address.clone();
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
