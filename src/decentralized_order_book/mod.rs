// my_dex/src/decentralized_order_book/mod.rs

pub mod matcher;              // Hauptlogik der Matching-Engine
pub mod order;                // Order-Datenstruktur
pub mod order_book;           // Verwaltung des Orderbuchs
pub mod conflict_resolution;  // Konfliktlösungen
pub mod assets;               // Assets, subunits
pub mod settlement;           // Settlement-Logik
pub mod exchange;             // Exchange-Logik (mehrere Orderbücher)
