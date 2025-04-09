///////////////////////////////////////////////////////
/// my_DEX/src/kademlia/kademlia.rs
///////////////////////////////////////////////////////

use super::*;
use num_bigint::BigUint;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::net::SocketAddr;

pub const ID_LENGTH: usize = 32;

#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct NodeId(pub [u8; ID_LENGTH]);

impl NodeId {
    pub fn random() -> Self {
        let mut rng = rand::thread_rng();
        let mut id = [0u8; ID_LENGTH];
        rng.fill(&mut id);
        NodeId(id)
    }

    pub fn xor(&self, other: &NodeId) -> NodeId {
        let mut result = [0u8; ID_LENGTH];
        for i in 0..ID_LENGTH {
            result[i] = self.0[i] ^ other.0[i];
        }
        NodeId(result)
    }

    pub fn distance(&self, other: &NodeId) -> BigUint {
        BigUint::from_bytes_be(&self.xor(other).0)
    }
}
