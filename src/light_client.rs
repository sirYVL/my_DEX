///////////////////////////////////////////////////////////
// my_dex/src/light_client.rs
///////////////////////////////////////////////////////////
//
// Dieses Modul implementiert einen produktionsreifen Light Client,
// der den aktuellen Blockheader von mehreren Peers abruft und einen
// Konsens herbeiführt. Dabei wird sichergestellt, dass Daten von
// mindestens einer definierten Anzahl (threshold) von unabhängigen
// Peers übereinstimmen. Jede Abfrage unterliegt einem Timeout, um
// hängende Peer-Verbindungen zu vermeiden. So wird verhindert,
// dass ein einzelner, potenziell kompromittierter Peer einen
// Eclipse-Angriff realisieren kann.

use anyhow::{Result, anyhow, Context};
use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use tokio::time::{sleep, timeout, Duration};
use tracing::{info, warn, error};

/// Repräsentiert einen Blockheader, den ein Peer zurückliefern kann.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    pub index: u64,
    pub previous_hash: String,
    pub timestamp: u64,
    pub nonce: u64,
    pub merkle_root: String,
    pub block_hash: String,
}

impl BlockHeader {
    /// Erzeugt einen neuen BlockHeader und berechnet den Blockhash.
    pub fn new(index: u64, previous_hash: String, timestamp: u64, nonce: u64, merkle_root: String) -> Self {
        let block_hash = Self::compute_block_hash(index, &previous_hash, timestamp, nonce, &merkle_root);
        BlockHeader {
            index,
            previous_hash,
            timestamp,
            nonce,
            merkle_root,
            block_hash,
        }
    }

    fn compute_block_hash(index: u64, previous_hash: &str, timestamp: u64, nonce: u64, merkle_root: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(index.to_le_bytes());
        hasher.update(previous_hash.as_bytes());
        hasher.update(timestamp.to_le_bytes());
        hasher.update(nonce.to_le_bytes());
        hasher.update(merkle_root.as_bytes());
        format!("{:x}", hasher.finalize())
    }
}

/// Trait, der die Funktionalität eines Peers beschreibt, der einen Blockheader liefern kann.
#[async_trait]
pub trait Peer {
    /// Ruft asynchron den aktuellen BlockHeader des Peers ab.
    async fn get_latest_block_header(&self) -> Result<BlockHeader>;
    /// Liefert eine eindeutige Kennung für den Peer.
    fn get_peer_id(&self) -> String;
}

/// Implementiert einen Light Client, der Blockheader von mehreren Peers abfragt
/// und einen Konsens herbeiführt. Dazu wird sichergestellt, dass mindestens
/// `threshold` Peers übereinstimmende Daten liefern. Jede Peer-Abfrage unterliegt
/// einem Timeout, um hängende oder langsame Antworten zu ignorieren.
pub struct LightClient {
    peers: Vec<Box<dyn Peer + Send + Sync>>,
    /// Mindestanzahl an Peers, die übereinstimmende Daten liefern müssen.
    threshold: usize,
    /// Timeout-Dauer für jede einzelne Peer-Anfrage.
    query_timeout: Duration,
}

impl LightClient {
    /// Erstellt einen neuen LightClient mit den angegebenen Peers, einem Konsens-Schwellenwert
    /// und einem Timeout für Peer-Abfragen.
    pub fn new(peers: Vec<Box<dyn Peer + Send + Sync>>, threshold: usize, query_timeout: Duration) -> Self {
        LightClient {
            peers,
            threshold,
            query_timeout,
        }
    }

    /// Ruft den BlockHeader von allen konfigurierten Peers ab und bildet einen Konsens.
    /// Es werden nur Antworten berücksichtigt, die innerhalb des Timeouts zurückkommen.
    pub async fn verify_latest_block(&self) -> Result<BlockHeader> {
        let mut headers_count: HashMap<String, (usize, BlockHeader)> = HashMap::new();

        // Abfrage bei allen Peers
        for peer in &self.peers {
            let peer_id = peer.get_peer_id();
            match timeout(self.query_timeout, peer.get_latest_block_header()).await {
                Ok(Ok(header)) => {
                    info!("Peer {} lieferte BlockHeader: {:?}", peer_id, header);
                    let key = header.block_hash.clone();
                    let entry = headers_count.entry(key.clone()).or_insert((0, header));
                    entry.0 += 1;
                },
                Ok(Err(e)) => {
                    warn!("Peer {} lieferte einen Fehler: {:?}", peer_id, e);
                },
                Err(e) => {
                    warn!("Peer {} antwortete nicht innerhalb des Timeouts: {:?}", peer_id, e);
                },
            }
        }

        // Bestimme den BlockHeader, der am häufigsten zurückgegeben wurde.
        if let Some((_, (count, header))) = headers_count.into_iter().max_by_key(|(_, (count, _))| *count) {
            if count >= self.threshold {
                info!("Konsens erreicht: {} Peers stimmten überein.", count);
                Ok(header)
            } else {
                Err(anyhow!("Konsens unzureichend: Nur {} Peers stimmten überein, benötigt werden mindestens {}.", count, self.threshold))
            }
        } else {
            Err(anyhow!("Keine gültigen BlockHeader von den Peers erhalten."))
        }
    }

    /// Führt periodisch eine Konsensüberprüfung durch und meldet das Ergebnis.
    pub async fn monitor_consensus(&self, interval: Duration) {
        loop {
            match self.verify_latest_block().await {
                Ok(header) => {
                    info!("Konsens erreicht: Aktueller BlockHeader: {:?}", header);
                },
                Err(e) => {
                    error!("Konsensüberprüfung fehlgeschlagen: {:?}", e);
                }
            }
            sleep(interval).await;
        }
    }
}
