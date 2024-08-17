use crate::node::config::AIZEL_CONFIG;
use common::error::Error;
use ethers::{
    contract::abigen,
    middleware::SignerMiddleware,
    providers::{Http, Provider},
    signers::{LocalWallet, Signer},
    types::{Address, U256},
};
use lazy_static::lazy_static;
use std::sync::Arc;
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
        function pubkeyExists(string calldata pubkey) public view returns (bool)
    ]"#,
);

abigen!(
    DataRegistryContract,
    r#"[
        function getUrl(uint256 nodeId) public view returns (string)
    ]"#,
);

pub struct Contract {}

impl Contract {
    pub async fn register(
        name: String,
        bio: String,
        url: String,
        pubkey: String,
        data_node_id: u64,
        tee_type: u32,
        stake_amount: u64,
    ) -> Result<(), Error> {
        let tx = INFERENCE_REGISTRY_CONTRACT.register_node(
            name,
            bio,
            url,
            pubkey,
            data_node_id.into(),
            tee_type.into(),
        );
        let tx = tx.value::<U256>(stake_amount.into());
        let _ = tx.send().await.map_err(|e| Error::RegistrationError {
            message: e.to_string(),
        })?;
        Ok(())
    }

    pub async fn query_data_node_url(data_node_id: u64) -> Result<String, Error> {
        let data_node_url: String = DATA_REGISTRY_CONTRACT
            .get_url(data_node_id.into())
            .call()
            .await
            .map_err(|e| Error::InvalidArgumentError {
                argument: format!("data node id {}", data_node_id),
                message: e.to_string(),
            })?;
        Ok(data_node_url)
    }

    pub async fn submit_inference(
        request_id: u64,
        output_hash: [u8; 32],
        report_hash: [u8; 32],
    ) -> Result<(), Error> {
        let tx = &INFERENCE_CONTRACT.submit_inference(request_id.into(), output_hash, report_hash);
        let _pending_tx = tx.send().await.map_err(|e| Error::InferenceError {
            message: format!("failed to submit inference reuslt {}", e.to_string()),
        })?;
        Ok(())
    }

    pub async fn query_public_key_exist(public_key: String) -> Result<bool, Error> {
        let exist: bool = INFERENCE_REGISTRY_CONTRACT.pubkey_exists(public_key).call().await.map_err(|e| Error::RegistrationError {
            message: e.to_string(),
        })?;
        return Ok(exist)
    }
}

lazy_static! {
    pub static ref WALLET: LocalWallet = {
        AIZEL_CONFIG
            .wallet_sk
            .parse::<LocalWallet>()
            .unwrap()
            .with_chain_id(AIZEL_CONFIG.chain_id)
    };
    pub static ref INFERENCE_CONTRACT: InferenceContract<SignerMiddleware<Provider<Http>, LocalWallet>> = {
        let provider = Provider::<Http>::try_from(AIZEL_CONFIG.endpoint.clone()).unwrap();
        let signer = Arc::new(SignerMiddleware::new(provider, WALLET.clone()));
        InferenceContract::new(
            AIZEL_CONFIG.inference_contract.parse::<Address>().unwrap(),
            signer,
        )
    };
    pub static ref DATA_REGISTRY_CONTRACT: DataRegistryContract<SignerMiddleware<Provider<Http>, LocalWallet>> = {
        let provider = Provider::<Http>::try_from(AIZEL_CONFIG.endpoint.clone()).unwrap();
        let signer = Arc::new(SignerMiddleware::new(provider, WALLET.clone()));
        DataRegistryContract::new(
            AIZEL_CONFIG
                .data_registry_contract
                .parse::<Address>()
                .unwrap(),
            signer,
        )
    };
    pub static ref INFERENCE_REGISTRY_CONTRACT: InferenceRegistryContract<SignerMiddleware<Provider<Http>, LocalWallet>> = {
        let provider = Provider::<Http>::try_from(AIZEL_CONFIG.endpoint.clone()).unwrap();
        let signer = Arc::new(SignerMiddleware::new(provider, WALLET.clone()));
        InferenceRegistryContract::new(
            AIZEL_CONFIG
                .inference_registry_contract
                .parse::<Address>()
                .unwrap(),
            signer,
        )
    };
}

#[tokio::test]
async fn query_url() {
    std::env::set_var(
        "ENDPOINT",
        "https://sepolia.infura.io/v3/250605a02ea74576bb2ab22f863a0ff8",
    );
    std::env::set_var(
        "DATA_REGISTRY_CONTRACT",
        "0x078ccaf6d3e1a3f37513158f4f944ef0936424a5",
    );
    std::env::set_var("CHAIN_ID", "11155111");
    let data_id: u64 = 1;
    let url: String = DATA_REGISTRY_CONTRACT
        .get_url(data_id.into())
        .call()
        .await
        .unwrap();
    println!("URL: {}", url);
}
