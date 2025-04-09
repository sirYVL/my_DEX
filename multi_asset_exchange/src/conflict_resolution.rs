// my_dex/multi_asset_exchange/src/conflict_resolution.rs

use crate::order::{Order, OrderType, OrderSide, OrderStatus};
use std::collections::HashMap;

/// Beispiel-Struct, falls du Track pro Order machst
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
    /// (Im Produktionsbetrieb würde man hier ggf. eine strengere Prüfung wünschen.)
    pub fn track_order_changes(&mut self, order_id: &str) -> bool {
        let c = self.order_history.entry(order_id.to_string()).or_insert(0);
        *c += 1;
        *c < 5
    }

    /// Sortierlogik:
    /// - Market zuerst
    /// - Bei Limit/Stop:
    ///   * Kauforders => descending Price
    ///   * Sellorders => ascending Price
    /// - FIFO bei Gleichstand
    ///
    /// Neu (Sicherheitsaspekt):
    ///   Wir werfen Orders, deren Signatur ungültig ist, optional aus der Sortierung heraus.
    ///   (In einer echten DEX sollte man sie evtl. erst gar nicht in die Liste aufnehmen.)
    pub fn prioritize_orders(orders: &mut [Order]) {
        // Filtern von Orders ohne gültige Signatur (Beispiel, falls du so ein Feld nutzt).
        // Wenn deine Order-Struktur gar keine Signatur hat, kannst du diesen Schritt anpassen.
        orders.retain(|o| {
            if !o.verify_signature() {
                // Du könntest hier loggen oder den Status auf 'Cancelled' setzen.
                println!("Warn: Ungültige Signatur in Order {} => ignoriert", o.id);
                false
            } else {
                true
            }
        });

        use std::cmp::Ordering::*;
        orders.sort_by(|a, b| {
            // Market vs. Market => FIFO
            let a_market = matches!(a.order_type, OrderType::Market);
            let b_market = matches!(b.order_type, OrderType::Market);
            if a_market && !b_market {
                return Less;
            }
            if !a_market && b_market {
                return Greater;
            }
            if a_market && b_market {
                return a.timestamp.cmp(&b.timestamp);
            }

            // Ansonsten Limit/Stop => Price
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
                    // descending
                    if aprice > bprice {
                        Less
                    } else if aprice < bprice {
                        Greater
                    } else {
                        a.timestamp.cmp(&b.timestamp)
                    }
                }
                (OrderSide::Sell, OrderSide::Sell) => {
                    // ascending
                    if aprice < bprice {
                        Less
                    } else if aprice > bprice {
                        Greater
                    } else {
                        a.timestamp.cmp(&b.timestamp)
                    }
                }
                // fallback
                _ => a.timestamp.cmp(&b.timestamp),
            }
        });
    }
}
