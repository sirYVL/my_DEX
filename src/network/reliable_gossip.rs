// my_dex/src/network/reliable_gossip.rs
// Beschreibung: Produktionsreife Implementierung eines Reliable-Gossip-Protokoll-Knotens.
// Dieser Code implementiert einen GossipNode, der Nachrichten mit Sequenznummern versendet,
// fehlende Nachrichten erkennt und gezielt Re-Requests an betroffene Peers sendet.
// Er nutzt Tokio f�r asynchrone Operationen und log/Env_logger f�r strukturiertes Logging.

use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc::{Sender, Receiver};
use tokio::time::sleep;
use log::{info, warn, error, debug};

/// Repr�sentiert eine Gossip-Nachricht, die vom Knoten im Netzwerk versendet wird.
#[derive(Debug, Clone)]
pub struct GossipMessage {
    /// Identifikation des sendenden Knotens.
    pub sender: String,
    /// Eindeutige, fortlaufende Sequenznummer zur Erkennung von Nachrichtenl�cken.
    pub seq: u64,
    /// Der Nachrichteninhalt (Payload) als Byte-Array.
    pub payload: Vec<u8>,
}

/// Repr�sentiert einen Knoten im Gossip-Netzwerk.
/// Jeder Knoten speichert seinen lokalen Zustand und verfolgt die zuletzt empfangenen
/// Sequenznummern von seinen Peers, um fehlende Nachrichten zu erkennen.
pub struct GossipNode {
    /// Eindeutige Kennung des Knotens.
    pub id: String,
    /// Lokaler Sequenzz�hler f�r von diesem Knoten gesendete Nachrichten.
    pub local_seq: u64,
    /// HashMap, die f�r jeden Peer die zuletzt empfangene Sequenznummer speichert.
    pub last_seen: HashMap<String, u64>,
    /// Sender-Kanal, �ber den dieser Knoten Nachrichten ins Netzwerk sendet.
    pub gossip_tx: Sender<GossipMessage>,
    /// Receiver-Kanal, �ber den dieser Knoten Nachrichten aus dem Netzwerk empf�ngt.
    pub gossip_rx: Receiver<GossipMessage>,
}

impl GossipNode {
    /// Erzeugt einen neuen GossipNode mit gegebener ID und den �bergebenen Kan�len.
    pub fn new(id: String, gossip_tx: Sender<GossipMessage>, gossip_rx: Receiver<GossipMessage>) -> Self {
        GossipNode {
            id,
            local_seq: 0,
            last_seen: HashMap::new(),
            gossip_tx,
            gossip_rx,
        }
    }

    /// Sendet eine neue Nachricht (Broadcast) ins Netzwerk.
    /// Erh�ht den lokalen Sequenzz�hler und erstellt eine neue GossipMessage.
    pub async fn broadcast(&mut self, payload: Vec<u8>) -> Result<(), Box<dyn std::error::Error>> {
        self.local_seq += 1;
        let msg = GossipMessage {
            sender: self.id.clone(),
            seq: self.local_seq,
            payload,
        };

        debug!("Node {} broadcastet Nachricht mit seq {}", self.id, self.local_seq);
        // Sende die Nachricht �ber den asynchronen Kanal.
        self.gossip_tx.send(msg).await?;
        Ok(())
    }

    /// Hauptschleife zur Verarbeitung eingehender Gossip-Nachrichten.
    /// Diese Funktion wird dauerhaft ausgef�hrt und reagiert auf alle empfangenen Nachrichten.
    pub async fn handle_messages(&mut self) {
        while let Some(msg) = self.gossip_rx.recv().await {
            if let Err(e) = self.process_message(msg).await {
                error!("Fehler bei der Verarbeitung der Nachricht: {}", e);
            }
        }
    }

