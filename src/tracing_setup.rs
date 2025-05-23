///////////////////////////////////////////////////////////
// my_dex/src/tracing_setup.rs
///////////////////////////////////////////////////////////
//
// Dieses Modul initialisiert einen Observability-Stack für euren DEX:
// - Distributed Tracing mit OpenTelemetry + Jaeger
// - JSON-basiertes Logging (Konsole & Rolling File)
// - Dynamischer Log-Level (EnvFilter)
//
// Damit lässt sich das System sowohl lokal debuggen (JSON, stündlich rotiert)
// als auch in einer Produktionsumgebung (Versand von Spans an Jaeger).
///////////////////////////////////////////////////////////

use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    EnvFilter,
    layer::SubscriberExt,
    Registry,
};
use opentelemetry::{global, runtime};
use opentelemetry_jaeger::PipelineBuilder;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::fmt::Layer as FmtLayer;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing::info;

use std::fs;
use std::path::Path;

/// Initialisiert Tracing + Logging.
/// 
/// - `service_name`: Anzeigename in Jaeger
/// - `jaeger_addr`:  z. B. "127.0.0.1:14268/api/traces"
/// - `log_level`:    z. B. "info", "debug", etc.
/// - `log_dir`:      z. B. "logs" oder ein Pfad aus ENV
pub fn init_tracing_with_otel(
    service_name: &str,
    jaeger_addr: &str,
    log_level: &str,
    log_dir: &str,
) {
    // Stelle sicher, dass das Log-Verzeichnis existiert
    if !Path::new(log_dir).exists() {
        fs::create_dir_all(log_dir)
            .expect("Konnte Log-Verzeichnis nicht erstellen");
    }

    // Jaeger Tracer
    let tracer = PipelineBuilder::default()
        .with_service_name(service_name)
        .with_endpoint(format!("http://{}", jaeger_addr))
        .install_batch(runtime::Tokio)
        .expect("Fehler beim Installieren der Jaeger-Pipeline");

    let otel_layer = OpenTelemetryLayer::new(tracer);

    // JSON-Konsole
    let console_layer = fmt::layer()
        .json()
        .with_span_events(FmtSpan::CLOSE);

    // JSON-File-Logging (stündlich rotierendes Logfile)
    let file_appender = RollingFileAppender::new(Rotation::HOURLY, log_dir, "my_dex.log");
    let file_layer = fmt::layer()
        .json()
        .with_writer(file_appender)
        .with_span_events(FmtSpan::CLOSE);

    // Dynamischer Log-Level über EnvFilter
    let env_filter = EnvFilter::new(log_level);

    let subscriber = Registry::default()
        .with(env_filter)
        .with(otel_layer)
        .with(console_layer)
        .with(file_layer);

    tracing::subscriber::set_global_default(subscriber)
        .expect("Fehler bei set_global_default(subscriber)");

    info!(
        "OpenTelemetry/Jaeger-Tracing initialisiert: service_name={}, endpoint={}, log_dir={}",
        service_name, jaeger_addr, log_dir
    );
}

/// Beendet das Tracing, flush ausstehende Spans und schließt den Tracer sauber.
pub fn shutdown_tracing() {
    global::shutdown_tracer_provider();
    info!("Tracing sauber heruntergefahren.");
}

// ─────────────────────────────────────────────────────────────
// NEU: init_tracing_with_otel_from_env
// ─────────────────────────────────────────────────────────────

use std::env;
use dotenv::dotenv;

/// Lädt `.env` (falls vorhanden) und leitet alle Werte an `init_tracing_with_otel(...)` weiter.
///
/// Erwartete `.env`-Variablen (oder System-Env):
///   - OTEL_SERVICE_NAME = "my-dex"
///   - OTEL_JAEGER_ADDR  = "127.0.0.1:14268/api/traces"
///   - LOG_LEVEL         = "info" (oder "debug", "warn", etc.)
///   - LOG_DIR           = "logs"
pub fn init_tracing_with_otel_from_env() {
    // .env laden, Fehler ignorieren falls nicht vorhanden
    dotenv().ok();

    let service_name = env::var("OTEL_SERVICE_NAME")
        .unwrap_or_else(|_| "my-dex".to_string());
    let jaeger_addr = env::var("OTEL_JAEGER_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:14268/api/traces".to_string());
    let log_level = env::var("LOG_LEVEL")
        .unwrap_or_else(|_| "info".to_string());
    let log_dir = env::var("LOG_DIR")
        .unwrap_or_else(|_| "logs".to_string());

    init_tracing_with_otel(
        &service_name,
        &jaeger_addr,
        &log_level,
        &log_dir,
    );
}
