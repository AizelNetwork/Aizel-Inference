use super::aizel::gate_service_client::GateServiceClient;
use super::aizel::{inference_server::InferenceServer, NodeRegistrationRequest, StatusCode};
use super::{
    config::{NodeConfig, NODE_KEY_FILENAME},
    server::AizelInference,
};
use crate::{
    crypto::secret::{Export, Secret},
    tee::attestation::AttestationAgent,
};
use common::error::Error;
use log::{error, info};
use std::path::PathBuf;
use tonic::transport::Server;
pub struct Node {
    pub config: NodeConfig,
    pub secret: Secret,
    pub agent: AttestationAgent,
}

impl Node {
    pub async fn new(config: NodeConfig) -> Result<Node, Error> {
        let secret_path = config.root_path.join(NODE_KEY_FILENAME);
        let secret = open_or_create_secret(secret_path)?;
        Ok(Node {
            config: config.clone(),
            secret,
            agent: AttestationAgent::new()?,
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
        let report: String = self.agent.get_attestation_report()?;
        let request = tonic::Request::new(NodeRegistrationRequest {
            ip: self.config.socket_address.ip().to_string(),
            pub_key: self.secret.name.to_string(),
            tee_type: self.agent.get_tee_type()?,
            report: report,
        });
        let res = client
            .node_registration(request)
            .await
            .map_err(|e| Error::NetworkError {
                address: gate_address,
                message: e.to_string(),
            })?
            .into_inner();
        if res.code != StatusCode::Success as i32 {
            error!("failed to register, reason {}", res.msg);
            return Err(Error::RegistrationError { message: res.msg });
        }
        Ok(())
    }

    pub async fn run_server(&self) -> Result<(), Error> {
        self.register().await?;
        let mut listen_addr = self.config.socket_address.clone();
        listen_addr.set_ip("0.0.0.0".parse().unwrap());
        Server::builder()
            .add_service(InferenceServer::new(AizelInference {
                config: self.config.clone(),
            }))
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
        info!("{:?}", path);
        Secret::read(path.to_str().unwrap())
    } else {
        let secret = Secret::new();
        secret.write(path.to_str().unwrap())?;
        Ok(secret)
    }
}
