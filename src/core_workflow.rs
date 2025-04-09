///////////////////////////////////////////////////////////
// my_dex/src/core_workflow.rs
///////////////////////////////////////////////////////////
//
// Dieses Modul integriert die zentralen Kerndienste Ihres DEX-Systems:
// - Matching Engine (f�r Order Matching und Trade-Processing)
// - Konsens-Engine (um Bl�cke final zu validieren und in die Kette aufzunehmen)
// - Settlement-Engine (f�r den sicheren Abschluss von Settlements)
// 
// Alle diese Komponenten werden mittels standardisierter Sicherheits-Schnittstellen
// abgesichert, indem jeweils ein zus�tzlicher Sicherheitslayer (�ber AdvancedSecurityValidator)
// als Decorator eingebunden wird.
// So wird sichergestellt, dass kritische Operationen (Order Matching, Settlement, Konsens)
// produktionsreif abgesichert und validiert werden.
///////////////////////////////////////////////////////////

use anyhow::Result;
use std::sync::{Arc, Mutex};
use crate::error::DexError;

// Importieren der Kernmodule:
use crate::matching_engine::MatchingEngine;
use crate::consensus::advanced_consensus::{AdvancedConsensusEngine, ConsensusEngine, Block};
use crate::consensus::secured_consensus::SecuredConsensusEngine;
use crate::settlement::secured_settlement::{SettlementEngineTrait, SettlementEngine, SecuredSettlementEngine};
use crate::security::security_validator::{SecurityValidator, AdvancedSecurityValidator};

/// CoreWorkflow integriert Matching, Konsens und Settlement in einem zentralen Ablauf.
/// Alle kritischen Operationen werden dabei zus�tzlich durch den Sicherheitslayer abgesichert.
pub struct CoreWorkflow {
    pub matching_engine: MatchingEngine,
    pub consensus_engine: Box<dyn ConsensusEngine>,
    pub settlement_engine: Box<dyn SettlementEngineTrait>,
}

impl CoreWorkflow {
    /// Erzeugt eine neue Instanz des CoreWorkflow, wobei alle Komponenten
    /// � inklusive der Sicherheitsdecorators � initialisiert werden.
    pub fn new() -> Self {
        // Initialize MatchingEngine (inklusive eigener Sicherheitsvalidierung, falls integriert)
        let matching_engine = MatchingEngine::new();

        // Erstellen der Basis-Konsens-Engine
        let base_consensus = AdvancedConsensusEngine::new();
        // Umschlie�en mit einem Sicherheitslayer (SecuredConsensusEngine)
        let secured_consensus: SecuredConsensusEngine<AdvancedConsensusEngine, AdvancedSecurityValidator> =
            SecuredConsensusEngine::new(base_consensus, AdvancedSecurityValidator::new());

        // Erstellen der Basis-Settlement-Engine
        let base_settlement = SettlementEngine::new();
        // Umschlie�en mit einem Sicherheitslayer (SecuredSettlementEngine)
        let secured_settlement: SecuredSettlementEngine<SettlementEngine, AdvancedSecurityValidator> =
            SecuredSettlementEngine::new(base_settlement, AdvancedSecurityValidator::new());

        Self {
            matching_engine,
            consensus_engine: Box::new(secured_consensus),
            settlement_engine: Box::new(secured_settlement),
        }
    }

    /// F�hrt einen kompletten Core-Workflow durch:
    /// 1. Verarbeitet Trades �ber die Matching Engine.
    /// 2. Finalisiert einen Block im Konsens-Prozess.
    /// 3. (Optional) Zus�tzliche Settlement-Prozesse k�nnen hier erg�nzt werden.
    pub fn process_core_workflow(&mut self) -> Result<(), DexError> {
        // Schritt 1: Trade Processing
        self.matching_engine.process_trades()?;
        
        // Schritt 2: Konsens � Neuen Block finalisieren
        let block = Block {
            round: self.consensus_engine.current_round() + 1,
            data: "Konsensblock basierend auf Trade-Daten".to_string(),
            proposer_id: 1, // Hier w�rde in einer echten Umgebung die ID des gew�hlten Proposers stehen
            block_hash: 12345, // Beispielhafter Hash, in der Realit�t wird er aus den Blockdaten berechnet
        };
        self.consensus_engine.finalize_block(block)?;
        
        // Schritt 3: Settlement-Prozess (wird in MatchingEngine intern genutzt, 
        // hier k�nnten weitere Settlement-Operationen erg�nzt werden)
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_core_workflow() {
        let mut workflow = CoreWorkflow::new();
        let result = workflow.process_core_workflow();
        assert!(result.is_ok());
    }
}
