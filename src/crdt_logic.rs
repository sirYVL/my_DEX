// my_dex/src/crdt_logic.rs
//
// CRDT-Logik für dein DEX-Projekt – GCounter-basierter Partial-Fill
// mit ein paar Edge-Case-Handlings.
//
// - Falls fill_amount <= 0.0 => error
// - Falls sum>=quantity => fully-filled => remove
// - Falls offline? Wir erlauben local changes.
// - Merge => max pro Node => konfliktfrei
// - (Kleine Warnungen / Logs)
//
// NEU: Signaturfelder in Order + verify_signature() + Optionale Methode
//      add_local_order_with_signature(...)

use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn, debug, instrument};

use crate::error::DexError;
use crate::metrics::{CRDT_MERGE_COUNT, PARTIAL_FILL_COUNT};

// Beispiel: Damit du Signaturen validieren kannst, brauchst du evtl. 
// eine Krypto-Lib wie ed25519_dalek. Hier minimal:
use ed25519_dalek::{PublicKey, Signature, Verifier}; 
use sha2::{Sha256, Digest};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Order {
    pub id: String,
    pub user_id: String,
    pub timestamp: u64,
    pub quantity: f64,
    pub price: f64,
    
    // NEU: Optionale Signaturfelder
    pub signature: Option<Vec<u8>>,
    pub public_key: Option<Vec<u8>>,
}

impl Order {
    /// Beispiel-Implementierung einer Signaturprüfung.
    /// In echter Produktion müsstest du definieren,
    /// wie genau du signierst (Nachricht, Hashtyp, etc.).
    pub fn verify_signature(&self) -> bool {
        // Falls wir gar keine Signatur haben, return false
        let (Some(sig_bytes), Some(pk_bytes)) = (self.signature.as_ref(), self.public_key.as_ref()) else {
            return false;
        };

        // Versuche, PublicKey + Signature zu parse
        let Ok(pubkey) = PublicKey::from_bytes(pk_bytes) else {
            return false;
        };
        let Ok(signature) = Signature::from_bytes(sig_bytes) else {
            return false;
        };

        // Definiere die zu signierende Nachricht – z. B. "id + user_id + quantity + price + timestamp"
        let msg = format!("{}:{}:{}:{}:{}",
            self.id, 
            self.user_id, 
            self.quantity, 
            self.price, 
            self.timestamp
        );
        // Digest optional, wir könnten die Bytes direkt signieren.
        // Hier als Bsp: Sha256
        let mut hasher = Sha256::new();
        hasher.update(msg.as_bytes());
        let hashed = hasher.finalize();

        // Prüfe Signatur
        pubkey.verify(&hashed, &signature).is_ok()
    }
}

#[derive(Clone, Debug)]
pub struct CrdtDot {
    pub node_id: String,
    pub counter: u64,
}

// GCounter => node => val
pub type GCounter = HashMap<String, u64>;

#[derive(Clone, Debug)]
pub struct CrdtORSet {
    pub adds: HashMap<Order, HashSet<CrdtDot>>,
    pub removes: HashMap<Order, HashSet<CrdtDot>>,
}

#[derive(Clone, Debug)]
pub struct CrdtState {
    pub orset: CrdtORSet,
    pub counters: HashMap<String, u64>,
    pub offline: bool,

    // fill_counters => Key=Order => GCounter
    pub fill_counters: HashMap<Order, GCounter>,
}

impl Default for CrdtState {
    fn default() -> Self {
        Self {
            orset: CrdtORSet {
                adds: HashMap::new(),
                removes: HashMap::new(),
            },
            counters: HashMap::new(),
            offline: false,
            fill_counters: HashMap::new(),
        }
    }
}

impl CrdtState {
    fn next_dot(&mut self, node_id: &str) -> CrdtDot {
        let ctr = self.counters.entry(node_id.to_string()).or_insert(0);
        *ctr += 1;
        CrdtDot {
            node_id: node_id.to_string(),
            counter: *ctr,
        }
    }

    /// Prüft, ob wir offline sind. In diesem Code erlauben wir local changes, 
    /// blocken nur merges => Du kannst es anpassen.
    fn is_visible(&self, ord: &Order) -> bool {
        let adds_opt = self.orset.adds.get(ord);
        if adds_opt.is_none() {
            return false;
        }
        let add_set = adds_opt.unwrap();
        let rm_set = self.orset.removes.get(ord).unwrap_or(&HashSet::new());

        for a in add_set {
            let covered = rm_set.iter().any(|r| r.node_id == a.node_id && r.counter >= a.counter);
            if !covered {
                return true;
            }
        }
        false
    }

