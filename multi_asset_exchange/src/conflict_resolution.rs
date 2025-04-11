// my_dex/multi_asset_exchange/src/conflict_resolution.rs

use crate::order::{Order, OrderType, OrderSide, OrderStatus};
use std::collections::HashMap;
use tracing::warn; // Verwende strukturiertes Logging statt println!

/// Struktur zur Konfliktverfolgung: z. B. wie oft eine Order geändert wurde
pub struct ConflictResolution {
    pub order_history: HashMap<String, u32>,
}

impl ConflictResolution {
    pub fn new() -> Self {
        Self {
            order_history: HashMap::new(),
        }
    }

    /// Wir tracken, wie oft eine bestimmte Order geändert wird.
    /// Sobald sie zu oft geändert wurde, könnte ein Manipulationsversuch vorliegen.
    /// Produktionssysteme sollten hier ggf. noch restriktiver reagieren.
    pub fn track_order_changes(&mut self, order_id: &str) -> bool {
        let c = self.order_history.entry(order_id.to_string()).or_insert(0);
        *c += 1;
        *c < 5
    }

    /// Sortierlogik:
    /// - Market Orders haben Priorität
    /// - Limit/Stop nach Preis (Buy = descending, Sell = ascending)
    /// - Bei Gleichstand => FIFO nach Zeitstempel
    ///
    /// Sicherheitsaspekt:
    ///   Orders mit ungültiger Signatur werden entfernt.
    pub fn prioritize_orders(orders: &mut [Order]) {
        // Sicherheits-Filter: nur gültig signierte Orders zulassen
        orders.retain(|o| {
            if !o.verify_signature() {
                warn!("Ungültige Signatur in Order {} => ignoriert", o.id);
                false
            } else {
                true
            }
        });

        use std::cmp::Ordering::*;
        orders.sort_by(|a, b| {
            // Market Orders haben Vorrang
            let a_market = matches!(a.order_type, OrderType::Market);
            let b_market = matches!(b.order_type, OrderType::Market);
            if a_market && !b_market {
                return Less;
            }
            if !a_market && b_market {
                return Greater;
            }
            if a_market && b_market {
                return a.timestamp.cmp(&b.timestamp); // FIFO
            }

            // Preisvergleich bei Limit/Stop
            let aprice = match a.order_type {
                OrderType::Limit(px) | OrderType::Stop(px) => px,
                OrderType::Market => f64::MAX,
            };
            let bprice = match b.order_type {
                OrderType::Limit(px) | OrderType::Stop(px) => px,
                OrderType::Market => f64::MAX,
            };

            match (a.side, b.side) {
                (OrderSide::Buy, OrderSide::Buy) => {
                    if aprice > bprice {
                        Less
                    } else if aprice < bprice {
                        Greater
                    } else {
                        a.timestamp.cmp(&b.timestamp)
                    }
                }
                (OrderSide::Sell, OrderSide::Sell) => {
                    if aprice < bprice {
                        Less
                    } else if aprice > bprice {
                        Greater
                    } else {
                        a.timestamp.cmp(&b.timestamp)
                    }
                }
                _ => a.timestamp.cmp(&b.timestamp), // Fallback
            }
        });
    }
}
