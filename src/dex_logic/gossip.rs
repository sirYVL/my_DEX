// my_dex/src/dex_logic/gossip.rs
//
// Gossip-Logik => Delta-Gossip => CRDT-Integrität => 
//
// NEU (Sicherheitsupdate):
// Wir führen eine Signaturprüfung für das Gossip-Delta ein,
// damit ein bösartiger Node keine manipulierten Deltas einschleusen kann.
//
// Schritte:
//  1) ITCBookDelta bekommt (signature, public_key) + sign_delta(...) & verify_signature(...)
//  2) Node kann optional ein Keypair halten und in create_delta() das Delta signieren.
//  3) merge_delta() prüft Signatur => bei ungültig => Abbruch.

use crate::dex_logic::itc_crdt_orderbook::ITCOrderBook;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;

// Für die Signatur => ed25519_dalek
use ed25519_dalek::{Keypair, PublicKey, Signature, Signer, Verifier};
use sha2::{Sha256, Digest};

/// Ein Gossip 'Delta'
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ITCBookDelta {
    pub version: serde_json::Value, // we might store partial version
    pub orset: serde_json::Value,

    // NEU: Signatur-Felder, um Manipulation zu verhindern
    pub signature: Option<Vec<u8>>,
    pub public_key: Option<Vec<u8>>,
}

impl ITCBookDelta {
    /// Signiert das Delta auf Basis von (version + orset).
    /// In echter Production könntest du auch Timestamps, Node-IDs etc. mit reinhashen.
    pub fn sign_delta(&mut self, keypair: &Keypair) {
        // 1) Serialize => Bytes
        let data = serde_json::to_vec(&( &self.version, &self.orset ))
            .expect("Failed to serialize delta data");

        // 2) Hash
        let mut hasher = Sha256::new();
        hasher.update(data);
        let hash = hasher.finalize();

        // 3) Sign
        let sig = keypair.sign(&hash);
        self.signature = Some(sig.to_bytes().to_vec());
        self.public_key = Some(keypair.public.to_bytes().to_vec());
    }

    /// Prüft, ob signature + public_key valide sind.
    pub fn verify_signature(&self) -> bool {
        // Falls keins von beiden existiert => false
        let (Some(sig_bytes), Some(pk_bytes)) = (self.signature.as_ref(), self.public_key.as_ref()) else {
            return false;
        };

        let Ok(pubkey) = PublicKey::from_bytes(pk_bytes) else {
            return false;
        };
        let Ok(signature) = Signature::from_bytes(sig_bytes) else {
            return false;
        };

        // Rebuild hash
        let data = serde_json::to_vec(&( &self.version, &self.orset ))
            .expect("Failed to serialize delta data");
        let mut hasher = Sha256::new();
        hasher.update(data);
        let hash = hasher.finalize();

        pubkey.verify(&hash, &signature).is_ok()
    }
}

/// Node => besitzt einen ITCOrderBook
#[derive(Clone, Debug)]
pub struct Node {
    pub node_id: String,
    pub itc_book: ITCOrderBook,

    // NEU: optionales Keypair => Falls ein Node signieren möchte
    pub keypair: Option<Keypair>,
}

impl Node {
    pub fn new(node_id: &str) -> Self {
        Self {
            node_id: node_id.to_string(),
            itc_book: ITCOrderBook::new(),
            keypair: None,
        }
    }

    /// Erzeugt das Delta (version + orset) und signiert es optional,
    /// falls wir ein Keypair besitzen.
    pub fn create_delta(&self) -> ITCBookDelta {
        let v = serde_json::to_value(&self.itc_book.version).unwrap();
        let o = serde_json::to_value(&self.itc_book.orset).unwrap();
        let mut delta = ITCBookDelta {
            version: v,
            orset: o,
            signature: None,
            public_key: None,
        };

        // Falls wir signieren wollen => Keypair anlegen in Node
        if let Some(ref kp) = self.keypair {
            delta.sign_delta(kp);
        }

        delta
    }

    /// Nimmt ein Delta an => verifiziert Signatur => merge, wenn ok
    pub fn merge_delta(&mut self, delta: ITCBookDelta) {
        // Signatur-Check
        if !delta.verify_signature() {
            // => Im Produktionscode ggf. logging, event, abbruch
            println!("Warn: Ungültige oder fehlende Signatur => Delta ignoriert.");
            return;
        }

        let other_version = serde_json::from_value(delta.version).unwrap();
        let other_orset = serde_json::from_value(delta.orset).unwrap();
        let other_book = ITCOrderBook {
            version: other_version,
            orset: other_orset,
        };
        self.itc_book.merge(&other_book);
    }
}

/// GossipNet => simuliert ein kleines Netzwerk
#[derive(Clone, Debug)]
pub struct GossipNet {
    pub nodes: HashMap<String, bool>, // node_id -> dummy
}

impl GossipNet {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
        }
    }

    pub fn add_node(&mut self, node: &Node) {
        self.nodes.insert(node.node_id.clone(), true);
    }

    pub fn tick(&self, node: &mut Node) {
        // Sende Delta an alle => in echt würdest du peers pingen
        let delta = node.create_delta();
        // normal: net würde "peer_node.merge_delta(delta)" aufrufen
        // hier nur demonstration => do nothing
    }
}
