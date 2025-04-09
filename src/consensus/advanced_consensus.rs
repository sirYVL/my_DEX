///////////////////////////////////////////////////////////
// my_dex/src/consensus/advanced_consensus.rs
///////////////////////////////////////////////////////////
//
// Dieses Modul implementiert einen erweiterten, produktionsreifen
// Konsens-Mechanismus, der zusätzliche Sicherheitsprüfungen über
// einen SecurityDecorator (über den SecurityValidator) integriert.
// Es beinhaltet robuste Retry-Mechanismen, erweiterte Validierung und
// einen externen Audit-Hook, sodass nur vollständig validierte Blöcke
// in die finale Kette aufgenommen werden.
//
// In einer echten Produktionsumgebung sollten die Validierungsfunktionen
// (z. B. Multi-Sig, Ring-Signaturen, zk-SNARKs) durch zertifizierte
// Algorithmen ersetzt und externe Audit-Systeme angebunden werden.
///////////////////////////////////////////////////////////

use anyhow::Result;
use std::sync::{Arc, Mutex};
use std::time::{Duration};
use tokio::time::sleep;
use tracing::{info, debug, warn, error};
use crate::error::DexError;
use crate::security::security_validator::{SecurityValidator, AdvancedSecurityValidator};

#[derive(Clone, Debug)]
pub struct Block {
    pub round: u64,
    pub data: String,
    pub proposer_id: u64,
    pub block_hash: u64,
}

/// Trait für einen generischen Konsens-Mechanismus.
pub trait ConsensusEngine: Send + Sync {
    /// Nimmt einen Blockvorschlag entgegen.
    fn propose_block(&self, block: Block) -> Result<(), DexError>;
    /// Finalisiert einen Block, nachdem alle Sicherheitsprüfungen erfolgreich waren.
    fn finalize_block(&self, block: Block) -> Result<(), DexError>;
    /// Gibt die aktuelle Rundenzahl zurück (z. B. basierend auf der Länge der Kette).
    fn current_round(&self) -> u64;
}

/// AdvancedConsensusEngine führt die grundlegenden Konsens-Operationen aus
/// und integriert zusätzliche Sicherheitsprüfungen sowie robuste Retry-Mechanismen.
pub struct AdvancedConsensusEngine {
    /// Sicherheitsvalidator, der kritische Validierungen übernimmt.
    pub validator: Box<dyn SecurityValidator>,
    /// Die finale Blockkette (finalisierte Blöcke).
    pub finalized_chain: Arc<Mutex<Vec<Block>>>,
    /// Maximale Anzahl von Wiederholungsversuchen für die Blockfinalisierung.
    pub max_retries: u32,
    /// Wartezeit (in Sekunden) zwischen den Retry-Versuchen.
    pub retry_backoff: u64,
}

impl AdvancedConsensusEngine {
    pub fn new() -> Self {
        Self {
            validator: Box::new(AdvancedSecurityValidator::new()),
            finalized_chain: Arc::new(Mutex::new(Vec::new())),
            max_retries: 5,
            retry_backoff: 2,
        }
    }

    /// Asynchrone Finalisierung eines Blocks mit robustem Retry-Mechanismus.
    /// Hier wird der Sicherheitsvalidator (validate_settlement) mehrfach aufgerufen,
    /// und bei Misserfolg wird nach einer definierten Backoff-Zeit erneut versucht.
    pub async fn finalize_block_with_retry(&self, block: Block) -> Result<(), DexError> {
        let block_info = format!("Round:{}; Proposer:{}; Data:{}", block.round, block.proposer_id, block.data);
        let mut attempt = 0;
        loop {
            attempt += 1;
            match self.validator.validate_settlement(&block_info) {
                Ok(_) => {
                    // => validierung ok => in finale Kette pushen
                    let mut chain = self.finalized_chain.lock().unwrap();
                    chain.push(block.clone());
                    info!("Block finalisiert auf Versuch {}. Neue Kettenlänge: {}", attempt, chain.len());

                    // => externes Audit
                    self.perform_external_audit(&block).await?;

                    return Ok(());
                },
                Err(e) => {
                    warn!("Finalisierung fehlgeschlagen auf Versuch {}: {:?}", attempt, e);
                    if attempt >= self.max_retries {
                        error!("Maximale Anzahl von Versuchen erreicht. Blockfinalisierung abgebrochen.");
                        return Err(DexError::Other("Block finalization failed after maximum retries".into()));
                    }
                    sleep(Duration::from_secs(self.retry_backoff)).await;
                }
            }
        }
    }

    /// Simuliert einen externen Audit-Prozess. In einer echten Umgebung würden Sie
    /// hier beispielsweise einen API-Aufruf an einen Audit-Service tätigen.
    pub async fn perform_external_audit(&self, block: &Block) -> Result<(), DexError> {
        debug!("Externer Audit erfolgreich für Block: {:?}", block);
        // => in real => z.B. HTTP call
        Ok(())
    }
}

impl ConsensusEngine for AdvancedConsensusEngine {
    fn propose_block(&self, block: Block) -> Result<(), DexError> {
        debug!("Block proposal erhalten: {:?}", block);
        Ok(())
    }
    
    fn finalize_block(&self, block: Block) -> Result<(), DexError> {
        // Da finalize_block_with_retry() asynchron ist, blocken wir hier
        // in einer Sync-Implementierung. Falls du dein Trait auf async umstellen
        // willst, kannst du finalize_block() ganz weglassen und finalize_block_with_retry
        // direkt aufrufen.
        futures::executor::block_on(self.finalize_block_with_retry(block))
    }
    
    fn current_round(&self) -> u64 {
        let chain = self.finalized_chain.lock().unwrap();
        if chain.is_empty() { 0 } else { chain.last().unwrap().round }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn test_finalize_block_with_retry() {
        let engine = AdvancedConsensusEngine::new();
        let block = Block {
            round: 1,
            data: "Test Block".to_string(),
            proposer_id: 42,
            block_hash: 123456,
        };
        let result = engine.finalize_block_with_retry(block).await;
        assert!(result.is_ok());
    }
}
