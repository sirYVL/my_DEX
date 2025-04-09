// identity/identity.rs
//
// Erweitertes Identity-System für Node-Betreiber in einer realen DEX.
// Basierend auf Ed25519-Schlüsseln. Bietet Key-Erzeugung, Signatur- und Verify-Logik,
// plus Access-Control für Private DEX-Netzwerke (bekannte Peer-Keys), TLS-Variante etc.
//
// Ursprünglicher Code:
// use super::*; 
// use ed25519_dalek::{Keypair, Signature, Signer, PublicKey};
// use sha2::{Sha256, Digest};
//
// pub struct Identity {
//     pub keypair: Keypair,
// }
//
// impl Identity {
//     pub fn new() -> Self {
//         let mut csprng = rand::rngs::OsRng {};
//         let keypair = Keypair::generate(&mut csprng);
//         Identity { keypair }
//     }
// }
//
// Hier nun integriert mit neuem Code.

use ed25519_dalek::{Keypair, PublicKey, Signature, Signer, Verifier};
use rand::rngs::OsRng;
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use thiserror::Error;

/// Auth-spezifische Fehler (könntest du auch in DexError integrieren)
#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Key generation error: {0}")]
    KeyGenError(String),

    #[error("Signature verification failed for peer {peer_id}")]
    VerificationFailed { peer_id: String },

    #[error("TLS certificate error: {0}")]
    TlsCertificateError(String),

    #[error("Other auth error: {0}")]
    Other(String),
}

/// Node-Identität via Ed25519
/// Reine Ed25519-Schlüssel, um Node zu identifizieren (PublicKey).
pub struct Identity {
    pub keypair: Keypair,
}

impl Identity {
    /// Ursprüngliche Erzeugung via OsRng
    pub fn new() -> Self {
        let mut csprng = OsRng;
        let keypair = Keypair::generate(&mut csprng);
        Identity { keypair }
    }

    /// Signiere beliebige Nachricht
    pub fn sign_message(&self, msg: &[u8]) -> Signature {
        self.keypair.sign(msg)
    }

    /// Verify => statisch auf PublicKey
    pub fn verify_message(
        pubkey: &PublicKey,
        msg: &[u8],
        signature: &Signature
    ) -> bool {
        pubkey.verify(msg, signature).is_ok()
    }

    /// Liefert den PublicKey als bytes
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.keypair.public.to_bytes()
    }

    pub fn public_key(&self) -> PublicKey {
        self.keypair.public
    }
}

/// AccessControl => für private DEX: Nur bekannte Node-Pubkeys
pub struct AccessControl {
    pub known_peers: HashMap<String, PublicKey>,
}

impl AccessControl {
    pub fn new() -> Self {
        AccessControl {
            known_peers: HashMap::new(),
        }
    }

    /// Merkt sich einen bekannten Peer (peer_id -> pubkey)
    pub fn register_peer(&mut self, peer_id: &str, pubkey: PublicKey) {
        self.known_peers.insert(peer_id.to_string(), pubkey);
    }

    /// Verifiziere Signatur eines Peers
    pub fn verify_peer_sig(
        &self,
        peer_id: &str,
        message: &[u8],
        signature: &Signature
    ) -> Result<(), AuthError> {
        let pk = self.known_peers.get(peer_id)
            .ok_or(AuthError::VerificationFailed { peer_id: peer_id.to_string() })?;
        if Identity::verify_message(pk, message, signature) {
            Ok(())
        } else {
            Err(AuthError::VerificationFailed { peer_id: peer_id.to_string() })
        }
    }
}

/// TLS-Variante => Node-Identität via TLS-Zertifikat
#[derive(Clone, Debug)]
pub struct TlsIdentity {
    pub cert_path: String,
    pub key_path: String,
}

impl TlsIdentity {
    pub fn load(cert_path: &str, key_path: &str) -> Result<Self, AuthError> {
        // In real code => parse PEM/P12 etc.
        // Hier nur Stub.
        Ok(TlsIdentity {
            cert_path: cert_path.to_string(),
            key_path: key_path.to_string(),
        })
    }
}