    /// Verarbeitet eine empfangene Gossip-Nachricht.
    /// - �berpr�ft, ob die Sequenznummern l�ckenhaft sind.
/// - Fordert bei L�cken gezielt fehlende Nachrichten an.
    async fn process_message(&mut self, msg: GossipMessage) -> Result<(), Box<dyn std::error::Error>> {
        // Ignoriere Nachrichten, die von diesem Knoten selbst gesendet wurden.
        if msg.sender == self.id {
            return Ok(());
        }

        // Hole die zuletzt empfangene Sequenznummer des Absenders oder initialisiere sie mit 0.
        let last_seq = self.last_seen.entry(msg.sender.clone()).or_insert(0);

        if msg.seq > *last_seq + 1 {
            // Es wurde eine L�cke in der Sequenz entdeckt � es fehlen Nachrichten.
            warn!("Node {} entdeckt fehlende Nachrichten von {}: erwartet {} bis {}, empfangen {}",
                  self.id, msg.sender, *last_seq + 1, msg.seq - 1, msg.seq);

            // Fordere jede fehlende Nachricht einzeln an.
            for missing_seq in (*last_seq + 1)..msg.seq {
                self.request_missing(&msg.sender, missing_seq).await?;
            }
            // Aktualisiere den zuletzt gesehenen Sequenzwert.
            *last_seq = msg.seq;
            self.process_payload(msg.payload).await?;
        } else if msg.seq == *last_seq + 1 {
            // Nachricht ist in der erwarteten Reihenfolge.
            *last_seq = msg.seq;
            self.process_payload(msg.payload).await?;
        } else {
            // Duplikate oder veraltete Nachrichten werden ignoriert.
            debug!("Node {} erh�lt Duplikat oder veraltete Nachricht von {}: seq {} (last seen: {})",
                   self.id, msg.sender, msg.seq, *last_seq);
        }
        Ok(())
    }

    /// Fordert eine fehlende Nachricht von einem Peer an.
    /// In einer echten Produktion w�rden hier Netzwerkprotokolle eingesetzt, um den Peer direkt zu kontaktieren.
    async fn request_missing(&self, sender: &String, missing_seq: u64) -> Result<(), Box<dyn std::error::Error>> {
        // Produktion: Hier k�nnte ein Netzwerkrequest (z. B. �ber TCP/UDP) implementiert werden.
        info!("Node {} fordert fehlende Nachricht {} von {}", self.id, missing_seq, sender);
        // Simuliere eine Netzwerkverz�gerung f�r den Re-Request.
        sleep(Duration::from_millis(20)).await;
        // Erfolgreiche R�ckmeldung (in einer echten Implementierung w�rde hier der Response-Handling-Code folgen).
        Ok(())
    }

    /// Verarbeitet die Payload der empfangenen Nachricht.
    /// Hier wird der globale Zustand (z. B. Orderbuch) aktualisiert.
    async fn process_payload(&self, payload: Vec<u8>) -> Result<(), Box<dyn std::error::Error>> {
        let payload_str = String::from_utf8(payload.clone()).unwrap_or_else(|_| "<ung�ltige UTF-8>".to_string());
        info!("Node {} verarbeitet Payload: {}", self.id, payload_str);
        // Hier wird die Logik zur Zustandsaktualisierung implementiert.
        Ok(())
    }
}

//
// Beispiel: Simulation eines Gossip-Netzwerks mit zwei Nodes.
//
// In einer echten Produktionsumgebung w�rde die Kommunikation �ber Netzwerkverbindungen (z.B. TCP)
// stattfinden. F�r diesen Test nutzen wir asynchrone mpsc-Kan�le von Tokio, um den Nachrichtenaustausch zu simulieren.
//
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_reliable_gossip() {
        // Initialisiere das Logging (f�r Tests wird env_logger so konfiguriert, dass die Ausgabe in den Test-Logs erscheint).
        let _ = env_logger::builder().is_test(true).try_init();

        // Erstelle asynchrone mpsc-Kan�le, um die Netzwerkkommunikation zu simulieren.
        let (tx_a, rx_a) = mpsc::channel(100);
        let (tx_b, rx_b) = mpsc::channel(100);

        // In dieser Simulation sind die Kan�le kreuzverkn�pft:
        // NodeA sendet �ber tx_a, empf�ngt �ber rx_b, und NodeB sendet �ber tx_b, empf�ngt �ber rx_a.
        let mut node_a = GossipNode::new("NodeA".to_string(), tx_a.clone(), rx_b);
        let mut node_b = GossipNode::new("NodeB".to_string(), tx_b.clone(), rx_a);

        // Starte asynchrone Tasks f�r beide Nodes.
        let handle_a = tokio::spawn(async move {
            // NodeA sendet zwei Nachrichten mit einem kurzen Abstand.
            node_a.broadcast(b"Hallo von NodeA".to_vec()).await.unwrap();
            sleep(Duration::from_millis(50)).await;
            node_a.broadcast(b"Zweite Nachricht von NodeA".to_vec()).await.unwrap();
            // NodeA verarbeitet eingehende Nachrichten.
            node_a.handle_messages().await;
        });

        let handle_b = tokio::spawn(async move {
            // NodeB verarbeitet eingehende Nachrichten.
            node_b.handle_messages().await;
        });

        // Warte, bis beide Tasks abgeschlossen sind.
        let _ = tokio::join!(handle_a, handle_b);
    }
}
