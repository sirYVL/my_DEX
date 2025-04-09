///////////////////////////////////////////////////////////
// my_dex/src/network/cluster_management.rs
///////////////////////////////////////////////////////////

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc::{self, Sender, Receiver};
use tokio::task::JoinHandle;
use tokio::time::{sleep, Instant};
use tracing::{info, debug, warn, error};

/// libmdns: LAN-Discovery, optional
use libmdns::{Responder, ServiceName};

/// Kademlia – wir nehmen an, du hast bereits ein Kademlia-Setup oder ein eigenes Crate.
use crate::kademlia::kademlia_service::{
    KademliaService, NodeId, SimpleStorage, KademliaP2PAdapter, KademliaMessage,
};
use crate::storage::replicated_db_layer::{DexDB, CrdtSnapshot};

/// ClusterManagerConfig => Konfiguration für mDNS, Kademlia, etc.
#[derive(Debug, Clone)]
pub struct ClusterManagerConfig {
    /// mDNS-Dienstname (z.?B. `_mydex._udp.local.`)
    pub mdns_service_name: String,
    /// Mögliche seeds (NodeId + SockAddr) für Kademlia
    pub kademlia_bootstrap_nodes: Vec<(NodeId, std::net::SocketAddr)>,
    /// Lokaler NodeId
    pub local_id: NodeId,
    /// Kademlia-Bucket-Größe
    pub kademlia_bucket_size: usize,
    /// Gossip-Intervall zum Snapshots-Sync
    pub snapshot_sync_interval: Duration,
}

/// ClusterManager => verwaltet mDNS + Kademlia + Snapshot-Sync
pub struct ClusterManager {
    config: ClusterManagerConfig,
    db: Arc<DexDB>,
    kademlia: Arc<Mutex<KademliaService>>,
    mdns_responder: Option<Responder>,
    stop_flag: Arc<Mutex<bool>>,
    snapshot_task: Option<JoinHandle<()>>,
    failover_task: Option<JoinHandle<()>>,
}

impl ClusterManager {
    /// Erzeugt einen neuen ClusterManager.
    pub fn new(config: ClusterManagerConfig, db: Arc<DexDB>) -> Self {
        // 1) RoutingTable + KademliaService anlegen
        let local_id = config.local_id.clone();
        let table = crate::kademlia::kademlia_service::RoutingTable::new(local_id.clone(), config.kademlia_bucket_size);

        // Speicher => wir könnten in Kademlia-Demos z. B. SimpleStorage nehmen
        let storage = SimpleStorage::new(
            Duration::from_secs(600), // cache-lifetime
            Duration::from_secs(300), // republish-interval
            Duration::from_secs(3600),// max_data_age
        );

        // Kademlia: Braucht einen P2P-Adapter => s. unten stub
        let p2p_adapter = Arc::new(Mutex::new(MockKademliaAdapter::new()));
        let mut kad_service = KademliaService::new(
            local_id.clone(),
            config.kademlia_bucket_size,
            p2p_adapter,
            3, // alpha
            Duration::from_secs(600), // refresh_interval
            storage.cache_lifetime,
            storage.republish_interval,
            storage.max_data_age
        );
        // Seeds => table.update_node(...) (falls du Kademlia seeds hast)
        for (seed_id, seed_addr) in &config.kademlia_bootstrap_nodes {
            kad_service.table.update_node(seed_id.clone(), *seed_addr, |_nid,_addr| true);
        }

        ClusterManager {
            config,
            db,
            kademlia: Arc::new(Mutex::new(kad_service)),
            mdns_responder: None,
            stop_flag: Arc::new(Mutex::new(false)),
            snapshot_task: None,
            failover_task: None,
        }
    }

    /// Startet mDNS + Kademlia + Snapshot-Sync
    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // (A) mDNS starten
        self.start_mdns()?;

        // (B) Kademlia => in eigenem Task
        {
            let kad_cloned = Arc::clone(&self.kademlia);
            let sf = Arc::clone(&self.stop_flag);
            tokio::spawn(async move {
                let k = kad_cloned.lock().unwrap();
                k.run_service().await; 
                // blockiert => wenn stop_flag => ...
                info!("KademliaService => beendet");
            });
        }

        // (C) Snapshot-Replikation => in eigener Task
        {
            let dbclone = Arc::clone(&self.db);
            let sf = Arc::clone(&self.stop_flag);
            let interval = self.config.snapshot_sync_interval;
            self.snapshot_task = Some(tokio::spawn(async move {
                while !*sf.lock().unwrap() {
                    // Warte
                    sleep(interval).await;
                    // Sende + Empfange
                    let local_snaps = match dbclone.replicate_state() {
                        Ok(s) => s,
                        Err(e) => {
                            warn!("snapshot replicate_state => {:?}", e);
                            continue;
                        }
                    };
                    debug!("Local Snapshots => #={}", local_snaps.len());

                    // => "Sende an Peers" => Im Demo stub
                    // => "Empfange remote" => in echt Kademlia store/findValue
                    // => Dann dbclone.sync_with_remote(...)

                    // Stub: wir tun so, als ob wir uns selbst syncen
                    let _ = dbclone.sync_with_remote(local_snaps.clone());
                    debug!("Sync => done");
                }
            }));
        }

