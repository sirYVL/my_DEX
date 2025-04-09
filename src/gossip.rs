// Folder: src
// File: gossip.rs

use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tokio::time::sleep;
use tracing::{info, warn};

/// Struktur, die eine Fehlermeldung (FaultMessage) repräsentiert.
/// Sie enthält wichtige Informationen, um eine Störung eindeutig zu identifizieren.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FaultMessage {
    pub node_id: String,
    pub fault_type: String,
    pub timestamp: DateTime<Utc>,
    pub log_excerpt: String,
    pub severity: String, // z.B. "critical", "warning"
    pub signature: Option<String>, // Optional: digitale Signatur zur Validierung
}

impl FaultMessage {
    /// Erzeugt eine neue FaultMessage.
    pub fn new(
        node_id: String,
        fault_type: String,
        log_excerpt: String,
        severity: String,
        signature: Option<String>,
    ) -> Self {
        FaultMessage {
            node_id,
            fault_type,
            timestamp: Utc::now(),
            log_excerpt,
            severity,
            signature,
        }
    }
}

/// GossipManager verwaltet den Versand und Empfang von Gossip-Nachrichten sowie einen lokalen Cache.
/// Im Cache werden die Nachrichten mit einer definierten Time-To-Live (TTL) gespeichert.
pub struct GossipManager {
    /// Sender-Channel zum Versenden von Nachrichten.
    pub sender: mpsc::Sender<FaultMessage>,
    /// Receiver-Channel zum Empfangen von Nachrichten.
    pub receiver: mpsc::Receiver<FaultMessage>,
    /// Lokaler Cache zur Speicherung von Nachrichten mit Ablaufzeit.
    cache: RwLock<HashMap<String, (FaultMessage, Instant)>>,
    /// Time-To-Live für Nachrichten im Cache.
    ttl: Duration,
}

impl GossipManager {
    /// Erzeugt einen neuen GossipManager mit gegebener TTL und Channel-Kapazität.
    pub fn new(ttl: Duration, channel_capacity: usize) -> Self {
        let (sender, receiver) = mpsc::channel(channel_capacity);
        GossipManager {
            sender,
            receiver,
            cache: RwLock::new(HashMap::new()),
            ttl,
        }
    }

    /// Broadcastet eine FaultMessage an alle Peers und speichert sie im lokalen Cache.
    pub async fn broadcast(&self, msg: FaultMessage) -> Result<(), String> {
        // Serialisiere die Nachricht als Schlüssel für den Cache
        let serialized = serde_json::to_string(&msg).map_err(|e| e.to_string())?;
        let expiration = Instant::now() + self.ttl;
        {
            let mut cache = self.cache.write().await;
            cache.insert(serialized.clone(), (msg.clone(), expiration));
        }
        // Sende die Nachricht über den Channel
        self.sender.send(msg).await.map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Verarbeitet eingehende Nachrichten aus dem Receiver-Channel.
    /// Nachrichten werden serialisiert und im Cache gespeichert, falls sie nicht schon vorhanden sind.
    pub async fn process_incoming(&self) {
        while let Some(msg) = self.receiver.recv().await {
            info!("Received gossip message: {:?}", msg);
            let serialized = match serde_json::to_string(&msg) {
                Ok(s) => s,
                Err(e) => {
                    warn!("Error serializing message: {}", e);
                    continue;
                }
            };
            {
                let mut cache = self.cache.write().await;
                if !cache.contains_key(&serialized) {
                    let expiration = Instant::now() + self.ttl;
                    cache.insert(serialized, (msg, expiration));
                }
            }
        }
    }

    /// Bereinigt den Cache regelmäßig von abgelaufenen Nachrichten.
    pub async fn cleanup_cache(&self) {
        loop {
            sleep(Duration::from_secs(10)).await; // Bereinigung alle 10 Sekunden
            let now = Instant::now();
            let mut cache = self.cache.write().await;
            let before = cache.len();
            cache.retain(|_, &mut (_, exp)| exp > now);
            let after = cache.len();
            if before != after {
                info!("Cache cleanup: {} messages removed", before - after);
            }
        }
    }
}

/// Dummy-Funktion zum Broadcasten einer Fehlermeldung über das Gossip-Protokoll.
/// In einer echten Implementierung würde diese Funktion die Nachricht an alle Peers senden.
pub async fn broadcast_fault_message(msg: FaultMessage) {
    println!("Broadcasting Fault Message: {:?}", msg);
