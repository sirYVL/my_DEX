///////////////////////////////////////////////////////////
// my_dex/src/monitoring/global_monitoring.rs
///////////////////////////////////////////////////////////
//
// Dieses Modul implementiert einen globalen Monitoring-Server, der
// globale Metriken (z. B. Gesamtanzahl verarbeiteter Orders und
// CRDT-Updates) im Prometheus-Format bereitstellt. Der Zugriff
// erfolgt ausschließlich über Mutual TLS (mTLS): Nur Clients mit
// einem gültigen Zertifikat (signiert von unserer CA) können den
// Endpoint aufrufen. Dieser Server ist für Entwickler/Zentralteams gedacht.
//
///////////////////////////////////////////////////////////

use std::fs::File as StdFile;
use std::io::BufReader;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

use hyper::{Body, Request, Response, Server, Error};
use hyper::service::{make_service_fn, service_fn};
use prometheus::{Encoder, TextEncoder, Registry, IntCounter, register_int_counter};
use rustls_pemfile;
use tokio_rustls::rustls::{
    self,
    Certificate,
    PrivateKey,
    ServerConfig,
    RootCertStore,
    AllowAnyAuthenticatedClient
};
use tokio_rustls::TlsAcceptor;
use tracing::{info, error, debug, warn};
use once_cell::sync::Lazy;

///////////////////////////////////////////////////////////
// Globale Registry und Beispiel-Metriken
///////////////////////////////////////////////////////////

static GLOBAL_REGISTRY: Lazy<Registry> = Lazy::new(|| {
    Registry::new_custom(Some("global".to_string()), None)
        .expect("Fehler beim Anlegen der GLOBAL_REGISTRY")
});

static GLOBAL_ORDER_COUNT: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "global_order_total",
        "Total number of orders processed globally"
    )
    .expect("Fehler beim Registrieren von global_order_total")
});

static GLOBAL_CRDT_UPDATE_COUNT: Lazy<IntCounter> = Lazy::new(|| {
    register_int_counter!(
        "global_crdt_updates",
        "Total number of CRDT updates globally"
    )
    .expect("Fehler beim Registrieren von global_crdt_updates")
});

/// Registriert die globalen Metriken in der GLOBAL_REGISTRY.
/// Du kannst sie in `start_global_monitoring_server` aufrufen, bevor
/// du den Server startest.
pub fn register_global_metrics() {
    // Falls schon registriert -> ignore error, in Prometheus doppelte Registration nicht erlaubt
    let _ = GLOBAL_REGISTRY.register(Box::new(GLOBAL_ORDER_COUNT.clone()));
    let _ = GLOBAL_REGISTRY.register(Box::new(GLOBAL_CRDT_UPDATE_COUNT.clone()));
}

///////////////////////////////////////////////////////////
// TLS-Funktionen: Laden von Zertifikaten und Privatekey
///////////////////////////////////////////////////////////

/// Lädt ein Bündel (chain) an Zertifikaten aus der pem/Pkcs#8-Datei.
fn load_certs(path: &Path) -> Result<Vec<Certificate>, std::io::Error> {
    let certfile = StdFile::open(path)?;
    let mut reader = BufReader::new(certfile);
    // rustls_pemfile::certs(...) gibt Vec<Vec<u8>>
    let certs = rustls_pemfile::certs(&mut reader)?
        .into_iter()
        .map(Certificate)
        .collect();
    Ok(certs)
}

/// Lädt einen privaten Schlüssel (im PKCS#8-Format) aus einer PEM-Datei.
fn load_private_key(path: &Path) -> Result<PrivateKey, std::io::Error> {
    let keyfile = StdFile::open(path)?;
    let mut reader = BufReader::new(keyfile);

    // pkcs8_private_keys(...) gibt einen Vector der Keys zurück
    let keys = rustls_pemfile::pkcs8_private_keys(&mut reader)?;
    if let Some(key_der) = keys.into_iter().next() {
        Ok(PrivateKey(key_der))
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "No PKCS#8 private key found",
        ))
    }
}

/// Lädt die CA-Root-Zertifikate, um Client-Zertifikate zu validieren (mTLS).
fn load_client_ca_certificates(path: &Path) -> Result<RootCertStore, std::io::Error> {
    let mut root_store = RootCertStore::empty();
    let ca_certs = load_certs(path)?;
    for cert in ca_certs {
        root_store
            .add(&cert)
            .map_err(|_e| std::io::Error::new(std::io::ErrorKind::Other, "Invalid CA cert"))?;
    }
    Ok(root_store)
}

