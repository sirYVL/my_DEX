///////////////////////////////////////////////////////////
// my_dex/src/dex_logic/advanced_crdt_sharding.rs
///////////////////////////////////////////////////////////
//
// Dieses Modul kombiniert:
//  1) Sharding & Delta-Gossip (damit wir nicht den gesamten CRDT-State broadcasten)
//  2) Snapshot-Bootstrap (neue Nodes können per Snapshot + Delta nachziehen)
//  3) Watchtower-Integration (um on-chain Settlement / Betrugsfälle im CRDT zu erkennen)
//  4) RocksDB-Optimierungen mit Column Families
//  5) Merkle-basierten Checkpoint-Mechanismus, optional on-chain verankerbar.
//
// Hinweis: Dieser Code bindet an vorhandene Strukturen an:
//  - crate::error::DexError (my_dex/src/error.rs)
//  - crate::watchtower::Watchtower (my_dex/src/watchtower.rs)
//  - crate::crdt_logic::{CrdtState, Order} (my_dex/src/crdt_logic.rs)
//  - crate::shard_logic::ShardManager (my_dex/src/shard_logic.rs)
//
// Passen Sie ggf. die Pfade an, falls Ihr Projekt andere Strukturen hat.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tracing::{info, debug, warn, error};
use anyhow::{Result, anyhow};
use rand::Rng;

// --- Aus Ihrem Projekt: ---
use crate::error::DexError;
use crate::watchtower::Watchtower;
use crate::crdt_logic::{CrdtState, Order};

// ### CHANGED: Manchmal heißt der Ordner "shard_logic", manchmal "shard_manager". 
// Bleiben wir bei shard_logic::ShardManager:
use crate::shard_logic::ShardManager;

// --- RocksDB: Column Families ---
use rocksdb::{DB, Options, ColumnFamilyDescriptor, ColumnFamily};

////////////////////////////////////////////////////////
// Delta-basiertes CRDT-Update (vermeidet Full-Sync)
////////////////////////////////////////////////////////

#[derive(Clone, Debug)]
pub struct CrdtDelta {
    // Minimaler "diff" (z. B. neue Orders, geänderte Fills etc.)
    pub updated_orders: Vec<Order>,  
    pub removed_orders: Vec<String>, // order_ids
}

#[derive(Clone, Debug)]
pub struct GossipMessage {
    pub shard_id: u32,
    pub delta: CrdtDelta,
    pub timestamp: Instant,
}

////////////////////////////////////////////////////////
// Snapshot => für Full-Bootstrap
////////////////////////////////////////////////////////

#[derive(Clone, Debug)]
pub struct CrdtShardSnapshot {
    pub shard_id: u32,
    pub orders: Vec<Order>,     // Kompletter Satz
    pub last_merkle_root: Vec<u8>,
    pub snapshot_time: Instant,
}

////////////////////////////////////////////////////////
// Checkpoint => Merkle Root
////////////////////////////////////////////////////////

#[derive(Clone, Debug)]
pub struct ShardCheckpoint {
    pub shard_id: u32,
    pub merkle_root: Vec<u8>,  // z. B. root über alle Orders
    pub block_height: u64,     // optional: für "Anker" in Block
    pub on_chain_txid: Option<String>,
}

pub const ORDERS_CF: &str = "orders_cf";
pub const SNAPSHOTS_CF: &str = "snapshots_cf";
pub const CHECKPOINTS_CF: &str = "checkpoints_cf";

////////////////////////////////////////////////////////
// AdvancedShardDB => CFs pro Shard
////////////////////////////////////////////////////////

#[derive(Clone)]
pub struct AdvancedShardDB {
    pub db: Arc<DB>,
    pub orders_cf: ColumnFamily,
    pub snapshots_cf: ColumnFamily,
    pub checkpoints_cf: ColumnFamily,
}

impl AdvancedShardDB {
    pub fn open(path: &str) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        let cfs = vec![
            ColumnFamilyDescriptor::new(ORDERS_CF, Options::default()),
            ColumnFamilyDescriptor::new(SNAPSHOTS_CF, Options::default()),
            ColumnFamilyDescriptor::new(CHECKPOINTS_CF, Options::default()),
        ];

