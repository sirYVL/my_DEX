/////////////////////////////////////////////////////////////
// my_DEX/src/plugin/plugin_registry.rs
/////////////////////////////////////////////////////////////

use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use ed25519_dalek::{PublicKey, Signature, Verifier, SIGNATURE_LENGTH};

/// Das PluginRegistry verwaltet registrierte Plugins und verifiziert deren Authentizität.
/// Es verwendet eine interne Whitelist, um sicherzustellen, dass nur von internen Entwicklern
/// freigegebene Plugins registriert werden.
pub struct PluginRegistry {
    /// Whitelist: Enthält die Namen der Plugins, die intern freigegeben sind.
    pub allowed_plugins: HashSet<String>,
    /// Liste der Namen registrierter Plugins.
    pub registered_plugins: Vec<String>,
    /// Öffentlicher Schlüssel, der zur Verifikation von Plugin-Signaturen verwendet wird.
    pub public_key: PublicKey,
}

impl PluginRegistry {
    /// Erstellt eine neue Instanz der PluginRegistry.
    /// `allowed_plugins` ist die Whitelist, die die Namen der erlaubten Plugins enthält.
    pub fn new(public_key: PublicKey, allowed_plugins: HashSet<String>) -> Self {
        PluginRegistry {
            allowed_plugins,
            registered_plugins: Vec::new(),
            public_key,
        }
    }

    /// Verifiziert die Plugin-Datei an `plugin_path` anhand der übergebenen Signatur.
    /// Die Signatur muss exakt SIGNATURE_LENGTH lang sein.
    pub fn verify_plugin<P: AsRef<Path>>(
        &self,
        plugin_path: P,
        signature_bytes: &[u8],
    ) -> Result<bool, Box<dyn std::error::Error>> {
        // Lese die Plugin-Datei in einen Byte-Vektor ein
        let mut file = File::open(plugin_path.as_ref())?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;

        if signature_bytes.len() != SIGNATURE_LENGTH {
            return Err("Ungültige Signaturlänge".into());
        }

        let signature = Signature::from_bytes(signature_bytes)?;

        // Überprüfe die Signatur anhand der Plugin-Daten
        match self.public_key.verify(&data, &signature) {
            Ok(_) => Ok(true),
            Err(e) => Err(Box::new(e)),
        }
    }

    /// Registriert ein Plugin, falls die Verifikation erfolgreich ist und das Plugin in der internen Whitelist steht.
    ///
    /// # Parameter
    /// - `plugin_name`: Der Name des Plugins.
    /// - `plugin_path`: Pfad zur Plugin-Datei.
    /// - `signature_bytes`: Die kryptographische Signatur, die mit dem internen Entwickler-Schlüssel erzeugt wurde.
    pub fn register_plugin<P: AsRef<Path>>(
        &mut self,
        plugin_name: String,
        plugin_path: P,
        signature_bytes: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Überprüfe, ob das Plugin in der internen Whitelist enthalten ist.
        if !self.allowed_plugins.contains(&plugin_name) {
            return Err("Plugin ist nicht in der internen Whitelist".into());
        }

        // Verifiziere die Plugin-Signatur.
        if self.verify_plugin(plugin_path, signature_bytes)? {
            self.registered_plugins.push(plugin_name);
            Ok(())
        } else {
            Err("Plugin-Signaturverifikation fehlgeschlagen".into())
        }
    }
}
