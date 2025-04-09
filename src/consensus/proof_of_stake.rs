////////////////////////////////////////    
// my_dex/src/consensus/proof_of_stake.rs
////////////////////////////////////////

use super::*;
use rand::distributions::{Distribution, WeightedIndex};

#[derive(Clone, Debug)]
pub struct Validator {
    pub id: String,
    pub stake: u64,
}

pub fn select_proposer(validators: &[Validator]) -> Option<Validator> {
    if validators.is_empty() {
        return None;
    }
    let stakes: Vec<u64> = validators.iter().map(|v| v.stake).collect();
    let dist = WeightedIndex::new(&stakes).ok()?;
    let mut rng = rand::thread_rng();
    let index = dist.sample(&mut rng);
    Some(validators[index].clone())
}
