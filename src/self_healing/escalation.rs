//////////////////////////////////////////////////
// my_dex/src/self_healing/escalation.rs
//////////////////////////////////////////////////

use std::collections::HashMap;
use reqwest::Client;
use tracing::{error, info};

/// Eskalationsmethode: Webhook POST mit JSON
pub async fn send_webhook(url: &str, payload: HashMap<&str, String>) -> Result<(), String> {
    let client = Client::new();

    let res = client.post(url)
        .json(&payload)
        .send()
        .await;

    match res {
        Ok(resp) if resp.status().is_success() => {
            info!("Webhook erfolgreich gesendet an {}", url);
            Ok(())
        }
        Ok(resp) => {
            let status = resp.status();
            error!("Webhook fehlgeschlagen mit Status: {}", status);
            Err(format!("Webhook fehlgeschlagen mit Status: {}", status))
        }
        Err(e) => {
            error!("Webhook-Fehler: {}", e);
            Err(format!("Webhook-Fehler: {}", e))
        }
    }
}

/// Helferfunktion: Standardpayload ausf�llen
pub fn build_default_payload(service: &str, node_id: &str, reason: &str) -> HashMap<&'static str, String> {
    let mut payload = HashMap::new();
    payload.insert("service", service.to_string());
    payload.insert("node_id", node_id.to_string());
    payload.insert("reason", reason.to_string());
    payload
}
