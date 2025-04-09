
////////////////////////////////////////
// my_dex/src/decentralized_order_book/assets.rs
////////////////////////////////////////

use serde::{Serialize, Deserialize};
use std::fmt::{Display, Formatter, Result as FmtResult};

/// Assets mit Untereinheiten
#[derive(Clone, Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub enum Asset {
    BTC,
    ETH,
    SOL,
    XMR,
    USDT,
    // etc.
}

/// Konvertierungen zwischen „base“ (z. B. 1.0 BTC) und Untereinheiten (Satoshi, Wei, Lamport, Piconero).
/// Intern speichern wir alles in `u128`:
pub fn base_to_subunits(asset: &Asset, base_amount: f64) -> u128 {
    // Hier sehr grob gerundet – in der Produktion brauchst du exaktere Conversion (oder BigRationals).
    match asset {
        Asset::BTC => {
            // 1 BTC = 100_000_000 Satoshi
            (base_amount * 100_000_000.0).round().max(0.0) as u128
        },
        Asset::ETH => {
            // 1 ETH = 1e18 Wei – hier nur 1e9 (Gwei) oder 1e18?
            // Wir nehmen mal 1e9 (Gwei), um nicht so große Zahlen zu haben
            (base_amount * 1_000_000_000.0).round().max(0.0) as u128
        },
        Asset::SOL => {
            // 1 SOL = 1e9 Lamport
            (base_amount * 1_000_000_000.0).round().max(0.0) as u128
        },
        Asset::XMR => {
            // 1 XMR = 1e12 piconero
            (base_amount * 1_000_000_000_000.0).round().max(0.0) as u128
        },
        Asset::USDT => {
            // USDT -> wir tun so, als ob 1 USDT = 1e6 micro-USDT
            (base_amount * 1_000_000.0).round().max(0.0) as u128
        },
    }
}

/// Umwandlung zurück in eine float-Anzeige
pub fn subunits_to_base(asset: &Asset, subunits: u128) -> f64 {
    match asset {
        Asset::BTC => subunits as f64 / 100_000_000.0,
        Asset::ETH => subunits as f64 / 1_000_000_000.0,
        Asset::SOL => subunits as f64 / 1_000_000_000.0,
        Asset::XMR => subunits as f64 / 1_000_000_000_000.0,
        Asset::USDT => subunits as f64 / 1_000_000.0,
    }
}

impl Display for Asset {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Asset::BTC => write!(f, "BTC"),
            Asset::ETH => write!(f, "ETH"),
            Asset::SOL => write!(f, "SOL"),
            Asset::XMR => write!(f, "XMR"),
            Asset::USDT => write!(f, "USDT"),
        }
    }
}
