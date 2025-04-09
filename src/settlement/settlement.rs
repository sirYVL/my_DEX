/////////////////////////////////////////////////////////////// 
// my_dex/src/settlement/settlement.rs
///////////////////////////////////////////////////////////////

use anyhow::Result;
use async_trait::async_trait;
use sha2::{Sha256, Digest};
use tracing::{info, error};
use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::Mutex; // Neu
use lazy_static::lazy_static; // Neu, um einen globalen Mutex zu definieren

/// GLOBALER Mutex, um commit-Operationen zu serialisieren
lazy_static! {
    static ref COMMIT_MUTEX: Mutex<()> = Mutex::new(());
}

/// Enum zur Kennzeichnung des Settlement-Typs.
#[derive(Debug, Clone)]
pub enum SettlementType {
    /// Standardsettlement ohne zusätzliche Anforderungen.
    Standard,
    /// Atomic Swap: Zwei Parteien tauschen Assets atomar.
    AtomicSwap {
        counterparty: String,
        asset_from: String,
        asset_to: String,
        amount_from: f64,
        amount_to: f64,
    },
    /// On-Chain HTLC: Hashed Time-Locked Contract zur Absicherung.
    HTLC {
        hash_lock: String,  // Erwarteter SHA256-Hash des Preimages
        timeout: u64,       // Timeout in Sekunden (angenommen als Unix-Timestamp)
    },
}

/// Struktur, die einen Settlement-Vorschlag repräsentiert.
#[derive(Debug, Clone)]
pub struct SettlementProposal {
    pub id: String,
    pub settlement_type: SettlementType,
    pub data: String,           // Zusätzliche, transaktionsspezifische Daten
    pub preimage: Option<String>, // Für HTLCs: Der Preimage, der den Hash-Lock freischaltet
}

impl SettlementProposal {
    pub fn new_standard(id: &str, data: &str) -> Self {
        Self {
            id: id.to_string(),
            settlement_type: SettlementType::Standard,
            data: data.to_string(),
            preimage: None,
        }
    }

    pub fn new_atomic_swap(
        id: &str,
        counterparty: &str,
        asset_from: &str,
        asset_to: &str,
        amount_from: f64,
        amount_to: f64,
        data: &str,
    ) -> Self {
        Self {
            id: id.to_string(),
            settlement_type: SettlementType::AtomicSwap {
                counterparty: counterparty.to_string(),
                asset_from: asset_from.to_string(),
                asset_to: asset_to.to_string(),
                amount_from,
                amount_to,
            },
            data: data.to_string(),
            preimage: None,
        }
    }

    pub fn new_htlc(
        id: &str,
        hash_lock: &str,
        timeout: u64,
        data: &str,
        preimage: Option<&str>,
    ) -> Self {
        Self {
            id: id.to_string(),
            settlement_type: SettlementType::HTLC {
                hash_lock: hash_lock.to_string(),
                timeout,
            },
            data: data.to_string(),
            preimage: preimage.map(|s| s.to_string()),
        }
    }
}

/// Trait, der den Settlement-Workflow definiert.
#[async_trait]
pub trait SettlementEngine: Send + Sync {
    async fn propose_settlement(&self, proposal: SettlementProposal) -> Result<String>;
    async fn validate_settlement(&self, proposal: &SettlementProposal) -> Result<bool>;
    async fn commit_settlement(&self, proposal: &SettlementProposal) -> Result<()>;
}

/// Basis-Implementierung des Settlement-Workflows.
pub struct BaseSettlementEngine;

#[async_trait]
impl SettlementEngine for BaseSettlementEngine {
    async fn propose_settlement(&self, proposal: SettlementProposal) -> Result<String> {
        info!("Proposing settlement with id: {}", proposal.id);
        Ok(proposal.id.clone())
    }

