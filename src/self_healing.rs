use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn, error};
use chrono::Utc;
use crate::gossip::{GossipMessage, broadcast_gossip_message};

/// HealthChecker: Dummy-Funktion für Service Health.
/// In Produktion: z. B. Port prüfen, HTTP-Abfrage, DB-Verbindung etc.
pub async fn check_service_health(service_name: &str) -> bool {
    // TODO: Ersetze durch echte Health-Prüfung
    false
}

/// RestartManager: Startet einen Systemdienst neu (z. B. my_dex_node.service).
/// Voraussetzung: Der ausführende Benutzer hat systemd-Zugriff (evtl. via sudo).
pub async fn restart_service(service_name: &str) -> Result<(), String> {
    let max_attempts = 5;
    let base_delay = Duration::from_secs(2);

    for attempt in 1..=max_attempts {
        info!("Versuche Neustart ({} / {}) für '{}'", attempt, max_attempts, service_name);

        let result = Command::new("systemctl")
            .arg("restart")
            .arg(service_name)
            .status();

        match result {
            Ok(status) if status.success() => {
                info!("Service '{}' erfolgreich neu gestartet", service_name);
                return Ok(());
            }
            Ok(status) => {
                warn!("systemctl exit code: {:?}", status.code());
            }
            Err(e) => {
                warn!("Fehler beim Aufruf von systemctl: {:?}", e);
            }
        }

        let delay = base_delay * attempt;
        warn!(
            "Neustartversuch {} fehlgeschlagen – neuer Versuch in {:?}",
            attempt, delay
        );
        sleep(delay).await;
    }

    Err(format!("Restart fehlgeschlagen nach {} Versuchen", max_attempts))
}

/// Überwacht kontinuierlich einen Dienst und führt Self-Healing durch.
/// Bei Fehler: Gossip-Nachricht senden und Neustart via systemctl versuchen.
pub async fn monitor_and_heal(service_name: &str, node_id: &str, interval_sec: u64) {
    let mut interval = tokio::time::interval(Duration::from_secs(interval_sec));
    loop {
        interval.tick().await;

        let healthy = check_service_health(service_name).await;
        if !healthy {
            warn!("Service '{}' ist ungesund – Self-Healing wird gestartet", service_name);

            let gossip_msg = GossipMessage::new(
                node_id.to_string(),
                format!("{} failure", service_name),
                format!("Service {} is unresponsive", service_name),
                "critical".to_string(),
                format!("Service {} did not respond in expected time", service_name),
                60,
                Some("signature_placeholder".to_string()),
            );

            broadcast_gossip_message(gossip_msg).await;

            match restart_service(service_name).await {
                Ok(_) => info!("Service '{}' wurde erfolgreich neu gestartet.", service_name),
                Err(e) => error!("Self-Healing fehlgeschlagen: {}", e),
            }
        } else {
            info!("Service '{}' ist gesund.", service_name);
        }
    }
}