        // (D) Failover => Node-Fail => wir könnten ein Mechanismus haben, 
        //  der periodisch pingt / Node anruft => if dead => re-shard ...
        {
            let sf = Arc::clone(&self.stop_flag);
            let dbclone = Arc::clone(&self.db);
            self.failover_task = Some(tokio::spawn(async move {
                loop {
                    if *sf.lock().unwrap() {
                        break;
                    }
                    // Stub: wir prüfen "Nodes" in Kademlia => if not responding => 
                    //  => re-shard or replicate 
                    // Hier nur Dummy
                    debug!("Failover Check => no real logic here (demo) ...");
                    sleep(Duration::from_secs(120)).await;
                }
            }));
        }

        // **NEU**: Ruft perform_initial_sync_for_new_node, 
        // falls wir erkennen, dass wir "ein Node" sind, 
        // der neu hinzukam. (In echt: Logik, wie man es feststellt.)
        let local_id_copy = self.config.local_id.clone();
        self.perform_initial_sync_for_new_node(&local_id_copy).await?;

        Ok(())
    }

    /// Stoppt => stop_flag = true => warte, bis tasks enden
    pub async fn stop(&mut self) {
        {
            let mut sf = self.stop_flag.lock().unwrap();
            *sf = true;
        }
        if let Some(h) = self.snapshot_task.take() {
            let _ = h.await;
        }
        if let Some(h) = self.failover_task.take() {
            let _ = h.await;
        }
        // Kademlia => da run_service blockiert => i. d. R. abort
        info!("ClusterManager => all tasks ended");
    }

    /// Startet mDNS => published service => discovered peers
    fn start_mdns(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // crate "libmdns"
        let responder = Responder::spawn(&tokio::runtime::Handle::current())?;
        let service_name = self.config.mdns_service_name.clone();
        let _svc = responder.register(
            ServiceName::new(&service_name)?,
            "_tcp", // protocol
            8000,   // your local port
            &[]
        );
        info!("mDNS => started => service_name={}", service_name);
        self.mdns_responder = Some(responder);
        Ok(())
    }

    /// Ein einfacher Stub-Adapter, der KademliaMessage per 
    /// Stub an handle_message(...) leitet.
    pub struct MockKademliaAdapter {
        pub inbound_tx: Sender<KademliaMessage>,
    }

    impl MockKademliaAdapter {
        pub fn new() -> Self {
            let (tx, mut rx) = mpsc::channel(100);

            // Worker => loop
            tokio::spawn(async move {
                while let Some(msg) = rx.recv().await {
                    debug!("(MockKademliaAdapter) => got msg: {:?}", msg);
                    // In real => wir würdest handle
                }
            });

            MockKademliaAdapter {
                inbound_tx: tx,
            }
        }
    }

    impl KademliaP2PAdapter for MockKademliaAdapter {
        fn send_kademlia_msg(&self, _addr: std::net::SocketAddr, msg: &KademliaMessage) {
            let msg_cloned = msg.clone();
            let _ = self.inbound_tx.try_send(msg_cloned);
        }

        fn local_address(&self) -> std::net::SocketAddr {
            "0.0.0.0:9999".parse().unwrap()
        }

        fn ping_node(&self, _node_id: &NodeId, _addr: std::net::SocketAddr) -> bool {
            true
        }
    }

    // --------------------------------------------------------
    // **NEU**: Logik, um "Initial Sync" von random Node zu bekommen.
    // --------------------------------------------------------
    /// Wird von `start()` aufgerufen, wenn wir feststellen "wir sind neu".
    /// Sucht im Kademlia-RoutingTable einen beliebigen existierenden Node,
    /// fragt dessen Snapshots ab, und synchronisiert in DB.
    pub async fn perform_initial_sync_for_new_node(
        &self,
        new_node_id: &NodeId
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Step 1: Schauen, ob wir schon Snapshots haben => falls >0 => wir sind kein brand-new Node
        let existing_list = self.db.list_crdt_snapshots()?;
        if !existing_list.is_empty() {
            // Dann brauchen wir kein Full-Sync => return Ok
            debug!("Node {:?} => already has snapshots => skip initial sync", new_node_id);
            return Ok(());
        }

        info!("Node {:?} => no local snapshots => performing initial sync from random peer...", new_node_id);

        // Step 2: Aus Kademlia => wähle random Peer
        let kad = self.kademlia.lock().unwrap();
        let peers = kad.table.all_entries();
        if peers.is_empty() {
            warn!("No peers known => can't do initial sync => maybe we are alone?");
            return Ok(()); 
        }
        use rand::seq::SliceRandom;
        let mut rng = rand::thread_rng();
        let random_peer = peers.choose(&mut rng)
            .ok_or_else(|| "No peers in list, can't initial sync")?;
        let (peer_nodeid, _last_seen, peer_addr) = random_peer;

        info!("Chose peer {:?} => sock={} as snapshot source", peer_nodeid, peer_addr);

        // Step 3: Sende KademliaMessage => "Give me all your snapshots"
        let request = KademliaMessage::FindValue {
            source: new_node_id.clone(),
            key: b"ALL_SNAPSHOTS".to_vec(), 
        };
        kad.send_msg(*peer_addr, &request);

        // Step 4: In echt => wir warten asynchron auf "FindValueResult" => remote_snaps => db.sync_with_remote
        // Hier machen wir minimal:
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        // stub => "simulate got remote 3 snapshots"
        let fake_remote = vec![
            CrdtSnapshot { version: 100, data: vec![11,22,33] },
            CrdtSnapshot { version: 101, data: vec![44,55,66] },
            CrdtSnapshot { version: 102, data: vec![77,88,99] },
        ];
        self.db.sync_with_remote(fake_remote)?;
        info!("Initial sync done => Node {:?} now has snapshots from peer={:?}.", new_node_id, peer_nodeid);

        Ok(())
    }
}
