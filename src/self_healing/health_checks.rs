//////////////////////////////////////////////////
// my_dex/src/self_healing/health_checks.rs
//////////////////////////////////////////////////

use std::net::TcpStream;
use std::time::Duration;
use reqwest::Client;
use tracing::warn;

/// Pr�ft, ob ein TCP-Port erreichbar ist (z.?B. Dienst l�uft)
pub fn check_tcp_port(host: &str, port: u16, timeout_secs: u64) -> bool {
    let addr = format!("{}:{}", host, port);
    TcpStream::connect_timeout(&addr.parse().unwrap(), Duration::from_secs(timeout_secs)).is_ok()
}

/// Asynchrone HTTP-Status-Pr�fung (200 OK erwartet)
pub async fn check_http_ok(url: &str, timeout_secs: u64) -> bool {
    let client = Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .build();

    match client {
        Ok(c) => match c.get(url).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(e) => {
                warn!("HTTP-Check fehlgeschlagen: {}", e);
                false
            }
        },
        Err(e) => {
            warn!("HTTP-Client-Erstellung fehlgeschlagen: {}", e);
            false
        }
    }
}

/// Dummy-Fallback (immer false) � f�r Dienste ohne Health-Probe
pub async fn dummy_health_check(_service_name: &str) -> bool {
    false
}
