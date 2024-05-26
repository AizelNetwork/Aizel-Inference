mod aizel {
    include!(concat!(env!("OUT_DIR"), "/aizel.rs"));
}
use super::aizel::inference_server::Inference;
use super::aizel::{InferenceRequest, InferenceResponse};
use super::config::NodeConfig;
use log::info;
use serde_derive::{Deserialize, Serialize};
use tonic::{Request, Response, Status};
#[derive(Debug)]
pub struct AizelInference {
    pub config: NodeConfig,
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
        // let url = format!("http://{}/completion", self.llama_server_address);
        // let url = Url::parse(&url).unwrap();
        // let client = Client::builder().build().unwrap();
        let reply: InferenceResponse = InferenceResponse {
            code: 0,
            msg: String::new(),
        };
        // match client
        //     .post(url)
        //     .header("Content-Type", "application/json")
        //     .json(&llama_request)
        //     .send()
        //     .await
        // {
        //     Ok(result) => {
        //         let response: Result<LlamaResponseBody, Error> = result.json().await;
        //         match response {
        //             Ok(res) => {
        //                 reply.output = res.content;
        //                 info!("{:?}", reply.output)
        //             }
        //             Err(e) => {
        //                 error!("failed to parse response: {:?}", e);
        //                 reply.output = e.to_string();
        //                 reply.code = 1;
        //             }
        //         }
        //     }
        //     Err(e) => {
        //         error!("failed to send request: {}", e);
        //         reply.output = e.to_string();
        //         reply.code = 1;
        //     }
        // };
        Ok(Response::new(reply))
    }
}
