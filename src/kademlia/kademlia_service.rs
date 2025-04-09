///////////////////////////////////////
// my_dex/src/kademlia/kademlia_service.rs
////////////////////////////////////////
//
// Basierend auf deinem bisherigen Code, aber erweitert um:
//  1) Optionalen Verweis auf einen ShardManager (zur Self-Healing-Integration).
//  2) Periodische Node-Failure-Erkennung in einer eigenen Task ("start_node_fail_detector").
//  3) Entfernen des unresponsive Node und Aufruf von "shard_manager.on_node_failed(...)"
//  4) Andernfalls blieb dein bestehender Code unverändert.
//
// Achte darauf, dass du evtl. in cargo.toml Features (mdns) etc. definierst,
// falls du run_mdns() nutzen willst.
//
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rand::Rng;
use serde::{Serialize, Deserialize};
use tokio::time::sleep;
use tracing::{info, warn, debug, error};

// NEU => Damit wir DexDB und CrdtSnapshot verwenden können
use crate::storage::replicated_db_layer::{DexDB, CrdtSnapshot};

// Optionales ShardManager, falls du Self-Healing willst:
use crate::shard_logic::ShardManager;

// -----------------------------------------
// NodeId: 256-Bit, Distanzberechnungen, Hilfsmethoden
// -----------------------------------------
pub const ID_LENGTH: usize = 32;

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct NodeId(pub [u8; ID_LENGTH]);

impl NodeId {
    /// Erstellt zufällige NodeId
    pub fn random() -> Self {
        let mut rng = rand::thread_rng();
        let mut id = [0u8; ID_LENGTH];
        rng.fill(&mut id);
        NodeId(id)
    }

    /// XOR mit anderem NodeId, Ergebnis als neue NodeId
    pub fn xor(&self, other: &NodeId) -> NodeId {
        let mut result = [0u8; ID_LENGTH];
        for i in 0..ID_LENGTH {
            result[i] = self.0[i] ^ other.0[i];
        }
        NodeId(result)
    }

    /// Distanz als 128 Bit (vereinfacht)
    pub fn distance_as_u128(&self, other: &NodeId) -> u128 {
        let x = self.xor(other);
        let mut arr = [0u8; 16];
        for i in 0..16 {
            arr[i] = x.0[i];
        }
        u128::from_be_bytes(arr)
    }
}

// -----------------------------------------
// KademliaMessage => P2P-Requests
// -----------------------------------------
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum KademliaMessage {
    Ping(NodeId),
    Pong(NodeId),

    FindNode {
        source: NodeId,
        target: NodeId,
    },
    FindNodeResult {
        source: NodeId,
        closer_nodes: Vec<(NodeId, SocketAddr)>,
    },

    Store {
        source: NodeId,
        key: Vec<u8>,
        data: Vec<u8>,
    },
    StoreResult {
        source: NodeId,
        stored: bool,
    },

    FindValue {
        source: NodeId,
        key: Vec<u8>,
    },
    FindValueResult {
        source: NodeId,
        key: Vec<u8>,
        data: Option<Vec<u8>>,
        closer_nodes: Vec<(NodeId, SocketAddr)>,
    },

    // NEU => Für CRDT-Sync
    CrdtSnapshots(Vec<CrdtSnapshot>),
}

// -----------------------------------------
// Bucket / RoutingTable
// -----------------------------------------
#[derive(Clone, Debug)]
pub struct BucketEntry {
    pub node_id: NodeId,
    pub address: SocketAddr,
    pub last_seen: Instant,
}

#[derive(Debug)]
pub struct KBucket {
    pub entries: VecDeque<BucketEntry>,
    pub capacity: usize,
}

