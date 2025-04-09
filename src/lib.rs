///////////////////////////////////////////////////////////
// my_dex/src/lib.rs
///////////////////////////////////////////////////////////
//
// Definiert und re-exportiert deine Module. 
// Alle Module bleiben unverändert an Ort und Stelle, 
// plus Identity (accounts, wallet) und Fees (fee_pool).
//
// Hinweis: Hier werden alle relevanten Module für
// Benutzer-/Wallet-Verwaltung, Fees, Storage und CRDT-Logik
// produktionsreif eingebunden, ohne gekürzte Demos.
//
// Zusätzlich haben wir im network-Ordner nun ein weiteres Submodul
// `p2p_adapter`, das den echten TCP-P2P-Adapter enthält.

pub mod distributed_dht;
pub mod kademlia;
pub mod crypto;

// Identity => Accounts, Wallets
pub mod identity {
    pub mod wallet;
    pub mod accounts;
}

// Sybil-Schutz, Protokoll, etc.
pub mod sybil;
pub mod protocol;

// Network => hier fügen wir das p2p_adapter hinzu:
pub mod network {
    pub mod tcp;
    pub mod noise;
    pub mod secure_channel;
    pub mod p2p_adapter; // NEU: echter P2P-TCP-Adapter
}

// Rate Limiting, Konsens, Noise, Secure Channel ...
pub mod rate_limiting;
pub mod consensus;
pub mod noise;
pub mod secure_channel;
pub mod p2p_order_matcher;

// Dezentralisierte Orderbuch-Logik (CRDT, Fees, usw.)
pub mod decentralized_order_book;

// Falls du schon eine dex_logic-Modulstruktur hast:
pub mod dex_logic {
    // ... vorhandene Unter-Module ...
    pub mod crdt_orderbook;
    pub mod limit_orderbook;
    pub mod orders;
    pub mod fees;
    pub mod htlc;
    pub mod sign_utils;
    pub mod time_limited_orders; // <== Hier einbinden
    // optional: gossip, fuzz_test, etc.
}

// Zusätzliche Demos (falls benötigt)
pub mod cross_chain_demo;
pub mod node_simulation;
pub mod limit_orderbook_demo;

// Logging, Metrik, Tracing
pub mod logging;
pub mod metrics;
pub mod tracing_setup;
pub mod config_loader;
pub mod node_logic;

// Storage + Error
pub mod error;
pub mod storage {
    pub mod db_layer;
    pub mod replicated_db_layer;
}

// Fees – inkl. fee_pool für globale/verteilte Gebührensammlung
pub mod fees {
    pub mod fee_pool;
    // ggf. weitere Fees-Module
}

// Utils => HLC / GeoIP etc.
pub mod utils {
    pub mod hlc;
    pub mod geoip_and_ntp;
}
