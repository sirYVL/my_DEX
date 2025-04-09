// my_dex/src/identity/keystore.rs
//
// Speichert PrivateKey verschl�sselt via AES-GCM.

use serde::{Serialize, Deserialize};
use anyhow::{Result, anyhow};
use std::fs;
use ed25519_dalek::{Keypair, PublicKey, SecretKey, Signer};
use rand::rngs::OsRng;
use tracing::{info, warn, instrument};

use crate::utils::aesgcm_utils::{aes_gcm_encrypt, aes_gcm_decrypt};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NodeIdentity {
    pub public_key: Vec<u8>,
    pub cipher_secret: Vec<u8>, // AES-GCM-ciphered secret
    pub nonce: Vec<u8>,
}

/// Keystore => kann mehrere Keys, hier nur 1
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Keystore {
    pub identity: NodeIdentity,
}

impl Keystore {
    #[instrument(name="keystore_generate")]
    pub fn generate(master_pass: &str) -> Result<Self> {
        let mut csprng = OsRng{};
        let keypair = Keypair::generate(&mut csprng);

        let secret_bytes = keypair.secret.to_bytes();
        let pub_bytes = keypair.public.to_bytes();

        // AES-GCM => keyable from master_pass (z. B. using a KDF, hier quick 'n dirty)
        let derived_key = crate::utils::aesgcm_utils::derive_key_from_pass(master_pass)?;
        let (cipher_secret, nonce) = aes_gcm_encrypt(&derived_key, &secret_bytes)?;

        let node_id = NodeIdentity {
            public_key: pub_bytes.to_vec(),
            cipher_secret,
            nonce,
        };
        Ok(Self { identity: node_id })
    }

    #[instrument(name="keystore_save", skip(self, path))]
    pub fn save_to_file(&self, path: &str) -> Result<()> {
        let data = serde_json::to_string_pretty(self)
            .map_err(|e| anyhow!("Ser. error: {:?}", e))?;
        fs::write(path, data)?;
        info!("Keystore saved to {}", path);
        Ok(())
    }

    #[instrument(name="keystore_load", skip(path, master_pass))]
    pub fn load_from_file(path: &str, master_pass: &str) -> Result<Self> {
        let data = fs::read_to_string(path)?;
        let ks: Keystore = serde_json::from_str(&data)
            .map_err(|e| anyhow!("DeSer. error: {:?}", e))?;
        // test decrypt:
        let _ = ks.get_secretkey(master_pass)?; 
        info!("Keystore loaded from {}", path);
        Ok(ks)
    }

    #[instrument(name="keystore_rotate")]
    pub fn rotate_key(&mut self, master_pass: &str) -> Result<()> {
        let new = Keystore::generate(master_pass)?;
        self.identity = new.identity;
        Ok(())
    }

    #[instrument(name="keystore_sign", skip(self, message, master_pass))]
    pub fn sign(&self, message: &[u8], master_pass: &str) -> Result<Vec<u8>> {
        let secret = self.get_secretkey(master_pass)?;
        let public = PublicKey::from(&secret);
        let keypair = Keypair { secret, public };
        let sig = keypair.sign(message).to_bytes().to_vec();
        Ok(sig)
    }

    /// Intern => decrypt & build ed25519::SecretKey
    fn get_secretkey(&self, master_pass: &str) -> Result<SecretKey> {
        let derived_key = crate::utils::aesgcm_utils::derive_key_from_pass(master_pass)?;
        let plain = aes_gcm_decrypt(
            &derived_key, 
            &self.identity.cipher_secret, 
            &self.identity.nonce
        )?;
        let sec = SecretKey::from_bytes(&plain)
            .map_err(|e| anyhow!("SecretKey invalid: {:?}", e))?;
        Ok(sec)
    }
}
