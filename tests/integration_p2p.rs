// my_dex/tests/integration_p2p.rs
//
// Integrationstest: 2 DexNode-Instanzen → Gossip-Sync prüfen

use my_dex::node_logic::{DexNode, NodeConfig};
use my_dex::dex_core::{Order, OrderSide, OrderType};
use std::net::TcpListener;
use std::sync::Arc;
use std::fs;
use std::path::Path;
use anyhow::Result;
use tokio::time::{sleep, Duration};

/// Hole einen freien TCP-Port
fn find_free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// Lösche temporäre RocksDB-Verzeichnisse
fn cleanup_db(path: &str) {
    if Path::new(path).exists() {
        let _ = fs::remove_dir_all(path);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_two_nodes_gossip_sync() -> Result<()> {
    // Ports automatisch wählen
    let port1 = find_free_port();
    let port2 = find_free_port();

    // Pfade für temporäre Datenbanken
    let db1 = "./test_db_node1";
    let db2 = "./test_db_node2";
    cleanup_db(db1);
    cleanup_db(db2);

    // Node-Konfigurationen
    let config1 = NodeConfig {
        node_address: format!("127.0.0.1:{}", port1),
        db_path: db1.to_string(),
        match_interval_sec: 3,
        ..Default::default()
    };
    let config2 = NodeConfig {
        node_address: format!("127.0.0.1:{}", port2),
        db_path: db2.to_string(),
        match_interval_sec: 3,
        ..Default::default()
    };

    let node1 = DexNode::new(config1.clone())?;
    let node2 = DexNode::new(config2.clone())?;

    // Start beide Nodes
    let handle1 = tokio::spawn({
        let node1 = node1.clone();
        async move {
            node1.start().await.unwrap();
        }
    });
    let handle2 = tokio::spawn({
        let node2 = node2.clone();
        async move {
            node2.start().await.unwrap();
        }
    });

    // Warten auf Netzwerksync
    sleep(Duration::from_secs(2)).await;

    // Order auf Node 1 einfügen
    let order = Order::new(
        "alice",
        OrderSide::Buy,
        1.0,
        OrderType::Limit(42_000.0),
        "BTC",
        "USDT",
    );
    node1.add_order(order.clone());

    // Warten auf Gossip-Synchronisierung
    sleep(Duration::from_secs(5)).await;

    // Check: Ist die Order bei Node 2 angekommen?
    let state2 = node2.state.lock().unwrap();
    let found = state2.orders.iter().any(|o| o.id == order.id);

    assert!(found, "Node 2 hat die Order NICHT synchronisiert bekommen.");

    // Cleanup & Testende
    handle1.abort();
    handle2.abort();
    cleanup_db(db1);
    cleanup_db(db2);
    Ok(())
}
