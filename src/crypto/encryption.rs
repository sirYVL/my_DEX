////////////////////////////////////////    
// my_dex/src/crypto/encryption.rs
////////////////////////////////////////

use super::*;
use aes_gcm_siv::{Aes256GcmSiv, KeyInit, Aead, Nonce};
use x25519_dalek::{PublicKey, StaticSecret};

pub fn perform_handshake(peer_pubkey_bytes: &[u8; 32]) -> Result<Aes256GcmSiv, &'static str> {
    let my_secret = StaticSecret::new(rand::thread_rng());
    let my_public = PublicKey::from(&my_secret);
    let peer_public = PublicKey::from(*peer_pubkey_bytes);
    let shared = my_secret.diffie_hellman(&peer_public);
    let shared_bytes = shared.to_bytes();
    let key = aes_gcm_siv::Key::from_slice(&shared_bytes);
    let cipher = Aes256GcmSiv::new(key);
    Ok(cipher)
}
