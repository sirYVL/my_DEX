/////////////////////////////////////////////////////////
// my_dex/src/consensus/vrf_committee_async.rs
/////////////////////////////////////////////////////////

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rand::Rng;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tracing::{info, debug, warn, error};

// --- Fiktive VRF-Funktionen (curve25519-dalek-VRF) DEMO ---
#[derive(Clone)]
pub struct VrfKeypair {
    pub sk: [u8; 64],
    pub pk: [u8; 32],
}
#[derive(Clone)]
pub struct VrfProof {
    pub bytes: Vec<u8>,
}

pub fn generate_keypair() -> VrfKeypair {
    let mut rng = rand::thread_rng();
    let mut sk = [0u8; 64];
    rng.fill(&mut sk);
    let mut pk = [0u8; 32];
    rng.fill(&mut pk);
    VrfKeypair { sk, pk }
}

/// Scheinfunktion: VRF-Sign => (value, proof)
pub fn vrf_sign(kp: &VrfKeypair, msg: &[u8]) -> (u64, VrfProof) {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    kp.sk.hash(&mut hasher);
    msg.hash(&mut hasher);
    let val = hasher.finish() % 1_000_000;
    let mut proofbytes = Vec::new();
    proofbytes.extend_from_slice(&kp.pk);
    proofbytes.extend_from_slice(&val.to_le_bytes());
    (val, VrfProof { bytes: proofbytes })
}

/// VRF-Verify => prüft pseudo
pub fn vrf_verify(pk: &[u8], msg: &[u8], val: u64, proof: &VrfProof) -> bool {
    if proof.bytes.len() < 40 {
        return false;
    }
    let stored_pk = &proof.bytes[0..32];
    let stored_val = &proof.bytes[32..40];
    if stored_pk != pk { 
        return false; 
    }
    let val_proof = u64::from_le_bytes(stored_val.try_into().unwrap());
    // Wir ignorieren msg in diesem Mock
    val_proof == val
}
// --- ENDE VRF-Stub ---

/////////////////////////////////////////////////////////
// Block-Struktur => wir keepen: 
// - round
// - block_data
// - state_root (optional, hier ein Dummy)
/////////////////////////////////////////////////////////

#[derive(Clone, Debug)]
pub struct Block {
    pub round: u64,
    pub block_data: String,
    pub proposer_id: u64,
    pub state_root: String,  // optional: z. B. "hash" 
}

/// Repräsentiert den finalisierten Blockchain-Zustand
#[derive(Default, Debug)]
pub struct FinalState {
    pub chain: Vec<Block>,
}

impl FinalState {
    /// Fügen wir den finalisierten Block ans Ende
    pub fn append_block(&mut self, blk: Block) {
        self.chain.push(blk);
    }

    /// Liefert den letzten finalisierten Block (falls existiert)
    pub fn last_block(&self) -> Option<&Block> {
        self.chain.last()
    }
}

/////////////////////////////////////////////////////////
// Node => VRF + Stake
/////////////////////////////////////////////////////////
#[derive(Clone)]
pub struct Node {
    pub node_id: u64,
    pub stake: u64,
    pub vrf_keypair: VrfKeypair,
}

impl Node {
    pub fn new(node_id: u64, stake: u64) -> Self {
        Node {
            node_id,
            stake,
            vrf_keypair: generate_keypair(),
        }
    }
}

/////////////////////////////////////////////////////////
// P2P-Message => Komitee
/////////////////////////////////////////////////////////
#[derive(Debug, Clone)]
pub enum CommitteeP2PMessage {
    Proposal {
        round: u64,
        proposer_id: u64,
        block_data: String,
        vrf_value: u64,
        vrf_proof: VrfProof,
        seed: u64,
    },
    Vote {
        round: u64,
        voter_id: u64,
    },
}

/// Trait => in p2p.rs implementieren. 
/// So ersetzen wir Channels durch echte Netzwerkkommunikation.
pub trait VRFCommitteeNetwork: Send + Sync {
    /// Broadcast => an alle relevant
    fn broadcast_message(&self, msg: &CommitteeP2PMessage);
    /// Direkt an Node
    fn send_message(&self, node_id: u64, msg: &CommitteeP2PMessage);
    /// Empfängt nächste Komitee-Nachricht (polling)
    fn recv_message(&self) -> Option<CommitteeP2PMessage>;
}

/////////////////////////////////////////////////////////
// VRF-/Komitee-basiertes, asynchrones Konsens-System
// => wir keepen finalisierte Blocks in final_state.chain
/////////////////////////////////////////////////////////
pub struct AsyncVRFCommitteeConsensus {
    pub nodes: Vec<Node>,
    pub total_stake: u64,

    /// final_state => wir keepen finalisierte Blöcke
    pub final_state: Arc<Mutex<FinalState>>,

