///////////////////////////////////////////
/// my_DEX/src/monitoring/logging.rs
///////////////////////////////////////////

use chrono::{DateTime, Utc};
use std::sync::{Mutex, Arc};

/// Struktur, die einen Log-Eintrag repr�sentiert.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub user_type: String, // "fullnode" oder "trader"
    pub event: String,
}

/// Ein einfacher, thread-sicherer Logger, der Log-Eintr�ge sammelt.
pub struct Logger {
    logs: Mutex<Vec<LogEntry>>,
}

impl Logger {
    /// Erzeugt einen neuen Logger.
    pub fn new() -> Self {
        Logger {
            logs: Mutex::new(Vec::new()),
        }
    }

    /// F�gt einen neuen Log-Eintrag hinzu.
    pub fn log_event(&self, user_type: &str, event: &str) {
        let log_entry = LogEntry {
            timestamp: Utc::now(),
            user_type: user_type.to_string(),
            event: event.to_string(),
        };
        let mut logs = self.logs.lock().unwrap();
        logs.push(log_entry);
    }

    /// Gibt alle Log-Eintr�ge f�r einen bestimmten Nutzer-Typ zur�ck.
    pub fn get_logs_for_user(&self, user_type: &str) -> Vec<LogEntry> {
        let logs = self.logs.lock().unwrap();
        logs.iter()
            .filter(|entry| entry.user_type == user_type)
            .cloned()
            .collect()
    }

    /// Gibt alle Log-Eintr�ge zur�ck (f�r Admins etc.).
    pub fn get_all_logs(&self) -> Vec<LogEntry> {
        let logs = self.logs.lock().unwrap();
        logs.clone()
    }
}

/// Erzeuge einen globalen Logger als Arc, damit er in verschiedenen Modulen genutzt werden kann.
pub fn get_global_logger() -> Arc<Logger> {
    Arc::new(Logger::new())
}
