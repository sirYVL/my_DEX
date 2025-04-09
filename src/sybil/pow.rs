///////////////////////////////////////
/// my_DEX/src/sybil/pow.rs
///////////////////////////////////////

use super::*;
use sha2::{Sha256, Digest};

pub fn validate_pow(node_id: &str, difficulty: usize) -> bool {
    let mut hasher = Sha256::new();
    hasher.update(node_id.as_bytes());
    let result = hasher.finalize();
    let mut count = 0;
    for byte in result.iter() {
        if *byte == 0 {
            count += 8;
        } else {
            count += byte.leading_zeros() as usize;
            break;
        }
    }
    count >= difficulty
}