impl KBucket {
    pub fn new(capacity: usize) -> Self {
        KBucket {
            entries: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// upsert => nach vorn
    pub fn upsert(&mut self, node_id: NodeId, address: SocketAddr) {
        if let Some(pos) = self.entries.iter().position(|e| e.node_id == node_id) {
            let mut entry = self.entries.remove(pos).unwrap();
            entry.last_seen = Instant::now();
            entry.address = address;
            self.entries.push_front(entry);
        } else {
            if self.entries.len() >= self.capacity {
                self.entries.pop_back();
            }
            let entry = BucketEntry {
                node_id,
                address,
                last_seen: Instant::now(),
            };
            self.entries.push_front(entry);
        }
    }

    pub fn closest(&self, target: &NodeId, k: usize) -> Vec<BucketEntry> {
        let mut items: Vec<_> = self.entries.iter().cloned().collect();
        items.sort_by_key(|e| e.node_id.distance_as_u128(target));
        items.truncate(k);
        items
    }

    pub fn remove(&mut self, node_id: &NodeId) {
        if let Some(pos) = self.entries.iter().position(|e| &e.node_id == node_id) {
            self.entries.remove(pos);
        }
    }
}

pub struct RoutingTable {
    pub local_id: NodeId,
    pub buckets: Vec<KBucket>,
    pub bucket_size: usize,
}

impl RoutingTable {
    pub fn new(local_id: NodeId, bucket_size: usize) -> Self {
        let mut buckets = Vec::with_capacity(ID_LENGTH * 8);
        for _ in 0..(ID_LENGTH * 8) {
            buckets.push(KBucket::new(bucket_size));
        }
        RoutingTable {
            local_id,
            buckets,
            bucket_size,
        }
    }

    fn bucket_index(&self, node_id: &NodeId) -> usize {
        let x = self.local_id.xor(node_id);
        for (i, byte) in x.0.iter().enumerate() {
            if *byte != 0 {
                let leading = byte.leading_zeros() as usize;
                let bit_pos = i * 8 + leading;
                return bit_pos;
            }
        }
        ID_LENGTH * 8 - 1
    }

    pub fn update_node(&mut self, node_id: NodeId, address: SocketAddr) {
        if node_id == self.local_id {
            return;
        }
        let idx = self.bucket_index(&node_id);
        self.buckets[idx].upsert(node_id, address);
    }

    pub fn remove_node(&mut self, node_id: &NodeId) {
        let idx = self.bucket_index(node_id);
        self.buckets[idx].remove(node_id);
    }

    pub fn find_closest(&self, target: &NodeId, k: usize) -> Vec<(NodeId, SocketAddr)> {
        let mut candidates = Vec::new();
        for bucket in &self.buckets {
            for e in &bucket.entries {
                candidates.push(e.clone());
            }
        }
        candidates.sort_by_key(|e| e.node_id.distance_as_u128(target));
        candidates.truncate(k);
        candidates
            .into_iter()
            .map(|e| (e.node_id, e.address))
            .collect()
    }

    /// Liefert alle (NodeId,Time,Addr) => z.B. in "detect_failures"
    pub fn all_entries(&self) -> Vec<(NodeId, Instant, SocketAddr)> {
        let mut out = Vec::new();
        for b in &self.buckets {
            for e in &b.entries {
                out.push((e.node_id.clone(), e.last_seen, e.address));
            }
        }
        out
    }
}

// -----------------------------------------
// SimpleStorage => optional
// -----------------------------------------
#[derive(Default)]
pub struct SimpleStorage {
    pub data: HashMap<Vec<u8>, Vec<u8>>,
}

impl SimpleStorage {
    pub fn new() -> Self {
        Self { data: HashMap::new() }
    }

    pub fn store(&mut self, key: Vec<u8>, val: Vec<u8>) {
        self.data.insert(key, val);
    }

    pub fn lookup(&self, key: &[u8]) -> Option<&[u8]> {
        self.data.get(key)
    }
}

// -----------------------------------------
// KademliaP2PAdapter => Schnittstelle
// -----------------------------------------
pub trait KademliaP2PAdapter {
    fn send_kademlia_msg(&self, addr: SocketAddr, msg: &KademliaMessage);
    fn local_address(&self) -> SocketAddr;

    /// Optionale ping-Funktion (kann in detect_failures genutzt werden)
    fn ping_node(&self, node_id: NodeId, addr: SocketAddr) -> bool {
        // Default: wir schicken Ping
        let msg = KademliaMessage::Ping(node_id);
        self.send_kademlia_msg(addr, &msg);
        // In echter Implementation => Async Wait => hier dummy
        true
    }
}

// -----------------------------------------
// KademliaService => inkl. Self-Healing
// -----------------------------------------
pub struct KademliaService {
    pub local_id: NodeId,
    pub table: RoutingTable,
    pub storage: SimpleStorage,

    pub p2p: Arc<Mutex<dyn KademliaP2PAdapter + Send>>,
    pub refresh_interval: Duration,
    pub stop_flag: Arc<Mutex<bool>>,

    // NEU => Optionale DB => CRDT-Snapshots sync
    pub db: Option<Arc<DexDB>>,

    // NEU => optionaler ShardManager (für on_node_failed)
    pub shard_manager: Option<Arc<ShardManager>>,

    // Timeout => wie lange "last_seen" in BucketEntry akzeptabel
    // z.B. 300 Sek => danach Node veraltet => wir checken => if unresponsive => remove
    pub node_fail_timeout: Duration,
}

impl KademliaService {
    /// Konstruktor
    pub fn new(
        local_id: NodeId,
        bucket_size: usize,
        p2p_adapter: Arc<Mutex<dyn KademliaP2PAdapter + Send>>
    ) -> Self {
        KademliaService {
            local_id: local_id.clone(),
            table: RoutingTable::new(local_id, bucket_size),
            storage: SimpleStorage::new(),
            p2p: p2p_adapter,
            refresh_interval: Duration::from_secs(600),
            stop_flag: Arc::new(Mutex::new(false)),
            db: None,
            shard_manager: None,
            node_fail_timeout: Duration::from_secs(300),
        }
    }

    /// Hängt DexDB an, damit CrdtSnapshots synchronisiert werden können.
    pub fn set_db(&mut self, db: Arc<DexDB>) {
        self.db = Some(db);
    }

    /// Falls du Self-Healing via shard_manager.on_node_failed => setze ihn
    pub fn set_shard_manager(&mut self, sm: Arc<ShardManager>) {
        self.shard_manager = Some(sm);
    }

    /// Startet die Hintergrundprozesse => bucket refresh + node-failure-detection
    pub async fn run_service(&self) {
        info!("KademliaService {} => starting main loop", hex::encode(&self.local_id.0));

        // 1) Bucket-Refresh + indefinite loop
        let sf_c = self.stop_flag.clone();
        let me_id = self.local_id.clone();
        let refresh_i = self.refresh_interval;
        let me = self as *const KademliaService; // raw pointer -> careful
        tokio::spawn(async move {
            let me_ref = unsafe { &*me };
            while !*sf_c.lock().unwrap() {
                me_ref.refresh_buckets().await;
                sleep(refresh_i).await;
            }
            debug!("Bucket-Refresh-Task ended => local_id={}", hex::encode(&me_id.0));
        });

        // 2) Node-Failure-Detection
        let sf2 = self.stop_flag.clone();
        let me2 = self as *const KademliaService;
        tokio::spawn(async move {
            let me_ref2 = unsafe { &*me2 };
            while !*sf2.lock().unwrap() {
                me_ref2.detect_failed_nodes().await;
                sleep(Duration::from_secs(60)).await; 
            }
            debug!("Node-Failure-Detection-Task ended => local_id={}", hex::encode(&me_ref2.local_id.0));
        });

        // Hier blocken wir nicht => caller kann await ...
    }

    /// stop => setze stop_flag => tasks enden
    pub fn stop(&self) {
        let mut sf = self.stop_flag.lock().unwrap();
        *sf = true;
    }

    /// bucket refresh => generiere IDs => find_node
    async fn refresh_buckets(&self) {
        debug!("Kademlia => refreshing buckets...");
        let buckets_count = ID_LENGTH * 8;
        for i in 0..buckets_count {
            if *self.stop_flag.lock().unwrap() {
                break;
            }
            let mut target = self.local_id.clone();
            let byte_index = i / 8;
            let bit_index = i % 8;
            target.0[byte_index] ^= 1 << (7 - bit_index);

            self.find_node(target).await;
            sleep(Duration::from_millis(50)).await;
        }
    }

    /// detect_failed_nodes => check last_seen older than node_fail_timeout => ping => if fail => remove + shard_manager?
    async fn detect_failed_nodes(&self) {
        debug!("Kademlia => detect_failed_nodes => checking ...");
        let now = Instant::now();
        let entries = self.table.all_entries();
        for (nid, seen, addr) in entries {
            let age = now.duration_since(seen);
            if age > self.node_fail_timeout {
                // versuche ping
                let ok = self.p2p.lock().unwrap().ping_node(nid.clone(), addr);
                if !ok {
                    // => remove
                    debug!("Node {:?} => ping_node immediate fail => remove", hex::encode(&nid.0[..4]));
                    self.remove_node(&nid);
                } else {
                    // In echter Implementierung => wait for Pong or not => hier Dummy
                    // wir simulieren => falls alt => wir entfernen anyway
                    let still_age = now.duration_since(seen);
                    if still_age > (self.node_fail_timeout * 2) {
                        debug!("Node {:?} => too old => removing", hex::encode(&nid.0[..4]));
                        self.remove_node(&nid);
                    }
                }
            }
        }
    }

    /// Node entfernen => optional shard_manager.on_node_failed
    pub fn remove_node(&self, node_id: &NodeId) {
        self.table.remove_node(node_id);
        if let Some(sm) = &self.shard_manager {
            info!("Kademlia => Node {:?} removed => call shard_manager.on_node_failed", hex::encode(&node_id.0[..4]));
            sm.on_node_failed(node_id);
        }
    }

    /// find_node => parallel alpha, wie gehabt
    pub async fn find_node(&self, target: NodeId) -> Vec<(NodeId, SocketAddr)> {
        let alpha = 3;
        let k = self.table.bucket_size;
        let mut closest = self.table.find_closest(&target, k);

        let mut queried = Vec::new();
        let mut improved = true;
        while improved {
            improved = false;
            let next_nodes: Vec<_> = closest
                .iter()
                .filter(|(nid, _)| !queried.contains(nid))
                .take(alpha)
                .cloned()
                .collect();
            if next_nodes.is_empty() {
                break;
            }
            for (nid, _) in &next_nodes {
                queried.push(*nid);
            }
            for (nid, addr) in next_nodes {
                debug!("Sending FIND_NODE({}) to {}", hex::encode(&target.0), addr);
                let msg = KademliaMessage::FindNode {
                    source: self.local_id.clone(),
                    target: target.clone(),
                };
                self.send_msg(addr, &msg);
            }
            sleep(Duration::from_millis(200)).await;
            let now_closest = self.table.find_closest(&target, k);
            if now_closest != closest {
                closest = now_closest;
                improved = true;
            }
        }
        closest
    }

    fn send_msg(&self, addr: SocketAddr, msg: &KademliaMessage) {
        let locked = self.p2p.lock().unwrap();
        locked.send_kademlia_msg(addr, msg);
    }

    /// handle_message => P2P-Callback
    pub fn handle_message(&mut self, sender_addr: SocketAddr, msg: KademliaMessage) {
        match msg {
            KademliaMessage::Ping(node_id) => {
                debug!("Received PING from {}", node_id_to_hex(&node_id));
                self.table.update_node(node_id.clone(), sender_addr);
                let pong = KademliaMessage::Pong(self.local_id.clone());
                self.send_msg(sender_addr, &pong);
            }
            KademliaMessage::Pong(node_id) => {
                debug!("Received PONG from {}", node_id_to_hex(&node_id));
                self.table.update_node(node_id, sender_addr);
            }
            KademliaMessage::FindNode { source, target } => {
                debug!("Received FIND_NODE from {}, target={}", node_id_to_hex(&source), node_id_to_hex(&target));
                self.table.update_node(source.clone(), sender_addr);
                let k = self.table.bucket_size;
                let closer = self.table.find_closest(&target, k);
                let result = KademliaMessage::FindNodeResult {
                    source: self.local_id.clone(),
                    closer_nodes: closer,
                };
                self.send_msg(sender_addr, &result);
            }
            KademliaMessage::FindNodeResult { source, closer_nodes } => {
                debug!("Received FindNodeResult from {}, {} nodes", node_id_to_hex(&source), closer_nodes.len());
                self.table.update_node(source.clone(), sender_addr);
                for (nid, addr) in closer_nodes {
                    self.table.update_node(nid, addr);
                }
            }
            KademliaMessage::Store { source, key, data } => {
                debug!("Received STORE from {}, key={:?}, data.len={}", node_id_to_hex(&source), key, data.len());
                self.table.update_node(source, sender_addr);
                self.storage.store(key.clone(), data.clone());
                let ack = KademliaMessage::StoreResult {
                    source: self.local_id.clone(),
                    stored: true,
                };
                self.send_msg(sender_addr, &ack);
            }
            KademliaMessage::StoreResult { source, stored } => {
                debug!("Received StoreResult => stored={}, from {}", stored, node_id_to_hex(&source));
                self.table.update_node(source, sender_addr);
            }
            KademliaMessage::FindValue { source, key } => {
                debug!("Received FIND_VALUE from {}, key={:?}", node_id_to_hex(&source), key);
                self.table.update_node(source.clone(), sender_addr);
                let data_opt = self.storage.lookup(&key).map(|v| v.to_vec());
                let mut closer_nodes = vec![];
                if data_opt.is_none() {
                    let k = self.table.bucket_size;
                    closer_nodes = self.table.find_closest(&NodeId::random(), k);
                }
                let resp = KademliaMessage::FindValueResult {
                    source: self.local_id.clone(),
                    key,
                    data: data_opt,
                    closer_nodes,
                };
                self.send_msg(sender_addr, &resp);
            }
            KademliaMessage::FindValueResult { source, key, data, closer_nodes } => {
                debug!("Received FIND_VALUE_RESULT from {} => data.len={:?}, {} closer nodes",
                    node_id_to_hex(&source),
                    data.as_ref().map(|d| d.len()),
                    closer_nodes.len()
                );
                self.table.update_node(source, sender_addr);
                // optional: hier local cachen
            }

            // NEU => CRDT-Snapshots
            KademliaMessage::CrdtSnapshots(remote_snaps) => {
                debug!("Received CRDTSnapshots => count={}", remote_snaps.len());
                if let Some(ref db) = self.db {
                    if let Err(e) = db.sync_with_remote(remote_snaps) {
                        error!("sync_with_remote => error: {:?}", e);
                    }
                } else {
                    warn!("Received CRDT-Snapshots, but no db is set in KademliaService!");
                }
            }
        }
    }
}

// Hilfsfunktion => NodeId gekürzt
fn node_id_to_hex(id: &NodeId) -> String {
    hex::encode(&id.0[..4])
}

// -----------------------------------------
// Optional: mDNS + run_kademlia Demo
// -----------------------------------------
#[cfg(feature = "mdns")]
use mdns::{RecordKind, Error as MdnsError};
#[cfg(feature = "mdns")]
use futures_util::{pin_mut, StreamExt};

pub async fn run_mdns() -> Result<(), Box<dyn std::error::Error>> {
    use mdns::discover;
    info!("Starte mDNS-Discovery für `_mydex._udp.local` ...");
    let stream = discover::all("_mydex._udp.local")?
        .listen();
    pin_mut!(stream);

    while let Some(Ok(resp)) = stream.next().await {
        for record in resp.records() {
            match record.kind {
                RecordKind::A(addr) => {
                    info!("mDNS gefunden: IP {:?}", addr);
                    // e.g. update table
                }
                RecordKind::AAAA(addr6) => {
                    info!("mDNS IPv6 gefunden: {:?}", addr6);
                }
                RecordKind::PTR(ptr) => {
                    info!("mDNS PTR: {:?}", ptr);
                }
                RecordKind::SRV(srv) => {
                    info!("mDNS SRV => Port={}, Target={}", srv.port, srv.target);
                }
                _ => {}
            }
        }
    }
    Ok(())
}

pub async fn run_kademlia() -> Result<(), Box<dyn std::error::Error>> {
    use tokio::time::sleep;
    info!("Starte Kademlia-Demo ... (Platzhalter)");
    sleep(Duration::from_secs(5)).await;
    info!("Kademlia-Demo => ende");
    Ok(())
}
