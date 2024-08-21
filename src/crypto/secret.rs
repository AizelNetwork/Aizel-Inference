use super::key::{generate_secp256k_keypair, PublicKey, SecretKey};
use common::error::Error;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::BufWriter;
use std::io::Write as _;

pub trait Export: Serialize + DeserializeOwned {
    fn read(path: &str) -> Result<Self, Error> {
        let reader = || -> Result<Self, std::io::Error> {
            let data = fs::read(path)?;
            Ok(serde_json::from_slice(data.as_slice())?)
        };
        reader().map_err(|e| Error::FileError {
            path: path.into(),
            message: e.to_string(),
        })
    }

    fn write(&self, path: &str) -> Result<(), Error> {
        let writer = || -> Result<(), std::io::Error> {
            let file = OpenOptions::new().create(true).write(true).open(path)?;
            let mut writer = BufWriter::new(file);
            let data = serde_json::to_string_pretty(self).unwrap();
            writer.write_all(data.as_ref())?;
            writer.write_all(b"\n")?;
            Ok(())
        };
        writer().map_err(|e| Error::FileError {
            path: path.into(),
            message: e.to_string(),
        })
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Secret {
    pub name: PublicKey,
    pub secret: SecretKey,
}

impl Secret {
    pub fn new() -> Self {
        let (name, secret) = generate_secp256k_keypair();
        Self { name, secret }
    }

    pub fn from_str(s: &str) -> Self {
        let secret = SecretKey::decode(s).unwrap();
        let name = secret.public_key();
        Self { name, secret }
    }
}

impl Default for Secret {
    fn default() -> Self {
        Self::new()
    }
}

impl Export for Secret {}
