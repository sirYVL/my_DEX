////////////////////////////////////////// 
// my_DEX/src/voting/fullnode_vote.rs
//////////////////////////////////////////

//
// NEU (Sicherheitsupdate):
//  1) cast_vote(...) identifiziert Wähler nur anhand eines String "voter" -> 
//     kein Auth/Signierung => Angreifer kann sich als "NodeA" ausgeben.
//  2) total_voters, threshold ohne Prüfung => threshold könnte > 1, 
//     oder total_voters != Summe d. realen Nodes => falsches Quorum.
//  3) concurrency => Voters sind in HashSet, kein Mutex => 
//     bei parallelem Zugriff drohen Race Conditions.
//  4) is_approved => Ja/Nein => 
//     kein Logging/Audit => Angreifer kann unbemerkt "spätes" Hinzufügen / 
//     Ggf. Krypto-Signatur?
//

use std::collections::HashSet;
use chrono::Utc;

#[derive(Debug)]
pub struct FullnodeVote {
    pub proposal: String,
    pub threshold: f64, // z.B. 0.7 für 70%
    pub total_voters: usize,
    pub votes_yes: usize,
    pub votes_no: usize,
    // Speichert die IDs der Fullnode-Betreiber, die bereits abgestimmt haben.
    pub voters: HashSet<String>,
}

impl FullnodeVote {
    pub fn new(proposal: String, threshold: f64, total_voters: usize) -> Self {
        FullnodeVote {
            proposal,
            threshold,
            total_voters,
            votes_yes: 0,
            votes_no: 0,
            voters: HashSet::new(),
        }
    }

    pub fn cast_vote(&mut self, voter: String, vote_yes: bool) -> Result<(), String> {
        if self.voters.contains(&voter) {
            return Err("Fullnode: Dieser Node hat bereits abgestimmt.".to_string());
        }
        self.voters.insert(voter);
        if vote_yes {
            self.votes_yes += 1;
        } else {
            self.votes_no += 1;
        }
        Ok(())
    }

    pub fn is_approved(&self) -> bool {
        if self.total_voters == 0 {
            return false;
        }
        let yes_ratio = self.votes_yes as f64 / self.total_voters as f64;
        yes_ratio >= self.threshold
    }
}
