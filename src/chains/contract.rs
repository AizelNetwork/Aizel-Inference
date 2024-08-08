use crate::node::config::WALLET_SK_FILE;
use common::error::Error;
use ethers::{
    contract::abigen,
    middleware::SignerMiddleware,
    providers::{Http, Provider},
    signers::{LocalWallet, Signer},
    types::Address,
};
use lazy_static::lazy_static;
use std::{env, fs, path::PathBuf, sync::Arc};
abigen!(
    InferenceContract,
    r#"[
        function submitInference(uint256 requestId,bytes32 output,bytes32 report) external
    ]"#,
);

abigen!(
    InferenceRegistryContract,
    r#"[
        function registerNode(string memory name,string memory bio,string memory url,string memory pubkey,uint256 dataNodeId,uint32 teeType) external payable returns (uint256 id)
        function getMinStake() external view returns (uint256)
    ]"#,
);

abigen!(
    DataRegistryContract,
    r#"[
        function getUrl(uint256 nodeId) public view returns (string)
    ]"#,
);

lazy_static! {
    pub static ref WALLET: LocalWallet = {
        let chain_id: u64 = env::var("CHAIN_ID").unwrap().parse().unwrap();
        let wallet_sk = fs::read_to_string(
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(WALLET_SK_FILE),
        )
        .map_err(|e| Error::FileError {
            path: WALLET_SK_FILE.into(),
            message: e.to_string(),
        })
        .unwrap();
        let wallet_sk = wallet_sk.trim();
        wallet_sk
            .parse::<LocalWallet>()
            .unwrap()
            .with_chain_id(chain_id)
    };
    pub static ref INFERENCE_CONTRACT: InferenceContract<SignerMiddleware<Provider<Http>, LocalWallet>> = {
        let provider = Provider::<Http>::try_from(env::var("ENDPOINT").unwrap()).unwrap();
        let signer = Arc::new(SignerMiddleware::new(provider, WALLET.clone()));
        let contract_address: String = env::var("INFERENCE_CONTRACT").unwrap().parse().unwrap();
        InferenceContract::new(contract_address.parse::<Address>().unwrap(), signer)
    };
    pub static ref DATA_REGISTRY_CONTRACT: DataRegistryContract<SignerMiddleware<Provider<Http>, LocalWallet>> = {
        let provider = Provider::<Http>::try_from(env::var("ENDPOINT").unwrap()).unwrap();
        let signer = Arc::new(SignerMiddleware::new(provider, WALLET.clone()));
        let contract_address: String = env::var("DATA_REGISTRY_CONTRACT").unwrap().parse().unwrap();
        DataRegistryContract::new(contract_address.parse::<Address>().unwrap(), signer)
    };
    pub static ref INFERENCE_REGISTRY_CONTRACT: InferenceRegistryContract<SignerMiddleware<Provider<Http>, LocalWallet>> = {
        let provider = Provider::<Http>::try_from(env::var("ENDPOINT").unwrap()).unwrap();
        let signer = Arc::new(SignerMiddleware::new(provider, WALLET.clone()));
        let contract_address: String = env::var("INFERENCE_REGISTRY_CONTRACT")
            .unwrap()
            .parse()
            .unwrap();
        InferenceRegistryContract::new(contract_address.parse::<Address>().unwrap(), signer)
    };
}

#[tokio::test]
async fn query_url() {
    env::set_var(
        "ENDPOINT",
        "https://sepolia.infura.io/v3/250605a02ea74576bb2ab22f863a0ff8",
    );
    env::set_var(
        "DATA_REGISTRY_CONTRACT",
        "0x078ccaf6d3e1a3f37513158f4f944ef0936424a5",
    );
    env::set_var("CHAIN_ID", "11155111");
    let data_id: u64 = 1;
    let url: String = DATA_REGISTRY_CONTRACT
        .get_url(data_id.into())
        .call()
        .await
        .unwrap();
    println!("URL: {}", url);
}
