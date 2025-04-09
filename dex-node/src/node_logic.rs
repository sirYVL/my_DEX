// my_dex/dex-node/src/node_logic.rs
//
// Node-Logik: Lädt Config, startet P2P (vereinfacht), speichert CRDT in sled
//
// NEU (Sicherheitsupdate):
//  - Plausibilitätsprüfung für cfg.fees und cfg.match_interval_sec (kein 0 oder negative Werte).
//  - Hinweis, dass man in add_order() optional signierte Orders checken könnte.

use anyhow::Result;
use serde::{Serialize, Deserialize};
use sled;
use dex_core::{CrdtState, Order, match_orders};
use std::sync::{Arc, Mutex};
use log::{info, error};

#[derive(Debug, Serialize, Deserialize)]
pub struct NodeConfig {
    pub fees: f64,
    pub node_address: String,
    pub db_path: String,
    pub match_interval_sec: u64,
    // weitere Parameter
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            fees: 0.001,
            node_address: "127.0.0.1:9000".to_string(),
            db_path: "./dex_node_db".to_string(),
            match_interval_sec: 10,
        }
    }
}

pub struct DexNode {
    pub config: NodeConfig,
    pub db: sled::Db,
    pub state: Arc<Mutex<CrdtState>>,
}

impl DexNode {
    pub fn new(cfg: NodeConfig) -> Result<Self> {
        // 1) Plausibilitätscheck => fees > 0, match_interval_sec >= 1?
        if cfg.fees <= 0.0 {
            return Err(anyhow::anyhow!("Fees <= 0 => invalid config"));
        }
        if cfg.match_interval_sec == 0 {
            return Err(anyhow::anyhow!("match_interval_sec=0 => invalid config"));
        }

        // 2) DB öffnen
        let db = sled::open(&cfg.db_path)?;
        let tree = db.open_tree("crdt")?;

        // 3) CRDT laden
        let raw = tree.get(b"state")?;
        let state = if let Some(bytes) = raw {
            match bincode::deserialize::<CrdtState>(&bytes) {
                Ok(s) => s,
                Err(e) => {
                    error!("Deserialization error: {:?}", e);
                    CrdtState::new()
                }
            }
        } else {
            CrdtState::new()
        };

        Ok(DexNode {
            config: cfg,
            db,
            state: Arc::new(Mutex::new(state)),
        })
    }

    pub fn save_state(&self) -> Result<()> {
        let tree = self.db.open_tree("crdt")?;
        let st = self.state.lock().unwrap();
        let encoded = bincode::serialize(&*st)?;
        tree.insert(b"state", encoded)?;
        tree.flush()?;
        Ok(())
    }

    pub async fn start(&self) -> Result<()> {
        // Hier könntest du ein P2P-Listener oder Gossip starten
        // Wir simulieren nur einen Matching-Loop
        let interval = self.config.match_interval_sec;
        let shared_state = self.state.clone();
        tokio::spawn(async move {
            loop {
                {
                    let mut st = shared_state.lock().unwrap();
                    match_orders(&mut *st);
                }
                tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
            }
        });

        info!("Node started with address={} fees={}", self.config.node_address, self.config.fees);
        Ok(())
    }

    /// Order hinzufügen (lokal). In echter Security-Logik könntest du hier
    /// checken, ob die Order signiert ist (z. B. order.verify_signature()) und sie bei Ungültigkeit verwerfen.
    pub fn add_order(&self, order: Order) {
        let mut st = self.state.lock().unwrap();
        st.add_order(order);
    }

    pub fn remove_order(&self, order_id: &str) {
        let mut st = self.state.lock().unwrap();
        st.remove_order(order_id);
    }

    pub fn merge_crdt(&self, other: &CrdtState) {
        let mut st = self.state.lock().unwrap();
        st.merge(other);
    }
}
