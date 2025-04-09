// my_dex/multi_asset_exchange/src/main.rs

use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use futures::{SinkExt, StreamExt};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use serde_json::Value;
use anyhow::Result;

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
    println!("Starte Multi-Asset-Exchange mit Untereinheiten + WebSocket.");

    // 1) Optionale globale Preisliste (extern)
    // Da wir in produktiver Umgebung besser mehrere Quellen validieren würden,
    // zeigt dieses Beispiel nur eine einzelne WebSocket-Verbindung (z. B. Binance).
    // Sicherheits-Erweiterung: In einer realen Implementierung sollte man
    // - Mehrere Feeds
    // - TLS-Zertifikatsprüfung
    // - ggf. Signierte Feeds
    // nutzen.
    let global_prices = Arc::new(Mutex::new(HashMap::<String, f64>::new()));
    // WebSocket-Task
    let gp_clone = global_prices.clone();
    let handle_ws = tokio::spawn(async move {
        // Bsp: BTCUSDT live feed
        // Sicherheitsaspekt: wss:// => TLS, aber wir könnten hier dennoch ein
        // Zertifikats-Pinning oder eine serverseitige Signaturprüfung einbauen.
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
                                // println!("Live BTC/USDT = {}", px);
                            }
                        }
                    }
                }
            }
        }
    });

    // 2) Exchange-Instanz
    // Hier könnte man optional: Signatur-Schlüssel, Security-Layer, etc. an Exchange übergeben.
    let mut ex = Exchange::new();

    // Märkte anlegen
    ex.create_market(Asset::BTC, Asset::USDT);
    ex.create_market(Asset::ETH, Asset::USDT);
    ex.create_market(Asset::BTC, Asset::ETH);
    // usw.

    // Accounts
    ex.create_account("alice");
    ex.create_account("bob");

    // Einzahlen
    ex.deposit("alice", Asset::USDT, 50000.0);
    ex.deposit("bob",   Asset::BTC, 2.0);

    // Bsp: Bob => Sell-Limit 1 BTC @ 40_000 USDT
    let bob_order = Order::new_with_signature(
        "bob",
        OrderSide::Sell,
        1.0,
        OrderType::Limit(40_000.0),
        // Hier würden wir produktiv Bob's public key & Signatur übergeben
        // (für das Beispiel: Vec::new())
        Vec::new(),   // placeholder for pubkey
        Vec::new(),   // placeholder for signature
    );
    ex.place_order(bob_order);

    // Alice => Buy-Limit 1 BTC @ 30_000 USDT
    let alice_order = Order::new_with_signature(
        "alice",
        OrderSide::Buy,
        1.0,
        OrderType::Limit(30_000.0),
        Vec::new(), // placeholder
        Vec::new(), // placeholder
    );
    ex.place_order(alice_order);

    // => kein Match, da 30k < 40k
    ex.match_orders("BTC", "USDT");
    ex.settlement.print_balances();

    println!("--- Nun heben wir Alices Limit an, z.B. Market => auto trade ---");
    let alice_market = Order::new_with_signature(
        "alice",
        OrderSide::Buy,
        1.0,
        OrderType::Market,
        Vec::new(),
        Vec::new(),
    );
    ex.place_order(alice_market);

    ex.match_orders("BTC", "USDT");
    ex.settlement.print_balances();

    // Lass den WebSocket-Task 10s laufen, damit wir Kursupdates sehen könnten
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;

    // Abbrechen
    handle_ws.abort();
    Ok(())
}
