// my_dex/src/dex_logic/orders.rs
//
// Definiert Order-Struct, Asset-Enum etc.
// Nun mit einem zusätzlichen Feld valid_until (Unix-Timestamp),
// um die Order automatisch nach Ablauf zu entfernen.
//
// NEU (Sicherheitsupdate):
//  - Signatur-Felder (signature, public_key) in Order
//  - verify_signature() für kryptographische Authentifizierung
//  - is_valid_at(...) für Ablaufprüfung
//
// Weiter NEU: Wir fangen negative/Null-Werte ab und geben Err(...) zurück.

use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use ed25519_dalek::{PublicKey, Signature, Verifier};
use anyhow::{Result, anyhow}; // Für die fehlerhafte Konstruktor-Rückgabe

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Asset {
    BTC,
    LTC,
    // ... beliebig erweiterbar
}

/// Repräsentiert eine Order in deiner DEX.
/// 'valid_until' definiert, bis wann die Order aktiv ist.
/// Neu: Wir haben Signaturfelder + eine Methode verify_signature().
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Order {
    /// Eindeutige ID dieser Order
    pub order_id: String,
    /// Der Benutzer- oder Node-Name, der die Order erstellt hat
    pub user_id: String,
    /// Welche Asset wird verkauft?
    pub asset_sell: Asset,
    /// Welche Asset wird dafür gekauft?
    pub asset_buy: Asset,
    /// Menge, die verkauft werden soll (z. B. 0.10 BTC)
    pub amount_sell: f64,
    /// Preis, z. B. 100 => 1 BTC = 100 LTC
    pub price: f64,
    /// Unix-Timestamp (Sekunden) bis zu dem die Order gültig ist
    /// Danach kann/muss sie automatisch entfernt werden
    pub valid_until: u64,

    // NEU: Felder für Signatur
    pub signature: Option<Vec<u8>>,
    pub public_key: Option<Vec<u8>>,
}

impl Order {
    /// Konstruktor für eine neue Order.
    /// 'valid_until' ist ein Unix-Timestamp, z. B. "jetzt + 24 * 3600" für 24h-Gültigkeit.
    /// 
    /// NEU (1): Wir validieren negative/Null-Werte => bei Fehlschlag Err(...)
    /// NEU (2): Wir geben Result<Self> statt Self zurück.
    pub fn new(
        order_id: &str,
        user_id: &str,
        sell: Asset,
        buy: Asset,
        amt: f64,
        price: f64,
        valid_until: u64
    ) -> Result<Self> {
        if amt <= 0.0 {
            return Err(anyhow!("amount_sell must be >0 (got {})", amt));
        }
        if price <= 0.0 {
            return Err(anyhow!("price must be >0 (got {})", price));
        }
        if valid_until == 0 {
            return Err(anyhow!("valid_until cannot be 0 (indefinite?)"));
        }

        Ok(Self {
            order_id: order_id.to_string(),
            user_id: user_id.to_string(),
            asset_sell: sell,
            asset_buy: buy,
            amount_sell: amt,
            price,
            valid_until,
            signature: None,
            public_key: None,
        })
    }

    /// Prüft, ob die Signatur (falls vorhanden) gültig ist.
    /// Wir hashen z. B. (order_id + user_id + amount_sell + price + valid_until).
    /// In echter Anwendung kannst du beliebige oder alle Felder reinnehmen.
    pub fn verify_signature(&self) -> bool {
        let (Some(sig_bytes), Some(pk_bytes)) = (self.signature.as_ref(), self.public_key.as_ref()) else {
            return false;
        };
        let Ok(pubkey)    = PublicKey::from_bytes(pk_bytes) else {
            return false;
        };
        let Ok(signature) = Signature::from_bytes(sig_bytes) else {
            return false;
        };

        // Erzeuge Hash => Minimale Felder
        let data = format!("{}:{}:{}:{}:{}",
            self.order_id,
            self.user_id,
            self.amount_sell,
            self.price,
            self.valid_until
        );
        let mut hasher = Sha256::new();
        hasher.update(data.as_bytes());
        let hashed = hasher.finalize();

        pubkey.verify(&hashed, &signature).is_ok()
    }

    /// Gibt zurück, ob die Order zum angegebenen Zeitpunkt noch gültig ist.
    /// => so kannst du abgelaufene Orders aussortieren.
    pub fn is_valid_at(&self, now: u64) -> bool {
        now < self.valid_until
    }
}
