// my_dex/src/limit_orderbook_demo.rs
//
// Enth�lt Demo-Funktionen, die unser Limit-Order-Book aufrufen

use crate::dex_logic::limit_orderbook::{LimitOrderBook, LimitOrder, Side};
use std::fmt::Write as _;

pub fn limit_orderbook_example() {
    let mut lob = LimitOrderBook::new();

    // F�ge BUY Orders ein
    // z.B. 3 Limit-Buy Orders
    let buy1 = LimitOrder {
        order_id: "buy1".to_string(),
        side: Side::Buy,
        price: 100.0,
        quantity: 0.05,
        user_id: "Alice".to_string(),
    };
    let buy2 = LimitOrder {
        order_id: "buy2".to_string(),
        side: Side::Buy,
        price: 101.0, // besser
        quantity: 0.10,
        user_id: "Bob".to_string(),
    };
    let buy3 = LimitOrder {
        order_id: "buy3".to_string(),
        side: Side::Buy,
        price: 99.0,
        quantity: 0.02,
        user_id: "Carol".to_string(),
    };

    lob.insert_limit_order(buy1);
    lob.insert_limit_order(buy2);
    lob.insert_limit_order(buy3);

    // F�ge SELL Orders ein
    let sell1 = LimitOrder {
        order_id: "sell1".to_string(),
        side: Side::Sell,
        price: 102.0,
        quantity: 0.10,
        user_id: "Dave".to_string(),
    };
    let sell2 = LimitOrder {
        order_id: "sell2".to_string(),
        side: Side::Sell,
        price: 100.5,
        quantity: 0.05,
        user_id: "Eve".to_string(),
    };

    lob.insert_limit_order(sell1);
    lob.insert_limit_order(sell2);

    // schau dir bestes buy und bestes sell an
    if let Some(bb) = lob.best_buy_level() {
        println!("Best Buy: Price {}, #orders {}", bb.price, bb.orders.len());
    }
    if let Some(bs) = lob.best_sell_level() {
        println!("Best Sell: Price {}, #orders {}", bs.price, bs.orders.len());
    }

    // Versuch ein match
    lob.match_once();

    // Erneut
    lob.match_once();
}
