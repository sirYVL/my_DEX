#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_two_nodes_gossip_sync() -> anyhow::Result<()> {
    use my_dex::node_logic::{DexNode, NodeConfig};
    use my_dex::dex_core::{Order, OrderSide, OrderType, CrdtState};

    // Node 1 Config
    let config1 = NodeConfig {
        node_address: "127.0.0.1:9001".to_string(),
        db_path: "./test_db_node1".to_string(),
        match_interval_sec: 5,
        ..Default::default()
    };

    // Node 2 Config
    let config2 = NodeConfig {
        node_address: "127.0.0.1:9002".to_string(),
        db_path: "./test_db_node2".to_string(),
        match_interval_sec: 5,
        ..Default::default()
    };

    // Starte beide Nodes
    let node1 = DexNode::new(config1.clone())?;
    let node2 = DexNode::new(config2.clone())?;

    let handle1 = tokio::spawn(async move {
        node1.start().await.unwrap();
    });

    let handle2 = tokio::spawn(async move {
        node2.start().await.unwrap();
    });

    // Warte auf Start
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Erstelle Order auf Node 1
    let order = Order::new(
        "alice",
        OrderSide::Buy,
        1.0,
        OrderType::Limit(42_000.0),
        "BTC",
        "USDT",
    );
    node1.add_order(order.clone());

    // Warte auf Gossip-Sync
    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

    // Prüfe, ob Node 2 die Order hat
    let state2 = node2.state.lock().unwrap();
    let synced = state2.orders.iter().any(|o| o.id == order.id);

    assert!(synced, "Node 2 hat die Order nicht synchronisiert erhalten");

    // Beende Test
    handle1.abort();
    handle2.abort();
    Ok(())
}
