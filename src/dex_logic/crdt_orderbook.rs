// my_dex/src/dex_logic/crdt_orderbook.rs
//
// Rudimentäre CRDT-Implementierung (OR-Set) + passendes Versioning
// für dein dezentrales Orderbuch.
//
// Zusätzlich: Wir prüfen in add_order(), ob die Order signiert ist
// (mithilfe von order.verify_signature()) und geben bei Ungültigkeit
// einen DexError zurück.

use serde::{Serialize, Deserialize};
use std::collections::{HashMap, HashSet};

use super::orders::Order; // <-- Stellt sicher, dass dieses 'Order' Signaturfelder und verify_signature() besitzt.
use crate::error::DexError;  // <-- Wir werfen DexError zurück, wenn Signatur invalid ist.

/// Dotted-Version / Dot
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DottedVersion {
    pub versions: HashMap<String, u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderDot {
    pub node_id: String,
    pub counter: u64,
}

impl DottedVersion {
    pub fn new() -> Self {
        Self {
            versions: HashMap::new(),
        }
    }

    pub fn increment(&mut self, node_id: &str) -> OrderDot {
        let c = self.versions.entry(node_id.to_string()).or_insert(0);
        *c += 1;
        OrderDot {
            node_id: node_id.to_string(),
            counter: *c,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ORSet<T> {
    pub adds: HashMap<T, HashSet<(String, u64)>>,
    pub removes: HashMap<T, HashSet<(String, u64)>>,
}

impl<T: std::cmp::Eq + std::hash::Hash + Clone> ORSet<T> {
    pub fn new() -> Self {
        Self {
            adds: HashMap::new(),
            removes: HashMap::new(),
        }
    }

    /// Fügt ein Element mit dem zugehörigen (node_id, counter) in das CRDT ein.
    pub fn add(&mut self, elem: T, dot: (String, u64)) {
        self.adds
            .entry(elem)
            .or_insert_with(HashSet::new)
            .insert(dot);
    }

    /// Markiert das Element als entfernt, indem wir die Dot-Einträge aus adds in removes übernehmen.
    pub fn remove(&mut self, elem: &T) {
        if let Some(adset) = self.adds.get(elem) {
            self.removes
                .entry(elem.clone())
                .or_insert_with(HashSet::new)
                .extend(adset.iter().cloned());
        }
    }

    /// Prüft, ob das Element sichtbar ist (d. h. es ist nicht "komplett" in removes).
    /// Wir sagen: falls adset eine Teilmenge von rmset ist => elem gilt als entfernt.
    pub fn lookup(&self, elem: &T) -> bool {
        if let Some(adset) = self.adds.get(elem) {
            let rmset = self.removes.get(elem).unwrap_or(&HashSet::new());
            !adset.is_subset(rmset)
        } else {
            false
        }
    }

    /// Vereinigt das ORSet mit einem anderen => union.
    /// adds => union, removes => union
    pub fn merge(&mut self, other: &Self) {
        // Union der adds
        for (k, vset) in &other.adds {
            self.adds
                .entry(k.clone())
                .or_insert_with(HashSet::new)
                .extend(vset.iter().cloned());
        }
        // Union der removes
        for (k, vset) in &other.removes {
            self.removes
                .entry(k.clone())
                .or_insert_with(HashSet::new)
                .extend(vset.iter().cloned());
        }
    }

    /// Liefert alle sichtbaren (aktiven) Elemente zurück.
    pub fn all_visible(&self) -> Vec<T> {
        let mut out = Vec::new();
        for (k, _) in &self.adds {
            if self.lookup(k) {
                out.push(k.clone());
            }
        }
        out
    }
}

/// Das eigentliche CRDT-Orderbuch
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderBookCRDT {
    pub version: DottedVersion,
    pub orset: ORSet<Order>,
}

impl OrderBookCRDT {
    pub fn new() -> Self {
        Self {
            version: DottedVersion::new(),
            orset: ORSet::new(),
        }
    }

    /// Fügt eine Order hinzu, führt vorher aber einen Signaturcheck durch.
    /// Bei ungültiger Signatur => Err(DexError).
    pub fn add_order(&mut self, order: Order, node_id: &str) -> Result<(), DexError> {
        // Falls Signatur ungültig => Abbruch
        if !order.verify_signature() {
            return Err(DexError::Other("Ungültige Order-Signatur".into()));
        }

        let dot = self.version.increment(node_id);
        self.orset.add(order, (dot.node_id, dot.counter));
        Ok(())
    }

    /// Entfernt eine Order => wir übernehmen alle Dot-Einträge aus adds in removes.
    pub fn remove_order(&mut self, order: &Order) {
        self.orset.remove(order);
    }

    /// Merge => wir vereinigen unser CRDT mit einem anderen.
    pub fn merge(&mut self, other: &Self) {
        // (Optional) Versions-Merge => hier weggelassen
        self.orset.merge(&other.orset);
    }

    /// Liefert alle sichtbaren Orders zurück.
    pub fn all_orders(&self) -> Vec<Order> {
        self.orset.all_visible()
    }
}
