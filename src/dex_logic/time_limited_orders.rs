//-------------------------------------
// my_dex/src/dex_logic/time_limited_orders.rs
//-------------------------------------
//
// Dieses Modul implementiert Orders mit folgenden Features:
//  1) Time-Limit (Endzeit) oder Angabe einer maximalen Dauer (1…30 Tage).
//  2) Teilfüllungen (Partial Fill).
//  3) Automatisches Wieder-Einstellen (Re-Listing) bis zu N (1…3) Mal, falls
//     am Ende der Zeit noch nicht 100 % verkauft/gekauft wurden.
//  4) Vorzeitiges Abbrechen (Cancel) durch Käufer oder Verkäufer.
//
// Wir verwenden ein HashMap<order_id, TimeLimitedOrder> in einem Mutex zur Demonstration.
//
// (c) Dein DEX-Projekt, ohne Kürzungen oder Platzhalter.
//
// NEU (2 Änderungen):
//  1) Ein globaler lazy_static! Mutex => schützt alle Methoden (add_order, cancel_order, partial_fill, poll_expirations).
//  2) Im partial_fill => wir checken is_expired => darf nicht mehr gefüllt werden.
//

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, Duration, UNIX_EPOCH};
use anyhow::{Result, anyhow};
use tracing::{info, warn, debug, error};

// NEU: Für Signaturchecks
use ed25519_dalek::{PublicKey, Signature, Verifier};
use sha2::{Sha256, Digest};

// NEU: Globaler Mutex
use lazy_static::lazy_static;

lazy_static! {
    static ref TIME_LIMITED_MUTEX: Mutex<()> = Mutex::new(());
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone)]
pub struct TimeLimitedOrder {
    pub order_id: String,
    pub user_id: String,
    pub side: OrderSide,

    /// Gesamtmenge (z. B. 1.0 BTC)
    pub quantity: f64,
    pub filled_amount: f64,
    pub price_per_unit: f64,

    /// Zeitstempel (UNIX) wann die Order erstellt wurde
    pub start_time: u64,

    /// Wann soll sie ablaufen? Entweder aus `duration_secs` berechnet 
    /// oder direkt gesetzt. 
    pub end_time: u64,

    /// Wieviele Male wurde sie schon neu eingestellt?
    pub auto_relist_count: u32,
    /// Wieviele Neuanläufe sind erlaubt? (1..3)
    pub max_relist: u32,

    /// Ob Order bereits abgebrochen oder komplett erfüllt wurde
    /// => Dann kein Re-Listing mehr
    pub cancelled: bool,
    pub fully_filled: bool,

    // NEU: Signaturfelder
    pub signature: Option<Vec<u8>>,
    pub public_key: Option<Vec<u8>>,
}

impl TimeLimitedOrder {
    /// Erzeugt eine neue Zeit-limitierte Order:
    pub fn new(
        order_id: &str,
        user_id: &str,
        side: OrderSide,
        quantity: f64,
        price_per_unit: f64,
        duration_secs: u64,
        max_relist: u32,
    ) -> Result<Self> {
        if quantity <= 0.0 {
            return Err(anyhow!("Quantity must be positive"));
        }
        if price_per_unit <= 0.0 {
            return Err(anyhow!("Price must be positive"));
        }
        if duration_secs < 60 {
            return Err(anyhow!("Duration must be at least 60 seconds (1 Minute)"));
        }
        if duration_secs > 2592000 {
            return Err(anyhow!("Max allowed duration is 30 days (2592000 seconds)"));
        }
        if max_relist < 1 || max_relist > 3 {
            return Err(anyhow!("max_relist must be between 1 and 3"));
        }
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        Ok(TimeLimitedOrder {
            order_id: order_id.to_string(),
            user_id: user_id.to_string(),
            side,
            quantity,
            filled_amount: 0.0,
            price_per_unit,
            start_time: now,
            end_time: now + duration_secs,
            auto_relist_count: 0,
            max_relist,
            cancelled: false,
            fully_filled: false,
            signature: None,
            public_key: None,
        })
    }

    /// Variante mit Signatur:
    pub fn new_with_signature(
        order_id: &str,
        user_id: &str,
        side: OrderSide,
        quantity: f64,
        price_per_unit: f64,
        duration_secs: u64,
        max_relist: u32,
        signature: Vec<u8>,
        public_key: Vec<u8>,
    ) -> Result<Self> {
        let mut tmp = Self::new(
            order_id,
            user_id,
            side,
            quantity,
            price_per_unit,
            duration_secs,
            max_relist,
        )?;
        tmp.signature = Some(signature);
        tmp.public_key = Some(public_key);
        if !tmp.verify_signature() {
            return Err(anyhow!("Signature invalid in new_with_signature()"));
        }
        Ok(tmp)
    }

    pub fn remaining_amount(&self) -> f64 {
        self.quantity - self.filled_amount
    }

    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)
            .unwrap_or_default().as_secs();
        now >= self.end_time
    }

    pub fn is_active(&self) -> bool {
        !self.cancelled && !self.fully_filled
    }

    /// Prüft Signatur
    pub fn verify_signature(&self) -> bool {
        let (Some(sig_bytes), Some(pk_bytes)) = (self.signature.as_ref(), self.public_key.as_ref()) else {
            return false;
        };
        let Ok(pubkey) = PublicKey::from_bytes(pk_bytes) else {
            return false;
        };
        let Ok(signature) = Signature::from_bytes(sig_bytes) else {
            return false;
        };

        let data = format!(
            "{}:{}:{}:{}:{}",
            self.order_id,
            self.user_id,
            self.quantity,
            self.price_per_unit,
            self.end_time
        );
        let mut hasher = Sha256::new();
        hasher.update(data.as_bytes());
        let hashed = hasher.finalize();
        pubkey.verify(&hashed, &signature).is_ok()
    }
}

