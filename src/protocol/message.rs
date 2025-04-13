////////////////////////////////////////////////////
/// my_dex/src/protocol/message.rs
////////////////////////////////////////////////////

use serde::{Serialize, Deserialize};
use crate::dex_logic::advanced_crdt_sharding::CrdtShardSnapshot;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ShardSnapshotMessage {
    pub shard_id: u32,
    pub snapshot: CrdtShardSnapshot,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum P2PMessage {
    Ping { seq: u64, timestamp: u64 },
    Pong { seq: u64, timestamp: u64 },
    FindNode { target: String, seq: u64, timestamp: u64 },
    Custom { data: String, seq: u64, timestamp: u64 },

    // 🆕 Snapshot-Replikation (z. B. bei Self-Healing)
    ShardSnapshot(ShardSnapshotMessage),
}

pub fn serialize_message(msg: &P2PMessage) -> Vec<u8> {
    bincode::serialize(msg).unwrap_or_default()
}

pub fn deserialize_message(data: &[u8]) -> Option<P2PMessage> {
    bincode::deserialize(data).ok()
}
