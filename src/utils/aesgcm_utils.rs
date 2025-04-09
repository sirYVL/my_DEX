////////////////////////////////////////////////////////
// my_DEX/src/utils/aesgcm_utils.rs
////////////////////////////////////////////////////////

// AES-GCM: Minimales KDF, Minimale Encryption/Decryption

use anyhow::{Result, anyhow};
use rand::rngs::OsRng;
use rand::RngCore;
use ring::aead::{
    LessSafeKey, UnboundKey, Aad, CHACHA20_POLY1305, // oder AES_256_GCM
    KeyInit, Nonce
};
use sha2::{Sha256, Digest};

pub fn derive_key_from_pass(pass: &str) -> Result<[u8;32]> {
    // minimal => sha256(pass)
    let mut hasher = Sha256::new();
    hasher.update(pass.as_bytes());
    let out = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&out[..32]);
    Ok(key)
}

pub fn aes_gcm_encrypt(key: &[u8;32], plaintext: &[u8]) -> Result<(Vec<u8>, Vec<u8>)> {
    // ring => CHACHA20_POLY1305 => man kann AES_256_GCM
    let unbound_key = UnboundKey::new(&CHACHA20_POLY1305, key)
        .map_err(|e| anyhow!("unbound_key: {:?}", e))?;
    let lesssafe = LessSafeKey::new(unbound_key);

    let mut nonce_bytes = [0u8;12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    let mut inbuf = plaintext.to_vec();
    let tag = lesssafe.seal_in_place_append_tag(nonce, Aad::empty(), &mut inbuf)
        .map_err(|e| anyhow!("seal_in_place: {:?}", e))?;

    let mut out = inbuf; // now has ciphertext+tag
    // return (ciphertext, nonce)
    Ok((out, nonce_bytes.to_vec()))
}

pub fn aes_gcm_decrypt(key: &[u8;32], ciphertext: &[u8], nonce_bytes: &[u8]) -> Result<Vec<u8>> {
    let unbound_key = UnboundKey::new(&CHACHA20_POLY1305, key)
        .map_err(|e| anyhow!("unbound_key: {:?}", e))?;
    let lesssafe = LessSafeKey::new(unbound_key);

    if nonce_bytes.len() != 12 {
        return Err(anyhow!("nonce invalid length"));
    }
    let mut c = ciphertext.to_vec();
    let nonce = Nonce::try_assume_unique_for_key(nonce_bytes)
        .map_err(|_| anyhow!("bad nonce"))?;

    let plain = lesssafe.open_in_place(nonce, Aad::empty(), &mut c)
        .map_err(|e| anyhow!("open_in_place: {:?}", e))?;
    Ok(plain.to_vec())
}

/// Minimales KeypairResolver (f�r Noise) => moved here 
use snow::KeypairResolver;
use snow::Keypair;
pub struct SimpleResolver(pub Vec<u8>);

impl KeypairResolver for SimpleResolver {
    fn resolve(&self, _ctx: &snow::KeypairLocator) -> Option<Keypair> {
        Some(Keypair {
            private: self.0.clone(),
            public: vec![0u8;32], // derive
        })
    }
}
