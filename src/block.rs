///////////////////////////////////////////////////////////
// my_dex/src/block.rs
///////////////////////////////////////////////////////////

use anyhow::Result;
use ed25519_dalek::{Keypair, PublicKey, Signature, Signer, Verifier};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Eine Transaktion im DEX-System.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Transaction {
    pub id: u32,
    pub from: String,
    pub to: String,
    pub amount: u64,
    // Weitere Felder nach Bedarf …
}

/// Ein Block, der Transaktionen, Metadaten und die digitale Signatur enthält.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Block {
    pub index: u64,
    pub previous_hash: String,
    pub timestamp: u64,
    pub nonce: u64,
    pub transactions: Vec<Transaction>,
    pub merkle_root: String,
    pub block_hash: String,
    pub signature: Option<Signature>,
}

impl Block {
    /// Erstellt einen neuen Block, berechnet dabei den Merkle-Root und den vollständigen Blockhash.
    pub fn new(
        index: u64,
        previous_hash: String,
        timestamp: u64,
        nonce: u64,
        transactions: Vec<Transaction>,
    ) -> Result<Self> {
        let merkle_root = compute_merkle_root(&transactions)?;
        let block_hash = compute_block_hash(index, &previous_hash, timestamp, nonce, &merkle_root);
        Ok(Block {
            index,
            previous_hash,
            timestamp,
            nonce,
            transactions,
            merkle_root,
            block_hash,
            signature: None,
        })
    }

    /// Signiert den Block mit dem übergebenen Keypair.
    pub fn sign_block(&mut self, keypair: &Keypair) {
        // Sicherheitsaspekt: Wir signieren `block_hash`, 
        // aber man könnte in einer realen Umgebung 
        // (index + previous_hash + merkle_root + …) 
        // in einen Hash packen, um Missbrauch zu vermeiden.
        let signature = keypair.sign(self.block_hash.as_bytes());
        self.signature = Some(signature);
    }

    /// Überprüft die Signatur des Blocks mit dem angegebenen öffentlichen Schlüssel.
    pub fn verify_block(&self, public_key: &PublicKey) -> bool {
        if let Some(sig) = &self.signature {
            public_key.verify(self.block_hash.as_bytes(), sig).is_ok()
        } else {
            false
        }
    }
}

/// Berechnet den Blockhash unter Einbeziehung von Index, vorherigem Hash, Zeitstempel, Nonce und Merkle-Root.
fn compute_block_hash(
    index: u64,
    previous_hash: &str,
    timestamp: u64,
    nonce: u64,
    merkle_root: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(index.to_le_bytes());
    hasher.update(previous_hash.as_bytes());
    hasher.update(timestamp.to_le_bytes());
    hasher.update(nonce.to_le_bytes());
    hasher.update(merkle_root.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Berechnet den Merkle-Root aus einer Liste von Transaktionen.
fn compute_merkle_root(transactions: &[Transaction]) -> Result<String> {
    if transactions.is_empty() {
        // Im Fall eines leeren Blocks einen Default-Hash zurückgeben
        return Ok(String::from("0"));
    }

    let mut tx_hashes: Vec<String> = transactions
        .iter()
        .map(|tx| {
            let serialized = serde_json::to_string(tx)
                .expect("Serialisierung der Transaktion sollte nicht fehlschlagen");
            let mut hasher = Sha256::new();
            hasher.update(serialized.as_bytes());
            format!("{:x}", hasher.finalize())
        })
        .collect();

    // Erstelle den Merkle-Tree, bis nur noch ein Hash übrig bleibt.
    while tx_hashes.len() > 1 {
        let mut new_hashes = Vec::new();
        for i in (0..tx_hashes.len()).step_by(2) {
            if i + 1 < tx_hashes.len() {
                let combined = format!("{}{}", tx_hashes[i], tx_hashes[i + 1]);
                let mut hasher = Sha256::new();
                hasher.update(combined.as_bytes());
                new_hashes.push(format!("{:x}", hasher.finalize()));
            } else {
                // Bei ungerader Anzahl wird der letzte Hash übernommen.
                new_hashes.push(tx_hashes[i].clone());
            }
        }
        tx_hashes = new_hashes;
    }

    Ok(tx_hashes[0].clone())
}
