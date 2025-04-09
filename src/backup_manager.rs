// Folder: src
// File: backup_manager.rs

use std::fs;
use std::path::Path;
use std::time::Duration;
use tokio::time;
use tracing::{info, warn};
use crate::storage::ipfs_storage::add_file_to_ipfs;

/// Speichert eine Datei, die als Backup dient, dezentral �ber IPFS und gibt den Hash zur�ck.
pub async fn backup_file(file_path: &str) -> Result<String, Box<dyn std::error::Error>> {
    if !Path::new(file_path).exists() {
        return Err(format!("Backup-Datei {} existiert nicht.", file_path).into());
    }
    let hash = add_file_to_ipfs(file_path).await?;
    Ok(hash)
}

/// Startet einen periodischen Backup-Task, der in regelm��igen Abst�nden die angegebene Datei sichert.
/// Der zur�ckgegebene Hash wird geloggt.
pub async fn start_periodic_backup(file_path: &str, interval_sec: u64) {
    let mut interval = time::interval(Duration::from_secs(interval_sec));
    loop {
        interval.tick().await;
        match backup_file(file_path).await {
            Ok(hash) => info!("Backup von {} erfolgreich, IPFS Hash: {}", file_path, hash),
            Err(e) => warn!("Fehler beim Backup von {}: {:?}", file_path, e),
        }
    }
}
