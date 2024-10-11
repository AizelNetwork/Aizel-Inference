use crate::node::config::AIZEL_CONFIG;
use common::error::Error;
use ethers::core::{
    abi::{self, Token},
    utils,
};
use ethers::{
    contract::abigen,
    middleware::SignerMiddleware,
    providers::{Http, Provider},
    signers::{LocalWallet, Signer},
    types::{Address, Bytes, U256},
};
use lazy_static::lazy_static;
use log::{error, info};
use std::str::FromStr;
use std::sync::Arc;
#[derive(Debug)]
pub struct ModelInfo {
    pub name: String,
    pub cid: String,
    pub id: u64,
}

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

abigen!(
    ModelContract,
    r#"[
		{
			"inputs": [
				{
					"internalType": "uint256",
					"name": "modelId",
					"type": "uint256"
				}
			],
			"name": "getModelDetails",
			"outputs": [
				{
					"components": [
						{
							"internalType": "uint256",
							"name": "modelId",
							"type": "uint256"
						},
						{
							"internalType": "string",
							"name": "modelName",
							"type": "string"
						},
						{
							"internalType": "string",
							"name": "CID",
							"type": "string"
						},
						{
							"internalType": "uint256",
							"name": "size",
							"type": "uint256"
						},
						{
							"internalType": "uint256",
							"name": "totalValue",
							"type": "uint256"
						}
					],
					"internalType": "struct Models.ModelDetails",
					"name": "",
					"type": "tuple"
				}
			],
			"stateMutability": "view",
			"type": "function"
		},
		{
			"inputs": [
				{
					"internalType": "uint256",
					"name": "dataNodeId",
					"type": "uint256"
				}
			],
			"name": "getModelsByDataNodeId",
			"outputs": [
				{
					"components": [
						{
							"internalType": "uint256",
							"name": "modelId",
							"type": "uint256"
						},
						{
							"internalType": "string",
							"name": "modelName",
							"type": "string"
						},
						{
							"internalType": "string",
							"name": "CID",
							"type": "string"
						},
						{
							"internalType": "uint256",
							"name": "size",
							"type": "uint256"
						},
						{
							"internalType": "uint256",
							"name": "totalValue",
							"type": "uint256"
						}
					],
					"internalType": "struct Models.ModelDetails[]",
					"name": "models",
					"type": "tuple[]"
				}
			],
			"stateMutability": "view",
			"type": "function"
		}
	]"#,
);

