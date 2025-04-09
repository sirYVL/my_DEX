///////////////////////////////////////////////////////////
// my_dex/src/monitoring/node_monitoring.rs
///////////////////////////////////////////////////////////
//
// Dieses Modul implementiert einen HTTP-Server, der
// ausschließlich node-spezifische Metriken (z. B. CPU- und
// Speicherauslastung) bereitstellt. Es bietet zwei Endpunkte:
//   - /metrics: liefert die Metriken im Prometheus-Format
//   - /node_metrics: liefert eine JSON-Darstellung der lokalen Metriken
//   - /health: ein einfacher Health-Check
//
// Dieser Server ist für Node-Betreiber gedacht und nicht global freigegeben.
//
// In einer echten Produktionsumgebung würdest du z. B. noch
// Access-Control (nur localhost), eine Authentifizierung oder mTLS
// (ähnlich wie im global_monitoring) in Erwägung ziehen.
//
///////////////////////////////////////////////////////////

use hyper::{Body, Request, Response, Server};
use hyper::service::{make_service_fn, service_fn};
use prometheus::{Encoder, TextEncoder, Registry, Gauge, register_gauge};
use serde_json::json;
use std::net::SocketAddr;
use tracing::{info, error, debug};

// Ein statisches (Lazy-initialisiertes) Prometheus-Registry speziell für Node-Metriken
pub static NODE_REGISTRY: once_cell::sync::Lazy<Registry> = once_cell::sync::Lazy::new(|| {
    // Du kannst mit .new_custom(...) ein Prefix angeben
    Registry::new_custom(Some("node".to_string()), None)
        .expect("Fehler beim Erstellen der Node-Registry")
});

// Beispielhafte Node-spezifische Metriken
pub static NODE_CPU_USAGE: once_cell::sync::Lazy<Gauge> = once_cell::sync::Lazy::new(|| {
    register_gauge!("node_cpu_usage", "Current CPU usage of the node in percent")
        .expect("Fehler beim Registrieren von node_cpu_usage")
});

pub static NODE_MEM_USAGE: once_cell::sync::Lazy<Gauge> = once_cell::sync::Lazy::new(|| {
    register_gauge!("node_mem_usage", "Current memory usage of the node in MB")
        .expect("Fehler beim Registrieren von node_mem_usage")
});

///////////////////////////////////////////////////////////
// Funktionen zum Registrieren & Aktualisieren unserer Node-Metriken
///////////////////////////////////////////////////////////

/// Registriert die node-spezifischen Metriken in der NODE_REGISTRY.
/// Normalerweise einmalig beim Serverstart aufrufen.
pub fn register_node_metrics() {
    // Achtung: Doppel-Registrierung wirft in Prometheus i. d. R. einen Fehler.
    // In diesem Code gelingt es u. U. mit .register(...) zu detecten.
    let _ = NODE_REGISTRY.register(Box::new(NODE_CPU_USAGE.clone()));
    let _ = NODE_REGISTRY.register(Box::new(NODE_MEM_USAGE.clone()));
}

/// Aktualisiert Beispielmetriken für die Node –
/// in einer realen Umgebung würdest du hier deine Systemabfragen durchführen:
///   - CPU => via sysinfo crate o. Ä.
///   - Memory => /proc/meminfo etc.
pub fn update_node_metrics() {
    // Beispielwerte hartkodiert
    // CPU => 25.0%
    NODE_CPU_USAGE.set(25.0);

    // Memory => 1024 MB
    NODE_MEM_USAGE.set(1024.0);
}

///////////////////////////////////////////////////////////
// HTTP-Handler: /metrics => liefert Prometheus-Format
///////////////////////////////////////////////////////////
async fn node_metrics_handler(_req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    // 1) Node-spezifische Metriken aktualisieren
    update_node_metrics();

    // 2) Alle Metriken aus Registry gathern
    let encoder = TextEncoder::new();
    let mf = NODE_REGISTRY.gather();
    let mut buffer = Vec::new();

    if let Err(e) = encoder.encode(&mf, &mut buffer) {
        error!("Fehler beim Kodieren der Node-Metriken: {:?}", e);
        // Gib eine 500 zurück
        return Ok(Response::builder()
            .status(500)
            .body(Body::from(format!("Metrics encoding error: {:?}", e)))?);
    }

    Ok(Response::new(Body::from(buffer)))
}

///////////////////////////////////////////////////////////
// HTTP-Handler: /node_metrics => liefert JSON
///////////////////////////////////////////////////////////
async fn node_metrics_json_handler(_req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    update_node_metrics();

    let json_data = json!({
        "cpu_usage_percent": NODE_CPU_USAGE.get(),
        "memory_usage_mb": NODE_MEM_USAGE.get(),
    });
    let body_str = json_data.to_string();

    // 200 OK => JSON
    Ok(Response::builder()
        .header("Content-Type", "application/json")
        .body(Body::from(body_str))?)
}

///////////////////////////////////////////////////////////
// HTTP-Handler: /health => einfacher Health-Check
///////////////////////////////////////////////////////////
async fn health_handler(_req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    debug!("Health-Check ausgeführt -> OK");
    Ok(Response::new(Body::from("OK")))
}

///////////////////////////////////////////////////////////
// Hauptfunktion: Node Monitoring Server starten
///////////////////////////////////////////////////////////
pub async fn start_node_monitoring_server(addr: SocketAddr) {
    // Registriere Metriken (falls nicht schon geschehen)
    register_node_metrics();
    info!("Node Monitoring Server startet auf {}", addr);

    // Erstelle Service
    let make_svc = make_service_fn(|_conn| async {
        // Jede Anfrage bekommt diese Service-Fn
        Ok::<_, hyper::Error>(service_fn(|req: Request<Body>| async move {
            match req.uri().path() {
                "/metrics" => node_metrics_handler(req).await,
                "/node_metrics" => node_metrics_json_handler(req).await,
                "/health" => health_handler(req).await,
                _ => {
                    // Fallback => 404
                    Ok(Response::builder()
                        .status(404)
                        .body(Body::from("Not Found"))
                        .unwrap())
                }
            }
        }))
    });

    // Binde Server an addr => Hyper
    let server = Server::bind(&addr).serve(make_svc);

    // Falls Fehler => logge
    if let Err(e) = server.await {
        error!("Node Monitoring Server error: {}", e);
    }
}