    pub current_round: u64,

    pub committee_size: usize,
    pub threshold: usize,

    pub round_delay: Duration,

    pub network: Arc<Mutex<dyn VRFCommitteeNetwork>>,
    pub consensus_task: Option<JoinHandle<()>>,
}

// Intern: wir tracken Votes pro Round
static mut VOTE_MAP: Option<HashMap<u64, HashSet<u64>>> = None;

impl AsyncVRFCommitteeConsensus {
    pub fn new(
        nodes: Vec<Node>,
        net: Arc<Mutex<dyn VRFCommitteeNetwork>>,
        committee_size: usize,
        threshold: usize,
    ) -> Self {
        let total = nodes.iter().map(|n| n.stake).sum();
        let st = Arc::new(Mutex::new(FinalState::default()));
        AsyncVRFCommitteeConsensus {
            nodes,
            total_stake: total,
            final_state: st,
            current_round: 0,
            committee_size,
            threshold,
            round_delay: Duration::from_millis(1000),
            network: net,
            consensus_task: None,
        }
    }

    /// Startet => wir spawnen run_loop
    pub fn start(&mut self) {
        let netc = self.network.clone();
        let stc = self.final_state.clone();
        let mut me = self.clone();
        self.consensus_task = Some(tokio::spawn(async move {
            me.run_loop(netc, stc).await;
        }));
    }

    pub async fn stop(&mut self) {
        if let Some(h) = self.consensus_task.take() {
            h.abort();
        }
    }

    async fn run_loop(
        &mut self,
        netc: Arc<Mutex<dyn VRFCommitteeNetwork>>,
        stc: Arc<Mutex<FinalState>>
    ) {
        info!("Async VRF+Committee => start main loop, #nodes={}", self.nodes.len());

        // Sekundäre Task => eingehende Nachrichten
        let mut handle_msgs = {
            let net2 = netc.clone();
            let st2 = stc.clone();
            let mut me2 = self.clone();
            tokio::spawn(async move {
                me2.handle_incoming_loop(net2, st2).await;
            })
        };

        loop {
            self.current_round += 1;
            let seed = self.compute_seed(self.current_round);
            debug!("Round {} => seed={}", self.current_round, seed);

            // 1) Wähle Proposer
            let (proposer, val, proof) = self.select_proposer(seed);
            let block_data = format!("BlockData(r={})", self.current_round);

            let msg = CommitteeP2PMessage::Proposal {
                round: self.current_round,
                proposer_id: proposer.node_id,
                block_data,
                vrf_value: val,
                vrf_proof: proof,
                seed,
            };
            netc.lock().unwrap().broadcast_message(&msg);

            // 2) Komitee => wähle N Knoten
            let comm = self.select_committee(seed, self.committee_size, &proposer);
            debug!("Round {} => committee = {:?}", self.current_round, comm);

            // 3) asynchron => votes
            for voter_id in comm {
                let netclone = netc.clone();
                let r = self.current_round;
                tokio::spawn(async move {
                    let delay = rand::thread_rng().gen_range(300..700);
                    sleep(Duration::from_millis(delay)).await;
                    let vote_msg = CommitteeP2PMessage::Vote {
                        round: r,
                        voter_id,
                    };
                    netclone.lock().unwrap().broadcast_message(&vote_msg);
                });
            }

            // Warten => Nächste Round
            sleep(self.round_delay).await;
            if self.current_round >= 10 {
                info!("Reached 10 rounds => stopping");
                break;
            }
        }
        handle_msgs.abort();
        info!("Async VRF+Committee => end run_loop");
    }

    async fn handle_incoming_loop(
        &mut self,
        net: Arc<Mutex<dyn VRFCommitteeNetwork>>,
        st: Arc<Mutex<FinalState>>,
    ) {
        loop {
            // poll
            sleep(Duration::from_millis(200)).await;
            let msg_opt = net.lock().unwrap().recv_message();
            if msg_opt.is_none() {
                continue;
            }
            let msg = msg_opt.unwrap();
            match msg {
                CommitteeP2PMessage::Proposal { 
                    round, proposer_id, block_data, vrf_value, vrf_proof, seed 
                } => {
                    debug!("handle_incoming => PROPOSAL, r={}, from={}", round, proposer_id);
                    let nopt = self.nodes.iter().find(|x| x.node_id == proposer_id);
                    if let Some(node) = nopt {
                        // VRF check
                        let test_msg = format!("seed={}#round={}", seed, round);
                        let ok = vrf_verify(&node.vrf_keypair.pk, test_msg.as_bytes(), vrf_value, &vrf_proof);
                        if !ok {
                            warn!("Proposal => VRF invalid => ignore");
                            continue;
                        }
                        debug!("Proposal => VRF ok => store ephemeral => round={}", round);
                        // In echtem System => wir würden block_data im mempool-lager cachen
                    } else {
                        warn!("Unknown proposer, id={}", proposer_id);
                    }
                }
                CommitteeP2PMessage::Vote { round, voter_id } => {
                    debug!("handle_incoming => VOTE => round={}, from={}", round, voter_id);
                    let count = self.register_vote(round, voter_id);
                    if count >= self.threshold {
                        // => finalize block
                        let block = Block {
                            round,
                            proposer_id: 999, // dummy, 
                            block_data: format!("FinalBlock(r={})", round),
                            state_root: format!("StateRoot({})", round),
                        };
                        let mut stlock = st.lock().unwrap();
                        stlock.append_block(block.clone());
                        info!("Round {} => final => appended block => chain.len={}", round, stlock.chain.len());
                    }
                }
            }
        }
    }

