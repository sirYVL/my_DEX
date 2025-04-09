///////////////////////////////////////////////////////////
// my_dex/src/security/privacy.rs
///////////////////////////////////////////////////////////
//
// Sicherheit und Datenschutz
// - Implementierung von sicherem Onion-Routing zur Privatsphäre beim Trading
// - Kryptografische Absicherung aller Off-chain-Daten und -Transaktionen
// - Detailliertes Logging und Audit-Log für Sicherheit und Transparenz
//
// NEU (Sicherheitsupdate):
//  1) "onion_encrypt"/"onion_decrypt" => reiner AES-GCM-Layer pro Key, 
//     Real "Onion Routing" würde pro Layer => Weitergabe an Zwischenknoten. 
//     Hier ist es nur "nested encryption".
//  2) Achte auf potenzielle Memory Overheads bei sehr großen Daten, da wir 
//     ciphertext immer in "data" laden und schichtweise vergrößern.
//  3) Jede Schicht generiert einen 12-Byte-Nonce => so weit OK, 
//     aber pass auf, dass du denselben Key nicht unendlich oft reuse. 
//  4) "secure_offchain_data" => Audit-Log => gut, aber pass auf Log-Spam.
//  5) Pfadangriff: write_audit_log => path, 
//     stelle sicher, dass du den Pfad validierst (kein Directory-Traversal).
///////////////////////////////////////////////////////////

use aes_gcm::Aes256Gcm; // AES-GCM mit 256 Bit Schlüssel
use aes_gcm::aead::{Aead, NewAead, generic_array::GenericArray};
use rand::rngs::OsRng;
use rand::RngCore;
use anyhow::{Result, anyhow};
use tracing::{info, error};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use chrono::Utc;
use std::sync::{Arc, Mutex};

/// NEU: Gemeinsamer Mutex, falls mehrere Threads gleichzeitig loggen
/// Du könntest es global definieren oder als local static:
lazy_static::lazy_static! {
    static ref LOG_MUTEX: Mutex<()> = Mutex::new(());
}

/// Verschlüsselt eine Nachricht schichtweise (Onion-Routing).
/// - `message`: Klartext-Daten.
/// - `keys`: Eine Liste von 32-Byte-Schlüsseln (als GenericArray) für jede Verschlüsselungsschicht.
/// Die Verschlüsselung erfolgt von der innersten Schicht (letzte im Array) bis zur äußersten.
///
/// HINWEIS (Security):
/// * Jede Schicht generiert eine Nonce, was gut ist. Achte darauf, 
///   dass dieselben Keys nicht zu oft mit neuen Nonces wiederverwendet werden.
/// * "Onion Routing" im klassischen Sinne => Pro Layer wird an den nächsten Node geschickt, 
///   hier nur "nested encryption".
/// * Bei sehr großen message kann das Schicht-für-Schicht-Kopieren viel RAM verbrauchen.
/// * GCM-Overhead: ciphertext ist plaintext + 16 Byte Tag => plus 12-Byte Nonce je Schicht.
pub fn onion_encrypt(message: &[u8], keys: &[GenericArray<u8, aes_gcm::consts::U32>]) -> Result<Vec<u8>> {
    let mut data = message.to_vec();
    // Verschlüssele in umgekehrter Reihenfolge (innerste zuerst)
    for key in keys.iter().rev() {
        let cipher = Aes256Gcm::new(key);
        // Erzeuge einen zufälligen Nonce (12 Byte, wie von AES-GCM gefordert)
        let mut nonce = [0u8; 12];
        OsRng.fill_bytes(&mut nonce);
        let nonce_ga = GenericArray::from_slice(&nonce);
        // Verschlüssele die aktuellen Daten
        let ciphertext = cipher.encrypt(nonce_ga, data.as_ref())
            .map_err(|e| anyhow!("Encryption failed: {:?}", e))?;
        // Kombiniere Nonce und Ciphertext: (Nonce || Ciphertext)
        let mut combined = nonce.to_vec();
        combined.extend_from_slice(&ciphertext);
        data = combined;
    }
    Ok(data)
}

