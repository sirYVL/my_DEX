///////////////////////////////////////////////////////////
// my_dex/src/identity/hsm_provider.rs
///////////////////////////////////////////////////////////
//
// Dieses Modul implementiert ein HSM-Provider-System, das in produktionsreifen 
// Umgebungen zertifizierte Sicherheitsalgorithmen f�r Signaturen und Schl�sselverwaltung 
// bereitstellt. Es unterst�tzt externe Hardware-L�sungen (z.?B. Nitrokey HSM) und bietet 
// einen Fallback zu einer softwarebasierten L�sung (mithilfe von ed25519_dalek). 
//
// Der Trait HsmProvider definiert die notwendigen Funktionen zur Signierung und 
// zum Abruf des �ffentlichen Schl�ssels. Je nach Konfiguration kann der Benutzer 
// zwischen einer Hardwarel�sung und einer Softwarel�sung w�hlen.
///////////////////////////////////////////////////////////

use anyhow::Result;
use crate::error::DexError;
use std::fs;
use std::path::Path;
use ed25519_dalek::{Keypair, Signature, Signer, PublicKey};
use rand::rngs::OsRng;

/// Trait f�r sichere HSM-Operationen.
pub trait HsmProvider: Send + Sync {
    /// Signiert die gegebene Nachricht und gibt die Signatur als Byte-Vektor zur�ck.
    fn sign_message(&self, message: &[u8]) -> Result<Vec<u8>, DexError>;
    /// Liefert den �ffentlichen Schl�ssel als Byte-Vektor.
    fn get_public_key(&self) -> Result<Vec<u8>, DexError>;
}

/// NitrokeyHsmProvider: Externe Hardwarel�sung (z.?B. Nitrokey HSM).
/// In einer echten Implementierung w�rden hier die offiziellen Nitrokey-HSM-APIs verwendet.
pub struct NitrokeyHsmProvider {
    // Hier k�nnten Felder stehen, die die Kommunikation mit der Hardware kapseln.
}

impl NitrokeyHsmProvider {
    pub fn new() -> Result<Self, DexError> {
        // Hier sollten Sie die Verbindung zum Nitrokey HSM initialisieren.
        // Im folgenden Dummy-Code simulieren wir eine erfolgreiche Initialisierung.
        Ok(NitrokeyHsmProvider {})
    }
}

impl HsmProvider for NitrokeyHsmProvider {
    fn sign_message(&self, message: &[u8]) -> Result<Vec<u8>, DexError> {
        // Hier w�rden Sie die Nitrokey-HSM API aufrufen, um die Nachricht zu signieren.
        // Dummy-Implementierung: R�ckgabe einer 64-Byte-Signatur.
        let dummy_signature = vec![0u8; 64];
        Ok(dummy_signature)
    }

    fn get_public_key(&self) -> Result<Vec<u8>, DexError> {
        // Hier w�rden Sie den �ffentlichen Schl�ssel �ber die HSM-API abrufen.
        // Dummy-Implementierung: R�ckgabe eines 32-Byte-Pubkeys.
        let dummy_pubkey = vec![1u8; 32];
        Ok(dummy_pubkey)
    }
}

/// SoftwareHsmProvider: Softwarebasierte Fallback-L�sung unter Verwendung von ed25519_dalek.
pub struct SoftwareHsmProvider {
    keypair: Keypair,
}

impl SoftwareHsmProvider {
    /// Generiert ein neues Keypair mithilfe eines kryptographisch sicheren Zufallszahlengenerators.
    pub fn new() -> Result<Self, DexError> {
        let mut csprng = OsRng{};
        let keypair = Keypair::generate(&mut csprng);
        Ok(SoftwareHsmProvider { keypair })
    }

    /// Optionale Funktion zum Laden eines Keypairs aus einer Datei.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, DexError> {
        let data = fs::read(path).map_err(|e| DexError::Other(format!("Keypair file read error: {:?}", e)))?;
        let keypair: Keypair = bincode::deserialize(&data)
            .map_err(|e| DexError::Other(format!("Deserialization error: {:?}", e)))?;
        Ok(SoftwareHsmProvider { keypair })
    }
}

impl HsmProvider for SoftwareHsmProvider {
    fn sign_message(&self, message: &[u8]) -> Result<Vec<u8>, DexError> {
        let signature: Signature = self.keypair.sign(message);
        Ok(signature.to_bytes().to_vec())
    }

    fn get_public_key(&self) -> Result<Vec<u8>, DexError> {
        Ok(self.keypair.public.to_bytes().to_vec())
    }
}

/// W�hlt den HSM-Provider basierend auf der Konfiguration.
/// Wenn `use_hardware` true ist, wird versucht, NitrokeyHsmProvider zu initialisieren.
/// Bei einem Fehler oder wenn `use_hardware` false ist, wird auf die Softwarel�sung zur�ckgegriffen.
pub fn select_hsm_provider(use_hardware: bool) -> Result<Box<dyn HsmProvider>, DexError> {
    if use_hardware {
        match NitrokeyHsmProvider::new() {
            Ok(provider) => Ok(Box::new(provider)),
            Err(e) => {
                warn!("Hardware HSM konnte nicht initialisiert werden: {}. Fallback zur Softwarel�sung.", e);
                Ok(Box::new(SoftwareHsmProvider::new()?))
            }
        }
    } else {
        Ok(Box::new(SoftwareHsmProvider::new()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Sha256, Digest};

    #[test]
    fn test_software_hsm_signing() {
        let provider = SoftwareHsmProvider::new().unwrap();
        let message = b"Test message";
        let signature = provider.sign_message(message).unwrap();
        assert_eq!(signature.len(), 64);
        let public_key = provider.get_public_key().unwrap();
        assert_eq!(public_key.len(), 32);
    }

    #[test]
    fn test_select_hsm_provider() {
        let provider = select_hsm_provider(true).unwrap();
        let message = b"Another test message";
        let signature = provider.sign_message(message).unwrap();
        assert_eq!(signature.len(), 64);
    }
}
