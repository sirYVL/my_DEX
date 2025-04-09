////////////////////////////////////////////////////
/// my_DEX/src/protocol/message.rs
/////////////////////////////////////////////////////

use super::*;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub enum P2PMessage {
    Ping { seq: u64, timestamp: u64 },
    Pong { seq: u64, timestamp: u64 },
    FindNode { target: String, seq: u64, timestamp: u64 },
    Custom { data: String, seq: u64, timestamp: u64 },
}

pub fn serialize_message(msg: &P2PMessage) -> Vec<u8> {
    bincode::serialize(msg).unwrap_or_default()
}

pub fn deserialize_message(data: &[u8]) -> Option<P2PMessage> {
    bincode::deserialize(data).ok()
}
