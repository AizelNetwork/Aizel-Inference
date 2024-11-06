use crate::node::config::{AIZEL_CONFIG, NETWORK_CONFIGS};
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
    types::{Address, Bytes, U256, H160},
};
use lazy_static::lazy_static;
use log::{error, info};
use std::{collections::HashMap, str::FromStr};
use std::sync::Arc;
use super::nonce_manager::LocalNonceManager;
#[derive(Debug)]
pub struct ModelInfo {
    pub name: String,
    pub cid: String,
    pub id: u64,
    pub network: String
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

lazy_static! {
    
}

impl Contract {
    pub async fn get_nonce(network: &str) -> Result<U256, Error> {
        let nonce_manager = NONCE_MANAGERS.get(network).ok_or(Error::NetworkConfigNotFoundError { network: network.to_string() })?;
        Ok(nonce_manager.next().await)
    }

    pub async fn unuse_nonce(network: &str, unused: U256) -> Result<(), Error> {
        let nonce_manager = NONCE_MANAGERS.get(network).ok_or(Error::NetworkConfigNotFoundError { network: network.to_string() })?;
        Ok(nonce_manager.save_unused(unused).await)
    }

    pub async fn register(
        name: String,
        bio: String,
        url: String,
        pubkey: String,
        data_node_id: u64,
        tee_type: u32,
        stake_amount: u64,
        network: &str,
    ) -> Result<(), Error> {
        let contract = INFERENCE_REGISTRY_CONTRACTS.get(network).ok_or(Error::NetworkConfigNotFoundError { network: network.to_string() })?;
        let tx = contract.register_node(
            name,
            bio,
            url,
            pubkey,
            data_node_id.into(),
            tee_type.into(),
        );
        let nonce = Self::get_nonce(network).await?;
        info!("register nonce {}", nonce);
        let tx = tx.nonce::<U256>(nonce.clone());
        let tx = tx.value::<U256>(stake_amount.into());
        match tx.send().await {
            Ok(_) => {}
            Err(e) => {
                Self::unuse_nonce(network, nonce).await?;
                return Err(Error::RegistrationError {
                    message: e.to_string(),
                });
            }
        }
        Ok(())
    }

    pub async fn query_data_node_url(data_node_id: u64, network: &str) -> Result<String, Error> {
        let contract = DATA_REGISTRY_CONTRACTS.get(network).ok_or(Error::NetworkConfigNotFoundError { network: network.to_string() })?;
        let data_node_url: String = contract
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
        network: &str
    ) -> Result<(), Error> {
        let contract = INFERENCE_CONTRACTS.get(network).ok_or(Error::NetworkConfigNotFoundError { network: network.to_string() })?;
        let tx = contract.submit_inference(request_id.into(), output_hash, report_hash);
        let nonce: U256 = Self::get_nonce(network).await?;
        info!("submit inference: network {} request id {}, nonce {}", network, request_id, nonce);
        let tx = tx.nonce::<U256>(nonce.clone());
        match tx.send().await {
            Ok(_) => {},
            Err(e) => {
                error!("failed to submit inference result: {}", e.to_string());
                Self::unuse_nonce(network, nonce).await?;
                return Err(Error::InferenceError {
                    message: format!("failed to submit inference reuslt {}", e.to_string()),
                });
            }
        }
        Ok(())
    }

    pub async fn query_public_key_exist(public_key: String, network: &str) -> Result<bool, Error> {
        let contract = INFERENCE_REGISTRY_CONTRACTS.get(network).ok_or(Error::NetworkConfigNotFoundError { network: network.to_string() })?;
        let exist: bool = contract
            .pubkey_exists(public_key)
            .call()
            .await
            .map_err(|e| Error::RegistrationError {
                message: e.to_string(),
            })?;
        return Ok(exist);
    }

    pub async fn query_model(model_id: u64, network: &str) -> Result<ModelInfo, Error> {
        let contract = MODEL_CONTRACTS.get(network).ok_or(Error::NetworkConfigNotFoundError { network: network.to_string() })?;
        let model: ModelDetails = contract
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
            network: network.to_string()
        });
    }

    pub async fn query_data_node_default_model(data_node_id: u64, network: &str) -> Result<ModelInfo, Error> {
        let contract = MODEL_CONTRACTS.get(network).ok_or(Error::NetworkConfigNotFoundError { network: network.to_string() })?;
        let models: Vec<ModelDetails> = contract
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
                network: network.to_string()
            });
        }
    }

    pub async fn transfer(
        request_id: u64,
        token_address: String,
        from: String,
        to: String,
        amount: U256,
        network: &str
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
        let chain_id = NETWORK_CONFIGS.get().unwrap().iter().find(|n| {
            n.network == network
        }).ok_or(Error::NetworkConfigNotFoundError { network: network.to_string() })?.chain_id;
        let wallet = AIZEL_CONFIG.wallet_sk.parse::<LocalWallet>().unwrap().with_chain_id(chain_id);
        let signature = wallet.sign_message(message).await.unwrap().to_vec();
        info!(
            "network {}: request id {}, token address {}, from {}, to {}, amount {}, signature 0x{}",
            network,
            request_id,
            token_address,
            from,
            to,
            amount,
            hex::encode(signature.clone())
        );
        let contract = TRANSFER_CONTRACTS.get(network).ok_or(Error::NetworkConfigNotFoundError { network: network.to_string() })?;
        let tx = contract.agent_transfer(
            request_id.into(),
            token_address.parse().unwrap(),
            from.parse().unwrap(),
            to.parse().unwrap(),
            amount,
            Bytes::from_iter(signature),
        );
        let nonce = Self::get_nonce(network).await?;
        let tx = tx.nonce::<U256>(nonce.clone());
        match tx.send().await {
            Ok(_) => {},
            Err(e) => {
                Self::unuse_nonce(network, nonce).await?;
                return Err(Error::InferenceError {
                    message: format!("failed to transfer token {}", e.to_string()),
                });
            }
        }
        Ok(())
    }
}

