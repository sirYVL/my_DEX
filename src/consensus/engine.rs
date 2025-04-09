///////////////////////////////////////////////////////
// my_dex/src/consensus/engine.rs
/////////////////////////////////////////////////////// 

use super::{vrf::VRFValidatorSelection, pbft::PBFTNode, nakamoto::NakamotoBlock};
use std::collections::HashMap;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};

pub struct ConsensusEngine {
    pub validators: Vec<String>,
    pub current_validator: String,
    pub pbft_node: PBFTNode,
    pub blockchain: Vec<NakamotoBlock>,
    pub network_sender: mpsc::Sender<String>,
}

impl ConsensusEngine {
    pub fn new(peers: Vec<String>, network_sender: mpsc::Sender<String>) -> Self {
        let vrf = VRFValidatorSelection::new(peers.clone());
        let selected_validator = vrf.select_validator();
        println!("?? Neuer Validator: {}", selected_validator);

        Self {
            validators: peers.clone(),
            current_validator: selected_validator.clone(),
            pbft_node: PBFTNode::new(selected_validator.clone()),
            blockchain: vec![NakamotoBlock::new(0, "genesis".to_string(), vec![])],
            network_sender,
        }
    }

    pub async fn run(&mut self) {
        loop {
            println!("?? Konsens-Engine l�uft...");
            
            if self.current_validator == self.pbft_node.node_id {
                let new_block_hash = format!("block_{}", self.blockchain.len());
                println!("?? Erzeuge neuen Block: {}", new_block_hash);

                if self.pbft_node.handle_message(super::pbft::PBFTMessage::PrePrepare {
                    block_hash: new_block_hash.clone(),
                }) {
                    println!("? PBFT-Konsens erreicht f�r Block {}", new_block_hash);

                    let new_block = NakamotoBlock::new(
                        self.blockchain.len() as u64,
                        self.blockchain.last().unwrap().calculate_hash(),
                        vec![new_block_hash.clone()],
                    );
                    self.blockchain.push(new_block);

                    let _ = self.network_sender.send(format!("finalized:{}", new_block_hash)).await;
