use secp256k1::{generate_keypair, rand::thread_rng, SecretKey as SecpSecretKey};
use serde::{de, ser, Deserialize, Serialize};
use std::fmt;
/// Represents a public key (in bytes).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct PublicKey(pub [u8; 33]);

impl Default for PublicKey {
    fn default() -> Self {
        PublicKey([0; 33])
    }
}

impl PublicKey {
    pub fn encode(&self) -> String {
        hex::encode(&self.0[..])
    }

    pub fn decode(s: &str) -> Result<Self, hex::FromHexError> {
        let bytes = hex::decode(s)?;
        let array = bytes[..33]
            .try_into()
            .map_err(|_| hex::FromHexError::InvalidStringLength)?;
        Ok(Self(array))
    }
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{}", self.encode())
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "{}...", self.encode().get(0..16).unwrap())
    }
}

impl Serialize for PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        serializer.serialize_str(&self.encode())
    }
}

impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let value = Self::decode(&s).map_err(|e| de::Error::custom(e.to_string()))?;
        Ok(value)
    }
}

impl AsRef<[u8]> for PublicKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Represents a secret key (in bytes).
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct SecretKey(pub [u8; 32]);

impl SecretKey {
    pub fn encode(&self) -> String {
        hex::encode(&self.0[..])
    }

    pub fn decode(s: &str) -> Result<Self, hex::FromHexError> {
        let bytes = hex::decode(s)?;
        let array = bytes[..32]
            .try_into()
            .map_err(|_| hex::FromHexError::InvalidStringLength)?;
        Ok(Self(array))
    }

    pub fn public_key(&self) -> PublicKey {
        let sk = SecpSecretKey::from_slice(&self.0).unwrap();
        let secp = secp256k1::Secp256k1::new();
        let pk = sk.public_key(&secp);
        PublicKey(pk.serialize())
    }
}

impl Serialize for SecretKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        serializer.serialize_str(&self.encode())
    }
}

impl<'de> Deserialize<'de> for SecretKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let value = Self::decode(&s).map_err(|e| de::Error::custom(e.to_string()))?;
        Ok(value)
    }
}

impl Drop for SecretKey {
    fn drop(&mut self) {
        self.0.iter_mut().for_each(|x| *x = 0);
    }
}

pub fn generate_secp256k_keypair() -> (PublicKey, SecretKey) {
    let (secret_key, public_key) = generate_keypair(&mut thread_rng());
    // let keypair = dalek::Keypair::generate(csprng);
    let public = PublicKey(public_key.serialize());
    let secret = SecretKey(secret_key.secret_bytes());
    (public, secret)
}
