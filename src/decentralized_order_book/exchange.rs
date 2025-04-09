
//////////////////////////////////////////////////////////////////////////
// my_dex/src/decentralized_order_book/exchange.rs
//////////////////////////////////////////////////////////////////////////
use std::collections::HashMap;
use crate::decentralized_order_book::settlement::SettlementEngine;
use crate::decentralized_order_book::assets::Asset;
use crate::decentralized_order_book::order::{Order, OrderSide, OrderType, OrderStatus};
use crate::decentralized_order_book::order_book::OrderBook;

/// Ein "Exchange" verwaltet mehrere OrderBooks (z. B. BTC/USDT, ETH/USDT usw.)
/// und hält eine gemeinsame SettlementEngine für Escrow- und Finalisierungs-Operationen.
pub struct Exchange {
    pub settlement: SettlementEngine,
    pub orderbooks: HashMap<(Asset, Asset), OrderBook>,
}

impl Exchange {
    pub fn new() -> Self {
        Self {
            settlement: SettlementEngine::new(),
            orderbooks: HashMap::new(),
        }
    }

    /// Einen neuen Markt (base vs. quote) anlegen:
    /// z. B. (BTC, USDT) => OrderBook
    pub fn create_market(&mut self, base: Asset, quote: Asset) {
        let node_id = format!("{:?}/{:?}", base, quote);
        let ob = OrderBook::new(&node_id);
        self.orderbooks.insert((base, quote), ob);
    }

    /// Settlement-Konto anlegen
    pub fn create_account(&mut self, user_id: &str) {
        self.settlement.create_account(user_id);
    }

    /// Guthaben einzahlen
    pub fn deposit(&mut self, user_id: &str, asset: Asset, amount: f64) {
        self.settlement.deposit(user_id, asset, amount);
    }

    /// Order aufgeben (Sperren in Settlement) + ins OrderBook einfügen
    ///
    /// - `base_asset` und `quote_asset` geben an, in welchem Markt wir sind.
    /// - `order` => enthält z. B. user_id, side=Buy/Sell, quantity usw.
    /// - Neu: Wir prüfen, ob die Order-Signatur valide ist. 
    ///        (In einer realen Applikation wäre das evtl. bereits 
    ///         beim Client-Request geschehen.)
    pub fn place_order(
        &mut self,
        base_asset: Asset,
        quote_asset: Asset,
        order: Order
    ) -> bool {
        // Sicherheitscheck
        if !order.verify_signature() {
            println!("Warn: place_order() => ungültige Signatur bei Order {}, abgelehnt.", order.id);
            return false;
        }

        if let Some(ob) = self.orderbooks.get_mut(&(base_asset.clone(), quote_asset.clone())) {
            // Sperre Guthaben
            match order.side {
                OrderSide::Sell => {
                    let needed = order.remaining_quantity();
                    let ok = self.settlement.lock_funds(&order.user_id, base_asset, needed);
                    if !ok {
                        println!("Nicht genug {:?}-Guthaben bei {}!", base_asset, order.user_id);
                        return false;
                    }
                },
                OrderSide::Buy => {
                    if let Some(px) = self.extract_price(&order.order_type) {
                        let needed = order.remaining_quantity() * px;
                        let ok = self.settlement.lock_funds(&order.user_id, quote_asset.clone(), needed);
                        if !ok {
                            println!("Nicht genug {:?}-Guthaben bei {}!", quote_asset, order.user_id);
                            return false;
                        }
                    } else {
                        println!("(Warn) Market-Buy => kein definierter Preis => unklare Obergrenze!");
                    }
                }
            }

            // Danach fügen wir die Order ins OrderBook ein
            ob.add_order(order);
            true
        } else {
            println!("Kein OrderBook für ({:?},{:?}) existiert!", base_asset, quote_asset);
            false
        }
    }

    /// Cancelt eine Order => Freed Escrow in Settlement
    pub fn cancel_order(&mut self, base_asset: Asset, quote_asset: Asset, order_id: &str) {
        if let Some(ob) = self.orderbooks.get_mut(&(base_asset.clone(), quote_asset.clone())) {
            let all = ob.book.all_visible_orders();
            if let Some(o) = all.iter().find(|x| x.id == order_id) {
                if matches!(o.status, OrderStatus::Filled | OrderStatus::Cancelled) {
                    println!("Order {} ist bereits Filled/Cancelled.", order_id);
                    return;
                }
                // Freed => side + Price
                match o.side {
                    OrderSide::Sell => {
                        let _ = self.settlement.release_funds(
                            &o.user_id,
                            base_asset,
                            o.remaining_quantity()
                        );
                    },
                    OrderSide::Buy => {
                        if let OrderType::Limit(px) | OrderType::Stop(px) = o.order_type {
                            let needed = o.remaining_quantity() * px;
                            let _ = self.settlement.release_funds(&o.user_id, quote_asset, needed);
                        } else {
                            let _ = self.settlement.release_funds(&o.user_id, quote_asset, 999999.0);
                        }
                    }
                }

                // Dann Order-Status => Cancelled (im OrderBook)
                ob.cancel_order(order_id);
            } else {
                println!("Order {} nicht gefunden", order_id);
            }
        }
    }

    /// Führt Matching im OrderBook aus und finalisiert via Settlement
    pub fn match_orders(&mut self, base_asset: Asset, quote_asset: Asset) {
        if let Some(ob) = self.orderbooks.get_mut(&(base_asset.clone(), quote_asset.clone())) {
            // pure Matching => Vec<(buy_id, sell_id, fill_amt)>
            let trades = ob.match_orders();
            for (bid, sid, amt) in trades {
                // Fülle Orders
                ob.fill_order(&bid, amt);
                ob.fill_order(&sid, amt);

                // Buyer / Seller => finalize
                let all = ob.book.all_visible_orders();
                let buy_ord = all.iter().find(|o| o.id == bid).unwrap();
                let sell_ord = all.iter().find(|o| o.id == sid).unwrap();

                let price = self.calc_price(buy_ord, sell_ord);
                let cost = amt * price;

                // Buyer => locked quote_asset, Seller => locked base_asset
                let ok = self.settlement.finalize_trade(
                    &buy_ord.user_id,
                    &sell_ord.user_id,
                    base_asset.clone(),
                    quote_asset.clone(),
                    amt,
                    cost
                );
                if ok {
                    println!(
                        "Trade finalisiert: buyer={} kauft {} {:?} für {} {:?}",
                        buy_ord.user_id, amt, base_asset, cost, quote_asset
                    );
                } else {
                    println!("Finalize fehlgeschlagen => nicht genug locked?");
                }
            }
        }
    }

    fn extract_price(&self, ot: &OrderType) -> Option<f64> {
        match ot {
            OrderType::Limit(px) | OrderType::Stop(px) => Some(*px),
            OrderType::Market => None,
        }
    }

    fn calc_price(&self, buy: &Order, sell: &Order) -> f64 {
        let b_px = match buy.order_type {
            OrderType::Limit(px) | OrderType::Stop(px) => px,
            OrderType::Market => f64::MAX,
        };
        let s_px = match sell.order_type {
            OrderType::Limit(px) | OrderType::Stop(px) => px,
            OrderType::Market => 0.0,
        };
        if b_px == f64::MAX && s_px == 0.0 {
            return 1.0; // fallback
        }
        (b_px + s_px)/2.0
    }
}
