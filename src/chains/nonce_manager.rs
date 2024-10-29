use std::sync::atomic::{AtomicU64, Ordering};
use std::collections::HashMap;
use ethers::types::U256;
use queues::{IsQueue, Queue};
use tokio::sync::Mutex;
use lazy_static::lazy_static;
use crate::node::config::NETWORK_CONFIGS;
lazy_static! {
    pub static ref NONCE_MANAGERS: HashMap<String, LocalNonceManager> = {
        NETWORK_CONFIGS.get().unwrap().iter().map(|c| {
            (c.network.clone(), LocalNonceManager::new())
        }).collect()
    };
}

pub struct LocalNonceManager {
    nonces: AtomicU64,
    unused: Mutex<Queue<u64>>,
}

impl LocalNonceManager {
    pub fn new() -> Self {
        Self {
            nonces: AtomicU64::new(0),
            unused: Mutex::new(Queue::new())
        }
    }

    /// Returns the next nonce to be used
    pub async fn next(&self) -> U256 {
        let mut queue = self.unused.lock().await;
        if queue.size() == 0 {
            let nonce = self.nonces.fetch_add(1, Ordering::SeqCst);
            nonce.into()
        } else {
            let unused = queue.remove().unwrap();
            unused.into()
        }
    }

    pub fn initialize_nonce(&self, initial_nonce: U256) {
        self.nonces.store(initial_nonce.as_u64(), Ordering::SeqCst);
    }

    pub async fn save_unused(&self, nonce: U256) {
        let mut queue = self.unused.lock().await;
        let _ = queue.add(nonce.as_u64());
    }
}

#[tokio::test]
async fn test_nonce_manager() {
    let nonce_manager = LocalNonceManager::new();
    nonce_manager.initialize_nonce(10.into());
    assert_eq!(nonce_manager.next().await, 10.into());
    nonce_manager.save_unused(10.into()).await;
    assert_eq!(nonce_manager.next().await, 10.into());
}