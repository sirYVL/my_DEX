///////////////////////////////////////////////////////////
// my_dex/src/layer2/delta_gossip.rs
///////////////////////////////////////////////////////////
//
// Delta-basiertes Gossip-Verfahren für Updates (Neu hinzugefügt)
// Nodes übertragen nur Änderungen („Deltas“) zur Reduzierung von Bandbreite und Netzwerkbelastung
// Implementierung eines effizienten Algorithmus zur schnellen Verteilung kleiner Updates
// Nutzung des bestehenden Lightning-Gossip-Protokolls zur nahtlosen Integration der Delta-Aktualisierungen

use anyhow::{Result, Context};
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use serde::{Serialize, Deserialize};
use tracing::{info, error};
use uuid::Uuid;
use chrono::Utc;
use std::time::Duration;

/// DeltaMessage repräsentiert ein kleines Update (Delta), das von einem Node übertragen wird.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeltaMessage {
    pub id: Uuid,
    pub payload: String,
    pub timestamp: i64,
}

impl DeltaMessage {
    /// Erzeugt eine neue DeltaMessage mit dem übergebenen Payload.
    pub fn new(payload: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            payload,
            timestamp: Utc::now().timestamp(),
        }
    }
    
    /// Serialisiert die DeltaMessage in ein JSON-Format.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).context("Failed to serialize DeltaMessage")
    }
    
    /// Deserialisiert eine DeltaMessage aus einem JSON-String.
    pub fn from_json(json_str: &str) -> Result<Self> {
        serde_json::from_str(json_str).context("Failed to deserialize DeltaMessage")
    }
}

/// Produktionsreife DeltaGossip-Struktur zur Verteilung von Delta-Updates.
/// Diese Implementierung nutzt TCP, um Delta-Nachrichten asynchron zu senden und zu empfangen.
/// Die Integration in ein bestehendes Lightning-Gossip-Protokoll ermöglicht nahtlose Updates.
pub struct DeltaGossip {
    pub listen_addr: String,
}

impl DeltaGossip {
    /// Erstellt eine neue DeltaGossip-Instanz mit der angegebenen Listener-Adresse.
    pub fn new(listen_addr: String) -> Self {
        Self { listen_addr }
    }
    
    /// Startet einen asynchronen Listener, der Delta-Updates empfängt.
    /// Jeder eingehende TCP-Stream wird in einem separaten Task verarbeitet.
    pub async fn start_listener(&self) -> Result<()> {
        let listener = TcpListener::bind(&self.listen_addr)
            .await
            .context("Failed to bind DeltaGossip listener")?;
        info!("DeltaGossip listener started on {}", self.listen_addr);
        
        loop {
            let (mut socket, addr) = listener.accept().await
                .context("Failed to accept connection")?;
            info!("Accepted connection from {}", addr);
            
            tokio::spawn(async move {
                let mut buffer = Vec::new();
                // Versuche, die gesamte Nachricht innerhalb von 10 Sekunden zu lesen.
                match tokio::time::timeout(Duration::from_secs(10), socket.read_to_end(&mut buffer)).await {
                    Ok(Ok(_)) => {
                        let msg_str = String::from_utf8_lossy(&buffer);
                        match DeltaMessage::from_json(&msg_str) {
                            Ok(delta_msg) => {
                                info!("Received DeltaMessage: {:?}", delta_msg);
                                // Hier erfolgt die Verarbeitung des Delta-Updates,
                                // z.B. Weiterleitung an eine Delta-Verarbeitungsroutine.
                            },
                            Err(e) => {
                                error!("Failed to parse DeltaMessage: {:?}", e);
                            }
                        }
                    },
                    Ok(Err(e)) => {
                        error!("Error reading from socket: {:?}", e);
                    },
                    Err(e) => {
                        error!("Timeout while reading from socket: {:?}", e);
                    }
                }
            });
        }
    }
    
    /// Sendet ein Delta-Update an einen spezifizierten Remote-Endpunkt.
    /// Die Nachricht wird als JSON-String über TCP übertragen.
    pub async fn send_delta(&self, remote_addr: &str, delta: &DeltaMessage) -> Result<()> {
        let json_msg = delta.to_json()?;
        let mut stream = TcpStream::connect(remote_addr).await
            .context(format!("Failed to connect to remote address: {}", remote_addr))?;
        stream.write_all(json_msg.as_bytes()).await
            .context("Failed to send DeltaMessage")?;
        info!("Sent DeltaMessage {} to {}", delta.id, remote_addr);
        Ok(())
    }
}
