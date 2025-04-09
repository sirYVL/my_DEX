////////////////////////////////////////    
// my_dex/src/consensus/vrf.rs
////////////////////////////////////////        

use rand::Rng;
use sha2::{Sha256, Digest};

pub struct VRFValidatorSelection {
    pub seed: u64, // Zuf�llige Startzahl f�r VRF
    pub participants: Vec<String>, // Liste der m�glichen Validatoren
}

impl VRFValidatorSelection {
    pub fn new(participants: Vec<String>) -> Self {
        let seed = rand::thread_rng().gen::<u64>(); // Zuf�llige Startnummer
        Self { seed, participants }
    }

    pub fn select_validator(&self) -> String {
        let mut best_hash = [255u8; 32];
        let mut best_validator = "".to_string();

        for participant in &self.participants {
            let mut hasher = Sha256::new();
            hasher.update(self.seed.to_le_bytes());
            hasher.update(participant.as_bytes());
            let hash_result = hasher.finalize();

            if hash_result < best_hash {
                best_hash = hash_result.into();
                best_validator = participant.clone();
            }
        }

        best_validator
    }
}
