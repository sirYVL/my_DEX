///////////////////////////////////////////////////
/// my_dex/src/consensus/nakamoto.rs
/////////////////////////////////////////////////// 

use sha2::{Sha256, Digest};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub struct NakamotoBlock {
    pub index: u64,
    pub previous_hash: String,
    pub timestamp: u64,
    pub transactions: Vec<String>,
    pub nonce: u64,
}

impl NakamotoBlock {
    pub fn new(index: u64, previous_hash: String, transactions: Vec<String>) -> Self {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        Self {
            index,
            previous_hash,
            timestamp,
            transactions,
            nonce: 0,
        }
    }

    pub fn mine_block(&mut self, difficulty: usize) {
        loop {
            let hash = self.calculate_hash();
            if hash.starts_with(&"0".repeat(difficulty)) {
                break;
            }
            self.nonce += 1;
        }
    }

    pub fn calculate_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.index.to_le_bytes());
        hasher.update(self.previous_hash.as_bytes());
        hasher.update(self.timestamp.to_le_bytes());
        hasher.update(self.nonce.to_le_bytes());
        format!("{:x}", hasher.finalize())
    }
}
