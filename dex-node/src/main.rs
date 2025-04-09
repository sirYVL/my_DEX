// my_dex/dex-node/src/main.rs
//
// Lädt Config via confy / YAML, startet DexNode
//
// NEU (Sicherheitsupdate):
//  1. Lies das Config-File roh ein (read_to_string).
//  2. Signaturprüfung: verify_config_signature(...).
//  3. Nur bei success => parse mit confy::from_str(...) => NodeConfig
//  4. Optionaler Fallback, falls Prüfung fehlschlägt.

use anyhow::{Result, anyhow, Context};
use confy;
use tokio;
use log::{info, warn, error};
use env_logger;
use crate::node_logic::{DexNode, NodeConfig};

// Wenn Node Logic in einer separaten Datei liegt
mod node_logic;

// Damit wir "verify_config_signature" aufrufen können, 
// brauchst du ggf. so eine Funktion in crate::crypto::fallback_config o.ä.:
use crate::crypto::fallback_config::verify_config_signature;

#[tokio::main]
async fn main() -> Result<()> {
    // Logging init
    env_logger::init();
    info!("Starting dex-node...");

    // Pfad zum Config-File
    let cfg_path = "./config/default_config.yaml";

    // 1) Lies das File "roh" ein
    let raw_cfg = match std::fs::read_to_string(cfg_path) {
        Ok(s) => s,
        Err(e) => {
            error!("Konnte Config-File {} nicht lesen: {:?}", cfg_path, e);
            // Falls du fallback willst, könntest du hier abbrechen oder Backup-Config laden.
            return Err(anyhow!("Config file not found."));
        }
    };

    // 2) Signatur-Prüfung
    //    Du brauchst eine signierte Config + passendes PublicKey. 
    //    Das hier ist nur ein DEMO-Aufruf (public_key_str = "my_public_key")
    let public_key_str = "my_public_key";
    if !verify_config_signature(&raw_cfg, public_key_str) {
        error!("Signatur der Config-Datei ist ungültig!");
        return Err(anyhow!("Config signature invalid"));
    }

    // 3) Parse => confy::from_str -> in NodeConfig
    //    Anstatt confy::load_path(...), 
    //    nehmen wir confy::from_str => wir haben den YAML-String ja schon.
    let cfg: NodeConfig = confy::from_str("dex_node_app", &raw_cfg)
        .context("Fehler beim Konvertieren von YAML in NodeConfig")?;
    info!("Loaded config: {:?}", cfg);

    // => DexNode erstellen und starten
    let node = DexNode::new(cfg)?;
    node.start().await?;

    // Warte auf Ctrl+C => geordneter Shutdown
    tokio::signal::ctrl_c().await?;
    info!("Shutting down dex-node...");
    Ok(())
}