    fn find_visible_order(&self, order_id: &str) -> Result<Order, DexError> {
        for (ord, _) in &self.orset.adds {
            if ord.id == order_id && self.is_visible(ord) {
                return Ok(ord.clone());
            }
        }
        Err(DexError::OrderNotFound { order_id: order_id.to_string() })
    }

    fn partial_filled_sum(&self, ord: &Order) -> f64 {
        match self.fill_counters.get(ord) {
            Some(gc) => {
                let mut s = 0.0;
                for (_node, val) in gc {
                    s += *val as f64;
                }
                s
            }
            None => 0.0,
        }
    }

    #[instrument(name="crdt_add_local_order", skip(self))]
    pub fn add_local_order(
        &mut self,
        node_id: &str,
        order_id: &str,
        user_id: &str,
        quantity: f64,
        price: f64,
    ) -> Result<(), DexError> {
        let dot = self.next_dot(node_id);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| DexError::Other("SystemTime error".into()))?
            .as_secs();

        if quantity <= 0.0 {
            return Err(DexError::Other("Quantity must be >0".into()));
        }

        // NEU: Falls du hier signierte Orders anlegen willst,
        // müsstest du signature / public_key generieren. 
        // Hier nur Demo => None
        let ord = Order {
            id: order_id.to_string(),
            user_id: user_id.to_string(),
            timestamp: now,
            quantity,
            price,
            signature: None,
            public_key: None,
        };

        let addset = self.orset.adds.entry(ord.clone()).or_insert_with(HashSet::new);
        addset.insert(dot);

        // fill_counters => init GCounter
        self.fill_counters.entry(ord.clone()).or_insert_with(HashMap::new);

