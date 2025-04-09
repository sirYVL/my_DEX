///////////////////////////////////////////////////////////
// my_dex/src/layer2/fees.rs
///////////////////////////////////////////////////////////
//
// Gebührenmodell und Verteilung
// Einrichtung eines zentralen, dezentral gesicherten Layer-2-Gebührenpools:
// Automatische Erhebung geringer Gebühren pro Trade
// Aufteilung: 20–30 % Entwicklerteam / 70–80 % Fullnode-Betreiber
// Wöchentliche automatische Auszahlung und optionale finale Auszahlung on-chain (Nutzer trägt Gebühren)

use anyhow::{Result, anyhow};
use tracing::info;
use std::sync::{Mutex, Arc};
use tokio::time::{interval, Duration};
use tokio::sync::RwLock;

/// FeePool sammelt alle anfallenden Gebühren aus den Trades.
pub struct FeePool {
    total_fees: Mutex<u64>,
    dev_share: u8,  // Anteil für das Entwicklerteam in Prozent
    node_share: u8, // Anteil für die Fullnode-Betreiber in Prozent
}

impl FeePool {
    /// Erzeugt einen neuen FeePool mit einem Startwert und den prozentualen Anteilen.
    pub fn new(initial_fees: u64, dev_share: u8, node_share: u8) -> Self {
        Self {
            total_fees: Mutex::new(initial_fees),
            dev_share,
            node_share,
        }
    }

    /// Fügt eine Gebühr (in kleinen Einheiten) zum Pool hinzu.
    pub fn add_fee(&self, fee: u64) -> Result<()> {
        let mut total = self.total_fees.lock().unwrap();
        *total += fee;
        info!("Gebühr hinzugefügt. Aktuelle Gesamtgebühren: {}", *total);
        Ok(())
    }

    /// Berechnet die Beträge, die an das Entwicklerteam und an die Node-Betreiber ausgezahlt werden.
    pub fn calculate_distribution(&self) -> (u64, u64) {
        let total = *self.total_fees.lock().unwrap();
        let dev_amount = total * (self.dev_share as u64) / 100;
        let node_amount = total * (self.node_share as u64) / 100;
        (dev_amount, node_amount)
    }

    /// Setzt den Gebührenpool nach erfolgter Auszahlung zurück.
    pub fn reset(&self) -> Result<()> {
        let mut total = self.total_fees.lock().unwrap();
        *total = 0;
        Ok(())
    }

    /// Führt die Verteilung der Gebühren durch, wenn mindestens 80 % der aktuell online befindlichen Nodes zustimmen.
    ///
    /// # Parameter
    /// - `online_nodes`: Anzahl der aktuell online befindlichen Fullnodes.
    /// - `approvals`: Anzahl der Nodes, die der Auszahlung zugestimmt haben.
    ///
    /// # Rückgabe
    /// Gibt die ausgezahlten Beträge als Tupel (Developer, Nodes) zurück.
    pub fn distribute(&self, online_nodes: usize, approvals: usize) -> Result<(u64, u64)> {
        // Erforderliche Zustimmung: 80 % der online Nodes
        let required = ((online_nodes as f64) * 0.8).ceil() as usize;
        if approvals < required {
            return Err(anyhow!("Nicht genügend Zustimmungen: {} von {} (erforderlich: {})", approvals, online_nodes, required));
        }
        let (dev_amount, node_amount) = self.calculate_distribution();
        info!("Gebührenverteilung: Entwickler erhalten {}, Node-Betreiber erhalten {}", dev_amount, node_amount);
        // Hier würden in einem echten System On-Chain-Transaktionen oder interne Buchungen erfolgen.
        self.reset()?;
        Ok((dev_amount, node_amount))
    }
}

/// Startet einen Hintergrundtask, der wöchentlich (simuliert) die Gebührenverteilung ausführt.
/// In einem echten System würde diese Funktion einmal pro Woche ausgeführt.
/// Hier simulieren wir dies mit einer kürzeren Zeitspanne (z. B. 60 Sekunden) für Testzwecke.
///
/// - `fee_pool`: Referenz auf den FeePool.
/// - `online_nodes`: Ein RwLock, der die Anzahl der aktuell online befindlichen Nodes enthält.
/// - `approvals_fn`: Eine Funktion, die die Anzahl der Nodes zurückgibt, die der Auszahlung zugestimmt haben.
pub async fn start_weekly_fee_distribution(
    fee_pool: Arc<FeePool>,
    online_nodes: Arc<RwLock<usize>>,
    approvals_fn: impl Fn() -> usize + Send + Sync + 'static,
) {
    // Simulation: 60 Sekunden entsprechen einer Woche in unserem Testsetup.
    let mut interval = interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        let nodes = *online_nodes.read().await;
        let approvals = approvals_fn();
        match fee_pool.distribute(nodes, approvals) {
            Ok((dev, node)) => {
                info!("Wöchentliche Gebührenverteilung durchgeführt: Entwickler: {}, Node-Betreiber: {}", dev, node);
                // Optionale finale Auszahlung on-chain kann hier initiiert werden.
            },
            Err(e) => {
                info!("Gebührenverteilung zurückgestellt: {}", e);
            }
        }
    }
}
