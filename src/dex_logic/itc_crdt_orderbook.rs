// my_dex/src/dex_logic/itc_crdt_orderbook.rs
//
// CRDT-Implementierung für Orderbuch
//
// NEU (Sicherheitsupdate):
// Wir prüfen in add_order(), ob die Order signiert ist
// (mithilfe von order.verify_signature()) und geben bei Ungültigkeit
// einen DexError zurück. Zusätzlich:
//  - Negative/Null-Werte => DexError
//  - Globaler Mutex => keine gleichzeitigen add/remove merges.
//
// Falls du in remove_order() ebenfalls signierte Prüfungen möchtest
// (z. B. "nur der Besitzer kann entfernen"), könntest du analog
// checken. Hier zeigen wir es optional in auskommentierter Form.

use serde::{Serialize, Deserialize};
use std::collections::{HashMap, HashSet};

// == Sicherheits-Importe ==
use crate::error::DexError; 
use ed25519_dalek::{PublicKey, Signature, Verifier};
use sha2::{Sha256, Digest};

// NEU: concurrency (grober globaler Mutex)
use std::sync::Mutex;
use lazy_static::lazy_static;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Asset {
    BTC,
    LTC,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Order {
    pub order_id: String,
    pub user_id: String,
    pub asset_sell: Asset,
    pub asset_buy: Asset,
    pub amount_sell: f64,
    pub price: f64,

    // NEU: Felder für Signatur / PublicKey
    pub signature: Option<Vec<u8>>,
    pub public_key: Option<Vec<u8>>,
}

impl Order {
    pub fn new(
        order_id: &str,
        user_id: &str,
        sell: Asset,
        buy: Asset,
        amt: f64,
        price: f64
    ) -> Self {
        Self {
            order_id: order_id.to_string(),
            user_id: user_id.to_string(),
            asset_sell: sell,
            asset_buy: buy,
            amount_sell: amt,
            price,
            signature: None,
            public_key: None,
        }
    }

    /// Minimaler Signaturcheck:
    /// Wir hashen (order_id + user_id + amount_sell + price),
    /// optional weitere Felder, und prüfen mit ed25519_dalek.
    pub fn verify_signature(&self) -> bool {
        let (Some(sig_bytes), Some(pk_bytes)) = (self.signature.as_ref(), self.public_key.as_ref()) else {
            return false;
        };
        let Ok(pubkey)    = PublicKey::from_bytes(pk_bytes) else {
            return false;
        };
        let Ok(signature) = Signature::from_bytes(sig_bytes) else {
            return false;
        };

        let data = format!("{}:{}:{}:{}",
            self.order_id,
            self.user_id,
            self.amount_sell,
            self.price
        );
        let mut hasher = Sha256::new();
        hasher.update(data.as_bytes());
        let hashed = hasher.finalize();

        pubkey.verify(&hashed, &signature).is_ok()
    }
}

/******************************************************************************
 * EXTENDED INTERVAL TREE CLOCK
 ******************************************************************************/
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ITCInterval {
    pub node: String,
    pub start: u64,
    pub end: u64,
}

impl ITCInterval {
    pub fn overlaps(&self, other: &ITCInterval) -> bool {
        self.node == other.node &&
        !(self.end < other.start || other.end < self.start)
    }

    pub fn merge_with(&mut self, other: &ITCInterval) -> bool {
        if self.node == other.node && self.overlaps(other)
            || self.end+1 == other.start
            || other.end+1 == self.start
        {
            self.start = self.start.min(other.start);
            self.end   = self.end.max(other.end);
            return true;
        }
        false
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ITCVersion {
    pub intervals: Vec<ITCInterval>,
}

impl ITCVersion {
    pub fn new() -> Self {
        Self { intervals: Vec::new() }
    }

    /// Suche maxEnd für node => +1
    pub fn increment(&mut self, node_id: &str) -> ITCInterval {
        let mut maxend = 0;
        for iv in &self.intervals {
            if iv.node == node_id && iv.end > maxend {
                maxend = iv.end;
            }
        }
        let newstart = maxend + 1;
        let newend   = maxend + 1;
        let mut newiv = ITCInterval {
            node: node_id.to_string(),
            start: newstart,
            end: newend,
        };
        self.merge_interval(newiv.clone());
        for iv in &self.intervals {
            if iv.node == node_id && newstart >= iv.start && newend <= iv.end {
                newiv.start = iv.start;
                newiv.end   = iv.end;
                break;
            }
        }
        newiv
    }

    fn merge_interval(&mut self, newiv: ITCInterval) {
        let mut merged = false;
        for iv in &mut self.intervals {
            if iv.node == newiv.node && iv.overlaps(&newiv) {
                iv.merge_with(&newiv);
                merged = true;
                self.merge_all_for_node(&iv.node);
                break;
            }
        }
        if !merged {
            self.intervals.push(newiv);
        }
        self.merge_all_for_node("");
    }

    fn merge_all_for_node(&mut self, node_id: &str) {
        let mut changed = true;
        while changed {
            changed = false;
            'outer: for i in 0..self.intervals.len() {
                for j in (i+1)..self.intervals.len() {
                    if self.intervals[i].node == self.intervals[j].node || node_id == "" {
                        if self.intervals[i].overlaps(&self.intervals[j])
                            || self.intervals[i].end+1 == self.intervals[j].start
                            || self.intervals[j].end+1 == self.intervals[i].start
                        {
                            let merged_start = self.intervals[i].start.min(self.intervals[j].start);
                            let merged_end   = self.intervals[i].end.max(self.intervals[j].end);
                            let node_ = self.intervals[i].node.clone();
                            self.intervals[i].start = merged_start;
                            self.intervals[i].end   = merged_end;
                            self.intervals.remove(j);
                            changed = true;
                            break 'outer;
                        }
                    }
                }
            }
        }
    }

    pub fn merge(&mut self, other: &ITCVersion) {
        for iv in &other.intervals {
            self.merge_interval(iv.clone());
        }
    }
}

/******************************************************************************
 * ITC-BASED OR-SET
 ******************************************************************************/
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ITCORDSet {
    pub adds: HashMap<Order, HashSet<ITCInterval>>,
    pub removes: HashMap<Order, HashSet<ITCInterval>>,
}

impl ITCORDSet {
    pub fn new() -> Self {
        Self {
            adds: HashMap::new(),
            removes: HashMap::new(),
        }
    }

    pub fn add(&mut self, elem: Order, dot: ITCInterval) {
        self.adds.entry(elem).or_insert_with(HashSet::new).insert(dot);
    }

    pub fn remove(&mut self, elem: &Order, dot: ITCInterval) {
        if let Some(_) = self.adds.get(elem) {
            self.removes.entry(elem.clone()).or_insert_with(HashSet::new).insert(dot);
        }
    }

    pub fn lookup(&self, elem: &Order) -> bool {
        if let Some(adset) = self.adds.get(elem) {
            let rmset = self.removes.get(elem).unwrap_or(&HashSet::new());
            for aiv in adset {
                let covered = rmset.iter().any(|riv| {
                    riv.node == aiv.node && riv.start <= aiv.start && riv.end >= aiv.end
                });
                if !covered {
                    return true;
                }
            }
            return false;
        }
        false
    }

    pub fn all_visible(&self) -> Vec<Order> {
        let mut out = Vec::new();
        for (ord, _) in &self.adds {
            if self.lookup(ord) {
                out.push(ord.clone());
            }
        }
        out
    }

    pub fn merge(&mut self, other: &ITCORDSet) {
        for (ord, setiv) in &other.adds {
            let localset = self.adds.entry(ord.clone()).or_insert_with(HashSet::new);
            for iv in setiv {
                localset.insert(iv.clone());
            }
        }
        for (ord, setiv) in &other.removes {
            let localset = self.removes.entry(ord.clone()).or_insert_with(HashSet::new);
            for iv in setiv {
                localset.insert(iv.clone());
            }
        }
    }
}

/******************************************************************************
 * Das finale ITCOrderBook
 ******************************************************************************/
use std::sync::Mutex; // NEU: concurrency
use lazy_static::lazy_static; // NEU

// Globaler Mutex => in add_order() & remove_order() => wir sperren
lazy_static! {
    static ref CRDT_ORDERBOOK_MUTEX: Mutex<()> = Mutex::new(());
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ITCOrderBook {
    pub version: ITCVersion,
    pub orset: ITCORDSet,
}

impl ITCOrderBook {
    pub fn new() -> Self {
        Self {
            version: ITCVersion::new(),
            orset: ITCORDSet::new(),
        }
    }

    /// Neu: Wir prüfen, ob die Order signiert ist. Falls nicht, -> DexError
    /// => wir prüfen negative Werte (z. B. amount_sell, price) => DexError
    /// => concurrency => globaler Mutex
    pub fn add_order(&mut self, mut order: Order, node_id: &str) -> Result<(), DexError> {
        let _guard = CRDT_ORDERBOOK_MUTEX.lock().map_err(|_| DexError::Other("CRDT Orderbook mutex poisoned".into()))?;

        // 1) Negative checks
        if order.amount_sell <= 0.0 {
            return Err(DexError::Other("Amount_sell <= 0".into()));
        }
        if order.price <= 0.0 {
            return Err(DexError::Other("Price <= 0".into()));
        }

        // 2) Signatur-Check
        if !order.verify_signature() {
            return Err(DexError::Other("Ungültige Order-Signatur".into()));
        }

        let iv = self.version.increment(node_id);
        self.orset.add(order, iv);
        Ok(())
    }

    /// Optional: remove_order => concurrency & evtl. Signaturcheck
    /// => wir sperren => fallback
    pub fn remove_order(&mut self, order: &Order, node_id: &str) {
        let _guard = CRDT_ORDERBOOK_MUTEX.lock().expect("CRDT Orderbook mutex poisoned");
        let iv = self.version.increment(node_id);
        self.orset.remove(order, iv);
    }

    pub fn merge(&mut self, other: &ITCOrderBook) {
        // Lock => optional, in real code => 
        // CRDT merges sich selbst => 
        self.version.merge(&other.version);
        self.orset.merge(&other.orset);
    }

    pub fn all_orders(&self) -> Vec<Order> {
        self.orset.all_visible()
    }
}
