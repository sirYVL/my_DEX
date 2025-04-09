// my_dex/src/decentralized_order_book/matcher.rs

use super::*;
use std::sync::{Arc, Mutex};
use std::collections::{HashSet};
use crate::decentralized_order_book::order::Order;
use crate::decentralized_order_book::order_book::OrderBook;
use crate::decentralized_order_book::conflict_resolution::ConflictResolution;
use crate::crdt::ORSet; // Für verteilte Speicherung der Orders

/// Ein P2POrderMatcher, der Orders über ein CRDT-ORSet synchronisiert.
pub struct P2POrderMatcher {
    pub order_crdt: Arc<Mutex<ORSet<Order>>>,     // Synchronisiert Orders über das Netzwerk
    pub open_orders: Arc<Mutex<HashSet<String>>>, // Offene Order-IDs
    pub order_book: Arc<Mutex<OrderBook>>,        // Unser (umbenanntes) OrderBook
    pub conflict_resolution: Arc<Mutex<ConflictResolution>>, // Konfliktlösungsmechanismus
}

impl P2POrderMatcher {
    pub fn new() -> Self {
        Self {
            order_crdt: Arc::new(Mutex::new(ORSet::new())),
            open_orders: Arc::new(Mutex::new(HashSet::new())),
            order_book: Arc::new(Mutex::new(OrderBook::new("default_node"))),
            conflict_resolution: Arc::new(Mutex::new(ConflictResolution::new())),
        }
    }

    /// 📌 Fügt eine Order zum Orderbuch hinzu und prüft auf Konflikte
    /// Neu: wir prüfen die Signatur ggf. direkt.
    pub fn add_order(&self, order: Order) {
        let mut crdt = self.order_crdt.lock().unwrap();
        let mut orders = self.open_orders.lock().unwrap();
        let mut order_book = self.order_book.lock().unwrap();
        let mut conflict_resolver = self.conflict_resolution.lock().unwrap();

        if !order.verify_signature() {
            println!("🚨 Order {} wurde zu oft geändert oder ungültig signiert. Abgelehnt!", order.id);
            return;
        }

        if !conflict_resolver.track_order_changes(&order.id) {
            println!("🚨 Order {} wurde zu oft geändert. Abgelehnt!", order.id);
            return;
        }

        if orders.contains(&order.id) {
            println!("⚠ Order {} existiert bereits!", order.id);
            return;
        }

        orders.insert(order.id.clone());
        crdt.add(order.clone());
        order_book.add_order(order);
    }

    /// 🔄 Matching von Orders ausführen
    pub fn match_orders(&self) {
        let mut order_book = self.order_book.lock().unwrap();
        order_book.match_orders();
    }
}
