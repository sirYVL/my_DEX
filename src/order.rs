///////////////////////////////////////////////////////////
// my_dex/src/order.rs
///////////////////////////////////////////////////////////
//
// Dieses Modul implementiert digitale Signaturen für Orders und Cancel-Nachrichten,
// um Manipulationen im Orderbuch zu verhindern. Jede Order wird vom Ersteller
// (User) signiert. Beim Empfang wird die Signatur gegen den bekannten öffentlichen
// Schlüssel des Users geprüft. Dadurch wird sichergestellt, dass nur authentische
// Orders ins Orderbuch gelangen und auch Löschungen (Cancel-Orders) nur von den
// Eigentümern initiiert werden können.
use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Keypair, PublicKey, Signature, Signer, Verifier};
use serde::{Serialize, Deserialize};

/// Definiert den Ordertyp (Buy oder Sell).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum OrderType {
    Buy,
    Sell,
}

/// Repräsentiert eine Order im DEX-System.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Order {
    pub order_id: u64,
    pub user_id: String,
    pub order_type: OrderType,
    pub amount: f64,
    pub price: f64,
    pub timestamp: DateTime<Utc>,
    /// Die digitale Signatur der Order (optional, bis sie gesetzt wird)
    pub signature: Option<Signature>,
}

impl Order {
    /// Erstellt eine neue Order ohne Signatur. Der Zeitstempel wird automatisch gesetzt.
    pub fn new(order_id: u64, user_id: String, order_type: OrderType, amount: f64, price: f64) -> Self {
        Order {
            order_id,
            user_id,
            order_type,
            amount,
            price,
            timestamp: Utc::now(),
            signature: None,
        }
    }

    /// Signiert die Order mit dem gegebenen Keypair. Dabei wird eine
    /// Signatur über einen konsistenten Nachrichten-String (Order-ID, User-ID, OrderType,
    /// Amount, Price und Timestamp) erzeugt.
    pub fn sign(&mut self, keypair: &Keypair) -> Result<()> {
        // In der Produktion sollte sichergestellt werden, dass der User (user_id)
        // dem Schlüssel des Keypairs entspricht.
        let message = self.message_to_sign();
        let signature = keypair.sign(message.as_bytes());
        self.signature = Some(signature);
        Ok(())
    }

    /// Überprüft die digitale Signatur der Order anhand des bekannten öffentlichen Schlüssels.
    pub fn verify(&self, public_key: &PublicKey) -> Result<()> {
        if let Some(signature) = &self.signature {
            let message = self.message_to_sign();
            public_key.verify(message.as_bytes(), signature)
                .map_err(|e| anyhow!("Order-Signatur-Überprüfung fehlgeschlagen: {:?}", e))
        } else {
            Err(anyhow!("Order besitzt keine Signatur"))
        }
    }

    /// Erzeugt den Nachrichtentext, der signiert bzw. überprüft wird.
    fn message_to_sign(&self) -> String {
        // Der String enthält alle relevanten Felder. Änderungen an einem dieser Felder
        // führen zu einer ungültigen Signatur.
        format!("{}|{}|{:?}|{}|{}|{}", 
            self.order_id, 
            self.user_id, 
            self.order_type, 
            self.amount, 
            self.price, 
            self.timestamp.timestamp()
        )
    }
}

/// Repräsentiert eine Cancel-Order-Nachricht, mit der eine zuvor aufgegebene Order storniert wird.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CancelOrder {
    pub order_id: u64,
    pub user_id: String,
    pub timestamp: DateTime<Utc>,
    /// Digitale Signatur der Cancel-Nachricht (optional, bis sie gesetzt wird)
    pub signature: Option<Signature>,
}

impl CancelOrder {
    /// Erstellt eine neue CancelOrder-Nachricht ohne Signatur.
    pub fn new(order_id: u64, user_id: String) -> Self {
        CancelOrder {
            order_id,
            user_id,
            timestamp: Utc::now(),
            signature: None,
        }
    }

    /// Signiert die CancelOrder-Nachricht mit dem gegebenen Keypair.
    pub fn sign(&mut self, keypair: &Keypair) -> Result<()> {
        let message = self.message_to_sign();
        let signature = keypair.sign(message.as_bytes());
        self.signature = Some(signature);
        Ok(())
    }

    /// Überprüft die Signatur der CancelOrder anhand des öffentlichen Schlüssels.
    pub fn verify(&self, public_key: &PublicKey) -> Result<()> {
        if let Some(signature) = &self.signature {
            let message = self.message_to_sign();
            public_key.verify(message.as_bytes(), signature)
                .map_err(|e| anyhow!("CancelOrder-Signatur-Überprüfung fehlgeschlagen: {:?}", e))
        } else {
            Err(anyhow!("CancelOrder besitzt keine Signatur"))
        }
    }

    /// Erzeugt den Nachrichtentext für die Signatur der CancelOrder.
    fn message_to_sign(&self) -> String {
        format!("{}|{}|{}", self.order_id, self.user_id, self.timestamp.timestamp())
    }
}
