use super::digest::Digest;
use super::key::{PublicKey, SecretKey};
use secp256k1::Secp256k1;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{channel, Sender};
use tokio::sync::oneshot;
/// Represents an ed25519 signature.
#[derive(Serialize, Deserialize, Clone, Default, Debug)]
pub struct Signature {
    part1: [u8; 32],
    part2: [u8; 32],
}

pub type CryptoError = secp256k1::Error;

impl Signature {
    pub fn new(digest: &Digest, secret: &SecretKey) -> Self {
        let secret_key =
            secp256k1::SecretKey::from_slice(&secret.0).expect("Unable to load secret key");
        let message = secp256k1::Message::from_digest(digest.0.clone());
        let sig = secret_key.sign_ecdsa(message).serialize_compact();
        let part1 = sig[..32].try_into().expect("Unexpected signature length");
        let part2 = sig[32..64].try_into().expect("Unexpected signature length");
        Signature { part1, part2 }
    }

    pub fn flatten(&self) -> [u8; 64] {
        [self.part1, self.part2]
            .concat()
            .try_into()
            .expect("Unexpected signature length")
    }

    pub fn verify(&self, digest: &Digest, public_key: &PublicKey) -> Result<(), CryptoError> {
        let signature = secp256k1::ecdsa::Signature::from_compact(&self.flatten())?;
        let message = secp256k1::Message::from_digest(digest.0.clone());
        let key = secp256k1::PublicKey::from_slice(&public_key.0)?;
        let secp = Secp256k1::verification_only();
        secp.verify_ecdsa(&message, &signature, &key)
    }

    pub fn verify_batch<'a, I>(digest: &Digest, votes: I) -> Result<(), CryptoError>
    where
        I: IntoIterator<Item = &'a (PublicKey, Signature)>,
    {
        let message = secp256k1::Message::from_digest(digest.0.clone());
        let secp = Secp256k1::verification_only();
        for (key, sig) in votes.into_iter() {
            let signature = secp256k1::ecdsa::Signature::from_compact(&sig.flatten())?;
            let pub_key = secp256k1::PublicKey::from_slice(&key.0)?;
            secp.verify_ecdsa(&message, &signature, &pub_key)?;
        }
        Ok(())
    }
}

/// This service holds the node's private key. It takes digests as input and returns a signature
/// over the digest (through a oneshot channel).
#[derive(Clone)]
pub struct SignatureService {
    channel: Sender<(Digest, oneshot::Sender<Signature>)>,
}

impl SignatureService {
    pub fn new(secret: SecretKey) -> Self {
        let (tx, mut rx): (Sender<(_, oneshot::Sender<_>)>, _) = channel(100);
        tokio::spawn(async move {
            while let Some((digest, sender)) = rx.recv().await {
                let signature = Signature::new(&digest, &secret);
                let _ = sender.send(signature);
            }
        });
        Self { channel: tx }
    }

    pub async fn request_signature(&mut self, digest: Digest) -> Signature {
        let (sender, receiver): (oneshot::Sender<_>, oneshot::Receiver<_>) = oneshot::channel();
        if let Err(e) = self.channel.send((digest, sender)).await {
            panic!("Failed to send message Signature Service: {}", e);
        }
        receiver
            .await
            .expect("Failed to receive signature from Signature Service")
    }
}
