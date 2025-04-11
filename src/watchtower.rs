// Folder: my_dex/src/watchtower.rs
// Erweiterung für produktionsreifen Watchtower mit Mehrheitsentscheidung

use crate::error::DexError;
use std::collections::{HashMap, HashSet};
use tracing::{info, warn, error, instrument};
use ed25519_dalek::{PublicKey, Signature, Verifier};

#[derive(Clone, Debug)]
pub struct Watchtower {
    channel_states: HashMap<String, WatchtowerState>,
    votes: HashMap<String, HashSet<String>>, // channel_id -> set of approving Watchtower-IDs
    threshold: usize,
}

#[derive(Clone, Debug)]
pub struct WatchtowerState {
    pub latest_commitment_tx: Vec<u8>,
    pub revocation_secret_hash: [u8; 32],
}

impl Watchtower {
    pub fn new() -> Self {
        Watchtower {
            channel_states: HashMap::new(),
            votes: HashMap::new(),
            threshold: 3, // z.B. 3 von 5 Watchtowers nötig für Strafe
        }
    }

    #[instrument(name="wt_register_channel", skip(self, commit_tx))]
    pub fn register_channel(
        &mut self,
        channel_id: &str,
        commit_tx: Vec<u8>,
        rev_hash: [u8; 32]
    ) -> Result<(), DexError> {
        let st = WatchtowerState {
            latest_commitment_tx: commit_tx,
            revocation_secret_hash: rev_hash,
        };
        self.channel_states.insert(channel_id.to_string(), st);
        Ok(())
    }

    #[instrument(name="wt_check_betrug", skip(self))]
    pub fn check_for_betrug(
        &mut self,
        channel_id: &str,
        published_commit: &[u8],
        sender_watchtower_id: &str
    ) -> Result<bool, DexError> {
        let existing = self.channel_states.get(channel_id)
            .ok_or(DexError::Other(format!("Unknown channel {}", channel_id)))?;

        if published_commit != existing.latest_commitment_tx {
            warn!("Betrugsversuch entdeckt in channel_id={}" channel_id);

            // Stimmen sammeln
            let entry = self.votes.entry(channel_id.to_string()).or_default();
            entry.insert(sender_watchtower_id.to_string());

            // Schwelle erreicht?
            if entry.len() >= self.threshold {
                self.punish_cheater(channel_id)?;
                self.votes.remove(channel_id); // Reset nach Aktion
            }
            return Ok(true);
        }
        Ok(false)
    }

    /// Strafaktion – hier Platzhalter für z. B. Broadcast an Chain
    pub fn punish_cheater(&self, channel_id: &str) -> Result<(), DexError> {
        error!("Strafe gegen Channel {} wird ausgeführt!", channel_id);
        // TODO: Transaktion auf Blockchain senden / Funds sichern / Audit-Log etc.
        Ok(())
    }
}
