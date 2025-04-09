// tests/integration_p2p.rs
//
// Startet 2 DexNode Instanzen, simuliert Gossip, pr�ft CRDT

use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use anyhow::Result;

// Ggf. spawn your node binaries, or test in-process

#[test]
fn test_two_nodes_gossip() -> Result<()> {
    // 1) Start node1 (binary "my_dex"?)
    // 2) Start node2
    // 3) Wait => then let node1 add orders
    // 4) Check node2 sees them
    // ...
    // Hier Code wenn du spawn child processes
    Ok(())
}
