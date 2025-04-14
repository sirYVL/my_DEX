//////////////////////////////////////////////////
// my_dex/src/self_healing/watchdog.rs
//////////////////////////////////////////////////

use std::process::Command;
use std::collections::HashSet;
use std::time::Duration;
use tokio::time::{sleep, interval};
use tracing::{info, warn, error};
use chrono::Utc;
use base64::{engine::general_purpose, Engine as _};

use crate::dex_logic::sign_utils::KeyPair;
use crate::crypto::key_loader::get_or_create_keypair;
use crate::gossip::{GossipMessage, broadcast_gossip_message};

/// Whitelist kritischer DEX-Dienste
fn allowed_services() -> HashSet<&'static str> {
    HashSet::from([
        "my_dex_node.service",
        "my_dex_api.service",
        "dex_db_sync.service",
    ])
}

/// Dummy-Gesundheitsprüfung – später anpassbar
pub async fn check_service_health(_service_name: &str) -> bool {
    // TODO: Replace with real health check
    false
}

/// Sichere Neustartlogik mit Whitelist-Schutz
pub async fn restart_service(service_name: &str) -> Result<(), String> {
    if !allowed_services().contains(service_name) {
        return Err("Dienst nicht autorisiert für Neustart".to_string());
    }

    let max_attempts = 3;
    let base_delay = Duration::from_secs(2);

    for attempt in 1..=max_attempts {
        info!("Restart-Versuch {} für '{}'", attempt, service_name);

        let result = Command::new("systemctl")
            .arg("restart")
            .arg(service_name)
            .status();

        match result {
            Ok(status) if status.success() => {
                info!("Dienst '{}' erfolgreich neugestartet", service_name);
                return Ok(());
            }
            Ok(status) => {
                warn!("systemctl Fehlercode: {:?}", status.code());
            }
            Err(e) => {
                warn!("Fehler bei systemctl-Aufruf: {:?}", e);
            }
        }

        sleep(base_delay * attempt).await;
    }

    Err(format!("Restart von '{}' fehlgeschlagen nach {} Versuchen", service_name, max_attempts))
}

/// Überwacht Dienst und heilt bei Fehler automatisch
pub async fn monitor_and_heal(service_name: &str, node_id: &str, interval_sec: u64) {
    let mut ticker = interval(Duration::from_secs(interval_sec));
    let keypair = get_or_create_keypair().expect("Keypair konnte nicht geladen werden");

    loop {
        ticker.tick().await;

        if !check_service_health(service_name).await {
            warn!("Dienst '{}' ungesund – starte Self-Healing", service_name);

            let timestamp = Utc::now().timestamp();
            let body = format!("{}:{}:{}", node_id, service_name, timestamp);
            let signature = keypair.sign_message(body.as_bytes());
            let sig_b64 = general_purpose::STANDARD.encode(signature.serialize_compact());

            let gossip_msg = GossipMessage::new(
                node_id.to_string(),
                format!("{} failure", service_name),
                format!("{} unresponsive", service_name),
                "critical".to_string(),
                body,
                60,
                Some(sig_b64),
            );

            broadcast_gossip_message(gossip_msg).await;

            if let Err(e) = restart_service(service_name).await {
                error!("Restart fehlgeschlagen: {}", e);
            }
        } else {
            info!("Dienst '{}' ist gesund", service_name);
        }
    }
}
