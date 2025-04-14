//////////////////////////////////////////////////
// my_dex/src/self_healing/config.rs
//////////////////////////////////////////////////

use std::collections::{HashMap, HashSet};
use serde::Deserialize;
use std::fs;
use tracing::warn;

#[derive(Debug, Deserialize)]
pub struct WatchdogConfig {
    pub services: HashMap<String, ServiceConfig>,
}

#[derive(Debug, Deserialize)]
pub struct ServiceConfig {
    pub interval_sec: u64,
    pub health: HealthCheckType,
    pub escalation_webhook: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthCheckType {
    Tcp { host: String, port: u16 },
    Http { url: String },
    Dummy,
}

pub fn load_config(path: &str) -> Option<WatchdogConfig> {
    match fs::read_to_string(path) {
        Ok(content) => match toml::from_str::<WatchdogConfig>(&content) {
            Ok(cfg) => Some(cfg),
            Err(e) => {
                warn!("Fehler beim Parsen der Konfigurationsdatei: {}", e);
                None
            }
        },
        Err(e) => {
            warn!("Fehler beim Laden der Konfigurationsdatei '{}': {}", path, e);
            None
        }
    }
}

/// Gibt Whitelist der erlaubten Dienste aus der Config zur�ck
pub fn extract_whitelist(config: &WatchdogConfig) -> HashSet<String> {
    config.services.keys().cloned().collect()
}
