///////////////////////////////////////////////////////
// my_dex/src/crypto/fallback_config.rs
///////////////////////////////////////////////////////

use anyhow::{Result, Context};
use tokio::time::{sleep, Duration};
use tracing::{info, warn, error};
use crate::storage::ipfs_storage::cat_file_from_ipfs;
use ed25519_dalek::{PublicKey, Signature, Verifier};
use hex;

/// L�dt eine Backup-Konfiguration von IPFS mit einer robusten Retry-Logik (exponentielles Backoff).
///
/// # Parameter
/// - `backup_identifier`: Ein eindeutiger Identifier (z.?B. ein IPFS-Hash) f�r die Konfigurationsdatei.
/// - `max_retries`: Maximale Anzahl an Wiederholungen bei Fehlern.
/// - `base_delay`: Basisverz�gerung (z.?B. 1 Sekunde), die bei jedem Fehlversuch erh�ht wird.
///
/// # R�ckgabe
/// Gibt die geladene Konfiguration als String zur�ck oder einen Fehler, falls alle Versuche fehlschlagen.
pub async fn load_backup_config_with_retry(backup_identifier: &str, max_retries: usize, base_delay: Duration) -> Result<String> {
    let mut attempt = 0;
    loop {
        match cat_file_from_ipfs(backup_identifier).await {
            Ok(data) => {
                let config = String::from_utf8(data).context("Failed to convert backup config to UTF-8")?;
                info!("Backup configuration successfully loaded on attempt {}", attempt + 1);
                return Ok(config);
            },
            Err(e) => {
                attempt += 1;
                if attempt >= max_retries {
                    error!("Failed to load backup config after {} attempts: {:?}", attempt, e);
                    return Err(e.into());
                } else {
                    let delay = base_delay * attempt as u32;
                    warn!("Attempt {} to load backup config failed: {:?}. Retrying in {:?}...", attempt, e, delay);
                    sleep(delay).await;
                }
            }
        }
    }
}

/// �berpr�ft die digitale Signatur einer wiederhergestellten Konfiguration.
///
/// # Parameter
/// - `config`: Die wiederhergestellte Konfiguration als String.
/// - `signature_hex`: Die digitale Signatur im Hexadezimalformat.
/// - `public_key_hex`: Der �ffentliche Schl�ssel im Hexadezimalformat, der zur Validierung verwendet wird.
///
/// # R�ckgabe
/// Gibt `true` zur�ck, wenn die Signatur g�ltig ist, andernfalls `false`.
pub fn verify_config_signature(config: &str, signature_hex: &str, public_key_hex: &str) -> bool {
    // Konvertiere den �ffentlichen Schl�ssel aus Hexadezimal in ein PublicKey-Objekt.
    let public_key_bytes = match hex::decode(public_key_hex) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let public_key = match PublicKey::from_bytes(&public_key_bytes) {
        Ok(pk) => pk,
        Err(_) => return false,
    };

    // Konvertiere die Signatur aus Hexadezimal in ein Signature-Objekt.
    let signature_bytes = match hex::decode(signature_hex) {
        Ok(bytes) => bytes,
        Err(_) => return false,
    };
    let signature = match Signature::from_bytes(&signature_bytes) {
        Ok(sig) => sig,
        Err(_) => return false,
    };

    // �berpr�fe, ob die Signatur f�r die Konfiguration g�ltig ist.
    public_key.verify(config.as_bytes(), &signature).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    
    #[tokio::test]
    async fn test_load_backup_config_with_retry_failure() {
        // Test: Bei ung�ltigem Identifier sollte ein Fehler zur�ckgegeben werden.
        let result = load_backup_config_with_retry("invalid_hash", 3, Duration::from_secs(1)).await;
        assert!(result.is_err());
    }
    
    #[test]
    fn test_verify_config_signature_failure() {
        let config = "Test configuration";
        let signature_hex = "deadbeef"; // ung�ltig
        let public_key_hex = "deadbeef"; // ung�ltig
        assert!(!verify_config_signature(config, signature_hex, public_key_hex));
    }
}
