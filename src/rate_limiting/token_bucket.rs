////////////////////////////////////////////////////
/// my_DEX/src/rate_limiting/token_bucket.rs
//////////////////////////////////////////////////// 

pub mod message;

pub mod token_bucket;

use std::time::Instant;

pub struct TokenBucket {
    capacity: u64,
    tokens: u64,
    refill_rate: u64,
    last_refill: Instant,
}

impl TokenBucket {
    pub fn new(capacity: u64, refill_rate: u64) -> Self {
        TokenBucket {
            capacity,
            tokens: capacity,
            refill_rate,
            last_refill: Instant::now(),
        }
    }
}
