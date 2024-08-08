use common::error::Error;
use hex;
use secp256k1::PublicKey;
use sha3::{Digest, Keccak256};
pub fn pubkey_to_address(pubkey: String) -> Result<String, Error> {
    let compressed_pub_key_bytes = hex::decode(&pubkey).unwrap();
    let public_key = PublicKey::from_slice(&compressed_pub_key_bytes).unwrap();
    ethereum_address(&public_key)
}

fn ethereum_address(pub_key: &PublicKey) -> Result<String, Error> {
    let full_pub_key_bytes = pub_key.serialize_uncompressed();

    let mut hasher = Keccak256::new();
    hasher.update(&full_pub_key_bytes[1..]);
    let hash = hasher.finalize();

    let mut address_bytes: [u8; 20] = Default::default();
    address_bytes.copy_from_slice(&hash[12..]);

    let eth_address = hex::encode(address_bytes);
    let eth_address_with_prefix = format!("0x{}", eth_address);
    Ok(eth_address_with_prefix)
}

#[cfg(test)]
mod test {
    use super::pubkey_to_address;

    #[test]
    fn test_pubkey_to_address() {
        let address = pubkey_to_address(
            "03cb3239925b509808de491d41aa17af5f7fee1a50431c9f0838f69bd422c883d7".to_string(),
        )
        .unwrap();

        assert_eq!(
            address,
            "0xc68884d8be3d37e2fd61837cb65bc72aa5a4ebcf".to_string()
        );
    }
}