/// Entschlüsselt eine onion-verschlüsselte Nachricht.
/// - `encrypted_data`: Die verschlüsselte Nachricht (bestehend aus mehreren Nonce||Ciphertext-Blöcken).
/// - `keys`: Die Liste der Schlüssel in derselben Reihenfolge wie bei der Verschlüsselung.
///
/// HINWEIS (Security):
/// * Wir erwarten hier, dass wir exakt so viele Schichten haben wie in `keys`.
/// * Prüfen, ob `data.len()` >= 12 (für Nonce), pro Schicht. Falls "keys" 
///   nicht zum tatsächlichen Layout passen, bricht die Entschlüsselung.
pub fn onion_decrypt(encrypted_data: &[u8], keys: &[GenericArray<u8, aes_gcm::consts::U32>]) -> Result<Vec<u8>> {
    let mut data = encrypted_data.to_vec();
    // Entschlüssele in derselben Reihenfolge wie die Verschlüsselung 
    // (von äußerster zu innerster Schicht)
    for key in keys {
        let cipher = Aes256Gcm::new(key);
        if data.len() < 12 {
            return Err(anyhow!("Data too short, missing nonce"));
        }
        let (nonce_bytes, ciphertext) = data.split_at(12);
        let nonce_ga = GenericArray::from_slice(nonce_bytes);
        let decrypted = cipher.decrypt(nonce_ga, ciphertext)
            .map_err(|e| anyhow!("Decryption failed: {:?}", e))?;
        data = decrypted;
    }
    Ok(data)
}

/// Verschlüsselt Off-chain-Daten mittels Onion-Routing und schreibt ein Audit-Log.
/// - `data`: Die zu schützenden Daten.
/// - `keys`: Die Liste der Verschlüsselungsschlüssel (für jede Schicht).
/// - `audit_log_path`: Pfad zur Audit-Log-Datei.
pub fn secure_offchain_data(data: &[u8], keys: &[GenericArray<u8, aes_gcm::consts::U32>], audit_log_path: &str) -> Result<Vec<u8>> {
    // Verschlüssele die Daten schichtweise.
    let encrypted = onion_encrypt(data, keys)?;
    // Schreibe ein Audit-Log.
    let log_message = format!("{} - Off-chain data encrypted. Original length: {}, Encrypted length: {}\n", 
                                Utc::now(), data.len(), encrypted.len());
    write_audit_log(audit_log_path, &log_message)?;
    Ok(encrypted)
}

/// Schreibt eine Audit-Log-Nachricht in eine Datei.
/// - `path`: Pfad zur Audit-Log-Datei.
/// - `message`: Die Log-Nachricht.
///
/// HINWEIS (Security):
/// * Prüfen, ob du evtl. Directory-Traversal befürchten musst? 
///   In Production => Pfade validieren oder fest definieren.
/// * Achte auf Synchronisation, wenn mehrere Threads/Prozesse parallel loggen.
pub fn write_audit_log<P: AsRef<Path>>(path: P, message: &str) -> Result<()> {
    // NEU: wir validieren Pfad => max. Dateiname => z. B. /var/log/dex/<filename>
    // Du kannst es strenger anlegen, hier minimal:
    let path_ref = path.as_ref();
    let log_base = Path::new("/var/log/dex/");
    let clean_name = path_ref.file_name()
        .ok_or_else(|| anyhow!("No valid filename in path"))?;

    // => In der Praxis müsstest du evtl. ASCII-check etc. 
    let final_path = log_base.join(clean_name);
    // => optional: check final_path.exists() => create etc.

    // Dann Logging-Operation => wir sperren globalen Mutex
    let _guard = LOG_MUTEX.lock().unwrap();

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&final_path)
        .map_err(|e| anyhow!("Failed to open audit log file: {:?}", e))?;
    file.write_all(message.as_bytes())
        .map_err(|e| anyhow!("Failed to write to audit log file: {:?}", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes_gcm::aead::generic_array::GenericArray;

    #[test]
    fn test_onion_encryption_decryption() -> Result<()> {
        // Beispielhafte Schlüssel: In Produktion sollten diese zufällig generiert werden.
        let key1 = GenericArray::clone_from_slice(&[0x11u8; 32]);
        let key2 = GenericArray::clone_from_slice(&[0x22u8; 32]);
        let keys = vec![key1, key2];

        let message = b"Sensitive off-chain trade data";
        let encrypted = onion_encrypt(message, &keys)?;
        let decrypted = onion_decrypt(&encrypted, &keys)?;
        assert_eq!(message.to_vec(), decrypted);
        Ok(())
    }

    #[test]
    fn test_audit_logging() -> Result<()> {
        let test_path = "test_privacy_audit.log";
        let message = "Test audit log message\n";
        write_audit_log(test_path, message)?;
        // Optional: Lese die Datei und überprüfe, ob die Nachricht enthalten ist.
        Ok(())
    }
}
