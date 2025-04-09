// src/logging.rs
//
// Falls du KEIN opentelemetry benutzen willst, kannst du 
// ein Basic Logging Setup via "tracing-subscriber::fmt()" machen.

use tracing_subscriber::{fmt, EnvFilter};
use tracing::{info};

pub fn init_basic_logging(log_level: &str) {
    let filter = EnvFilter::new(log_level);
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .init();

    info!("Basic logging initialisiert mit Level={}", log_level);
}
