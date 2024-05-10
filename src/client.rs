use aizel::InferenceRequest;
use aizel::inference_client::InferenceClient;
pub mod aizel {
    tonic::include_proto!("aizel"); // The string specified here must match the proto package name
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut client = InferenceClient::connect("http://[::1]:50051").await?;
    let request = tonic::Request::new(InferenceRequest {
        input: "What's tonic".into(),
    });

    let response = client.llama_inference(request).await?;

    println!("RESPONSE={:?}", response);

    Ok(())

}