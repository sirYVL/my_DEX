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
use tracing::{info};

/// Initialisiert die Tracing-Pipeline mit OpenTelemetry und Jaeger,
/// JSON-Logging (Konsole & Rolling File) und einem Filter für das Log-Level.
/// Verwendet ausschließlich tracing und tracing-subscriber, ohne env_logger.
/// 
/// - `service_name`: Anzeigename in Jaeger
/// - `jaeger_addr`:  z. B. "127.0.0.1:14268/api/traces"
/// - `log_level`:    z. B. "info", "debug", etc.
pub fn init_tracing_with_otel(service_name: &str, jaeger_addr: &str, log_level: &str) {
    // Erzeuge den Jaeger-Tracer über OpenTelemetry
    let tracer = PipelineBuilder::default()
        .with_service_name(service_name)
        .with_endpoint(format!("http://{}", jaeger_addr))
        .install_batch(runtime::Tokio)
        .expect("Fehler beim Installieren der Jaeger-Pipeline");

    // Erstelle ein Layer, das unsere Spans an OpenTelemetry weiterleitet
    let otel_layer = OpenTelemetryLayer::new(tracer);

    // JSON-Konsole
    let console_layer = fmt::layer()
        .json()
        .with_span_events(FmtSpan::CLOSE);

    // Rolling File Logging (stündlich rotierte Log-Datei in ./logs/my_dex.log)
    let file_appender = RollingFileAppender::new(Rotation::HOURLY, "logs", "my_dex.log");
    let file_layer = fmt::layer()
        .json()
        .with_writer(file_appender)
        .with_span_events(FmtSpan::CLOSE);

    // Environment-Filter basierend auf dem angegebenen log_level
    let env_filter = EnvFilter::new(log_level);

    // Kombiniere alle Layers zu einem Subscriber
    let subscriber = Registry::default()
        .with(env_filter)
        .with(otel_layer)
        .with(console_layer)
        .with(file_layer);

    // Setze den Subscriber global
    tracing::subscriber::set_global_default(subscriber)
        .expect("Fehler bei set_global_default(subscriber)");

    info!(
        "OpenTelemetry/Jaeger-Tracing initialisiert: service_name={}, endpoint={}",
        service_name,
        jaeger_addr
    );
}

/// Beendet das Tracing, flush ausstehende Spans und schließt den Tracer sauber.
pub fn shutdown_tracing() {
    global::shutdown_tracer_provider();
    info!("Tracing sauber heruntergefahren.");
}
