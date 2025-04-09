////////////////////////////////////////////////////////////
// my_dex/src/shard_logic/shard_manager.rs
////////////////////////////////////////////////////////////

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use anyhow::Result;
use tracing::{info, debug, warn};
use crate::dex_logic::advanced_crdt_sharding::{
    AdvancedShardState, CrdtDelta, GossipMessage, CrdtShardSnapshot,
};
use crate::watchtower::Watchtower; // optional
use crate::storage::replicated_db_layer::DexDB;

////////////////////////////////////////////////////////////
// ShardSubscription => wer "abonniert" welchen Shard
////////////////////////////////////////////////////////////

#[derive(Clone, Debug)]
pub struct ShardSubscription {
    /// Node-ID (String oder NodeId) -> set of shard_ids
    pub subscriptions: HashMap<String, HashSet<u32>>,
}

impl ShardSubscription {
    pub fn new() -> Self {
        Self {
            subscriptions: HashMap::new(),
        }
    }

    /// Node abonniert Shard
    pub fn subscribe(&mut self, node_id: &str, shard_id: u32) {
        let entry = self.subscriptions
            .entry(node_id.to_string())
            .or_insert_with(HashSet::new);
        entry.insert(shard_id);
        debug!("Node {} subscribed to shard {}", node_id, shard_id);
    }

    /// Node de-abonniert Shard
    pub fn unsubscribe(&mut self, node_id: &str, shard_id: u32) {
        if let Some(set) = self.subscriptions.get_mut(node_id) {
            set.remove(&shard_id);
        }
        debug!("Node {} unsubscribed from shard {}", node_id, shard_id);
    }

    /// Liefert alle Node-IDs, die einen bestimmten Shard abonniert haben
    pub fn get_subscribers(&self, shard_id: u32) -> Vec<String> {
        let mut out = Vec::new();
        for (nid, shards) in &self.subscriptions {
            if shards.contains(&shard_id) {
                out.push(nid.clone());
            }
        }
        out
    }
}

////////////////////////////////////////////////////////////
// ShardManager => verwaltet pro Shard ein AdvancedShardState
////////////////////////////////////////////////////////////

#[derive(Clone)]
pub struct ShardManager {
    /// ShardID -> AdvancedShardState
    pub shards: Arc<Mutex<HashMap<u32, AdvancedShardState>>>,
    /// Wer abonniert welchen Shard?
    pub subscriptions: Arc<Mutex<ShardSubscription>>,
}

impl ShardManager {
    pub fn new() -> Self {
        Self {
            shards: Arc::new(Mutex::new(HashMap::new())),
            subscriptions: Arc::new(Mutex::new(ShardSubscription::new())),
        }
    }

    /// Erzeugt einen neuen Shard (z. B. shard_id=0)
    /// pfad = Pfad zur RocksDB (z. B. "db_shard_{id}.db")
    /// watchtower: falls du es brauchst
    pub fn create_shard(
        &self,
        shard_id: u32,
        path: &str,
        watchtower: Watchtower
    ) -> Result<()> {
        let mut lock = self.shards.lock().unwrap();
        if lock.contains_key(&shard_id) {
            warn!("Shard {} already exists", shard_id);
            return Ok(());
        }
        let st = AdvancedShardState::new(shard_id, path, watchtower)?;
        lock.insert(shard_id, st);
        info!("Shard {} created => path={}", shard_id, path);
        Ok(())
    }

    /// Node abonniert Shard => speichert in subscriptions
    pub fn subscribe_node_to_shard(&self, node_id: &str, shard_id: u32) {
        let mut lock = self.subscriptions.lock().unwrap();
        lock.subscribe(node_id, shard_id);
    }

    pub fn unsubscribe_node_from_shard(&self, node_id: &str, shard_id: u32) {
        let mut lock = self.subscriptions.lock().unwrap();
        lock.unsubscribe(node_id, shard_id);
    }

    /// Wendet Delta auf einen Shard an
    pub fn apply_delta(&self, shard_id: u32, delta: &CrdtDelta) -> Result<()> {
        let mut lock = self.shards.lock().unwrap();
        if let Some(sh) = lock.get_mut(&shard_id) {
            sh.apply_delta(delta)?;
        } else {
            warn!("Shard {} not found => ignoring delta", shard_id);
        }
        Ok(())
    }

    /// Shard => Full Snapshot & store
    pub fn store_shard_snapshot(&self, shard_id: u32) -> Result<()> {
        let mut lock = self.shards.lock().unwrap();
        if let Some(sh) = lock.get_mut(&shard_id) {
            sh.store_shard_snapshot()?;
        } else {
            warn!("Shard {} not found => cannot store snapshot", shard_id);
        }
        Ok(())
    }

    /// Shard => Load snapshot from DB
    pub fn load_shard_snapshot(&self, shard_id: u32) -> Result<()> {
        let mut lock = self.shards.lock().unwrap();
        if let Some(sh) = lock.get_mut(&shard_id) {
            sh.load_shard_snapshot()?;
        } else {
            warn!("Shard {} not found => cannot load snapshot", shard_id);
        }
        Ok(())
    }

    /// Shard => Erzeugt CrdtShardSnapshot
    pub fn create_shard_snapshot(&self, shard_id: u32) -> Option<CrdtShardSnapshot> {
        let lock = self.shards.lock().unwrap();
        lock.get(&shard_id).map(|sh| sh.create_shard_snapshot())
    }

    /// Gossip Delta => wir ermitteln, wer shard_id abonniert hat,
    /// und senden an diese Knoten => in einer realen Implementation
    /// br�uchte man p2p-Aufrufe, z. B. p2p.send_message(nodeId, deltaMsg).
    pub fn broadcast_delta(&self, shard_id: u32, delta: CrdtDelta) {
        let subs = self.subscriptions.lock().unwrap();
        let subscribers = subs.get_subscribers(shard_id);
        debug!("Broadcasting delta to {} subscribers for shard={}", subscribers.len(), shard_id);
        // => In echt: p2p.sendDelta(...) an node_id
        // for node_id in subscribers { p2p.send_message(node_id, deltaMsg); }
    }

    /// Checkpoint => MerkleRoot verankern
    pub fn checkpoint_and_store(&self, shard_id: u32, block_height: u64, txid: Option<String>) -> Result<()> {
        let mut lock = self.shards.lock().unwrap();
        if let Some(sh) = lock.get_mut(&shard_id) {
            sh.checkpoint_and_store(block_height, txid)?;
        } else {
            warn!("Shard {} not found => cannot checkpoint", shard_id);
        }
        Ok(())
    }
}

////////////////////////////////////////////////////////////
// DEMO-FUNKTION, wie man das anwendet
////////////////////////////////////////////////////////////

#[allow(dead_code)]
pub fn demo_shard_manager_advanced() -> Result<()> {
    let sm = ShardManager::new();
    // 1) Erzeuge Shard0
    let wt = Watchtower::new(); // minimal
    sm.create_shard(0, "db_shard_0.db", wt)?;

    // 2) Node "Alice" abonniert shard=0
    sm.subscribe_node_to_shard("AliceNode", 0);

    // 3) wende Delta an:
    let deltaA = CrdtDelta {
        updated_orders: vec![], // z.B. Orders
        removed_orders: vec![]
    };
    sm.apply_delta(0, &deltaA)?;

    // 4) broadcast => "theoretisch" an Abonnenten
    sm.broadcast_delta(0, deltaA);

    // 5) snapshot => store + checkpoint
    sm.store_shard_snapshot(0)?;
    sm.checkpoint_and_store(0, 123_456, None)?;

    Ok(())
}