    async fn validate_settlement(&self, proposal: &SettlementProposal) -> Result<bool> {
        match &proposal.settlement_type {
            SettlementType::Standard => {
                info!("Validating standard settlement: {}", proposal.id);
                if proposal.data.trim().is_empty() {
                    error!("Standard settlement data must not be empty for id: {}", proposal.id);
                    return Err(anyhow::anyhow!("Settlement data is empty"));
                }
                Ok(true)
            }
            SettlementType::AtomicSwap { counterparty, asset_from, asset_to, amount_from, amount_to } => {
                info!("Validating atomic swap settlement: {} with counterparty {}", proposal.id, counterparty);
                if counterparty.trim().is_empty() || asset_from.trim().is_empty() || asset_to.trim().is_empty() {
                    return Err(anyhow::anyhow!("Atomic swap parameters must be non-empty"));
                }
                if *amount_from <= 0.0 || *amount_to <= 0.0 {
                    return Err(anyhow::anyhow!("Atomic swap amounts must be positive"));
                }
                info!("Atomic swap settlement {} validated successfully", proposal.id);
                Ok(true)
            }
            SettlementType::HTLC { hash_lock, timeout } => {
                info!("Validating HTLC settlement: {}", proposal.id);
                let preimage = proposal.preimage.as_ref()
                    .ok_or_else(|| anyhow::anyhow!("HTLC settlement requires a preimage for id: {}", proposal.id))?;
                let computed_hash = compute_sha256(preimage);
                if computed_hash != *hash_lock {
                    return Err(anyhow::anyhow!("HTLC validation failed: preimage hash mismatch for id: {}", proposal.id));
                }

                // NEU: Timeout-Check => falls "timeout" abgelaufen => invalid
                let now_sec = SystemTime::now().duration_since(UNIX_EPOCH)
                    .map_err(|e| anyhow::anyhow!("Time error: {:?}", e))?
                    .as_secs();
                if now_sec >= *timeout {
                    return Err(anyhow::anyhow!("HTLC settlement => Timeout abgelaufen (id={})", proposal.id));
                }

                if *timeout == 0 {
                    return Err(anyhow::anyhow!("HTLC timeout must be > 0 for id: {}", proposal.id));
                }
                info!("HTLC settlement {} validated successfully", proposal.id);
                Ok(true)
            }
        }
    }

    async fn commit_settlement(&self, proposal: &SettlementProposal) -> Result<()> {
        // NEU: Globaler Mutex => keine parallele Commit-Operation
        let _commit_guard = COMMIT_MUTEX.lock().unwrap();

        info!("Committing settlement: {}", proposal.id);
        // In einer produktionsreifen Umgebung => On-Chain Commit
        Ok(())
    }
}

/// Hilfsfunktion: SHA256-Hash als Hex
pub fn compute_sha256(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let hash_result = hasher.finalize();
    hex::encode(hash_result)
}

/// SecuredSettlementEngine => Decorator
pub struct SecuredSettlementEngine<E: SettlementEngine> {
    inner: E,
}

impl<E: SettlementEngine> SecuredSettlementEngine<E> {
    pub fn new(inner: E) -> Self {
        Self { inner }
    }

    async fn perform_additional_security_checks(&self, proposal: &SettlementProposal) -> Result<()> {
        info!("Performing additional security checks for settlement: {}", proposal.id);
        // z. B. Signaturen / Multi-Sig / ...
        Ok(())
    }

    async fn audit_settlement(&self, proposal: &SettlementProposal) -> Result<()> {
        info!("Auditing settlement: {}", proposal.id);
        // z. B. Externes Audit
        Ok(())
    }
}

#[async_trait]
impl<E: SettlementEngine> SettlementEngine for SecuredSettlementEngine<E> {
    async fn propose_settlement(&self, proposal: SettlementProposal) -> Result<String> {
        self.inner.propose_settlement(proposal).await
    }

    async fn validate_settlement(&self, proposal: &SettlementProposal) -> Result<bool> {
        self.perform_additional_security_checks(proposal).await?;
        let valid = self.inner.validate_settlement(proposal).await?;
        if !valid {
            error!("Settlement validation failed for id: {}", proposal.id);
            return Err(anyhow::anyhow!("Settlement validation failed"));
        }
        Ok(valid)
    }

    async fn commit_settlement(&self, proposal: &SettlementProposal) -> Result<()> {
        self.perform_additional_security_checks(proposal).await?;
        self.inner.commit_settlement(proposal).await?;
        self.audit_settlement(proposal).await?;
        Ok(())
    }
}
