///////////////////////////////////////////////////////////
// my_dex/src/logging/enhanced_logging.rs
///////////////////////////////////////////////////////////
//
// Erweiterte Logging-Implementation für strukturierte JSON-Logs
// mit täglicher Rotation (Rotation::Daily). Zusätzlich bietet
// es zwei zentrale Hilfsfunktionen:
//
//   log_error<E: Error>(err: E) => Schreibt einen Fehler ins Log
//   write_audit_log(&str)      => Schreibt Audit-Ereignisse (z. B. "Trade finalisiert")
//
// Sowohl stdout als auch Log-Datei erhalten JSON-formatierte Logs.
// Externe Tools (z. B. Fluentd, Promtail, Logstash ...) können
// diese JSON-Logs sammeln und weiterverarbeiten.
///////////////////////////////////////////////////////////

use tracing_subscriber::{fmt, EnvFilter, layer::SubscriberExt, Registry};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing::{info, error};
use std::io;

/// Initialisiert das erweiterte, strukturierte Logging-System.
///
/// - `log_level`: Gewünschtes Log-Level (z. B. "info", "debug", "trace").
/// - `log_dir`: Verzeichnis, in dem Log-Dateien abgelegt werden sollen.
/// - `log_file`: Dateiname (z. B. "audit.log") für die Log-Rotation.
///
/// Die Logs werden JSON-formatiert sowohl an stdout als auch
/// in eine tagesrotierte Datei geschrieben. Dadurch kannst du sie
/// in einem zentralen Log-System analysieren.
pub fn init_enhanced_logging(log_level: &str, log_dir: &str, log_file: &str) {
    // 1) Definiere einen EnvFilter auf Basis des angegebenen Log-Levels.
    let filter = EnvFilter::new(log_level);

    // 2) Erstelle einen RollingFileAppender, der täglich rotiert.
    let file_appender = RollingFileAppender::new(Rotation::Daily, log_dir, log_file);
    // Non-blocking, damit das Schreiben ins Log nicht blockiert, falls IO langsam ist.
    let (non_blocking_writer, _guard) = tracing_appender::non_blocking(file_appender);

    // 3) JSON-Layer für stdout
    let stdout_layer = fmt::layer()
        .json()                     // JSON-Ausgabe
        .with_writer(io::stdout)    // in die Konsole
        .with_target(true)          // Ziel (target) mit ausgeben
        .with_thread_names(true);   // Thread-IDs im Log vermerken

    // 4) JSON-Layer für das rotierende Logfile
    let file_layer = fmt::layer()
        .json()
        .with_writer(non_blocking_writer)
        .with_target(true)
        .with_thread_names(true);

    // 5) Baue einen Subscriber aus Filter + stdout + file-Layer
    let subscriber = Registry::default()
        .with(filter)
        .with(stdout_layer)
        .with(file_layer);

    // 6) Registriere diesen Subscriber global
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set global tracing subscriber");

    info!(
        "Enhanced logging initialisiert: level={}, log_dir={}, log_file={}",
        log_level, log_dir, log_file
    );
}

/// Zentrale Funktion zur Fehlerprotokollierung. Diese Funktion kann in allen Modulen
/// verwendet werden, um Fehler konsistent zu loggen und ggf. Auditprozesse anzustoßen.
pub fn log_error<E: std::error::Error>(err: E) {
    error!("Fehler aufgetreten: {}", err);
}

/// Schreibt einen Audit-Trail-Eintrag. In einer echten Produktionsumgebung
/// würde man so etwas möglicherweise zusätzlich in einer Audit-Datenbank
/// oder einem speziellen System sichern.
pub fn write_audit_log(event: &str) {
    // Wir nehmen hier das `info!`-Level, damit klar ersichtlich ist,
    // dass ein Audit-Event vorliegt. Du könntest auch ein eigenes Level definieren.
    info!("AUDIT: {}", event);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::info;

    #[test]
    fn test_enhanced_logging() {
        // 1) Logging initialisieren.
        //    Achtung: wiederholte Aufrufe in Tests könnten sich überschneiden.
        init_enhanced_logging("debug", "./logs", "audit_test.log");

        // 2) Eine normale Info-Lognachricht
        info!("Test-Logeintrag: Enhanced Logging funktioniert");

        // 3) Audit-Log
        write_audit_log("Test-Audit-Ereignis => alles ok.");

        // 4) Simulierter Fehler => log_error aufrufen
        let sample_err = std::io::Error::new(std::io::ErrorKind::Other, "Fake-IO-Error");
        log_error(sample_err);
    }
}
