//////////////////////////////////////////////////
// my_dex/src/self_healing/config.rs
//////////////////////////////////////////////////

use std::collections::{HashMap, HashSet};
use serde::Deserialize;
use std::fs;
use tracing::{warn, info, error};

#[derive(Debug, Deserialize)]
pub struct WatchdogConfig {
    pub services: HashMap<String, ServiceConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServiceConfig {
    pub interval_sec: u64,
    pub health: HealthCheckType,
    pub escalation_webhook: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum HealthCheckType {
    Tcp { host: String, port: u16 },
    Http { url: String },
    Dummy,
}

pub fn load_config(path: &str) -> Option<WatchdogConfig> {
    match fs::read_to_string(path) {
        Ok(content) => match toml::from_str::<WatchdogConfig>(&content) {
            Ok(cfg) => {
                info!("Watchdog-Konfiguration erfolgreich geladen: {} Dienste", cfg.services.len());
                Some(cfg)
            },
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

/// Gibt Whitelist der erlaubten Dienste aus der Config zurück
pub fn extract_whitelist(config: &WatchdogConfig) -> HashSet<String> {
    config.services.keys().cloned().collect()
}

/// Debug-Ausgabe aller geladenen Dienste (für Startup-Check)
pub fn print_loaded_services(config: &WatchdogConfig) {
    for (name, svc) in &config.services {
        info!("Service '{}' konfiguriert mit {}s-Intervall", name, svc.interval_sec);
    }
}

/// Validiert die Watchdog-Konfiguration auf offensichtliche Fehler
pub fn validate_config(config: &WatchdogConfig) -> bool {
    let mut valid = true;

    for (name, svc) in &config.services {
        if svc.interval_sec == 0 {
            warn!("Service '{}' hat ungültiges Intervall: 0s", name);
            valid = false;
        }

        match &svc.health {
            HealthCheckType::Tcp { host, port } => {
                if host.is_empty() || *port == 0 {
                    warn!("TCP-HealthCheck von '{}' ist ungültig: host='{}', port={}", name, host, port);
                    valid = false;
                }
            }
            HealthCheckType::Http { url } => {
                if url.is_empty() {
                    warn!("HTTP-HealthCheck von '{}' ist ungültig: url fehlt", name);
                    valid = false;
                }
            }
            HealthCheckType::Dummy => {
                info!("Service '{}' verwendet Dummy-Check", name);
            }
        }
    }

    if !valid {
        error!("Die Watchdog-Konfiguration enthält ungültige Einträge.");
    }

    valid
}
