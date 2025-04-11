// my_dex/multi_asset_exchange/src/main.rs

use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use futures::{SinkExt, StreamExt};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use serde_json::Value;
use anyhow::Result;
use std::time::{SystemTime, UNIX_EPOCH};

use ed25519_dalek::{Keypair, Signer};

mod assets;
mod settlement;
mod conflict_resolution;
mod order;
mod order_book;
mod exchange;

use crate::assets::Asset;
use crate::order::{Order, OrderSide, OrderType};
use crate::exchange::Exchange;

#[tokio::main]
async fn main() -> Result<()> {
    println!("Starte Multi-Asset-Exchange mit Signatur-Unterstützung.");

    // 1) Preis-Feed via WebSocket (optional)
    let global_prices = Arc::new(Mutex::new(HashMap::<String, f64>::new()));
    let gp_clone = global_prices.clone();
    let handle_ws = tokio::spawn(async move {
        let url = "wss://stream.binance.com:9443/ws/btcusdt@trade";
        let (ws_stream, _) = connect_async(url).await.expect("Fehler Connect");
        let (_write, mut read) = ws_stream.split();

        while let Some(msg) = read.next().await {
            if let Ok(m) = msg {
                if let Message::Text(txt) = m {
                    if let Ok(js) = serde_json::from_str::<Value>(&txt) {
                        if let Some(price_str) = js.get("p").and_then(|v| v.as_str()) {
                            if let Ok(px) = price_str.parse::<f64>() {
                                let mut map = gp_clone.lock().unwrap();
                                map.insert("BTC/USDT".to_string(), px);
                            }
                        }
                    }
                }
            }
        }
    });

    // 2) Exchange Setup
    let mut ex = Exchange::new();
    ex.create_market(Asset::BTC, Asset::USDT);
    ex.create_account("alice");
    ex.create_account("bob");
    ex.deposit("alice", Asset::USDT, 50000.0);
    ex.deposit("bob",   Asset::BTC, 2.0);

    // 3) Keypair erstellen für Bob
    let mut csprng = rand::rngs::OsRng {};
    let keypair_bob = Keypair::generate(&mut csprng);
    let keypair_alice = Keypair::generate(&mut csprng);

    // Order-Daten vorbereiten
    let user_id = "bob";
    let base_quantity = 1.0;
    let order_type = OrderType::Limit(40_000.0);
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let id = format!("{}_{}", user_id, now);
    let payload = format!("{}:{}:{}:{}", id, user_id, base_quantity, now);
    let signature = keypair_bob.sign(payload.as_bytes());

    // Order mit Signatur erstellen
    let bob_order = Order {
        id,
        user_id: user_id.to_string(),
        timestamp: now,
        side: OrderSide::Sell,
        base_quantity,
        filled_quantity: 0.0,
        order_type,
        status: crate::order::OrderStatus::Open,
        public_key: Some(keypair_bob.public.to_bytes().to_vec()),
        signature: Some(signature.to_bytes().to_vec()),
    };
    ex.place_order(bob_order);

    // Alice setzt Buy-Market-Order mit Signatur
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let payload_alice = format!("alice:{}:{}:{}", "alice", 1.0, now);
    let signature_alice = keypair_alice.sign(payload_alice.as_bytes());

    let alice_order = Order {
        id: format!("alice_{}", now),
        user_id: "alice".to_string(),
        timestamp: now,
        side: OrderSide::Buy,
        base_quantity: 1.0,
        filled_quantity: 0.0,
        order_type: OrderType::Market,
        status: crate::order::OrderStatus::Open,
        public_key: Some(keypair_alice.public.to_bytes().to_vec()),
        signature: Some(signature_alice.to_bytes().to_vec()),
    };
    ex.place_order(alice_order);

    // 4) Matching + Settlement
    ex.match_orders("BTC", "USDT");
    ex.settlement.print_balances();

    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    handle_ws.abort();
    Ok(())
}