///////////////////////////////////////////////////////////
// Eigentliche TLS-Server-Konfiguration
///////////////////////////////////////////////////////////

/// Erzeugt das ServerConfig für Rustls, inkl. mTLS.
/// In einer realen Umgebung würdest du die Pfade (server_cert_path, ...)
/// nicht fest verdrahten, sondern z. B. per Konfiguration/ENV.
fn build_tls_config() -> Result<ServerConfig, Box<dyn std::error::Error>> {
    // Pfade zu Zertifikatsdateien:
    let server_cert_path = Path::new("certs/server.crt");
    let server_key_path = Path::new("certs/server.key");
    let client_ca_cert_path = Path::new("certs/client_ca.crt");

    // Lade Serverzertifikat + PrivateKey
    let cert_chain = load_certs(server_cert_path)?;
    let priv_key = load_private_key(server_key_path)?;
    // Lade CA, die Client-Zertifikate signiert hat
    let root_store = load_client_ca_certificates(client_ca_cert_path)?;

    // => Clients brauchen gültiges Zertifikat, signiert von obiger CA
    let client_auth = AllowAnyAuthenticatedClient::new(root_store);

    // Bau TLS-Config
    let mut cfg = ServerConfig::builder()
        .with_safe_defaults()
        .with_client_cert_verifier(client_auth)
        .with_single_cert(cert_chain, priv_key)?;

    // Evtl. ciphersuites etc. anpassen
    cfg.alpn_protocols = vec![b"http/1.1".to_vec()];

    Ok(cfg)
}

///////////////////////////////////////////////////////////
// HTTP-Handler => /metrics
///////////////////////////////////////////////////////////

/// HTTP-Handler, der globale Metriken im Prometheus-Format zurückgibt.
async fn global_metrics_handler(_req: Request<Body>) -> Result<Response<Body>, Error> {
    let encoder = TextEncoder::new();
    let metric_families = GLOBAL_REGISTRY.gather();

    let mut buf = Vec::new();
    if let Err(e) = encoder.encode(&metric_families, &mut buf) {
        error!("Fehler beim Kodieren der Prometheus-Metriken: {:?}", e);
    }
    Ok(Response::new(Body::from(buf)))
}

///////////////////////////////////////////////////////////
// Start-Funktion => Der Global Monitoring Server
///////////////////////////////////////////////////////////

/// Startet den globalen Monitoring-Server mit mTLS auf der angegebenen Adresse.
pub async fn start_global_monitoring_server(addr: SocketAddr) {
    // 1) Metriken registrieren (falls nicht schon geschehen)
    register_global_metrics();
    info!("Global Monitoring Server startet (mTLS) auf {}", addr);

    // 2) TLS-Config laden
    let tls_cfg = match build_tls_config() {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("TLS-Konfiguration konnte nicht geladen werden: {}", e);
            return;
        }
    };

    // 3) TlsAcceptor
    let tls_acceptor = TlsAcceptor::from(Arc::new(tls_cfg));

    // 4) HTTP-Service erstellen
    let make_svc = make_service_fn(|_conn| async {
        Ok::<_, Error>(service_fn(global_metrics_handler))
    });

    // 5) TCP-Listener binden
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => {
            info!("TCP-Listener gestartet auf {}", addr);
            l
        }
        Err(e) => {
            error!("Konnte {} nicht binden: {:?}", addr, e);
            return;
        }
    };

    // 6) Hauptschleife => Accept + TLS-Handshake + Hyper-Server
    loop {
        let (socket, peer_addr) = match listener.accept().await {
            Ok(s) => s,
            Err(e) => {
                warn!("Fehler bei accept(): {:?}", e);
                continue;
            }
        };

        let acceptor = tls_acceptor.clone();
        let service = make_svc.clone();

        tokio::spawn(async move {
            // TLS-Handshake
            let tls_stream = match acceptor.accept(socket).await {
                Ok(s) => s,
                Err(e) => {
                    error!("TLS-Handshake mit {} fehlgeschlagen: {:?}", peer_addr, e);
                    return;
                }
            };

            // Dann Hyper-HTTP
            if let Err(e) = hyper::server::conn::Http::new()
                .serve_connection(tls_stream, service)
                .await
            {
                error!("Fehler bei Verbindung mit {}: {:?}", peer_addr, e);
            }
        });
    }
}