        let db = DB::open_cf_descriptors(&opts, path, cfs)?;
        let orders_cf = db.cf_handle(ORDERS_CF).ok_or_else(|| anyhow!("orders_cf missing"))?;
        let snapshots_cf = db.cf_handle(SNAPSHOTS_CF).ok_or_else(|| anyhow!("snapshots_cf missing"))?;
        let checkpoints_cf = db.cf_handle(CHECKPOINTS_CF).ok_or_else(|| anyhow!("checkpoints_cf missing"))?;

        Ok(Self {
            db: Arc::new(db),
            orders_cf,
            snapshots_cf,
            checkpoints_cf
        })
    }

    pub fn store_order(&self, shard_id: u32, order: &Order) -> Result<()> {
        let key = format!("shard_{}_{}", shard_id, order.id);
        let val = bincode::serialize(order)?;
        self.db.put_cf(self.orders_cf, key.as_bytes(), val)?;
        Ok(())
    }

    pub fn remove_order(&self, shard_id: u32, order_id: &str) -> Result<()> {
        let key = format!("shard_{}_{}", shard_id, order_id);
        self.db.delete_cf(self.orders_cf, key.as_bytes())?;
        Ok(())
    }

    pub fn store_snapshot(&self, snapshot: &CrdtShardSnapshot) -> Result<()> {
        let key = format!("snapshot_{}", snapshot.shard_id);
        let val = bincode::serialize(snapshot)?;
        self.db.put_cf(self.snapshots_cf, key.as_bytes(), val)?;
        Ok(())
    }

    pub fn load_snapshot(&self, shard_id: u32) -> Result<Option<CrdtShardSnapshot>> {
        let key = format!("snapshot_{}", shard_id);
        if let Some(bytes) = self.db.get_cf(self.snapshots_cf, key.as_bytes())? {
            let snap: CrdtShardSnapshot = bincode::deserialize(&bytes)?;
            Ok(Some(snap))
        } else {
            Ok(None)
        }
    }

    pub fn store_checkpoint(&self, cp: &ShardCheckpoint) -> Result<()> {
        let key = format!("checkpoint_{}", cp.shard_id);
        let val = bincode::serialize(cp)?;
        self.db.put_cf(self.checkpoints_cf, key.as_bytes(), val)?;
        Ok(())
    }
}

////////////////////////////////////////////////////////
// Watchtower-Integration => wir binden watchtower
////////////////////////////////////////////////////////

#[derive(Clone)]
pub struct AdvancedWatchtower {
    pub w: Arc<Mutex<Watchtower>>,
}

impl AdvancedWatchtower {
    pub fn new(wt: Watchtower) -> Self {
        Self { w: Arc::new(Mutex::new(wt)) }
    }

    /// Prüft Betrug, indem wir den watchtower heranziehen
    pub fn check_onchain_settlement(&self, order_id: &str, published_commit: &[u8]) -> bool {
        let mut lock = self.w.lock().unwrap();
        match lock.check_for_betrug(order_id, published_commit) {
            Ok(betrug) => !betrug,
            Err(e) => {
                warn!("Watchtower => check_for_betrug error: {:?}", e);
                false
            }
        }
    }
}

////////////////////////////////////////////////////////
// AdvancedShardState => Shard-spezifische CRDT + DB
////////////////////////////////////////////////////////

pub struct AdvancedShardState {
    pub shard_id: u32,
    pub crdt_state: CrdtState,
    pub db: AdvancedShardDB,
    pub watchtower: AdvancedWatchtower,
}

impl AdvancedShardState {
    pub fn new(shard_id: u32, path: &str, wt: Watchtower) -> Result<Self> {
        let db = AdvancedShardDB::open(path)?;
        let st = CrdtState::default();
        let advwt = AdvancedWatchtower::new(wt);
        Ok(Self {
            shard_id,
            crdt_state: st,
            db,
            watchtower: advwt,
        })
    }

