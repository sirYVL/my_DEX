///////////////////////////////////////////////////////////
// my_dex/src/crypto/hsm_provider.rs
///////////////////////////////////////////////////////////
//
// Diese Datei implementiert eine produktionsreife HSM-L�sung f�r USB?HSMs,
// die mehrere Hardwareanbieter (z. B. Nitrokey HSM, YubiHSM) unterst�tzt.
// Der Nutzer kann in der Konfiguration ausw�hlen, welche L�sung zum Einsatz kommt.
// Die Implementierung basiert auf der PKCS#11?Schnittstelle (mittels der "cryptoki" Crate).
//
// Voraussetzung: Die entsprechenden PKCS#11?Treiber/Bibliotheken sind installiert und
// die Konfiguration (in z.?B. node_config.yaml) enth�lt den Pfad zur PKCS#11?Bibliothek,
// die Slot-ID sowie den User-PIN.

use std::fmt;
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use thiserror::Error;
use tracing::{info, debug, error, warn};

// Wir verwenden die "cryptoki" Crate als PKCS#11-Wrapper:
use cryptoki::context::{Pkcs11, CInitializeArgs};
use cryptoki::object::{Attribute, ObjectHandle, ObjectClass, AttributeType};
use cryptoki::session::{Session, SessionFlags, UserType};
use cryptoki::mechanism::Mechanism;

///////////////////////////////////////////////
// Fehlerdefinitionen
///////////////////////////////////////////////

#[derive(Error, Debug)]
pub enum HsmError {
    #[error("PKCS#11 Error: {0}")]
    Pkcs11Error(#[from] cryptoki::error::Error),
    #[error("Operation failed: {0}")]
    OperationFailed(String),
    #[error("Key not found")]
    KeyNotFound,
    #[error("Configuration error: {0}")]
    ConfigError(String),
}

///////////////////////////////////////////////
// Strukturen für Schlüssel und Signaturen
///////////////////////////////////////////////

#[derive(Debug, Clone)]
pub struct KeyPair {
    pub public: Vec<u8>,
    // Hinweis: Der private Schlüssel bleibt im HSM und wird nicht exportiert.
}

#[derive(Debug, Clone)]
pub struct Signature {
    pub signature: Vec<u8>,
}

///////////////////////////////////////////////
// Trait-Definition: HsmProvider
///////////////////////////////////////////////

pub trait HsmProvider: Send + Sync {
    fn generate_keypair(&mut self) -> Result<KeyPair, HsmError>;
    fn sign_message(&mut self, message: &[u8]) -> Result<Signature, HsmError>;
    fn get_public_key(&self) -> Result<Vec<u8>, HsmError>;
    fn rotate_key(&mut self) -> Result<KeyPair, HsmError>;
}

///////////////////////////////////////////////
// Konfigurationsparameter für HSM-Anbieter
///////////////////////////////////////////////

#[derive(Debug, Clone)]
pub enum HsmType {
    Nitrokey,
    YubiHsm,
}

#[derive(Debug, Clone)]
pub struct HsmConfig {
    pub hsm_type: HsmType,
    // Pfad zur PKCS#11-Bibliothek
    pub pkcs11_lib_path: String,
    // Slot-ID (zum Beispiel USB-HSM-Identifikation)
    pub slot_id: u64,
    // User-PIN für den Zugriff auf den HSM
    pub user_pin: String,
}

///////////////////////////////////////////////
// Nitrokey HSM Provider Implementierung
///////////////////////////////////////////////

pub struct NitrokeyHsmProvider {
    config: HsmConfig,
    pkcs11: Pkcs11,
    session: Option<Session>,
    key_handle: Option<ObjectHandle>,
    public_key: Option<Vec<u8>>,
}

impl NitrokeyHsmProvider {
    pub fn new(config: HsmConfig) -> Result<Self, HsmError> {
        let pkcs11 = Pkcs11::new(Path::new(&config.pkcs11_lib_path))?;
        pkcs11.initialize(CInitializeArgs::OsThreads)?;
        Ok(NitrokeyHsmProvider {
            config,
            pkcs11,
            session: None,
            key_handle: None,
            public_key: None,
        })
    }

