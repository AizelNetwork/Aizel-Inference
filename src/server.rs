use tonic::{transport::Server, Request, Response, Status};
use aizel::inference_server::{Inference, InferenceServer};
use aizel::{InferenceRequest, InferenceResponse};
pub mod aizel {
    tonic::include_proto!("aizel"); // The string specified here must match the proto package name
}

#[derive(Debug, Default)]
pub struct AizelInference {

}


#[tonic::async_trait]
impl Inference for AizelInference {
    async fn llama_inference(&self, request: Request<InferenceRequest>,) -> Result<Response<InferenceResponse>, Status> {
        println!("Got a request: {:?}", request);
        let reply = InferenceResponse {
            output: format!("Get input  {}!", request.into_inner().input)
        };

        Ok(Response::new(reply))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr: std::net::SocketAddr = "[::1]:50051".parse()?;
    let inference = AizelInference::default();

    Server::builder()
        .add_service(InferenceServer::new(inference))
        .serve(addr)
        .await?;

    Ok(())
}