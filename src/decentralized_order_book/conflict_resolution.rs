//////////////////////////////////////////////////////////////////////////
// my_dex/src/decentralized_order_book/conflict_resolution.rs
//////////////////////////////////////////////////////////////////////////

use crate::decentralized_order_book::order::{Order, OrderType, OrderSide, OrderStatus};
use std::collections::HashMap;

/// Einfache Struktur zur Manipulationsüberwachung
pub struct ConflictResolution {
    // Kann bei Bedarf public gemacht werden, wenn du es anderswo auslesen willst
    order_history: HashMap<String, u32>,
}

impl ConflictResolution {
    pub fn new() -> Self {
        Self {
            order_history: HashMap::new(),
        }
    }

    /// Überwacht, wie oft eine Order geändert wurde (z. B. um Spam zu entdecken)
    /// Erhöht den Zähler für die gegebene Order-ID. Wenn ein gewisser Grenzwert
    /// überschritten wird, meldet sie verdächtiges Verhalten zurück.
    pub fn track_order_changes(&mut self, order_id: &str) -> bool {
        let count = self.order_history.entry(order_id.to_string()).or_insert(0);
        *count += 1;
        if *count > 5 {
            println!("?? Verdächtiges Order-Manipulationsverhalten erkannt! (ID = {})", order_id);
            return false;
        }
        true
    }

    /// Vereinheitlichte Sortierlogik für Orders
    /// - Market zuerst
    /// - Unter Limit/Stop:
    ///   * Kauforders => descending price
    ///   * Sellorders => ascending price
    /// - Bei Gleichstand => FIFO per Zeitstempel
    ///
    /// Neu: Wir filtern Orders, deren Signaturen ungültig sind (falls `verify_signature() == false`).
    ///      In einer echten Implementierung würde man die Orders schon vor dem Einpflegen verwerfen.
    pub fn prioritize_orders(orders: &mut [Order]) {
        use std::cmp::Ordering::*;

        // Filter ungültig signierte Orders (Demo).
        orders.retain(|ord| {
            if !ord.verify_signature() {
                println!("Warn: Order {} hat ungültige Signatur => Ignorieren", ord.id);
                false
            } else {
                true
            }
        });

        orders.sort_by(|a, b| {
            // (1) Market vs Market => FIFO
            let a_market = matches!(a.order_type, OrderType::Market);
            let b_market = matches!(b.order_type, OrderType::Market);
            if a_market && b_market {
                // Gleiche Ordertype => FIFO
                return a.timestamp.cmp(&b.timestamp);
            } else if a_market {
                return Less;
            } else if b_market {
                return Greater;
            }

            // (2) Limit/Stop => Buy vs Sell
            let aprice = match a.order_type {
                OrderType::Limit(px) | OrderType::Stop(px) => px,
                OrderType::Market => f64::MAX, // fallback
            };
            let bprice = match b.order_type {
                OrderType::Limit(px) | OrderType::Stop(px) => px,
                OrderType::Market => f64::MAX, // fallback
            };

            match (a.side, b.side) {
                // Buy => absteigend
                (OrderSide::Buy, OrderSide::Buy) => {
                    if aprice > bprice {
                        Less
                    } else if aprice < bprice {
                        Greater
                    } else {
                        a.timestamp.cmp(&b.timestamp)
                    }
                },
                // Sell => aufsteigend
                (OrderSide::Sell, OrderSide::Sell) => {
                    if aprice < bprice {
                        Less
                    } else if aprice > bprice {
                        Greater
                    } else {
                        a.timestamp.cmp(&b.timestamp)
                    }
                },
                // Buy vs Sell => wenn unterschiedlich => FIFO
                _ => a.timestamp.cmp(&b.timestamp),
            }
        });
    }
}
