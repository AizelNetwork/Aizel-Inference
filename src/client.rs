use aizel::inference_client::InferenceClient;
use aizel::InferenceRequest;
pub mod aizel {
    tonic::include_proto!("aizel"); // The string specified here must match the proto package name
}
use chrono::Local;
use clap::Parser;
use env_logger::Env;
use std::io::Write;
/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Name of the person to greet
    #[arg(short, long)]
    prompt: String,

    #[arg(short, long)]
    ip: String,

    #[arg(long)]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
    // let args = Args::parse();
    let url = format!("http://{}:{}", "35.247.43.255", 8080);
    let mut client = InferenceClient::connect(url).await?;
    let request = tonic::Request::new(InferenceRequest {
        request_id: 0,
        input: "hello ".to_string(),
        model_id: 0,
        user_pk: String::new(),
        req_type: aizel::InferenceType::Llama as i32,
        network: "aizel".to_string()
    });
    let response = client.llama_inference(request).await?;
    let _ = response.into_inner();
    Ok(())
}