    fn open_session(&mut self) -> Result<(), HsmError> {
        if self.session.is_none() {
            let slot = self.pkcs11.get_slot_list(true)?
                .into_iter()
                .find(|s| s.slot_id() == self.config.slot_id)
                .ok_or_else(|| HsmError::ConfigError(format!("Slot {} nicht gefunden", self.config.slot_id)))?;
            let mut session = self.pkcs11.open_session(slot, SessionFlags::RW_SESSION | SessionFlags::SERIAL_SESSION, None, None)?;
            session.login(UserType::User, Some(&self.config.user_pin))?;
            self.session = Some(session);
        }
        Ok(())
    }

    fn create_keypair(&mut self) -> Result<KeyPair, HsmError> {
        self.open_session()?;
        let session = self.session.as_mut().ok_or_else(|| HsmError::OperationFailed("Session nicht verfügbar".into()))?;
        
        // Parameter zur Schlüsselerzeugung: Passen Sie die Mechanismen und Attribute gemäß Ihren Anforderungen an.
        let mechanism = Mechanism::Ecdsa;
        let public_template = vec![
            Attribute::new(AttributeType::Token, true),
            Attribute::new(AttributeType::Private, false),
            Attribute::new(AttributeType::Label, "Nitrokey Public Key"),
        ];
        let private_template = vec![
            Attribute::new(AttributeType::Token, true),
            Attribute::new(AttributeType::Private, true),
            Attribute::new(AttributeType::Sensitive, true),
            Attribute::new(AttributeType::Label, "Nitrokey Private Key"),
        ];
        
        let (pub_handle, priv_handle) = session.generate_key_pair(mechanism, &public_template, &private_template)?;
        
        let pub_key_attr = session.get_attribute_value(pub_handle, &[AttributeType::Value])?;
        let public_key = pub_key_attr[0].clone().into_bytes()
            .ok_or_else(|| HsmError::OperationFailed("Kein öffentlicher Schlüsselwert".into()))?;
        
        self.key_handle = Some(priv_handle);
        self.public_key = Some(public_key.clone());
        
        info!("Nitrokey HSM: Schlüsselpaar generiert");
        Ok(KeyPair { public: public_key })
    }
}

impl HsmProvider for NitrokeyHsmProvider {
    fn generate_keypair(&mut self) -> Result<KeyPair, HsmError> {
        self.create_keypair()
    }

    fn sign_message(&mut self, message: &[u8]) -> Result<Signature, HsmError> {
        self.open_session()?;
        let session = self.session.as_mut().ok_or_else(|| HsmError::OperationFailed("Session nicht verfügbar".into()))?;
        let key_handle = self.key_handle.ok_or(HsmError::KeyNotFound)?;
        let mechanism = Mechanism::Ecdsa;
        session.sign_init(mechanism, key_handle)?;
        let sig_bytes = session.sign(message)?;
        info!("Nitrokey HSM: Nachricht signiert");
        Ok(Signature { signature: sig_bytes })
    }

    fn get_public_key(&self) -> Result<Vec<u8>, HsmError> {
        self.public_key.clone().ok_or(HsmError::KeyNotFound)
    }

    fn rotate_key(&mut self) -> Result<KeyPair, HsmError> {
        // Optionale Implementierung: Löschen Sie den alten Schlüssel (falls erforderlich)
        // und generieren Sie ein neues Schlüsselpaar.
        self.generate_keypair()
    }
}

///////////////////////////////////////////////
// YubiHSM Provider Implementierung
///////////////////////////////////////////////

pub struct YubiHsmProvider {
    config: HsmConfig,
    pkcs11: Pkcs11,
    session: Option<Session>,
    key_handle: Option<ObjectHandle>,
    public_key: Option<Vec<u8>>,
}

impl YubiHsmProvider {
    pub fn new(config: HsmConfig) -> Result<Self, HsmError> {
        let pkcs11 = Pkcs11::new(Path::new(&config.pkcs11_lib_path))?;
        pkcs11.initialize(CInitializeArgs::OsThreads)?;
        Ok(YubiHsmProvider {
            config,
            pkcs11,
            session: None,
            key_handle: None,
            public_key: None,
        })
    }

