use super::config::{models_dir, root_dir, LLAMA_SERVER_PORT, MODEL_BUCKET, TRANSFER_AGENT_ID};
use crate::chains::contract::ModelInfo;
use crate::s3_minio::client::MinioClient;
use common::error::Error;
use log::{error, info};
use std::fs;
use std::process::{Child, Command, Stdio};
use tonic::async_trait;

#[async_trait]
pub trait ModelServer: Send + Sync {
    async fn run_server(model_info: &ModelInfo) -> Result<(), Error>;
}

pub struct LlamaServer {
    pub child: Child,
    pub current_model: u64,
}

async fn prepare_model(model_info: &ModelInfo) -> Result<(), Error> {
    let model_path = models_dir().join(&model_info.name);

    if model_path.exists() {
        return Ok(());
    }
    let client = MinioClient::get_data_client().await;
    client
        .download_model(
            MODEL_BUCKET,
            &model_info.cid,
            &models_dir().join(&model_info.name),
        )
        .await?;
    Ok(())
}

impl LlamaServer {
    async fn run_llama_server(model_info: &ModelInfo) -> Result<Child, Error> {
        prepare_model(model_info).await?;
        let llama_server_output = fs::File::create(root_dir().join("llama_stdout.txt")).unwrap();
        let llama_server_error = fs::File::create(root_dir().join("llama_stderr.txt")).unwrap();
        let model_path = models_dir().join(&model_info.name);
        info!(
            "llama cpp server model path {}",
            model_path.to_str().unwrap()
        );
        let mut command = Command::new("/python/bin/python3");
        let mut command = command
            .arg("-m")
            .arg("llama_cpp.server")
            .arg("--model")
            .arg(model_path.to_str().unwrap())
            .arg("--seed")
            .arg("-1")
            .arg("--n_threads")
            .arg("-1")
            .arg("--n_threads_batch")
            .arg("-1")
            .arg("--port")
            .arg::<String>(format!("{}", LLAMA_SERVER_PORT))
            .stdout(Stdio::from(llama_server_output))
            .stderr(Stdio::from(llama_server_error));

        if model_info.id == TRANSFER_AGENT_ID {
            command = command.arg("--chat_format").arg("chatml-function-calling");
        }
        let child = command.spawn().expect("Failed to start Python script");
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        Ok(child)
    }

    pub async fn new(model_info: &ModelInfo) -> Result<Self, Error> {
        let child = LlamaServer::run_llama_server(model_info).await?;
        Ok(Self {
            child,
            current_model: model_info.id,
        })
    }

    pub async fn run(&mut self, model_info: &ModelInfo) -> Result<(), Error> {
        let model_id = model_info.id;
        if model_id == self.current_model {
            return Ok(());
        }
        info!("change model from {} to {}", self.current_model, model_id);
        match self.child.kill() {
            Ok(()) => {
                let _ = self.child.wait();
                let child = LlamaServer::run_llama_server(model_info).await?;
                self.current_model = model_id;
                self.child = child;
            }
            Err(e) => {
                error!("failed to kill llama server {}", e.to_string());
                return Err(Error::InferenceError {
                    message: "failed to change model".to_string(),
                });
            }
        }
        Ok(())
    }
}
