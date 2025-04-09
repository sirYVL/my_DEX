// my_dex/src/dex_logic/sign_utils.rs
//
// Beispiel: Echte Signatur-Routinen mit secp256k1.
// Simuliert "private key" & "public key" als c+public, und signiert Hashes.
//
// NEU (Sicherheitsverbesserung):
//  - Domain-Separation (DOMAIN_PREFIX), um Replay/Verwechslungsangriffe zwischen verschiedenen Protokollen zu erschweren.
//  - Hinweis, dass SecretKeys in echter Produktion nicht im Klartext gespeichert werden sollten.

use secp256k1::{Secp256k1, Message, SecretKey, PublicKey, Signature};
use secp256k1::rand::rngs::OsRng;
use sha2::{Sha256, Digest};

/// Dieses Präfix nutzen wir beim Hashing, damit Signaturen nicht in anderen Kontexten wiederverwendet werden können.
/// In echten Projekten kann das z. B. "my_dex/mainnet/v1" oder Ähnliches sein.
const DOMAIN_PREFIX: &str = "my_dex_sign_v1:";

#[derive(Clone, Debug)]
pub struct KeyPair {
    pub secret: SecretKey,
    pub public: PublicKey,
}

impl KeyPair {
    /// Erstellt ein zufälliges (SecretKey, PublicKey)-Paar.
    /// Achtung: In echter Produktion solltest du den SecretKey *nicht* unverschlüsselt auf der Platte ablegen.
    pub fn new_random() -> Self {
        let secp = Secp256k1::new();
        let mut rng = OsRng;
        let secret = SecretKey::new(&mut rng);
        let public = PublicKey::from_secret_key(&secp, &secret);
        Self { secret, public }
    }

    /// Signiert die übergebenen Bytes (mit Domain-Separation).
    /// Wir nehmen das byte[] + Domain-Prefix => hash => ECDSA-Signatur.
    pub fn sign_message(&self, msg_bytes: &[u8]) -> Signature {
        let secp = Secp256k1::new();
        let mut hasher = Sha256::new();
        // Domain-Prefix einbinden, damit Signaturen nicht in anderem Kontext
        // missbraucht werden können.
        hasher.update(DOMAIN_PREFIX.as_bytes());
        hasher.update(msg_bytes);
        let digest = hasher.finalize();
        let msg = Message::from_slice(&digest).expect("Must be 32 bytes");
        secp.sign_ecdsa(&msg, &self.secret)
    }

    /// Verifiziert die Signatur (ECDSA secp256k1).
    /// Wir packen dieselbe Domain + msg_bytes in den Hash,
    /// der Key und die Signatur müssen übereinstimmen.
    pub fn verify_message(pub_key: &PublicKey, msg_bytes: &[u8], sig: &Signature) -> bool {
        let secp = Secp256k1::new();
        let mut hasher = Sha256::new();
        hasher.update(DOMAIN_PREFIX.as_bytes());
        hasher.update(msg_bytes);
        let digest = hasher.finalize();
        let msg = match Message::from_slice(&digest) {
            Ok(m) => m,
            Err(_) => return false,
        };
        secp.verify_ecdsa(&msg, sig, pub_key).is_ok()
    }
}
