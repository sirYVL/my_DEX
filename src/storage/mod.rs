////////////////////////////////////////////////////
/// my_DEX/src/storage/mod.rs
////////////////////////////////////////////////////

//! Dieses Modul fasst alle Speicher-bezogenen Implementierungen zusammen,
//! darunter:
//! - db_layer.rs: Basis-Datenbank-Logik mit InMemory-Fallback
//! - dex_db.rs: Persistente Speicherung via RocksDB mit Column Families
//! - distributed_db.rs: Erweiterte, verteilte DB-Logik (Replikation & Synchronisation)
//! - ipfs_storage.rs: Funktionen zur Integration von IPFS
//! - replicated_db_layer.rs: Erweiterter DB-Layer mit Replikationsmechanismen

pub mod db_layer;
pub mod dex_db;
pub mod distributed_db;
pub mod ipfs_storage;
pub mod replicated_db_layer;
