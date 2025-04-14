//////////////////////////////////////////
/// my_dex/src/crypto/key_loader.rs
//////////////////////////////////////////

use secp256k1::{SecretKey, PublicKey, Secp256k1};
use crate::dex_logic::sign_utils::KeyPair;
use std::fs;
use std::fs::create_dir_all;
use std::path::Path;
use std::io::Write;

/// Pfad zur Key-Datei (z. B. im Home-Verzeichnis)
const DEFAULT_KEY_PATH: &str = ".my_dex/keys/node_key.hex";

/// Erstellt oder lädt ein KeyPair aus Datei
pub fn get_or_create_keypair() -> Result<KeyPair, Box<dyn std::error::Error>> {
    let home = dirs::home_dir().ok_or("Kein Home-Verzeichnis gefunden")?;
    let key_path = home.join(DEFAULT_KEY_PATH);

    if key_path.exists() {
        let content = fs::read_to_string(&key_path)?;
        let hex_str = content.trim();
        let secret = SecretKey::from_str(hex_str)?;
        let secp = Secp256k1::new();
        let public = PublicKey::from_secret_key(&secp, &secret);
        Ok(KeyPair { secret, public })
    } else {
        // Verzeichnis anlegen, falls nicht vorhanden
        if let Some(parent) = key_path.parent() {
            create_dir_all(parent)?;
        }

        let secret = SecretKey::new(&mut secp256k1::rand::rngs::OsRng);
        let hex = hex::encode(secret.secret_bytes());

        let mut file = fs::File::create(&key_path)?;
        file.write_all(hex.as_bytes())?;
        file.sync_all()?;

        let secp = Secp256k1::new();
        let public = PublicKey::from_secret_key(&secp, &secret);
        Ok(KeyPair { secret, public })
    }
}
