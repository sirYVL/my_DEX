//////////////////////////////////////////////////
/// my_DEX/src/network/security_monitor.rs
//////////////////////////////////////////////////

use std::sync::{Arc, Mutex};
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};
use snow::Session;

pub struct SecurityMonitor {
    logs: Arc<Mutex<Vec<String>>>,
    replay_attempts: Arc<Mutex<HashMap<u64, u32>>>,
}

impl SecurityMonitor {
    pub fn new() -> Self {
        Self {
            logs: Arc::new(Mutex::new(Vec::new())),
            replay_attempts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn log_event(&self, message: &str) {
        let mut logs = self.logs.lock().unwrap();
        logs.push(format!("{} - {}", Self::current_timestamp(), message));
        println!("?? [Security Log] {}", message);
    }

    pub fn is_valid_nonce(&self, nonce: u64) -> bool {
        let mut attempts = self.replay_attempts.lock().unwrap();
        let count = attempts.entry(nonce).or_insert(0);
        *count += 1;

        if *count > 1 {
            self.log_event(&format!("?? Replay-Angriff erkannt! Nonce: {} ({} Versuche)", nonce, count));
            return false;
        }
        true
    }

    pub fn monitor_noise_handshake(&self, session: &Session) {
        if session.is_handshake_complete() {
            self.log_event("?? Noise Handshake erfolgreich abgeschlossen.");
        } else {
            self.log_event("? Noise Handshake FEHLGESCHLAGEN.");
        }
    }

    pub fn generate_nonce() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
    }

    fn current_timestamp() -> String {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        format!("[{}]", now)
    }
}
