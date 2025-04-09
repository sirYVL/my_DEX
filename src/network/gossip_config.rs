///////////////////////////////////////////////////////////
// my_dex/src/network/gossip_config.rs
///////////////////////////////////////////////////////////
//
// Dieses Modul implementiert eine konfigurierbare Gossip-Schicht,
// die es erm�glicht, zwischen Push- und Pull-Mechanismen zu w�hlen,
// Zeitintervalle f�r den State-Austausch anzupassen und zu entscheiden,
// ob Deltas oder der vollst�ndige State gesendet wird.
///////////////////////////////////////////////////////////

use std::time::Duration;
use serde::{Serialize, Deserialize};
use tracing::{info, debug};

/// Enum zur Auswahl des Gossip-Modus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GossipMode {
    Push,
    Pull,
}

/// Konfigurationsparameter f�r das Gossip-Protokoll
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipConfig {
    /// Modus: Push oder Pull
    pub mode: GossipMode,
    /// Zeitintervall zwischen regelm��igen Gossip-Runden
    pub interval: Duration,
    /// Senden wir nur Deltas oder den vollst�ndigen State
    pub use_deltas: bool,
}

impl GossipConfig {
    pub fn new() -> Self {
        Self {
            mode: GossipMode::Push,
            interval: Duration::from_secs(5),
            use_deltas: true,
        }
    }
}

/// Beispiel-Funktion, die anhand der Konfiguration entscheidet,
/// ob der vollst�ndige State oder nur Deltas gesendet werden soll.
pub fn should_send_full_state(config: &GossipConfig) -> bool {
    !config.use_deltas
}

/// Beispiel-Funktion, die den n�chsten Gossip-Zeitpunkt berechnet.
/// In einer echten Implementierung w�rden Sie diesen Wert als Timer verwenden.
pub fn next_gossip_interval(config: &GossipConfig) -> Duration {
    config.interval
}

/// Loggt die aktuelle Konfiguration des Gossip-Protokolls.
pub fn log_gossip_config(config: &GossipConfig) {
    info!("Gossip-Konfiguration: Modus: {:?}, Interval: {:?}, Deltas: {}",
          config.mode, config.interval, config.use_deltas);
}
