///////////////////////////////////////////////////////////
// my_dex/src/settlement/fees_config.rs
///////////////////////////////////////////////////////////
//
// Dieses Modul enth�lt eine zentrale Struktur f�r s�mtliche
// Settlement-bezogenen Geb�hren. So k�nnen verschiedene
// Settlement-Typen einheitlich konfiguriert werden.
//
// Du kannst sp�ter weitere Felder hinzuf�gen (z.B. htlc_fee_rate, cross_chain_fee_rate, ...),
// ohne alle Modules / Engines anpassen zu m�ssen.
//
use serde::{Serialize, Deserialize};

/// Struktur f�r s�mtliche Fees im Settlement-Bereich
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettlementFees {
    /// Standard-Fee (z.B. 0.001 = 0.1%) f�r �normale� Trades
    pub standard_fee_rate: f64,

    /// Atomic Swap Fee (z.B. 0.002 = 0.2%)
    pub atomic_swap_fee_rate: f64,

    // Hier k�nnten Sie weitere Felder anlegen, falls n�tig:
    // pub htlc_fee_rate: f64,
    // pub cross_chain_fee_rate: f64,
}

impl SettlementFees {
    /// Erzeugt eine Default-Konfiguration
    pub fn new(standard: f64, atomic: f64) -> Self {
        SettlementFees {
            standard_fee_rate: standard,
            atomic_swap_fee_rate: atomic,
        }
    }

    // Optional: Laden/Speichern aus YAML/JSON/...
    // ...
}