    fn register_vote(&mut self, round: u64, voter_id: u64) -> usize {
        unsafe {
            if VOTE_MAP.is_none() {
                VOTE_MAP = Some(HashMap::new());
            }
            let vm = VOTE_MAP.as_mut().unwrap();
            let set = vm.entry(round).or_insert_with(HashSet::new);
            set.insert(voter_id);
            set.len()
        }
    }

    fn compute_seed(&self, round: u64) -> u64 {
        let mut rng = rand::thread_rng();
        let x = rng.gen_range(0..1_000_000_000);
        round.wrapping_mul(x)
    }

    fn select_proposer(&self, seed: u64) -> (Node, u64, VrfProof) {
        let mut best: Option<(u64, &Node, VrfProof)> = None;
        for nd in &self.nodes {
            let msg = format!("seed={}#round={}", seed, 0);
            let (val, proof) = vrf_sign(&nd.vrf_keypair, msg.as_bytes());
            let wval = val / (nd.stake + 1);
            if let Some((bv, _bn, _bp)) = best {
                if wval < bv {
                    best = Some((wval, nd, proof));
                }
            } else {
                best = Some((wval, nd, proof));
            }
        }
        let (val, node, pr) = best.unwrap();
        (node.clone(), val, pr)
    }

    fn select_committee(&self, seed: u64, size: usize, skip_node: &Node) -> Vec<u64> {
        let mut scored = Vec::new();
        for nd in &self.nodes {
            if nd.node_id == skip_node.node_id {
                continue;
            }
            let msg = format!("committee#seed={}", seed);
            let (val, _pf) = vrf_sign(&nd.vrf_keypair, msg.as_bytes());
            let wval = val / (nd.stake + 1);
            scored.push((wval, nd.node_id));
        }
        scored.sort_by_key(|(wv, _)| *wv);
        scored.truncate(size);
        scored.into_iter().map(|(_, nid)| nid).collect()
    }
}

// Demo-Funktion
#[allow(dead_code)]
pub async fn demo_vrf_comm_async_p2p() {
    // z. B. 8 Nodes
    let mut nodes = Vec::new();
    for i in 0..8 {
        let stake = rand::thread_rng().gen_range(1..10);
        nodes.push(Node::new(i, stake));
    }
    // p2p => wir nehmen Mock
    let p2p_mock = Arc::new(Mutex::new(MockCommitteeNetwork::new()));

    let mut cons = AsyncVRFCommitteeConsensus::new(nodes, p2p_mock.clone(), 3, 2);
    cons.start();

    // Warten 15s
    sleep(Duration::from_secs(15)).await;
    cons.stop().await;

    // Gucken wir => chain
    let locked_st = cons.final_state.lock().unwrap();
    info!("Final Chain => #blocks={}, last={:?}", locked_st.chain.len(), locked_st.last_block());
}

/////////////////////////////////////////////////////////
// Mock-Implementierung => In real implement in p2p.rs
/////////////////////////////////////////////////////////

#[derive(Clone)]
pub struct MockCommitteeNetwork {
    pub inbox: Vec<CommitteeP2PMessage>,
}

impl MockCommitteeNetwork {
    pub fn new() -> Self {
        Self {
            inbox: Vec::new(),
        }
    }
}

impl VRFCommitteeNetwork for MockCommitteeNetwork {
    fn broadcast_message(&self, msg: &CommitteeP2PMessage) {
        let mut me = self.clone();
        me.inbox.push(msg.clone());
    }
    fn send_message(&self, _node_id: u64, msg: &CommitteeP2PMessage) {
        let mut me = self.clone();
        me.inbox.push(msg.clone());
    }
    fn recv_message(&self) -> Option<CommitteeP2PMessage> {
        let mut me = self.clone();
        if me.inbox.is_empty() {
            None
        } else {
            Some(me.inbox.remove(0))
        }
    }
}
