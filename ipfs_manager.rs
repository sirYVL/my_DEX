// Folder: src
// File: my_dex/src/ipfs_manager.rs

use std::process::{Command, Stdio};
use std::path::PathBuf;
use std::env;

/// Ermittelt den Pfad zum IPFS-Binary basierend auf dem Betriebssystem.
/// Erwartet, dass f�r Windows die Datei "my_dex/.ipfs/bin/go-ipfs-win-pc" 
/// und f�r Linux "my_dex/.ipfs/bin/go-ipfs-linux" im aktuellen Arbeitsverzeichnis vorhanden ist.
pub fn get_ipfs_binary_path() -> Result<PathBuf, String> {
    let current_dir = env::current_dir().map_err(|e| e.to_string())?;
    // Verwende den expliziten Ordnerpfad "my_dex/.ipfs/bin/"
    let base_path = current_dir.join("my_dex").join(".ipfs").join("bin");
    let os = env::consts::OS;
    match os {
        "windows" => Ok(base_path.join("go-ipfs-win-pc")),
        "linux" => Ok(base_path.join("go-ipfs-linux")),
        other => Err(format!("Unsupported OS: {}", other)),
    }
}

/// Startet den lokalen IPFS-Daemon im Hintergrund.
/// Der Daemon wird �ber den lokalen IPFS-Binary gestartet, der im Paket enthalten ist.
pub fn start_ipfs_daemon() -> Result<(), String> {
    let ipfs_path = get_ipfs_binary_path()?;
    if !ipfs_path.exists() {
        return Err(format!("IPFS binary not found at {:?}", ipfs_path));
    }

    // Starte den IPFS-Daemon im Hintergrund; stdout und stderr werden unterdr�ckt.
    let child = Command::new(ipfs_path)
        .arg("daemon")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;

    println!("Local IPFS daemon started (PID: {})", child.id());
    Ok(())
}
