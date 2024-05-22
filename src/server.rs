use aizel::inference_server::{Inference, InferenceServer};
use aizel::{DemoAttestationResponse, Empty, InferenceRequest, InferenceResponse};
use chrono::Local;
use env_logger::Env;
use log::{error, info};
use reqwest::{Client, Error};
use serde_derive::{Deserialize, Serialize};
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tonic::{transport::Server, Request, Response, Status};

use aizel_inference::tee::{attestation::Attestation, provider::TEEProvider};
use url::Url;
pub mod aizel {
    tonic::include_proto!("aizel"); // The string specified here must match the proto package name
}

#[derive(Debug)]
pub struct AizelInference {
    llama_server_address: SocketAddr,
}

#[derive(Serialize, Debug)]
pub struct LlamaRequestBody {
    prompt: String,
}

#[derive(Deserialize, Debug)]
pub struct LlamaResponseBody {
    content: String,
}
#[tonic::async_trait]
impl Inference for AizelInference {
    async fn llama_inference(
        &self,
        request: Request<InferenceRequest>,
    ) -> Result<Response<InferenceResponse>, Status> {
        let llama_request = LlamaRequestBody {
            prompt: request.into_inner().input,
        };
        info!("receive a request {:?}", llama_request);
        let url = format!("http://{}/completion", self.llama_server_address);
        let url = Url::parse(&url).unwrap();
        let client = Client::builder().build().unwrap();
        let mut reply = InferenceResponse {
            input: llama_request.prompt.clone(),
            output: String::new(),
            code: 0,
        };
        match client
            .post(url)
            .header("Content-Type", "application/json")
            .json(&llama_request)
            .send()
            .await
        {
            Ok(result) => {
                let response: Result<LlamaResponseBody, Error> = result.json().await;
                match response {
                    Ok(res) => {
                        reply.output = res.content;
                        info!("{:?}", reply.output)
                    }
                    Err(e) => {
                        error!("failed to parse response: {:?}", e);
                        reply.output = e.to_string();
                        reply.code = 1;
                    }
                }
            }
            Err(e) => {
                error!("failed to send request: {}", e);
                reply.output = e.to_string();
                reply.code = 1;
            }
        };
        Ok(Response::new(reply))
    }

    async fn demo_attestation_report(
        &self,
        request: Request<Empty>,
    ) -> Result<Response<DemoAttestationResponse>, Status> {
        let mut reply = DemoAttestationResponse {
            jwt_token: String::new(),
            code: 0,
        };
        let attestation = Attestation::new();
        match attestation {
            Ok(attestation) => {
                let report = attestation.get_attestation_report().unwrap();
                reply.jwt_token = report;
            }
            Err(e) => {
                error!("failed to get attestation {}", e);
                reply.jwt_token = String::new();
                reply.code = 1;
            }
        }
        Ok(Response::new(reply))
    }
}

fn print_token() {
    tokio::spawn(async {
        loop {
            let attestation = Attestation::new().unwrap();
            info!("token: {}", attestation.get_attestation_report().unwrap());
            std::thread::sleep(std::time::Duration::from_secs(600));
        }
    });
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr: std::net::SocketAddr = "[::1]:50051".parse()?;
    let inference = AizelInference {
        llama_server_address: SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080),
    };

    let _logger = env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .format(|buf, record| {
            let level = { buf.default_level_style(record.level()) };
            writeln!(
                buf,
                "{} {} [{}:{}] {}",
                Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                format_args!("{:>5}", level),
                record.module_path().unwrap_or("<unnamed>"),
                record.line().unwrap_or(0),
                &record.args()
            )
        })
        .init();
    info!("listening on {}", addr);
    print_token();
    Server::builder()
        .add_service(InferenceServer::new(inference))
        .serve(addr)
        .await?;

    Ok(())
}
