use super::aizel::inference_server::InferenceServer;
use super::{
    config::{
        NodeConfig, DEFAULT_MODEL, DEFAULT_MODEL_DIR, INITAIL_STAKE_AMOUNT, NODE_KEY_FILENAME,
    },
    server::AizelInference,
};
use crate::chains::contract::INFERENCE_REGISTRY_CONTRACT;
use crate::{
    crypto::secret::{Export, Secret},
    tee::attestation::AttestationAgent,
};
use common::error::Error;
use ethers::types::U256;
use log::{error, info};
use serde::Serialize;
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
        let min_stake: u64 = INFERENCE_REGISTRY_CONTRACT
            .get_min_stake()
            .call()
            .await
            .unwrap()
            .try_into()
            .unwrap();
        if *INITAIL_STAKE_AMOUNT < min_stake {
            return Err(Error::RegistrationError {
                message: format!(
                    "initial stake amount is smaller than minimal requirement {}",
                    min_stake
                ),
            });
        }
        let tx = INFERENCE_REGISTRY_CONTRACT.register_node(
            "test_name".to_string(),
            "test bio".to_string(),
            format!("http://{}", self.config.socket_address.to_string()),
            self.secret.name.encode(),
            self.config.data_id.into(),
            self.agent.get_tee_type().unwrap().try_into().unwrap(),
        );
        info!("{:?}", tx);
        let tx = tx.value(U256::from(*INITAIL_STAKE_AMOUNT));
        let pending_tx = tx.send().await.map_err(|e| Error::RegistrationError {
            message: e.to_string(),
        })?;
        info!("{:?}", pending_tx);
        // let gate_address = self.config.gate_address.to_string();
        // let url = format!("http://{}", gate_address);
        // let mut client =
        //     GateServiceClient::connect(url)
        //         .await
        //         .map_err(|e| Error::NetworkError {
        //             address: gate_address.clone(),
        //             message: e.to_string(),
        //         })?;
        // let report: String = self
        //     .agent
        //     .get_attestation_report(hex::encode(self.secret.name.0))
        //     .await?;
        // let request = tonic::Request::new(NodeRegistrationRequest {
        //     ip: format!("http://{}", self.config.socket_address.to_string()),
        //     pub_key: hex::encode(self.secret.name.0),
        //     tee_type: self.agent.get_tee_type()?,
        //     report: report,
        // });
        // let _ = client
        //     .node_registration(request)
        //     .await
        //     .map_err(|e| {
        //         error!("failed to register node: {:#?}", e.message().to_string());
        //         Error::RegistrationError {
        //             message: e.message().to_string(),
        //         }
        //     })?
        //     .into_inner();
        info!("successfully registered");
        Ok(())
    }

    pub async fn run_server(&self) -> Result<(), Error> {
        let aizel_inference_service = AizelInference {
            config: self.config.clone(),
            secret: self.secret.clone(),
        };
        // if !aizel_inference_service
        //     .check_model_exist(DEFAULT_MODEL.to_string())
        //     .await?
        // {
        //     aizel_inference_service
        //         .download_model(DEFAULT_MODEL.to_string())
        //         .await?;
        // }
        self.register().await?;
        // info!(
        //     "attestation report {}",
        //     self.agent
        //         .get_attestation_report("hello world".to_string())
        //         .await?
        // );
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