    fn open_session(&mut self) -> Result<(), HsmError> {
        if self.session.is_none() {
            let slot = self.pkcs11.get_slot_list(true)?
                .into_iter()
                .find(|s| s.slot_id() == self.config.slot_id)
                .ok_or_else(|| HsmError::ConfigError(format!("Slot {} nicht gefunden", self.config.slot_id)))?;
            let mut session = self.pkcs11.open_session(slot, SessionFlags::RW_SESSION | SessionFlags::SERIAL_SESSION, None, None)?;
            session.login(UserType::User, Some(&self.config.user_pin))?;
            self.session = Some(session);
        }
        Ok(())
    }

    fn create_keypair(&mut self) -> Result<KeyPair, HsmError> {
        self.open_session()?;
        let session = self.session.as_mut().ok_or_else(|| HsmError::OperationFailed("Session nicht verfügbar".into()))?;
        
        let mechanism = Mechanism::Ecdsa;
        let public_template = vec![
            Attribute::new(AttributeType::Token, true),
            Attribute::new(AttributeType::Private, false),
            Attribute::new(AttributeType::Label, "YubiHSM Public Key"),
        ];
        let private_template = vec![
            Attribute::new(AttributeType::Token, true),
            Attribute::new(AttributeType::Private, true),
            Attribute::new(AttributeType::Sensitive, true),
            Attribute::new(AttributeType::Label, "YubiHSM Private Key"),
        ];
        
        let (pub_handle, priv_handle) = session.generate_key_pair(mechanism, &public_template, &private_template)?;
        
        let pub_key_attr = session.get_attribute_value(pub_handle, &[AttributeType::Value])?;
        let public_key = pub_key_attr[0].clone().into_bytes()
            .ok_or_else(|| HsmError::OperationFailed("Kein öffentlicher Schlüsselwert".into()))?;
        
        self.key_handle = Some(priv_handle);
        self.public_key = Some(public_key.clone());
        
        info!("YubiHSM: Schlüsselpaar generiert");
        Ok(KeyPair { public: public_key })
    }
}

impl HsmProvider for YubiHsmProvider {
    fn generate_keypair(&mut self) -> Result<KeyPair, HsmError> {
        self.create_keypair()
    }

    fn sign_message(&mut self, message: &[u8]) -> Result<Signature, HsmError> {
        self.open_session()?;
        let session = self.session.as_mut().ok_or_else(|| HsmError::OperationFailed("Session nicht verfügbar".into()))?;
        let key_handle = self.key_handle.ok_or(HsmError::KeyNotFound)?;
        let mechanism = Mechanism::Ecdsa;
        session.sign_init(mechanism, key_handle)?;
        let sig_bytes = session.sign(message)?;
        info!("YubiHSM: Nachricht signiert");
        Ok(Signature { signature: sig_bytes })
    }

    fn get_public_key(&self) -> Result<Vec<u8>, HsmError> {
        self.public_key.clone().ok_or(HsmError::KeyNotFound)
    }

    fn rotate_key(&mut self) -> Result<KeyPair, HsmError> {
        self.generate_keypair()
    }
}

///////////////////////////////////////////////
// Abstrakte Fabrikfunktion: Auswahl des HSM-Providers
///////////////////////////////////////////////

pub fn create_hsm_provider(config: HsmConfig) -> Result<Arc<Mutex<dyn HsmProvider>>, HsmError> {
    match config.hsm_type {
        HsmType::Nitrokey => {
            let provider = NitrokeyHsmProvider::new(config)?;
            Ok(Arc::new(Mutex::new(provider)))
        },
        HsmType::YubiHsm => {
            let provider = YubiHsmProvider::new(config)?;
            Ok(Arc::new(Mutex::new(provider)))
        },
    }
}

// Optional: Debug-Implementierung für HsmProvider-Trait-Objekte
impl fmt::Debug for dyn HsmProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HsmProvider trait object")
    }
}
