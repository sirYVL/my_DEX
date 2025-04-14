///////////////////////////////
/// my_DEX/src/crypto/key_loader.rs
///////////////////////////////

use secp256k1::{SecretKey, PublicKey, Secp256k1};
use crate::dex_logic::sign_utils::KeyPair;
use std::fs;
use std::path::Path;

/// Lädt einen privaten Schlüssel im Hex-Format aus einer Datei
pub fn load_keypair_from_file(path: &str) -> Result<KeyPair, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(Path::new(path))?;
    let hex_str = content.trim();
    let secret = SecretKey::from_str(hex_str)?;
    let secp = Secp256k1::new();
    let public = PublicKey::from_secret_key(&secp, &secret);
    Ok(KeyPair { secret, public })
}
