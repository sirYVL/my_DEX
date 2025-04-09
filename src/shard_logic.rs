////////////////////////////////////////////////////////////
// my_dex/src/shard_logic.rs
////////////////////////////////////////////////////////////
//
// Erweiterter ShardManager mit parted storing (Sharding) + Self-Healing:
//  - Wir verwalten für jeden Shard einen AdvancedShardState
//  - Wir halten fest, welche NodeIds Replikate eines Shards besitzen (ShardReplicaInfo)
//  - Beim Node-Ausfall (on_node_failed) verteilen wir den Shard an eine neue Node
//  - Periodisches maintain_shards() prüft, ob unser Replication-Factor erfüllt ist.
//
// Voraussetzung:
//  - advanced_crdt_sharding.rs (AdvancedShardState) ist vorhanden
//  - ggf. KademliaService, um next best Node zu finden (hier optional, als Option)
//  - Ein Replication-Factor, z.B. = 3
//
// (c) Ihr DEX-Projekt
//

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use anyhow::Result;
use tracing::{info, debug, warn};
use crate::error::DexError;

// Falls du Orders / CRDTState etc. brauchst:
use crate::crdt_logic::{CrdtState, Order};  

// NEU: advanced_crdt_sharding
use crate::dex_logic::advanced_crdt_sharding::{
    AdvancedShardState,
    CrdtDelta,
    CrdtShardSnapshot,
    // GossipMessage, send_delta_message, ...
};
use crate::watchtower::Watchtower;

// Falls du Node-Failure-Detection via Kademlia willst:
use crate::kademlia::kademlia_service::{KademliaService, NodeId};

////////////////////////////////////////////////////////////
// Hilfsstruct: ShardReplicaInfo => speichert Replikate pro Shard
////////////////////////////////////////////////////////////

#[derive(Debug)]
pub struct ShardReplicaInfo {
    pub shard_replicas: HashMap<u32, HashSet<NodeId>>,
    pub replication_factor: usize,
}

impl ShardReplicaInfo {
    pub fn new(replication_factor: usize) -> Self {
        Self {
            shard_replicas: HashMap::new(),
            replication_factor,
        }
    }

