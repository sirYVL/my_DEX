///////////////////////////////////////
// my_dex/src/consensus/pbft.rs
/////////////////////////////////////// 

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PBFTMessage {
    PrePrepare { block_hash: String },
    Prepare { block_hash: String },
    Commit { block_hash: String },
}

pub struct PBFTNode {
    pub node_id: String,
    pub state: HashMap<String, usize>, // Z�hlt Stimmen f�r einen Block
}

impl PBFTNode {
    pub fn new(node_id: String) -> Self {
        Self {
            node_id,
            state: HashMap::new(),
        }
    }

    pub fn handle_message(&mut self, msg: PBFTMessage) -> bool {
        match msg {
            PBFTMessage::PrePrepare { block_hash }
            | PBFTMessage::Prepare { block_hash }
            | PBFTMessage::Commit { block_hash } => {
                let counter = self.state.entry(block_hash.clone()).or_insert(0);
                *counter += 1;
                return *counter >= 2; // Beispiel: Konsens ab 2 Stimmen
            }
        }
        false
    }
}
