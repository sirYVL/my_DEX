// src/identity/access_control.rs

use anyhow::Result;
use ed25519_dalek::{Signature, PublicKey, Verifier};
use tracing::{warn, instrument};

#[derive(Debug, Default)]
pub struct AccessPolicy {
    pub allowed_pubkeys: Vec<Vec<u8>>, // Byte-Arrays
}

#[instrument(name="is_allowed", skip(policy, pubkey_bytes))]
pub fn is_allowed(policy: &AccessPolicy, pubkey_bytes: &[u8]) -> bool {
    policy.allowed_pubkeys.iter().any(|k| k == pubkey_bytes)
}

#[instrument(name="verify_message", skip(pubkey, message, signature))]
pub fn verify_message(pubkey: &[u8], message: &[u8], signature: &[u8]) -> Result<bool> {
    let pk = PublicKey::from_bytes(pubkey)?;
    let sig = Signature::from_bytes(signature)?;
    Ok(pk.verify(message, &sig).is_ok())
}