// ------------------------------------------------
// Eine Manager-Struktur, die die Zeit-limit. Orders verwaltet
// ------------------------------------------------
#[derive(Default)]
pub struct TimeLimitedOrderBook {
    /// Order-ID -> TimeLimitedOrder
    pub orders: HashMap<String, TimeLimitedOrder>,
}

impl TimeLimitedOrderBook {
    pub fn new() -> Self {
        Self {
            orders: HashMap::new(),
        }
    }

    /// Fügt eine neue Order ein.
    pub fn add_order(&mut self, order: TimeLimitedOrder) -> Result<()> {
        if self.orders.contains_key(&order.order_id) {
            return Err(anyhow!("OrderID '{}' already exists", order.order_id));
        }
        self.orders.insert(order.order_id.clone(), order);
        Ok(())
    }

    /// Vorzeitiges Cancel
    pub fn cancel_order(&mut self, order_id: &str) -> Result<()> {
        let ord = self.orders.get_mut(order_id)
            .ok_or_else(|| anyhow!("OrderID '{}' not found", order_id))?;
        if ord.cancelled || ord.fully_filled {
            return Ok(());
        }
        ord.cancelled = true;
        Ok(())
    }

    /// Partial Fill
    pub fn partial_fill_order(&mut self, order_id: &str, fill_amt: f64) -> Result<f64> {
        let ord = self.orders.get_mut(order_id)
            .ok_or_else(|| anyhow!("OrderID '{}' not found", order_id))?;

        // NEU: Wir prüfen, ob Order abgelaufen oder inaktiv
        if ord.is_expired() || !ord.is_active() {
            return Err(anyhow!("Order not active or expired"));
        }
        if fill_amt <= 0.0 {
            return Err(anyhow!("fill_amt <= 0 => invalid"));
        }
        let remain = ord.remaining_amount();
        if remain <= 0.0 {
            ord.fully_filled = true;
            return Err(anyhow!("Already fully filled or no remain"));
        }
        let actual_fill = if fill_amt > remain { remain } else { fill_amt };
        ord.filled_amount += actual_fill;
        if ord.filled_amount >= ord.quantity {
            ord.fully_filled = true;
        }
        Ok(actual_fill)
    }

    /// Check + neu einstellen
    pub fn check_and_handle_expirations(&mut self) {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)
            .unwrap_or_default().as_secs();

        for (_oid, ord) in &mut self.orders {
            if ord.is_active() && ord.is_expired() {
                let remain = ord.remaining_amount();
                if remain <= 0.0 {
                    ord.fully_filled = true;
                    continue;
                }
                if ord.auto_relist_count < ord.max_relist {
                    let old_dur = ord.end_time - ord.start_time;
                    ord.auto_relist_count += 1;
                    ord.start_time = now;
                    ord.end_time = now + old_dur;
                } else {
                    ord.cancelled = true;
                }
            }
        }
    }

    pub fn list_active_orders(&self) -> Vec<TimeLimitedOrder> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)
            .unwrap_or_default().as_secs();
        self.orders.values()
            .filter(|o| o.is_active() && o.end_time > now)
            .cloned()
            .collect()
    }
}

// ---------------------------------------------------------------
// Globaler Mutex => einfache concurrency
use lazy_static::lazy_static;

lazy_static! {
    static ref TIMELIMITED_MUTEX: Mutex<()> = Mutex::new(());
}

#[derive(Clone)]
pub struct TimeLimitedOrderManager {
    pub orderbook: Arc<Mutex<TimeLimitedOrderBook>>,
}

impl TimeLimitedOrderManager {
    pub fn new() -> Self {
        Self {
            orderbook: Arc::new(Mutex::new(TimeLimitedOrderBook::new())),
        }
    }

    pub fn place_time_limited_order(
        &self,
        order_id: &str,
        user_id: &str,
        side: OrderSide,
        quantity: f64,
        price_per_unit: f64,
        duration_secs: u64,
        max_relist: u32,
    ) -> Result<()> {
        let _guard = TIMELIMITED_MUTEX.lock().unwrap();
        let order = TimeLimitedOrder::new(
            order_id, 
            user_id, 
            side, 
            quantity, 
            price_per_unit,
            duration_secs,
            max_relist
        )?;
        let mut ob = self.orderbook.lock().unwrap();
        ob.add_order(order)?;
        Ok(())
    }

    pub fn cancel(&self, order_id: &str) -> Result<()> {
        let _guard = TIMELIMITED_MUTEX.lock().unwrap();
        let mut ob = self.orderbook.lock().unwrap();
        ob.cancel_order(order_id)?;
        Ok(())
    }

    pub fn partial_fill(&self, order_id: &str, fill_amt: f64) -> Result<f64> {
        let _guard = TIMELIMITED_MUTEX.lock().unwrap();
        let mut ob = self.orderbook.lock().unwrap();
        let actual_fill = ob.partial_fill_order(order_id, fill_amt)?;
        Ok(actual_fill)
    }

    pub fn poll_expirations(&self) {
        let _guard = TIMELIMITED_MUTEX.lock().unwrap();
        let mut ob = self.orderbook.lock().unwrap();
        ob.check_and_handle_expirations();
    }

    pub fn get_active_orders(&self) -> Vec<TimeLimitedOrder> {
        // read => lock
        let _guard = TIMELIMITED_MUTEX.lock().unwrap();
        let ob = self.orderbook.lock().unwrap();
        ob.list_active_orders()
    }
}
