// my_dex/src/itc_crdt_demo.rs
//
// Enth�lt Demo-Funktionen, die unser ITC-CRDT, Gossip & Fuzz aufrufen

use crate::dex_logic::itc_crdt_orderbook::{ITCOrderBook, Order, Asset};
use crate::dex_logic::gossip::{Node, GossipNet};
use crate::dex_logic::fuzz_test::fuzz_simulation;

/// Demo: Einfach ITC-basiertes CRDT-Orderbuch -> Add/Remove
pub fn demo_itc_crdt() {
    println!("--- Demo: ITC-CRDT Add/Remove ---");

    let mut book = ITCOrderBook::new();
    book.add_order(Order::new("O1", "Alice", Asset::BTC, Asset::LTC, 0.1, 100.0), "NodeA");
    book.add_order(Order::new("O2", "Bob", Asset::LTC, Asset::BTC, 5.0, 0.02), "NodeA");

    println!("Book after additions: {:?}", book.all_orders());

    // remove
    if let Some(o2) = book.all_orders().iter().find(|o| o.order_id == "O2") {
        book.remove_order(o2, "NodeA");
    }

    println!("Book after remove O2: {:?}", book.all_orders());
}

/// Demo: Gossip zwischen 3 Knoten
pub fn demo_gossip() {
    println!("--- Demo: Gossip ---");

    // Erstelle 3 Knoten
    let mut net = GossipNet::new();
    let mut nodeA = Node::new("NodeA");
    let mut nodeB = Node::new("NodeB");
    let mut nodeC = Node::new("NodeC");

    net.add_node(&mut nodeA);
    net.add_node(&mut nodeB);
    net.add_node(&mut nodeC);

    // NodeA f�gt was hinzu
    nodeA.itc_book.add_order(
        Order::new("O99", "Carol", Asset::BTC, Asset::LTC, 0.3, 99.0),
        "NodeA"
    );

    // Gossip => wir lassen net "tick()" 2-3 mal
    net.tick(&mut nodeA);
    net.tick(&mut nodeB);
    net.tick(&mut nodeC);

    // NodeB, NodeC => sollen Book haben
    println!("NodeA: {:?}", nodeA.itc_book.all_orders());
    println!("NodeB: {:?}", nodeB.itc_book.all_orders());
    println!("NodeC: {:?}", nodeC.itc_book.all_orders());
}

/// Demo: Fuzzing-Simulation
pub fn demo_fuzz() {
    println!("--- Demo: Fuzz-Simulation ---");
    fuzz_simulation(5, 50);
}
