// src/node_simulation.rs
//
// Ein stark vereinfachtes "Node-Simulations"-Skript, das
// 2 Knoten darstellt, die CRDT-Orderbuch-Updates austauschen.

use crate::dex_logic::crdt_orderbook::{OrderBookCRDT};
use crate::dex_logic::orders::{Order, Asset};

/// Repr�sentiert einen Node im P2P-Netz
#[derive(Clone, Debug)]
pub struct DexNode {
    pub node_id: String,
    pub book: OrderBookCRDT,
}

impl DexNode {
    pub fn new(node_id: &str) -> Self {
        Self {
            node_id: node_id.to_string(),
            book: OrderBookCRDT::new(),
        }
    }

    pub fn add_order(&mut self, order: Order) {
        self.book.add_order(order, &self.node_id);
    }

    pub fn sync(&mut self, other: &DexNode) {
        // merges
        self.book.merge(&other.book);
    }

    pub fn all_orders(&self) -> Vec<Order> {
        self.book.all_orders()
    }
}

pub fn node_sim_demo() {
    let mut nodeA = DexNode::new("NodeA");
    let mut nodeB = DexNode::new("NodeB");

    let o1 = Order::new("A1", "Alice", Asset::BTC, Asset::LTC, 0.05, 100.0);
    let o2 = Order::new("B1", "Bob", Asset::LTC, Asset::BTC, 5.0, 0.02);

    nodeA.add_order(o1);
    nodeB.add_order(o2);

    println!("NodeA Orders: {:?}", nodeA.all_orders());
    println!("NodeB Orders: {:?}", nodeB.all_orders());

    // sync
    nodeA.sync(&nodeB);
    nodeB.sync(&nodeA);

    println!("After sync => NodeA: {:?}", nodeA.all_orders());
    println!("After sync => NodeB: {:?}", nodeB.all_orders());
}