        info!("Local add => order_id={}, node_id={}, q={}", order_id, node_id, quantity);
        Ok(())
    }

    /// NEU: Variante, die bereits Signatur + PubKey nimmt
    /// und verify_signature() prüft. Schlägt fehl => kein Add.
    #[instrument(name="crdt_add_local_order_with_signature", skip(self, signature, public_key))]
    pub fn add_local_order_with_signature(
        &mut self,
        node_id: &str,
        order_id: &str,
        user_id: &str,
        quantity: f64,
        price: f64,
        signature: Vec<u8>,
        public_key: Vec<u8>,
    ) -> Result<(), DexError> {
        if quantity <= 0.0 {
            return Err(DexError::Other("Quantity must be >0".into()));
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| DexError::Other("SystemTime error".into()))?
            .as_secs();

        let dot = self.next_dot(node_id);

        // Baue signiertes Order
        let ord = Order {
            id: order_id.to_string(),
            user_id: user_id.to_string(),
            timestamp: now,
            quantity,
            price,
            signature: Some(signature),
            public_key: Some(public_key),
        };

        // => Check sign
        if !ord.verify_signature() {
            return Err(DexError::Other("Invalid signature in add_local_order_with_signature"));
        }

        let addset = self.orset.adds.entry(ord.clone()).or_insert_with(HashSet::new);
        addset.insert(dot);

        // fill_counters => init
        self.fill_counters.entry(ord.clone()).or_insert_with(HashMap::new);

        info!("Local add (signed) => order_id={}, node_id={}, q={}", order_id, node_id, quantity);
        Ok(())
    }

    #[instrument(name="crdt_remove_local_order", skip(self))]
    pub fn remove_local_order(&mut self, node_id: &str, order_id: &str) -> Result<(), DexError> {
        let dot = self.next_dot(node_id);

        let found = self.find_visible_order(order_id)?;
        let rmset = self.orset.removes.entry(found.clone()).or_insert_with(HashSet::new);
        rmset.insert(dot);

        info!("Local remove => order_id={}, node_id={}", order_id, node_id);
        Ok(())
    }

    /// GCounter-basierter partial fill
    #[instrument(name="crdt_partial_fill", skip(self))]
    pub fn partial_fill(
        &mut self,
        node_id: &str,
        order_id: &str,
        fill_amount: f64,
        min_fill: f64
    ) -> Result<(), DexError> {
        // check fill_amount > 0
        if fill_amount <= 0.0 {
            return Err(DexError::PartialFillError {
                order_id: order_id.to_string(),
                reason: "fill_amount <= 0".into(),
            });
        }
        if fill_amount < min_fill {
            return Err(DexError::PartialFillError {
                order_id: order_id.to_string(),
                reason: format!("fill_amount {} < min_fill {}", fill_amount, min_fill),
            });
        }

        let ord = self.find_visible_order(order_id)?;
        let sum_now = self.partial_filled_sum(&ord);
        let remain = ord.quantity - sum_now;
        if remain <= 0.0 {
            return Err(DexError::PartialFillError {
                order_id: order_id.to_string(),
                reason: "already fully filled".into()
            });
        }
        if fill_amount > remain {
            return Err(DexError::PartialFillError {
                order_id: order_id.to_string(),
                reason: "fill_amount bigger than remain".into()
            });
        }

        // increment GCounter
        let dot = self.next_dot(node_id); 
        let gc = self.fill_counters.entry(ord.clone()).or_insert_with(HashMap::new);
        let old_val = gc.get(node_id).cloned().unwrap_or(0);
        let inc = fill_amount as u64; // naive rounding
        let new_val = old_val + inc;
        gc.insert(node_id.to_string(), new_val);

        PARTIAL_FILL_COUNT.inc();
        info!("Partial fill => order_id={}, fill_amount={}, newSum={}", order_id, fill_amount, sum_now + (inc as f64));

        // if sum >= quantity => remove
        let new_sum = sum_now + (inc as f64);
        if new_sum >= ord.quantity {
            let rmset = self.orset.removes.entry(ord.clone()).or_insert_with(HashSet::new);
            rmset.insert(dot); 
            info!("Order {} => fully filled => removing from CRDT", order_id);
        }

        Ok(())
    }

    #[instrument(name="crdt_set_offline", skip(self))]
    pub fn set_offline(&mut self, offline: bool) {
        self.offline = offline;
        if offline {
            warn!("Node => offline => merges blocked!");
        } else {
            info!("Node => online => merges allowed!");
        }
    }

    #[instrument(name="crdt_merge_remote", skip(self, remote))]
    pub fn merge_remote(&mut self, node_id: &str, remote: &CrdtState) -> Result<(), DexError> {
        if self.offline {
            return Err(DexError::NetworkPartition);
        }
        CRDT_MERGE_COUNT.inc();

        // union => orset adds, removes
        for (o, adddots) in &remote.orset.adds {
            let local = self.orset.adds.entry(o.clone()).or_insert_with(HashSet::new);
            for d in adddots {
                local.insert(d.clone());
            }
        }
        for (o, rmdots) in &remote.orset.removes {
            let local = self.orset.removes.entry(o.clone()).or_insert_with(HashSet::new);
            for d in rmdots {
                local.insert(d.clone());
            }
        }

        // counters => maxima
        for (nid, c) in &remote.counters {
            let localctr = self.counters.entry(nid.clone()).or_insert(0);
            if *c > *localctr {
                *localctr = *c;
            }
        }

        // fill_counters => GCounter => node => max
        for (ord, their_gc) in &remote.fill_counters {
            let local_gc = self.fill_counters.entry(ord.clone()).or_insert_with(HashMap::new);
            for (their_node, their_val) in their_gc {
                let local_val = local_gc.entry(their_node.clone()).or_insert(0);
                if *their_val > *local_val {
                    *local_val = *their_val;
                }
            }
        }

        debug!("merge_remote => done for node_id={}", node_id);
        Ok(())
    }

    #[instrument(name="crdt_visible_orders", skip(self))]
    pub fn visible_orders(&self) -> Vec<Order> {
        let mut out = Vec::new();
        for ord in self.orset.adds.keys() {
            if self.is_visible(ord) {
                out.push(ord.clone());
            }
        }
        debug!("crdt_visible_orders => found {} orders", out.len());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::DexError;

    #[test]
    fn test_gcounter_partial_fill_edgecases() {
        let mut st = CrdtState::default();
        st.add_local_order("NodeA", "o1", "alice", 5.0, 100.0).unwrap();

        // fill negative => err
        let e1 = st.partial_fill("NodeA", "o1", -2.0, 0.0001);
        assert!(e1.is_err());

        // fill zero => err
        let e2 = st.partial_fill("NodeA", "o1", 0.0, 0.0001);
        assert!(e2.is_err());

        // fill bigger => 7 => remain=5 => => err
        let e3 = st.partial_fill("NodeA", "o1", 7.0, 0.0001);
        assert!(e3.is_err());

        // partial fill ok => 2
        let pf2 = st.partial_fill("NodeA", "o1", 2.0, 0.0001);
        assert!(pf2.is_ok());
        let sum = st.partial_filled_sum(&st.find_visible_order("o1").unwrap());
        assert_eq!(sum, 2.0);

        // partial fill => 3 => => sum=5 => full => remove
        let pf3 = st.partial_fill("NodeA", "o1", 3.0, 0.0001);
        assert!(pf3.is_ok());
        // => order removed
        let x = st.find_visible_order("o1");
        assert!(x.is_err());
    }
}
