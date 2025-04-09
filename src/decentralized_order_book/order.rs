// my_dex/src/decentralized_order_book/order.rs

use serde::{Serialize, Deserialize};
use std::time::{SystemTime, UNIX_EPOCH};
use std::fmt;

/// Art der Order (Market, Limit, Stop)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    Market,
    Limit(f64),
    Stop(f64),
}

/// Kauf- oder Verkaufsorder
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderSide {
    Buy,
    Sell,
}

/// Status der Order
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderStatus {
    Open,
    PartiallyFilled,
    Filled,
    Cancelled,
}

/// Die primäre Order-Struktur
/// - `id`: Eindeutige Order-ID
/// - `user_id`: Zuordnung zum Besitzer
/// - `timestamp`: Unix-Sekunden, wann die Order erstellt wurde
/// - `order_type`: Market, Limit, oder Stop
/// - `side`: Buy oder Sell
/// - `quantity`: Gewünschte Gesamtmenge
/// - `filled_quantity`: Bereits ausgeführte Menge
/// - `status`: Open, PartiallyFilled, Filled, Cancelled
///
/// Neu: Felder `signature` und `pub_key` für die Authentizität.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Order {
    pub id: String,
    pub user_id: String,
    pub timestamp: u64,
    pub order_type: OrderType,
    pub side: OrderSide,
    pub quantity: f64,
    pub filled_quantity: f64,
    pub status: OrderStatus,
    // Neu: Signatur
    pub signature: Option<Vec<u8>>,
    pub pub_key: Option<Vec<u8>>,
}

impl Order {
    /// Erstellt eine neue Order mit automatisch generierter ID und Zeitstempel
    pub fn new(user_id: &str, order_type: OrderType, side: OrderSide, quantity: f64) -> Self {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let unique_id = format!("{}_{}", user_id, now);
        Self {
            id: unique_id,
            user_id: user_id.to_string(),
            timestamp: now,
            order_type,
            side,
            quantity,
            filled_quantity: 0.0,
            status: OrderStatus::Open,
            signature: None,
            pub_key: None,
        }
    }

    /// Gibt an, wie viel von der Order noch offen ist
    pub fn remaining_quantity(&self) -> f64 {
        self.quantity - self.filled_quantity
    }

    /// Führt die Order (teilweise oder vollständig) aus
    pub fn fill(&mut self, amount: f64) {
        self.filled_quantity += amount;
        if self.remaining_quantity() <= 0.0 {
            self.status = OrderStatus::Filled;
        } else {
            self.status = OrderStatus::PartiallyFilled;
        }
    }

    /// Cancelt die Order, falls sie noch offen oder teilweise gefüllt ist
    pub fn cancel(&mut self) {
        if matches!(self.status, OrderStatus::Open | OrderStatus::PartiallyFilled) {
            self.status = OrderStatus::Cancelled;
        }
    }

    /// Minimalbeispiel für Signaturprüfung.
    /// Du müsstest in einer produktiven Umgebung
    /// - pub_key => ed25519_dalek::PublicKey
    /// - signature => ed25519_dalek::Signature
    /// - message = "id+user_id+timestamp+quantity" (Hash)
    /// verarbeiten und verifizieren.
    pub fn verify_signature(&self) -> bool {
        if let (Some(sig_bytes), Some(pk_bytes)) = (self.signature.as_ref(), self.pub_key.as_ref()) {
            // Beispiel: Wir checken nur, dass sign. + pub_key existieren.
            // In einer echten Implementation => ed25519_dalek::PublicKey::from_bytes(pk_bytes)
            // => pk.verify(msg, &Signature::from_bytes(sig_bytes)) etc.
            // Hier nur Dummy:
            !sig_bytes.is_empty() && !pk_bytes.is_empty()
        } else {
            false
        }
    }
}

impl fmt::Display for Order {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Order[{}] user={} type={:?} side={:?} qty={} fill={} status={:?}, signed={}",
            self.id, self.user_id, self.order_type, self.side,
            self.quantity, self.filled_quantity, self.status,
            self.signature.is_some()
        )
    }
}
