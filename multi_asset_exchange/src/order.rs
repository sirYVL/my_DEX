// my_dex/multi_asset_exchange/src/order.rs

use serde::{Serialize, Deserialize};
use std::time::{SystemTime, UNIX_EPOCH};
use std::fmt;

// Neu: Wir fügen eine rudimentäre Signatur-Logik hinzu
// In echter Produktion: ECDSA/Ed25519 + validated user identity
use ed25519_dalek::{Keypair, PublicKey, Signature, Signer, Verifier};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderStatus {
    Open,
    PartiallyFilled,
    Filled,
    Cancelled,
}

/// Wir fügen hier optional StopLimit hinzu, um Market/Limit/Stop/StopLimit zu vereinheitlichen
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum OrderType {
    Market,
    Limit(f64),
    Stop(f64),
}

/// Falls du in diesem Projekt mehr brauchen würdest (StopLimit, etc.),
// könntest du es hier erweitern.

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Order {
    pub id: String,
    pub user_id: String,
    pub timestamp: u64,
    pub side: OrderSide,
    /// Gesamtmenge (Basis-Einheit: z. B. 1.0 BTC). Intern wandeln wir das in subunits um,
    /// hier lassen wir es als f64, weil wir's in der Order nur deklarieren.
    pub base_quantity: f64,
    pub filled_quantity: f64,
    pub order_type: OrderType,
    pub status: OrderStatus,

    // Neu: Ggf. Signatur-Felder, um die Authentizität der Order zu prüfen
    pub signature: Option<Vec<u8>>,
    pub public_key: Option<Vec<u8>>,
}

impl Order {
    /// Erzeugt eine neue Order (ohne Signatur).
    /// In einer Produktion sollte man besser `new_with_signature` nutzen oder die Signatur später einfügen.
    pub fn new(
        user_id: &str,
        side: OrderSide,
        base_quantity: f64,
        order_type: OrderType,
        base_asset: &str,  // ignoriert, da hier nicht mehr genutzt
        quote_asset: &str, // ignoriert, da hier nicht mehr genutzt
    ) -> Self {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let id = format!("{}_{}", user_id, now);
        Self {
            id,
            user_id: user_id.to_string(),
            timestamp: now,
            side,
            base_quantity,
            filled_quantity: 0.0,
            order_type,
            status: OrderStatus::Open,
            signature: None,
            public_key: None,
        }
    }

    /// Erzeugt eine neue Order mit Signaturfeldern, um bösartige Knoten/Manipulationen zu erschweren.
    pub fn new_with_signature(
        user_id: &str,
        side: OrderSide,
        base_quantity: f64,
        order_type: OrderType,
        public_key: Vec<u8>,
        signature: Vec<u8>,
    ) -> Self {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let id = format!("{}_{}", user_id, now);
        Self {
            id,
            user_id: user_id.to_string(),
            timestamp: now,
            side,
            base_quantity,
            filled_quantity: 0.0,
            order_type,
            status: OrderStatus::Open,
            signature: Some(signature),
            public_key: Some(public_key),
        }
    }

    pub fn remaining_quantity(&self) -> f64 {
        self.base_quantity - self.filled_quantity
    }

    pub fn fill(&mut self, amount: f64) {
        self.filled_quantity += amount;
        if self.filled_quantity >= self.base_quantity {
            self.status = OrderStatus::Filled;
        } else {
            self.status = OrderStatus::PartiallyFilled;
        }
    }

    pub fn cancel(&mut self) {
        if matches!(self.status, OrderStatus::Open | OrderStatus::PartiallyFilled) {
            self.status = OrderStatus::Cancelled;
        }
    }

    /// Minimalbeispiel, wie man Signaturen überprüfen könnte.
    /// In echter Produktion => robustere Hasher, konstanter Datensatz, ECDSA/Ed25519, etc.
    pub fn verify_signature(&self) -> bool {
        // Falls Order keine Signatur oder public_key hat => false
        let (Some(sig_bytes), Some(pub_bytes)) = (self.signature.as_ref(), self.public_key.as_ref()) else {
            return false;
        };

        if let Ok(pk) = PublicKey::from_bytes(pub_bytes) {
            if let Ok(sig) = Signature::from_bytes(sig_bytes) {
                // Example: Hash aus (id + user_id + base_quantity + timestamp)
                let to_sign = format!("{}:{}:{}:{}", self.id, self.user_id, self.base_quantity, self.timestamp);
                return pk.verify(to_sign.as_bytes(), &sig).is_ok();
            }
        }
        false
    }
}

impl fmt::Display for Order {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Order[{}] user={} side={:?} base_qty={} filled={} type={:?} status={:?}",
            self.id, self.user_id, self.side, self.base_quantity, self.filled_quantity,
            self.order_type, self.status
        )
    }
}
