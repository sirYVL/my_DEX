///////////////////////////////////////////////////////////
// my_dex/src/audit/audit_log.rs
///////////////////////////////////////////////////////////
//
// Dieses Modul implementiert ein Audit-Log f�r Handelsereignisse.
// Es werden Details wie Event-Typ (Kauf, Verkauf, Transfer),
// eindeutige Event-ID, Timestamp, Asset-ID, Menge, K�ufer und Verk�ufer
// protokolliert. Die Daten werden in einer JSON-Logdatei abgespeichert,
// sodass sie sp�ter zur Pr�fung (Audit) verwendet werden k�nnen.
///////////////////////////////////////////////////////////

use serde::{Serialize, Deserialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

/// Enum zur Darstellung des Typs eines Handelsereignisses.
#[derive(Serialize, Deserialize, Debug)]
pub enum TradeEventType {
    Buy,
    Sell,
    Transfer,
}

/// Struktur zur Darstellung eines Audit-Events f�r einen Handel.
/// - `event_id`: Eine eindeutige ID des Ereignisses.
/// - `event_type`: Der Typ des Ereignisses (Kauf, Verkauf, Transfer).
/// - `timestamp`: Der Zeitpunkt des Ereignisses in Millisekunden seit dem UNIX-Epoch.
/// - `asset_id`: Die ID des gehandelten Assets.
/// - `quantity`: Die gehandelten Menge.
/// - `buyer`: Optional der K�ufer (bei Kauf/Transfer).
/// - `seller`: Optional der Verk�ufer (bei Verkauf/Transfer).
#[derive(Serialize, Deserialize, Debug)]
pub struct TradeAuditEvent {
    pub event_id: String,
    pub event_type: TradeEventType,
    pub timestamp: u128,
    pub asset_id: String,
    pub quantity: f64,
    pub buyer: Option<String>,
    pub seller: Option<String>,
}

impl TradeAuditEvent {
    /// Erzeugt ein neues Audit-Event mit den angegebenen Parametern.
    /// Es wird eine eindeutige ID mithilfe von nanoid generiert
    /// und der aktuelle Timestamp gesetzt.
    pub fn new(
        event_type: TradeEventType,
        asset_id: &str,
        quantity: f64,
        buyer: Option<String>,
        seller: Option<String>,
    ) -> Self {
        // Generiere eine eindeutige Event-ID mithilfe der nanoid-Bibliothek.
        let event_id = nanoid::nanoid!();
        // Erhalte den aktuellen Timestamp in Millisekunden seit UNIX_EPOCH.
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis();
        TradeAuditEvent {
            event_id,
            event_type,
            timestamp,
            asset_id: asset_id.to_string(),
            quantity,
            buyer,
            seller,
        }
    }
}

/// Schreibt ein einzelnes Audit-Event in die angegebene Log-Datei.
/// Die Events werden im JSON-Format gespeichert, jeweils in einer neuen Zeile.
pub fn log_trade_event(event: &TradeAuditEvent, log_file_path: &str) -> std::io::Result<()> {
    // �ffne oder erstelle die Log-Datei im Append-Modus.
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file_path)?;
    // Serialisiere das Event in eine JSON-Zeichenkette.
    let serialized = serde_json::to_string(event)
        .expect("Fehler beim Serialisieren des Audit-Events");
    // Schreibe die JSON-Zeichenkette in die Datei, gefolgt von einem Zeilenumbruch.
    writeln!(file, "{}", serialized)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_event_creation_and_logging() {
        // Erstelle ein Beispiel-Audit-Event f�r einen Kauf.
        let event = TradeAuditEvent::new(
            TradeEventType::Buy,
            "BTC",
            0.5,
            Some("Alice".to_string()),
            Some("Bob".to_string()),
        );
        // Pr�fe, ob die Felder korrekt gesetzt wurden.
        println!("{:?}", event);
        // Versuche das Event in eine tempor�re Log-Datei zu schreiben.
        let result = log_trade_event(&event, "trade_audit_test.log");
        assert!(result.is_ok());
    }
}