    /// Delta-Anwendung => parted storing => wir speichern Orders in orders_cf
    ///
    /// NEU (Sicherheit):
    ///  - Prüfe, ob Order eine gültige Signatur hat (falls `Order` das unterstützt).
    ///  - Nur dann CRDT-state updaten + store_order.
    pub fn apply_delta(&mut self, delta: &CrdtDelta) -> Result<()> {
        for o in &delta.updated_orders {
            // Beispiel: Falls du in `crdt_logic::Order` => verify_signature() hast
            if !o.verify_signature() {
                warn!("Order {} hat ungültige Signatur => Delta-Anwendung übersprungen", o.id);
                continue;
            }
            self.crdt_state.add_local_order("NodeX", &o.id, &o.user_id, o.quantity, o.price)?;
            self.db.store_order(self.shard_id, o)?;
        }
        for rid in &delta.removed_orders {
            self.crdt_state.remove_local_order("NodeX", rid)?;
            self.db.remove_order(self.shard_id, rid)?;
        }
        Ok(())
    }

    /// Bilde Merkle-Root => naive Variante
    pub fn compute_merkle_root(&self) -> Vec<u8> {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        // iteriere alle visible Orders
        for (ord, _) in &self.crdt_state.orset.adds {
            hasher.update(ord.id.as_bytes());
            // optional: auch user_id, quantity, price etc. 
            // => so stellst du sicher, dass jede Änderung am Order erfasst wird.
        }
        hasher.finalize().to_vec()
    }

    /// Erzeugt einen Checkpoint => z. B. on-chain anchor
    pub fn checkpoint_and_store(
        &mut self,
        block_height: u64,
        txid: Option<String>
    ) -> Result<()> {
        let root = self.compute_merkle_root();
        let cp = ShardCheckpoint {
            shard_id: self.shard_id,
            merkle_root: root,
            block_height,
            on_chain_txid: txid,
        };
        self.db.store_checkpoint(&cp)?;
        info!("Stored checkpoint => shard={}, block_height={}", self.shard_id, block_height);
        Ok(())
    }

    /// Snapshot => Full-Sync
    pub fn create_shard_snapshot(&self) -> CrdtShardSnapshot {
        let orders = self.crdt_state.visible_orders();
        CrdtShardSnapshot {
            shard_id: self.shard_id,
            orders,
            last_merkle_root: self.compute_merkle_root(),
            snapshot_time: Instant::now(),
        }
    }

    /// Lädt Snapshot => wendet an
    pub fn load_shard_snapshot(&mut self) -> Result<()> {
        if let Some(snap) = self.db.load_snapshot(self.shard_id)? {
            self.crdt_state = CrdtState::default();
            for o in snap.orders {
                // Auch hier ggf. Signaturcheck => 
                // Aber wir gehen davon aus, dass der Snapshot 
                // von einem vertrauenswürdigen Knoten signiert sein könnte
                self.crdt_state.add_local_order("NodeX", &o.id, &o.user_id, o.quantity, o.price)?;
            }
            info!("Loaded snapshot => shard={}, #orders={}",
                  self.shard_id,
                  self.crdt_state.visible_orders().len());
        } else {
            warn!("No snapshot found in DB for shard={}", self.shard_id);
        }
        Ok(())
    }

    /// Speichere Snapshot in DB
    pub fn store_shard_snapshot(&self) -> Result<()> {
        let snap = self.create_shard_snapshot();
        self.db.store_snapshot(&snap)?;
        info!("Stored shard snapshot => shard={} #orders={}", self.shard_id, snap.orders.len());
        Ok(())
    }
}

////////////////////////////////////////////////////////
// AdvancedGossipNode => Shard-Sets + Delta-Gossip
////////////////////////////////////////////////////////

#[derive(Clone)]
pub struct AdvancedGossipNode {
    pub shard_states: Arc<Mutex<HashMap<u32, AdvancedShardState>>>,
    pub node_id: String,
}

impl AdvancedGossipNode {
    pub fn new(node_id: &str) -> Self {
        Self {
            shard_states: Arc::new(Mutex::new(HashMap::new())),
            node_id: node_id.to_string(),
        }
    }

    /// Füge einen ShardState hinzu (z. B. wenn wir gerade 
    /// von ShardManager create_shard gemacht haben).
    pub fn add_shard_state(&mut self, shard: AdvancedShardState) {
        let mut lock = self.shard_states.lock().unwrap();
        lock.insert(shard.shard_id, shard);
    }

