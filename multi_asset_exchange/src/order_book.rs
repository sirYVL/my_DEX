// my_dex/multi_asset_exchange/src/order_book.rs

use std::collections::HashMap;
use crate::order::{Order, OrderStatus, OrderSide, OrderType};
use crate::conflict_resolution::ConflictResolution;

/// Minimaler CRDT-Speicher
#[derive(Clone, Debug)]
pub struct CrdtStorage {
    pub orders: HashMap<String, Order>,
}

impl CrdtStorage {
    pub fn new() -> Self {
        Self {
            orders: HashMap::new(),
        }
    }

    pub fn add_order(&mut self, order: Order) {
        // Sicherheitscheck (Beispiel): Falls Signatur ungültig => ggf. nicht aufnehmen
        if !order.verify_signature() {
            println!("Warn: Versuche add_order() mit ungültiger Signatur. Ignoriert. ID={}", order.id);
            return;
        }
        self.orders.insert(order.id.clone(), order);
    }

    pub fn remove_order(&mut self, order_id: &str) {
        self.orders.remove(order_id);
    }

    pub fn all_orders(&self) -> Vec<Order> {
        self.orders.values().cloned().collect()
    }

    /// Merge: timestamp-based override
    /// Sicherheitsanmerkung:
    ///   Falls du CRDT-Logik ernsthaft verteilen willst, 
    ///   müsste hier z. B. jede Änderung signiert und 
    ///   gegen Replay oder Timestampspoofing gesichert sein.
    pub fn merge(&mut self, other: &CrdtStorage) {
        for (id, other_ord) in &other.orders {
            if let Some(local_ord) = self.orders.get(id) {
                if other_ord.timestamp > local_ord.timestamp {
                    // Optional: Signature-Check
                    if other_ord.verify_signature() {
                        self.orders.insert(id.clone(), other_ord.clone());
                    } else {
                        println!("Warn: merge() => fremde Order {} Signatur ungültig => skip", id);
                    }
                }
            } else {
                if other_ord.verify_signature() {
                    self.orders.insert(id.clone(), other_ord.clone());
                } else {
                    println!("Warn: merge() => fremde Order {} Signatur ungültig => skip", id);
                }
            }
        }
    }
}

pub struct OrderBook {
    pub pair_name: String,
    pub storage: CrdtStorage,
    pub conflict_resolver: ConflictResolution,
}

impl OrderBook {
    pub fn new(pair_name: &str) -> Self {
        Self {
            pair_name: pair_name.to_string(),
            storage: CrdtStorage::new(),
            conflict_resolver: ConflictResolution::new(),
        }
    }

    /// Merge
    pub fn merge_with(&mut self, other: &CrdtStorage) {
        self.storage.merge(other);
    }

    /// Orders hinzufügen
    pub fn add_order(&mut self, order: Order) {
        self.storage.add_order(order);
    }

    pub fn all_orders(&self) -> Vec<Order> {
        self.storage.all_orders()
    }

    /// Sortierung & Ermittlung von Match-Kandidaten
    /// Rückgabe: Vec<(buy_id, sell_id, fill_amt)>
    pub fn match_orders(&mut self) -> Vec<(String, String, f64)> {
        let mut all = self.all_orders();
        // Gefiltert um Cancelled/Filled
        all.retain(|o| !matches!(o.status, OrderStatus::Cancelled | OrderStatus::Filled));

        let mut buys: Vec<_> = all.iter().filter(|o| o.side == OrderSide::Buy).cloned().collect();
        let mut sells: Vec<_> = all.iter().filter(|o| o.side == OrderSide::Sell).cloned().collect();

        // Nutzung der ConflictResolution-Sortierung (inkl. Signaturcheck)
        ConflictResolution::prioritize_orders(&mut buys);
        ConflictResolution::prioritize_orders(&mut sells);

        let mut trades = Vec::new();
        for buy in &mut buys {
            let needed = buy.remaining_quantity();
            if needed <= 0.0 {
                continue;
            }

            for sell in &mut sells {
                let avail = sell.remaining_quantity();
                if avail <= 0.0 {
                    continue;
                }
                if self.price_match_ok(buy, sell) {
                    let fill_amt = needed.min(avail);
                    trades.push((buy.id.clone(), sell.id.clone(), fill_amt));

                    if (needed - fill_amt) <= 0.0 {
                        break;
                    }
                }
            }
        }
        trades
    }

    fn price_match_ok(&self, buy: &Order, sell: &Order) -> bool {
        let b_px = match buy.order_type {
            OrderType::Market => f64::MAX,
            OrderType::Limit(px) | OrderType::Stop(px) => px,
        };
        let s_px = match sell.order_type {
            OrderType::Market => 0.0,
            OrderType::Limit(px) | OrderType::Stop(px) => px,
        };
        b_px >= s_px
    }

    /// Füllt und aktualisiert CRDT
    pub fn fill_order(&mut self, order_id: &str, amt: f64) {
        let existing = self.storage.all_orders();
        if let Some(o) = existing.iter().find(|x| x.id == order_id) {
            if matches!(o.status, OrderStatus::Filled | OrderStatus::Cancelled) {
                return;
            }
            let mut cpy = o.clone();
            cpy.fill(amt);
            self.storage.remove_order(&cpy.id);
            self.storage.add_order(cpy);
        }
    }
}
