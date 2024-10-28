use super::config::{llama_server_port, logs_dir, ml_model_config, ml_model_config_with_id, ml_models_dir, ml_models_start_script, ml_server_port, models_dir, root_dir, source_ml_models_dir, MODEL_BUCKET, TRANSFER_AGENT_ID};
use crate::chains::contract::ModelInfo;
use crate::s3_minio::client::MinioClient;
use common::error::Error;
use log::{error, info};
use std::fs;
use std::process::{Child, Command, Stdio};
use tonic::async_trait;
use flate2::read::GzDecoder;
use tar::Archive;

#[async_trait]
pub trait ModelServer: Send + Sync {
    async fn run_server(model_info: &ModelInfo) -> Result<(), Error>;
}

pub struct LlamaServer {
    pub child: Child,
    pub current_model: u64,
}

impl LlamaServer {
    async fn prepare_model(model_info: &ModelInfo) -> Result<(), Error> {
        let network = &model_info.network;
        let model_path = models_dir(network).join(&model_info.name);
    
        if model_path.exists() {
            return Ok(());
        }
        let client = MinioClient::get_data_client(network).await;
        client
            .download_model(
                MODEL_BUCKET,
                &model_info.cid,
                &models_dir(network).join(&model_info.name),
            )
            .await?;
        Ok(())
    }

    async fn run_llama_server(model_info: &ModelInfo) -> Result<Child, Error> {
        LlamaServer::prepare_model(model_info).await?;
        let network = &model_info.network;
        
        let llama_server_output = fs::File::create(logs_dir(network).join(format!("llama_stdout_{}.txt", model_info.network))).unwrap();
        let llama_server_error = fs::File::create(logs_dir(network).join(format!("llama_stderr_{}.txt", model_info.network))).unwrap();
        let model_path = models_dir(network).join(&model_info.name);
        info!(
            "llama cpp server model path {}",
            model_path.to_str().unwrap()
        );
        let mut command = Command::new("/python/bin/python3");
        let port: u16 = llama_server_port(network)?;
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
            .arg::<String>(format!("{}", port))
            .stdout(Stdio::from(llama_server_output))
            .stderr(Stdio::from(llama_server_error));
        info!("run llama cpp server for network {} on port {}", network, port);
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
        info!("change model from {} to {} in network {}", self.current_model, model_id, model_info.network);
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

pub struct MlServer {
    pub child: Child,
    pub current_model: u64,
}

impl MlServer {
    fn save_model_config(model_info: &ModelInfo) -> Result<(), Error> {
        fs::copy(ml_model_config(&model_info.network), ml_model_config_with_id(&model_info.network, model_info.id)).unwrap();
        Ok(())
    }

    fn recover_model_config(model_info: &ModelInfo) -> Result<(), Error> {
        fs::copy(ml_model_config_with_id(&model_info.network, model_info.id), ml_model_config(&model_info.network)).unwrap();
        Ok(())
    }

    async fn prepare_model(model_info: &ModelInfo) -> Result<(), Error> {
        let network = &model_info.network;
        if !ml_models_dir(network).exists() {
            copy_dir::copy_dir(source_ml_models_dir(), ml_models_dir(network)).map_err(|e| {
                Error::InferenceError { message: format!("failed to copy models dir {:?}", e) }
            })?;
        }
        let model_path = ml_models_dir(network).join(&model_info.name);
    
        if model_path.exists() {
            let _ = MlServer::recover_model_config(model_info);
            return Ok(());
        }

        // if !model_info.name.ends_with("tar.gz") {
        //     return Err(Error::InferenceError { message: format!("model format not supported {}", model_info.name) })
        // }

        let client = MinioClient::get_data_client(&model_info.network).await;
        client
            .download_model(
                MODEL_BUCKET,
                &model_info.cid,
                &model_path,
            )
            .await?;
        
        let tar_gz = fs::File::open(model_path).unwrap();
        let tar = GzDecoder::new(tar_gz);
        let mut archive = Archive::new(tar);
        archive.unpack(ml_models_dir(network)).unwrap();
        let _ = MlServer::save_model_config(model_info);
        Ok(())
    }

    async fn run_ml_server(model_info: &ModelInfo) -> Result<Child, Error> {
        MlServer::prepare_model(&model_info).await?;
        let network = &model_info.network;
        let ml_server_output = fs::File::create(logs_dir(network).join(format!("ml_stdout_{}.txt", model_info.network))).unwrap();
        let ml_server_error = fs::File::create(logs_dir(network).join(format!("ml_stderr_{}.txt", model_info.network))).unwrap();
        
        let mut command: Command = Command::new("bash");
        let command = command.arg(ml_models_start_script().to_str().unwrap())
            .arg(format!("{}", ml_server_port(network)?))
            .arg(ml_models_dir(network))
            .stdout(Stdio::from(ml_server_output))
            .stderr(Stdio::from(ml_server_error));
        let child = command.spawn().expect("Failed to start Python script");
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        Ok(child)
    }

    pub async fn new(model_info: &Option<ModelInfo>) -> Result<Self, Error> {
        match model_info {
            Some(m) => {
                let child = MlServer::run_ml_server(m).await?;
                Ok(Self {
                    child,
                    current_model: m.id,
                })
            }
            None => {
                let child = Command::new("sleep").arg("infinity").spawn().expect("Failed to run sleep command");
                Ok(Self {
                    child,
                    current_model: 0,
                })
            }
        }
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
                let child = MlServer::run_ml_server(model_info).await?;
                self.current_model = model_id;
                self.child = child;
            }
            Err(e) => {
                error!("failed to kill ml server {}", e.to_string());
                return Err(Error::InferenceError {
                    message: "failed to change model".to_string(),
                });
            }
        }
        Ok(())
    }
}


#[tokio::test]
async fn request_ml_model() {
    use crate::chains::contract::Contract;
    use crate::node::config::{NETWORK_CONFIGS, initialize_network_configs, source_ml_models_dir, ml_dir};
    NETWORK_CONFIGS.set(initialize_network_configs().await.unwrap()).unwrap();
    println!("{:?} ", source_ml_models_dir());
    fs::create_dir_all(ml_dir("krest")).unwrap();
    let model_info = Contract::query_model(3, "krest").await.unwrap();
    println!("{:?} ", model_info);
    // MlServer::prepare_model(&model_info).await.unwrap();
}