    /// Empfängt Delta => nur, wenn wir shard_id haben
    pub fn handle_delta_gossip(&mut self, msg: &GossipMessage) -> Result<()> {
        let mut lock = self.shard_states.lock().unwrap();
        if let Some(state) = lock.get_mut(&msg.shard_id) {
            state.apply_delta(&msg.delta)?;
            debug!("Node {} applied delta on shard={}", self.node_id, msg.shard_id);
        } else {
            warn!("Shard {} not found on Node {}", msg.shard_id, self.node_id);
        }
        Ok(())
    }

    /// Full-Snapshot => wenn Node2 neu joined
    pub fn receive_shard_snapshot(&mut self, snap: CrdtShardSnapshot) -> Result<()> {
        let mut lock = self.shard_states.lock().unwrap();
        let entry = lock.entry(snap.shard_id).or_insert_with(|| {
            // In echt => Pfad definieren => "db_shard_<id>.db"
            AdvancedShardState {
                shard_id: snap.shard_id,
                crdt_state: CrdtState::default(),
                db: AdvancedShardDB::open(&format!("db_shard_{}.db", snap.shard_id)).unwrap(),
                watchtower: AdvancedWatchtower::new(Watchtower::new()),
            }
        });
        entry.crdt_state = CrdtState::default();
        for o in &snap.orders {
            // Optional: signatur-check, falls wir Snapshots 
            // nicht 100% vertrauen. 
            entry.crdt_state.add_local_order("NodeX", &o.id, &o.user_id, o.quantity, o.price).ok();
        }
        entry.db.store_snapshot(&snap)?;
        Ok(())
    }
}

////////////////////////////////////////////////////////
// Delta-Senden => Minimal-Demo
////////////////////////////////////////////////////////

pub fn send_delta_message(
    sender: &mut AdvancedGossipNode,
    receiver: &mut AdvancedGossipNode,
    shard_id: u32,
    delta: CrdtDelta
) -> Result<()> {
    let msg = GossipMessage {
        shard_id,
        delta,
        timestamp: Instant::now()
    };
    receiver.handle_delta_gossip(&msg)
}

////////////////////////////////////////////////////////
// DEMO-FUNKTION: Wie man das Ganze nutzen könnte
////////////////////////////////////////////////////////

#[allow(dead_code)]
pub fn demo_advanced_crdt_sharding() -> Result<()> {
    // 1) Watchtower anlegen
    let wt = Watchtower::new();

    // 2) ShardState
    let mut shardA = AdvancedShardState::new(
        0, 
        "db_shard_0.db",
        wt
    )?;

    // 3) Delta => fügen wir eine Order ein
    let deltaA = CrdtDelta {
        updated_orders: vec![
            // ACHTUNG: In einer produktiven Implementierung bräuchte
            // diese Order eine gültige Signatur. 
            // Hier nur Demo:
            Order {
                id: "o1".to_string(),
                user_id: "Alice".to_string(),
                timestamp: 0,
                quantity: 5.0,
                price: 100.0,
                // Falls Signatur-Felder existieren:
                signature: None,
                public_key: None,
            }
        ],
        removed_orders: vec![]
    };
    shardA.apply_delta(&deltaA)?;

    // 4) Checkpoint => optional
    shardA.checkpoint_and_store(12345, None)?;

    // 5) Snapshot => wir legen Full-Dump an
    shardA.store_shard_snapshot()?;

    // 6) Node1 + Node2 => Gossip
    let mut node1 = AdvancedGossipNode::new("Node1");
    node1.add_shard_state(shardA);

    let mut node2 = AdvancedGossipNode::new("Node2");

    // Snapshot -> node2 => Full-Bootstrap
    {
        let lock = node1.shard_states.lock().unwrap();
        let st = lock.get(&0).unwrap();
        let snap = st.create_shard_snapshot();
        node2.receive_shard_snapshot(snap)?;
    }

    // 7) Delta => Node2
    let deltaB = CrdtDelta {
        updated_orders: vec![
            Order {
                id: "o2".to_string(),
                user_id: "Bob".to_string(),
                timestamp: 0,
                quantity: 2.5,
                price: 101.0,
                signature: None,
                public_key: None,
            }
        ],
        removed_orders: vec![]
    };
    // Sende => Node2
    send_delta_message(&mut node1, &mut node2, 0, deltaB)?;

    info!("Demo finished => Node2 should have o1 & o2 in shard=0");
    Ok(())
}