abigen!(
    TransferContract,
    r#"[
    {
        "inputs": [
          {
            "internalType": "uint256",
            "name": "requestId",
            "type": "uint256"
          },
          {
            "internalType": "address",
            "name": "tokenAddress",
            "type": "address"
          },
          {
            "internalType": "address",
            "name": "from",
            "type": "address"
          },
          {
            "internalType": "address",
            "name": "to",
            "type": "address"
          },
          {
            "internalType": "uint256",
            "name": "value",
            "type": "uint256"
          },
          {
            "internalType": "bytes",
            "name": "signature",
            "type": "bytes"
          }
        ],
        "name": "AgentTransfer",
        "outputs": [],
        "stateMutability": "nonpayable",
        "type": "function"
    }
    ]"#
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
        let _pending_tx = tx.send().await.map_err(|e| {
            error!("failed to submit inference result: {}", e.to_string());
            Error::InferenceError {
                message: format!("failed to submit inference reuslt {}", e.to_string()),
            }
        })?;
        Ok(())
    }

    pub async fn query_public_key_exist(public_key: String) -> Result<bool, Error> {
        let exist: bool = INFERENCE_REGISTRY_CONTRACT
            .pubkey_exists(public_key)
            .call()
            .await
            .map_err(|e| Error::RegistrationError {
                message: e.to_string(),
            })?;
        return Ok(exist);
    }

    pub async fn query_model(model_id: u64) -> Result<ModelInfo, Error> {
        let model: ModelDetails = MODEL_CONTRACT
            .get_model_details(model_id.into())
            .call()
            .await
            .map_err(|e| Error::ContractError {
                message: e.to_string(),
            })?;
        return Ok(ModelInfo {
            name: model.model_name,
            cid: model.cid,
            id: model_id,
        });
    }

    pub async fn query_data_node_default_model(data_node_id: u64) -> Result<ModelInfo, Error> {
        let models: Vec<ModelDetails> = MODEL_CONTRACT
            .get_models_by_data_node_id(data_node_id.into())
            .call()
            .await
            .map_err(|e| Error::ContractError {
                message: e.to_string(),
            })?;
        if models.is_empty() {
            panic!("the data node doesn't have any models");
        } else {
            return Ok(ModelInfo {
                name: models[0].model_name.clone(),
                cid: models[0].cid.clone(),
                id: models[0].model_id.try_into().unwrap(),
            });
        }
    }

    pub async fn transfer(
        request_id: u64,
        token_address: String,
        from: String,
        to: String,
        amount: U256,
    ) -> Result<(), Error> {
        // signature
        let encoded_data = [
            abi::encode(&[Token::Uint(
                U256::from_dec_str(&request_id.to_string()).unwrap(),
            )]),
            abi::encode_packed(&[
                Token::Address(Address::from_str(&token_address).unwrap()),
                Token::Address(Address::from_str(&from).unwrap()),
                Token::Address(Address::from_str(&to).unwrap()),
            ])
            .unwrap(),
            abi::encode(&[Token::Uint(amount)]),
        ]
        .concat();
        let message = utils::keccak256(&encoded_data);
        let signature = WALLET.sign_message(message).await.unwrap().to_vec();
        info!(
            "request id {}, token address {}, from {}, to {}, amount {}, signature 0x{}",
            request_id,
            token_address,
            from,
            to,
            amount,
            hex::encode(signature.clone())
        );
        let tx = TRANSFER_CONTRACT.agent_transfer(
            request_id.into(),
            token_address.parse().unwrap(),
            from.parse().unwrap(),
            to.parse().unwrap(),
            amount,
            Bytes::from_iter(signature),
        );
        let _pending_tx = tx.send().await.map_err(|e| Error::InferenceError {
            message: format!("failed to submit inference reuslt {}", e.to_string()),
        })?;
        Ok(())
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
    pub static ref MODEL_CONTRACT: ModelContract<SignerMiddleware<Provider<Http>, LocalWallet>> = {
        let provider = Provider::<Http>::try_from(AIZEL_CONFIG.endpoint.clone()).unwrap();
        let signer = Arc::new(SignerMiddleware::new(provider, WALLET.clone()));
        ModelContract::new(
            AIZEL_CONFIG.model_contract.parse::<Address>().unwrap(),
            signer,
        )
    };
    pub static ref TRANSFER_CONTRACT: TransferContract<SignerMiddleware<Provider<Http>, LocalWallet>> = {
        let provider = Provider::<Http>::try_from(AIZEL_CONFIG.endpoint.clone()).unwrap();
        let signer = Arc::new(SignerMiddleware::new(provider, WALLET.clone()));
        TransferContract::new(
            AIZEL_CONFIG.transfer_contract.parse::<Address>().unwrap(),
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

#[tokio::test]
async fn test_call_contract() {
    use ethers::core::utils::{parse_units, ParseUnits};
    use hex::FromHex;
    let request_id = 135;
    let token_address = "0x411A42fE3F187b778e8D2dAE41E062D3F417929a";
    let from = "0xc68884d8be3d37e2fd61837cb65bc72aa5a4ebcf";
    let to = "0xC68884D8bE3D37E2fD61837cB65bc72Aa5a4EBcf";
    let pu: ParseUnits = parse_units(10, 18).unwrap();
    let amount = U256::from(pu);
    let signature = "82c1f5687c4f0353e36b1d735ebb9ce35f0646cf0d0674e3aae5bbb35b7175b15a6491b712689e11e6b7a559f09f6db85042c18358a25d07b0eba9cc110d1d881b";
    let tx = TRANSFER_CONTRACT.agent_transfer(
        request_id.into(),
        token_address.parse().unwrap(),
        from.parse().unwrap(),
        to.parse().unwrap(),
        amount,
        Bytes::from_hex(signature).unwrap(),
    );
    let _pending_tx = tx
        .send()
        .await
        .map_err(|e| Error::InferenceError {
            message: format!("failed to submit inference reuslt {}", e.to_string()),
        })
        .unwrap();
}

#[tokio::test]
async fn query_model() {
    use crate::node::config::ml_models_dir;
    use crate::s3_minio::client::MinioClient;
    use std::fs::File;
    use flate2::read::GzDecoder;
    use tar::Archive;
    let model_info = Contract::query_model(8).await.unwrap();
    println!("{:?}", model_info);
    let model_path = ml_models_dir().join(&model_info.name);    
    let client = MinioClient::get_data_client().await;
    client
        .download_model(
            "models",
            &model_info.cid,
            &model_path,
        )
        .await.unwrap();

    let tar_gz = File::open(model_path).unwrap();
    let tar = GzDecoder::new(tar_gz);
    let mut archive = Archive::new(tar);
    archive.unpack(ml_models_dir()).unwrap();
}
