// my_dex/src/dex_logic/fuzz_test.rs
//
// Fuzz-Simulation => X nodes, Y steps
//  - random picks a node, random op => add or remove
//  - after partial merges => final full merges => check if converge
//
// NEU: Demonstration, wie man Orders signieren könnte.
// => Voraussetzung: your Order::verify_signature() existiert
// => und ITCOrderBook checkt signierte Orders.

use crate::dex_logic::itc_crdt_orderbook::{ITCOrderBook, Order, Asset};
use crate::error::DexError;

use rand::{thread_rng, Rng};
use std::collections::VecDeque;

// Ed25519 – in echter Code ggf. error handling / crates
use ed25519_dalek::{Keypair, Signer};
use rand::rngs::OsRng;

// Diese Struktur repräsentiert einen Node inkl. Keypair
#[derive(Clone)]
struct FuzzNode {
    book: ITCOrderBook,
    keypair: Keypair,
    node_id: String,
}

impl FuzzNode {
    fn new(id: &str) -> Self {
        let mut csprng = OsRng {};
        let kp = Keypair::generate(&mut csprng);
        Self {
            book: ITCOrderBook::new(),
            keypair: kp,
            node_id: id.to_string(),
        }
    }

    fn sign_order(&self, order: &mut Order) {
        // define message to sign => e.g. "id + user + assets + quantity + price + timestamp"
        let msg = format!(
            "{}:{}:{:?}:{:?}:{}:{}:{}",
            order.order_id, order.user_id, order.base, order.quote,
            order.quantity, order.price, order.timestamp
        );
        let sig_bytes = self.keypair.sign(msg.as_bytes()).to_bytes().to_vec();
        order.signature = Some(sig_bytes);
        // public_key
        order.public_key = Some(self.keypair.public.to_bytes().to_vec());
    }
}

pub fn fuzz_simulation(num_nodes: usize, steps: usize) {
    println!("Fuzz simulation: {} nodes, {} steps", num_nodes, steps);

    // Erzeuge Node-Objekte, jedes mit eigenem Keypair
    let mut nodes: Vec<FuzzNode> = (0..num_nodes)
        .map(|i| FuzzNode::new(&format!("Node{}", i)))
        .collect();

    let mut rng = thread_rng();
    let mut ops_log = VecDeque::new();

    for step in 0..steps {
        let node_i = rng.gen_range(0..num_nodes);
        let op_type = rng.gen_range(0..2);

        // Generiere random Order
        let order_id = format!("FZ{}", rng.gen_range(0..1000));
        let user = format!("User{}", rng.gen_range(0..10));
        let mut order = Order::new(
            &order_id,
            &user,
            Asset::BTC,
            Asset::LTC,
            rng.gen_range(0.01..1.0),
            rng.gen_range(50.0..150.0)
        );
        // Setze timestamp random
        order.timestamp = rng.gen_range(1_600_000_000..1_700_000_000);

        // Node signiert die Order => in real usage: "owner" sign
        // => im Fuzz Test signiert einfach der Node, der die Order "ausführt"
        nodes[node_i].sign_order(&mut order);

        if op_type == 0 {
            // add
            let res = nodes[node_i].book.add_order(order.clone(), &nodes[node_i].node_id);
            match res {
                Ok(_) => {
                    ops_log.push_back(format!("step {}: Node{} ADD {}", step, node_i, order_id));
                },
                Err(e) => {
                    // z.B. Signatur invalid, negative quantity => in diesem Fuzz
                    // normal unwahrscheinlich
                    ops_log.push_back(format!("step {}: Node{} ADD {} => ERR={:?}", step, node_i, order_id, e));
                }
            }
        } else {
            // remove => pick random existing order from that node's book
            let all = nodes[node_i].book.all_orders();
            if !all.is_empty() {
                let pick = rng.gen_range(0..all.len());
                let orem = &all[pick];
                nodes[node_i].book.remove_order(orem, &nodes[node_i].node_id);
                ops_log.push_back(format!("step {}: Node{} REMOVE {}", step, node_i, orem.order_id));
            }
        }

        // ab und zu partial merges
        if step % 10 == 0 && num_nodes > 1 {
            // pick random pair (a,b)
            let a = rng.gen_range(0..num_nodes);
            let b = rng.gen_range(0..num_nodes);
            if a != b {
                // clone A => merge into B
                let cloneA = nodes[a].book.clone();
                nodes[b].book.merge(&cloneA);
            }
        }
    }
    // final => full merge => check
    for i in 1..num_nodes {
        let clone0 = nodes[0].book.clone();
        nodes[i].book.merge(&clone0);
        let cloneI = nodes[i].book.clone();
        nodes[0].book.merge(&cloneI);
    }

    let final0 = nodes[0].book.all_orders();
    for i in 1..num_nodes {
        let fi = nodes[i].book.all_orders();
        if fi != final0 {
            println!("Mismatch: Node0 != Node{} => len0={}, lenI={}", i, final0.len(), fi.len());
        }
    }

    println!(
        "Fuzz sim ended => final all orders in Node0 = {}, logs recorded = {} ops",
        final0.len(), ops_log.len()
    );
}
