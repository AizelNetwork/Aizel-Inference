use aizel_inference::node::config::{DEFAULT_ROOT_DIR, DATA_ADDRESS, GATE_ADDRESS, prepare_config};
use aizel_inference::node::{config::NodeConfig, node::Node};
use chrono::Local;
use clap::Parser;
use env_logger::Env;
use std::path::PathBuf;
use std::{
    io::Write,
    net::{IpAddr, SocketAddr},
};
/// Simple program to greet a person
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Ip of the node
    #[arg(short, long)]
    ip: String,
    /// Port of the node
    #[arg(short, long)]
    port: u16
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

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
    let base_dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(DEFAULT_ROOT_DIR);
    prepare_config().await?;
    let config = NodeConfig {
        socket_address: SocketAddr::new(IpAddr::V4(args.ip.parse().unwrap()), args.port),
        root_path: base_dir,
        gate_address: DATA_ADDRESS.clone(),
        data_address: GATE_ADDRESS.clone(),
    };
    let node = Node::new(config).await?;
    // node.init().await?;
    node.run_server().await?;
    Ok(())
}
