//////////////////////////////////////////////////
// my_dex/src/self_healing/custom_checks.rs
//////////////////////////////////////////////////

use crate::dex_logic::crdt_orderbook::OrderBookCRDT;
use std::sync::Mutex;
use lazy_static::lazy_static;

lazy_static! {
    static ref GLOBAL_CRDT_ORDERBOOK: Mutex<OrderBookCRDT> = Mutex::new(OrderBookCRDT::new());
}

/// Überprüft, ob das Orderbuch Einträge enthält (Produktiv-Check)
pub async fn check_orderbook_state() -> bool {
    let book = GLOBAL_CRDT_ORDERBOOK.lock().unwrap();
    let active_orders = book.all_orders();
    if active_orders.is_empty() {
        tracing::warn!("Watchdog: Orderbuch ist leer.");
        false
    } else {
        tracing::info!("Watchdog: {} aktive Orders gefunden", active_orders.len());
        true
    }
}

/// Zugriff für Tests oder Integration
pub fn inject_orderbook(book: OrderBookCRDT) {
    let mut global = GLOBAL_CRDT_ORDERBOOK.lock().unwrap();
    *global = book;
} 