lazy_static! {
    pub static ref NONCE_MANAGERS: HashMap<String, LocalNonceManager> = {
        NETWORK_CONFIGS.get().unwrap().iter().map(|c| {
            (c.network.clone(), LocalNonceManager::new())
        }).collect()
    };

    pub static ref INFERENCE_CONTRACTS: HashMap<String, InferenceContract<SignerMiddleware<Provider<Http>, LocalWallet>>> = {
        NETWORK_CONFIGS.get().unwrap().iter().map(|c| {
            let provider = Provider::<Http>::try_from(c.rpc_url.clone()).unwrap();
            let wallet = AIZEL_CONFIG.wallet_sk.parse::<LocalWallet>().unwrap().with_chain_id(c.chain_id);
            let signer = Arc::new(SignerMiddleware::new(provider, wallet));
            let address = c.contracts.iter().find(|a| {
                a.name == "INFERENCE"
            }).unwrap().address;
            (c.network.clone(), InferenceContract::new(address, signer))
        }).collect()
    };

    pub static ref DATA_REGISTRY_CONTRACTS: HashMap<String, DataRegistryContract<SignerMiddleware<Provider<Http>, LocalWallet>>> = {
        NETWORK_CONFIGS.get().unwrap().iter().map(|c| {
            let provider = Provider::<Http>::try_from(c.rpc_url.clone()).unwrap();
            let wallet = AIZEL_CONFIG.wallet_sk.parse::<LocalWallet>().unwrap().with_chain_id(c.chain_id);
            let signer = Arc::new(SignerMiddleware::new(provider, wallet));
            let address = c.contracts.iter().find(|a| {
                a.name == "DATA_NODE"
            }).unwrap().address;
            (c.network.clone(), DataRegistryContract::new(address, signer))
        }).collect()
    };

    pub static ref INFERENCE_REGISTRY_CONTRACTS: HashMap<String, InferenceRegistryContract<SignerMiddleware<Provider<Http>, LocalWallet>>> = {
        NETWORK_CONFIGS.get().unwrap().iter().map(|c| {
            let provider = Provider::<Http>::try_from(c.rpc_url.clone()).unwrap();
            let wallet = AIZEL_CONFIG.wallet_sk.parse::<LocalWallet>().unwrap().with_chain_id(c.chain_id);
            let signer = Arc::new(SignerMiddleware::new(provider, wallet));
            let address = c.contracts.iter().find(|a| {
                a.name == "INFERENCE_NODE"
            }).unwrap().address;
            (c.network.clone(), InferenceRegistryContract::new(address, signer))
        }).collect()
    };

    pub static ref MODEL_CONTRACTS: HashMap<String, ModelContract<SignerMiddleware<Provider<Http>, LocalWallet>>> = {
        NETWORK_CONFIGS.get().unwrap().iter().map(|c| {
            let provider = Provider::<Http>::try_from(c.rpc_url.clone()).unwrap();
            let wallet = AIZEL_CONFIG.wallet_sk.parse::<LocalWallet>().unwrap().with_chain_id(c.chain_id);
            let signer = Arc::new(SignerMiddleware::new(provider, wallet));
            let address = c.contracts.iter().find(|a| {
                a.name == "MODEL"
            }).unwrap().address;
            (c.network.clone(), ModelContract::new(address, signer))
        }).collect()
    };

    pub static ref TRANSFER_CONTRACTS: HashMap<String, TransferContract<SignerMiddleware<Provider<Http>, LocalWallet>>> = {
        NETWORK_CONFIGS.get().unwrap().iter().map(|c| {
            let provider = Provider::<Http>::try_from(c.rpc_url.clone()).unwrap();
            let wallet = AIZEL_CONFIG.wallet_sk.parse::<LocalWallet>().unwrap().with_chain_id(c.chain_id);
            let signer = Arc::new(SignerMiddleware::new(provider, wallet));
            let address = match c.contracts.iter().find(|a| {
                a.name == "TransferAgent"
            }) {
                Some(c) => c.address,
                None => H160::zero()
            };
            (c.network.clone(), TransferContract::new(address, signer))
        }).collect()
    };
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
    let tx = TRANSFER_CONTRACTS.get("aizel").unwrap().agent_transfer(
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
    let model_info = Contract::query_model(1, "aizel").await.unwrap();
    println!("{:?}", model_info);
    let model_path = ml_models_dir("aizel").join(&model_info.name);    
    let client = MinioClient::get_data_client("aizel").await;
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
    archive.unpack(ml_models_dir("aizel")).unwrap();
}
