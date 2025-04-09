//////////////////////////////////////////////////////////////////////////////////
/// my_DEX/src/dht/distributed_dht.rs
//////////////////////////////////////////////////////////////////////////////////

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use once_cell::sync::Lazy;
use rand::seq::SliceRandom;

#[derive(Clone)]
pub struct LocalDHT {
    pub orders: Arc<Mutex<HashSet<String>>>,
}

impl LocalDHT {
    pub fn new() -> Self {
        Self { orders: Arc::new(Mutex::new(HashSet::new())) }
    }

    pub fn lookup_order(&self, order_id: &str) -> bool {
        let orders = self.orders.lock().unwrap();
        orders.contains(order_id)
    }

    pub fn store_order(&self, order_id: &str) {
        let mut orders = self.orders.lock().unwrap();
        orders.insert(order_id.to_string());
    }
}

pub struct DistributedDHT {
    pub nodes: Vec<LocalDHT>,
}

impl DistributedDHT {
    pub fn new(num_nodes: usize) -> Self {
        let mut nodes = Vec::new();
        for _ in 0..num_nodes {
            nodes.push(LocalDHT::new());
        }
        DistributedDHT { nodes }
    }

    pub fn lookup_order(&self, order_id: &str) -> bool {
        self.nodes.iter().any(|node| node.lookup_order(order_id))
    }

    pub fn store_order(&self, order_id: &str) {
        let mut rng = rand::thread_rng();
        let sample = self.nodes.choose_multiple(&mut rng, 3);
        for node in sample {
            node.store_order(order_id);
        }
    }
}

pub static GLOBAL_DISTRIBUTED_DHT: Lazy<DistributedDHT> = Lazy::new(|| {
    DistributedDHT::new(10)
});
