///////////////////////////////////////////////////////////
// my_dex/src/consensus/secured_consensus.rs
///////////////////////////////////////////////////////////
//
// Dieses Modul implementiert einen Sicherheits-Decorator für den Konsens-Mechanismus.
// Der SecuredConsensusEngine-Decorator umschließt eine bestehende ConsensusEngine und
// führt vor dem finalen Abschluss eines Blocks zusätzliche Sicherheitsprüfungen
// durch, indem er ein standardisiertes Sicherheitsinterface (SecurityValidator)
// verwendet. So wird sichergestellt, dass nur Blöcke, die den
// erforderlichen Sicherheitsanforderungen entsprechen, finalisiert werden.
///////////////////////////////////////////////////////////

use anyhow::Result;
use crate::consensus::advanced_consensus::{Block, ConsensusEngine};
use crate::error::DexError;
use crate::security::security_validator::SecurityValidator;

/// SecuredConsensusEngine umschließt eine bestehende ConsensusEngine (inner) und
/// einen Sicherheitsvalidator. Vor der Finalisierung eines Blocks wird der Validator
/// aufgerufen.
pub struct SecuredConsensusEngine<E: ConsensusEngine, S: SecurityValidator> {
    pub inner: E,
    pub validator: S,
}

impl<E: ConsensusEngine, S: SecurityValidator> SecuredConsensusEngine<E, S> {
    pub fn new(inner: E, validator: S) -> Self {
        Self { inner, validator }
    }
}

impl<E: ConsensusEngine, S: SecurityValidator> ConsensusEngine for SecuredConsensusEngine<E, S> {
    fn propose_block(&self, block: Block) -> Result<(), DexError> {
        // Optional: Sicherheitsprüfungen beim Blockvorschlag können hier erfolgen.
        self.inner.propose_block(block)
    }
    
    fn finalize_block(&self, block: Block) -> Result<(), DexError> {
        // Erstelle eine Zusammenfassung der Blockinformationen.
        let block_info = format!("Round:{}; Proposer:{}; Data:{}", block.round, block.proposer_id, block.data);
        // Führe Sicherheitsvalidierung vor finalem Abschluss des Blocks durch.
        self.validator.validate_settlement(&block_info)?;
        // Wenn die Validierung erfolgreich ist, delegiere an die innere Konsens-Engine.
        self.inner.finalize_block(block)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consensus::advanced_consensus::{AdvancedConsensusEngine, ConsensusEngine, Block};
    use crate::security::security_validator::AdvancedSecurityValidator;
    use anyhow::Result;

    #[test]
    fn test_secured_consensus_finalize() -> Result<()> {
        let base_engine = AdvancedConsensusEngine::new();
        let validator = AdvancedSecurityValidator::new();
        let secured_engine = SecuredConsensusEngine::new(base_engine, validator);
        let block = Block {
            round: 1,
            data: "Test Block".to_string(),
            proposer_id: 42,
            block_hash: 12345,
        };
        secured_engine.finalize_block(block)?;
        Ok(())
    }
}
