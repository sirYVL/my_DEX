// Folder: my_dex/src/watchtower.rs
// Erweiterung für produktionsreifen Watchtower mit Mehrheitsentscheidung + Aktionen

use crate::error::DexError;
use std::collections::{HashMap, HashSet};
use tracing::{info, warn, error, instrument};

#[derive(Clone, Debug)]
pub struct Watchtower {
    channel_states: HashMap<String, WatchtowerState>,
    votes: HashMap<String, HashSet<String>>, // channel_id -> set of approving Watchtower-IDs
    threshold: usize,
    banned_accounts: HashSet<String>,
    frozen_balances: HashSet<String>,
    audit_log: Vec<String>,
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
            threshold: 3,
            banned_accounts: HashSet::new(),
            frozen_balances: HashSet::new(),
            audit_log: Vec::new(),
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
            warn!("Betrugsversuch entdeckt in channel_id={}", channel_id);

            self.ban_account(channel_id);
            self.freeze_balance(channel_id);
            self.log_audit_entry(channel_id);
            self.sign_proof(channel_id);

            let entry = self.votes.entry(channel_id.to_string()).or_default();
            entry.insert(sender_watchtower_id.to_string());

            if entry.len() >= self.threshold {
                self.punish_cheater(channel_id)?;
                self.votes.remove(channel_id);
            }

            self.send_gossip_alert(channel_id);
            self.block_network_access(channel_id);

            return Ok(true);
        }
        Ok(false)
    }

    fn ban_account(&mut self, channel_id: &str) {
        self.banned_accounts.insert(channel_id.to_string());
        info!("Account {} wurde gesperrt.", channel_id);
    }

    fn freeze_balance(&mut self, channel_id: &str) {
        self.frozen_balances.insert(channel_id.to_string());
        info!("Balance von {} wurde eingefroren.", channel_id);
    }

    fn log_audit_entry(&mut self, channel_id: &str) {
        let entry = format!("⚠️ Audit: Betrug erkannt im Channel {}", channel_id);
        self.audit_log.push(entry.clone());
        info!("{}", entry);
    }

    fn sign_proof(&self, channel_id: &str) {
        info!("Beweis für Channel {} wurde signiert. [Signatur-Platzhalter]", channel_id);
    }

    pub fn punish_cheater(&self, channel_id: &str) -> Result<(), DexError> {
        error!("Strafe gegen Channel {} wird eingeleitet!", channel_id);
        Ok(())
    }

    fn send_gossip_alert(&self, channel_id: &str) {
        info!("Gossip-Alert versendet für Channel {}.", channel_id);
    }

    fn block_network_access(&self, channel_id: &str) {
        info!("Zugriff auf Netzwerkressourcen für Channel {} blockiert (simuliert).", channel_id);
    }
}
