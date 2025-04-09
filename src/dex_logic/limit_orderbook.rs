// my_dex/src/dex_logic/limit_orderbook.rs

use std::collections::{BTreeMap, VecDeque};
use crate::dex_logic::orders::{Order, Asset};
// NEU: Für Sicherheitsfehler
use crate::error::DexError;

// Für den Mutex-Lock (Concurrency):
use std::sync::Mutex;
use lazy_static::lazy_static;

/// Zwei Handelsseiten
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

/// In diesem LimitOrder struct fehlt bisher eine Signaturprüfung.
/// NEU: Wir fügen Felder für Signatur/PublicKey hinzu und eine verify_signature().
#[derive(Clone, Debug)]
pub struct LimitOrder {
    pub order_id: String,
    pub side: Side,
    pub price: f64,
    pub quantity: f64,
    pub user_id: String,

    // NEU: Signaturfelder
    pub signature: Option<Vec<u8>>,
    pub public_key: Option<Vec<u8>>,
}

impl LimitOrder {
    /// Prüft, ob signiert + Hash korrekt.
    /// Minimalbeispiel (ähnlich wie in anderen Dateien).
    pub fn verify_signature(&self) -> bool {
        use ed25519_dalek::{PublicKey, Signature, Verifier};
        use sha2::{Sha256, Digest};

        // Falls keins von beiden existiert => false
        let (Some(sig_bytes), Some(pk_bytes)) = (self.signature.as_ref(), self.public_key.as_ref()) else {
            return false;
        };

        let Ok(pubkey)    = PublicKey::from_bytes(pk_bytes) else {
            return false;
        };
        let Ok(signature) = Signature::from_bytes(sig_bytes) else {
            return false;
        };

        // Baue Hash => z.B. user_id + side + price + quantity + order_id
        let data = format!("{}:{:?}:{}:{}:{}",
            self.user_id, 
            self.side,
            self.price,
            self.quantity,
            self.order_id
        );
        let mut hasher = Sha256::new();
        hasher.update(data.as_bytes());
        let hashed = hasher.finalize();

        pubkey.verify(&hashed, &signature).is_ok()
    }
}

// PriceLevel => alle Orders auf diesem Price
#[derive(Clone, Debug)]
pub struct PriceLevel {
    pub price: f64,
    pub orders: VecDeque<LimitOrder>,
}

/// BTree-basiertes Orderbuch

// (1) NEU: globaler Mutex für ALLE Zugriffe => einfache, grobkörnige Lösung
lazy_static! {
    static ref ORDERBOOK_MUTEX: Mutex<()> = Mutex::new(());
}

pub struct LimitOrderBook {
    // K : i64 => price * 1000
    // buy_map => absteigend => wir lagern price * -1
    buy_map: BTreeMap<i64, PriceLevel>,
    // sell_map => aufsteigend
    sell_map: BTreeMap<i64, PriceLevel>,
}

impl LimitOrderBook {
    pub fn new() -> Self {
        Self {
            buy_map: BTreeMap::new(),
            sell_map: BTreeMap::new(),
        }
    }

    fn price_to_key(price: f64) -> i64 {
        (price * 1000.0).round() as i64
    }

    /// NEU (Sicherheitsupdate): Wir geben ein Result zurück und prüfen:
    ///  - Signatur
    ///  - Preis + Quantity > 0
    /// => Zusätzlich globaler Lock
    pub fn insert_limit_order(&mut self, ord: LimitOrder) -> Result<(), DexError> {
        // (2) Lock => verhindert paralleles Einfügen
        let _guard = ORDERBOOK_MUTEX.lock().map_err(|_| DexError::Other("Orderbook mutex poisoned".into()))?;

        // 1) Negative/Nullwerte abfangen
        if ord.price <= 0.0 || ord.quantity <= 0.0 {
            return Err(DexError::Other("Price oder Quantity <= 0".into()));
        }

        // 2) Signatur prüfen
        if !ord.verify_signature() {
            return Err(DexError::Other("Ungültige Order-Signatur".into()));
        }

        // 3) Falls ok => Einfügen
        let key = Self::price_to_key(ord.price);
        match ord.side {
            Side::Buy => {
                let neg_key = -key;
                let entry = self.buy_map.entry(neg_key).or_insert_with(|| PriceLevel {
                    price: ord.price,
                    orders: VecDeque::new(),
                });
                entry.orders.push_back(ord);
            }
            Side::Sell => {
                let entry = self.sell_map.entry(key).or_insert_with(|| PriceLevel {
                    price: ord.price,
                    orders: VecDeque::new(),
                });
                entry.orders.push_back(ord);
            }
        }
        Ok(())
    }

    /// Beste Kauf-Preislevel => smallest neg_key
    pub fn best_buy_level(&self) -> Option<&PriceLevel> {
        let first = self.buy_map.first_entry()?;
        Some(first.get())
    }

    /// Beste Verkaufs-Preislevel => smallest positive key
    pub fn best_sell_level(&self) -> Option<&PriceLevel> {
        let first = self.sell_map.first_entry()?;
        Some(first.get())
    }

    /// Minimales Matching => wenn bester Buy >= bester Sell => match
    /// => In realer Welt: Teilausführung, vol. calculation, etc.
    /// => Ebenfalls Mutex => da wir hier ins BTreeMap schreiben
    pub fn match_once(&mut self) {
        // (2) globaler Lock => keine parallele Ausführung
        let _guard = ORDERBOOK_MUTEX.lock().expect("Orderbook mutex poisoned");

        let mut best_buy_entry = match self.buy_map.first_entry() {
            None => return,
            Some(e) => e,
        };
        let mut best_sell_entry = match self.sell_map.first_entry() {
            None => return,
            Some(e) => e,
        };
        let buy_price = best_buy_entry.get().price;
        let sell_price = best_sell_entry.get().price;

        if buy_price < sell_price {
            // kein match
            return;
        }
        println!("MATCHED: buy {} vs sell {}", buy_price, sell_price);

        let best_buy_level_key = *best_buy_entry.key();
        let best_sell_level_key = *best_sell_entry.key();

        let buyer_queue = self.buy_map.get_mut(&best_buy_level_key).unwrap();
        if let Some(buy_order) = buyer_queue.orders.pop_front() {
            // ...
        }
        let seller_queue = self.sell_map.get_mut(&best_sell_level_key).unwrap();
        if let Some(sell_order) = seller_queue.orders.pop_front() {
            // ...
        }
        if buyer_queue.orders.is_empty() {
            self.buy_map.remove(&best_buy_level_key);
        }
        if seller_queue.orders.is_empty() {
            self.sell_map.remove(&best_sell_level_key);
        }
    }
}
