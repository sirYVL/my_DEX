// my_dex/src/watchtower.rs
//
// Watchtower-Mechanismus: Falls Payment-Channels / LN-ähnliche Mechanik.
// Betrugsnachweis => Alte Channel-State-Publizierung => WT kann "Strafe" broadcasten.

use crate::error::DexError;
use std::collections::HashMap;
use tracing::{info, warn, instrument};

#[derive(Clone, Debug)]
pub struct Watchtower {
    // mapping: ChannelId -> latest revocation secret or commitment
    channel_states: HashMap<String, WatchtowerState>,
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
        }
    }

    #[instrument(name="wt_register_channel", skip(self, commit_tx))]
    pub fn register_channel(
        &mut self,
        channel_id: &str,
        commit_tx: Vec<u8>,
        rev_hash: [u8; 32]
    ) -> Result<(), DexError> {
        // Falls channel schon exist => update 
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
        published_commit: &[u8]
    ) -> Result<bool, DexError> {
        // Falls unknown => not our business
        let existing = self.channel_states.get(channel_id)
            .ok_or(DexError::Other(format!("Unknown channel {}", channel_id)))?;

        if published_commit != existing.latest_commitment_tx {
            warn!("Potential Betrugsversuch in channel_id={}", channel_id);
            return Ok(true);
        }
        Ok(false)
    }

    // Im Betrugsfall => Strafe
    // ...
}