    pub fn get_replicas(&self, shard_id: u32) -> HashSet<NodeId> {
        self.shard_replicas
            .get(&shard_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn add_replica(&mut self, shard_id: u32, node_id: NodeId) {
        let entry = self.shard_replicas.entry(shard_id).or_insert_with(HashSet::new);
        entry.insert(node_id);
    }

    pub fn remove_replica(&mut self, shard_id: u32, node_id: &NodeId) {
        if let Some(set) = self.shard_replicas.get_mut(&shard_id) {
            set.remove(node_id);
        }
    }

    /// Prüft, ob wir weniger als replication_factor Kopien haben
    pub fn needs_new_replica(&self, shard_id: u32) -> bool {
        let current = self.get_replicas(shard_id).len();
        current < self.replication_factor
    }
}

////////////////////////////////////////////////////////////
// ShardSubscription => wer "abonniert" welchen Shard (bei Ihnen schon vorhanden)
////////////////////////////////////////////////////////////
#[derive(Clone, Debug)]
pub struct ShardSubscription {
    /// Node-ID -> set of shard_ids
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
// NEU: parted storing + Self-Healing => shard_info + optional Kademlia
////////////////////////////////////////////////////////////

#[derive(Clone)]
pub struct ShardManager {
    /// ShardID -> AdvancedShardState (lokal)
    pub shards: Arc<Mutex<HashMap<u32, AdvancedShardState>>>,

    /// Wer abonniert welchen Shard?
    pub subscriptions: Arc<Mutex<ShardSubscription>>,

    /// Wer hält Kopien (=Replikate) welches Shards? 
    pub shard_info: Arc<Mutex<ShardReplicaInfo>>,

    /// Optional: Kademlia => um Node-Failure-Detection & Peer-Find durchzuführen
    pub kademlia: Option<Arc<Mutex<KademliaService>>>,
}

impl ShardManager {
    /// Erzeugt neuen ShardManager
    ///  - replication_factor => z. B. 3
    ///  - optional kademlia, wenn Sie Node-Failure-Detection und Peer-Suche wollen
    pub fn new(replication_factor: usize, kademlia: Option<Arc<Mutex<KademliaService>>>) -> Self {
        Self {
            shards: Arc::new(Mutex::new(HashMap::new())),
            subscriptions: Arc::new(Mutex::new(ShardSubscription::new())),
            shard_info: Arc::new(Mutex::new(ShardReplicaInfo::new(replication_factor))),
            kademlia,
        }
    }

    /// Erzeugt einen neuen Shard
    ///  - Pfad => RocksDB
    ///  - watchtower => Falls Sie es brauchen
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

        // Wir selbst sind (lokaler Node) => fügen wir uns als Replica hinzu
        if let Some(kad) = &self.kademlia {
            let local_id = kad.lock().unwrap().local_id.clone();
            self.shard_info.lock().unwrap().add_replica(shard_id, local_id);
        }

        info!("Shard {} created => path={}", shard_id, path);
        Ok(())
    }

    /// Node abonniert Shard => speichert in subscriptions
    /// (unverändert)
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
    /// bräuchte man p2p-Aufrufe, z. B. p2p.send_message(nodeId, deltaMsg).
    pub fn broadcast_delta(&self, shard_id: u32, delta: CrdtDelta) {
        let subs = self.subscriptions.lock().unwrap();
        let subscribers = subs.get_subscribers(shard_id);
        debug!("Broadcasting delta to {} subscribers for shard={}", subscribers.len(), shard_id);
        // In echt => p2p.sendDelta(...) 
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

    // ----------------------------------------------------------------
    // NEU: parted storing + Self-Healing => wir tracken Repliken,
    // falls Node ausfällt => replicate to new node
    // ----------------------------------------------------------------

    /// Wird aufgerufen, wenn Kademlia oder P2P feststellt, dass node_id tot ist.
    /// => Wir entfernen node_id als Replica, ersetzen ggf. via replicate_shard_to_new_node
    pub fn on_node_failed(&self, dead_node: &NodeId) {
        let mut replica_info = self.shard_info.lock().unwrap();

        let shard_ids: Vec<u32> = replica_info
            .shard_replicas
            .keys()
            .cloned()
            .collect();

        for sid in shard_ids {
            let had_it = replica_info.get_replicas(sid).contains(dead_node);
            if had_it {
                replica_info.remove_replica(sid, dead_node);
                if replica_info.needs_new_replica(sid) {
                    if let Err(e) = self.replicate_shard_to_new_node(sid) {
                        warn!("Error replicate shard {} => {:?}", sid, e);
                    }
                }
            }
        }
    }

    /// Falls wir local Shard 'shard_id' haben => wir suchen via Kademlia
    /// einen neuen Node, der nicht in shard_info, und replicaten Snapshot
    fn replicate_shard_to_new_node(&self, shard_id: u32) -> Result<()> {
        let local_map = self.shards.lock().unwrap();
        let local_shard = match local_map.get(&shard_id) {
            Some(s) => s,
            None => {
                warn!("We do not hold shard {}, can't replicate", shard_id);
                return Ok(());
            }
        };
        drop(local_map);

        let mut rep_info = self.shard_info.lock().unwrap();
        let existing = rep_info.get_replicas(shard_id);

        let kad_opt = match &self.kademlia {
            Some(k) => k.clone(),
            None => {
                warn!("No Kademlia => can't replicate automatically!");
                return Ok(());
            }
        };
        let kad = kad_opt.lock().unwrap();
        let candidates = kad.table.find_closest(&kad.local_id, 20);
        drop(kad);

        let mut chosen: Option<NodeId> = None;
        for (nid, _addr) in candidates {
            if !existing.contains(&nid) && nid != kad_opt.lock().unwrap().local_id {
                chosen = Some(nid);
                break;
            }
        }

        let new_node = match chosen {
            Some(n) => n,
            None => {
                warn!("No suitable node found to replicate shard {}", shard_id);
                return Ok(());
            }
        };

        // => wir versenden Snapshot => p2p call
        let snap = local_shard.create_shard_snapshot();
        info!("Replicate shard {} => new node: {:?}, sending snapshot", shard_id, new_node);

        // TODO => p2p_send_shard_snapshot(new_node, snap) => in echt real Code
        // Evtl. wir fügen in "subscribe_node_to_shard" => ...
        // Nach dem Versenden:
        rep_info.add_replica(shard_id, new_node);

        Ok(())
    }

    /// Manuell periodisch aufrufen => check if needs new replica
    pub fn maintain_shards(&self) {
        let shard_ids: Vec<u32> = {
            let s = self.shards.lock().unwrap();
            s.keys().cloned().collect()
        };
        for sid in shard_ids {
            let mut rep_info = self.shard_info.lock().unwrap();
            if rep_info.needs_new_replica(sid) {
                if let Err(e) = self.replicate_shard_to_new_node(sid) {
                    warn!("Error replicate shard {} => {:?}", sid, e);
                }
            }
        }
    }
}

////////////////////////////////////////////////////////////
// DEMO-FUNKTION
////////////////////////////////////////////////////////////

#[allow(dead_code)]
pub fn demo_shard_manager_advanced() -> Result<()> {
    let sm = ShardManager::new(/* replication_factor=3 */3, /* kademlia= */ None);

    // 1) Erzeuge Shard0
    let wt = Watchtower::new(); 
    sm.create_shard(0, "db_shard_0.db", wt)?;

    // 2) Node "Alice" abonniert shard=0
    sm.subscribe_node_to_shard("AliceNode", 0);

    // 3) wende Delta an:
    let deltaA = CrdtDelta {
        updated_orders: vec![
            Order {
                id: "oA".to_string(),
                user_id: "Alice".to_string(),
                timestamp: 0,
                quantity: 3.0,
                price: 99.0,
            }
        ],
        removed_orders: vec![]
    };
    sm.apply_delta(0, &deltaA)?;

    // 4) broadcast => "theoretisch" an Abonnenten
    sm.broadcast_delta(0, deltaA);

    // 5) snapshot
    sm.store_shard_snapshot(0)?;

    // 6) checkpoint
    sm.checkpoint_and_store(0, 123_456, None)?;

    // => maintain shards => in einer loop, e.g. tokio spawn ...
    sm.maintain_shards();

    Ok(())
}
