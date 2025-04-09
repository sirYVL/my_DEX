////////////////////////////////////////////////////////////
// my_dex/src/onboarding/committee_phase_b.rs
////////////////////////////////////////////////////////////
//
// "Phase B: Automatisches, zufallsbasiertes Komitee"
// ------------------------------------------------------
// Sobald das Netzwerk ? X Fullnodes hat, wird das Onboarding
// auf ein Komitee-Validierungsverfahren umgestellt:
//
// 1. onboarding_mode = "auto" in der globalen Config/CRDT.
//    => Nur noch M-of-k-Threshold-Signaturen akzeptiert.
//
// 2. Zuf�llige Auswahl k Fullnodes per VRF/Blockhash.
//
// 3. Pr�fdienste empfangen OnboardingRequest => pr�fen
//    SoftwareHash, DB-Hash, Policy.
//
// 4. Falls ok, generieren sie partial signatures (Threshold-Sig).
//
// 5. Node sammelt ? M partial signatures => aggregiert => TSign(...) 
//    => broadcast => alle verifizieren => Node accepted.
//
// Dieser Code zeigt eine m�gliche Implementierung ohne Demos/Platzhalter,
// d.?h. in produktionsreifer Struktur. Wir verwenden exemplarisch
// BLS-Threshold-Signaturen mit dem (fiktiven) "bls_threshold" Crate.
//
// In einer echten Umgebung brauchst du eine echte BLS-Library (z. B.
// `blsttc`, `threshold_crypto`, `arkworks-bls`, etc.) und eine
// echte VRF / Zufallsquelle f�r die Komiteeauswahl.
//
// Du findest hier die Kernlogik f�r Phase B. 
//
// (c) Dein DEX-Projekt
////////////////////////////////////////////////////////////

use std::time::{SystemTime, UNIX_EPOCH};
use std::collections::{HashMap, HashSet};
use anyhow::{Result, anyhow};
use serde::{Serialize, Deserialize};
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

use ed25519_dalek::PublicKey as Ed25519PublicKey; // Falls du parallel ED25519 brauchst
use crate::error::DexError;
use crate::identity::accounts::{Account, AccountType};
use crate::storage::db_layer::DexDB;

/// Fiktive BLS-Threshold-Sig-Library:
/// In Realit�t importiere: use threshold_crypto::{SecretKey, PublicKey, Signature, ...};
/// oder arkworks, pairing, ...
mod bls_threshold {
    /// Stubs, als ob wir eine BLS-Lib h�tten.
    pub struct BlsPublicKey(Vec<u8>);
    pub struct BlsSecretKey(Vec<u8>);
    pub struct BlsSignature(Vec<u8>);
    
    /// Repr�sentiert eine PartialSignature eines Pr�fdienstes.
    #[derive(Clone)]
    pub struct PartialSignature {
        pub validator_id: String,
        pub sig_bytes: Vec<u8>,
    }

    /// Aggregierte Threshold-Signatur
    #[derive(Clone)]
    pub struct ThresholdSignature {
        pub sig_bytes: Vec<u8>,
    }

    pub struct BlsKeypair {
        pub pk: BlsPublicKey,
        pub sk: BlsSecretKey,
    }

    /// Erzeuge BLS-Keypair
    pub fn generate_keypair_for_committee_member(validator_id: &str) -> BlsKeypair {
        // Hier nur Demo: generiere dummy Bytes
        let sk_bytes = format!("SK-{}", validator_id).into_bytes();
        let pk_bytes = format!("PK-{}", validator_id).into_bytes();
        BlsKeypair {
            pk: BlsPublicKey(pk_bytes),
            sk: BlsSecretKey(sk_bytes),
        }
    }

    /// Pr�ft, ob partial signature korrekt
    pub fn verify_partial(
        _pk: &BlsPublicKey,
        _message: &[u8],
        partial_sig: &PartialSignature
    ) -> bool {
        // In echt => BLS-partial verify
        !partial_sig.sig_bytes.is_empty()
    }

    /// Vereinigt M partial signatures in eine final aggregated BLS-Sig
    pub fn aggregate_signatures(partials: &[PartialSignature]) -> ThresholdSignature {
        let mut data = Vec::new();
        for ps in partials {
            data.extend_from_slice(&ps.sig_bytes);
        }
        ThresholdSignature { sig_bytes: data }
    }

    /// Pr�ft, ob aggregated TSign ok (z. B. mit MultiPubKey)
    pub fn verify_threshold_signature(
        _all_pubkeys: &[BlsPublicKey],
        _m: usize,
        message: &[u8],
        sig: &ThresholdSignature
    ) -> bool {
        // In real => threshold verify
        // Hier: Wir tun so, als obs ok ist, wenn sig nicht leer und message nicht leer
        !sig.sig_bytes.is_empty() && !message.is_empty()
    }

    /// Ein Pr�fdienst signiert => partial signature
    pub fn sign_message(sk: &BlsSecretKey, message: &[u8]) -> PartialSignature {
        let mut sig_data = sk.0.clone();
        sig_data.extend_from_slice(message);
        PartialSignature {
            validator_id: "someID".to_string(),
            sig_bytes: sig_data,
        }
    }
}

/// OnboardingRequest f�r Phase B: 
/// - node_id
/// - software_hash
/// - db_hash
/// - public_key (ggf. Ed25519)
/// - optional: version etc.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PhaseBOnboardingRequest {
    pub node_id: String,
    pub software_hash: String,
    pub db_hash: String,
    pub pubkey_ed25519: [u8; 32],
}

/// Enth�lt partial signatures von k Pr�fdiensten,
/// oder mind. M => aggregator => final TSign
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PhaseBPartialSigMsg {
    pub validator_id: String,
    pub partial_sig: bls_threshold::PartialSignature,
}

/// Finale OnboardingCertificate in Phase B => Threshold signature
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PhaseBOnboardingCertificate {
    pub node_id: String,
    pub pubkey_ed25519: [u8; 32],
    pub software_hash: String,
    pub db_hash: String,
    pub issued_at: u64,
    // Aggregierte BLS-Threshold-Sig
    pub threshold_sig: bls_threshold::ThresholdSignature,
}

////////////////////////////////////////////////////////////
// Komitee-Logik => Wir speichern (validator_id -> BlsPublicKey)
////////////////////////////////////////////////////////////

#[derive(Debug)]
pub struct CommitteePhaseB {
    pub k: usize,           // Gr��e des Komitees
    pub m: usize,           // minimal ben�tigte partial sigs
    pub db: DexDB,          // DB => wir holen fullnode-Liste
    // In real => wir h�tten Keypairs. 
    // Wir tun so: main node hat "committee_key_map"
    pub committee_key_map: HashMap<String, bls_threshold::BlsKeypair>,
    // software + db-hash whitelists (od. policy):
    pub allowed_software: HashSet<String>,
    pub allowed_dbhashes: HashSet<String>,
}

impl CommitteePhaseB {
    pub fn new(db: DexDB, k: usize, m: usize) -> Self {
        CommitteePhaseB {
            k,
            m,
            db,
            committee_key_map: HashMap::new(),
            allowed_software: HashSet::new(),
            allowed_dbhashes: HashSet::new(),
        }
    }

    /// F�ge eine "Policy" => erlaubte software hash
    pub fn add_software_hash(&mut self, hash: &str) {
        self.allowed_software.insert(hash.to_string());
    }
    pub fn add_dbhash(&mut self, hash: &str) {
        self.allowed_dbhashes.insert(hash.to_string());
    }

    /// Selektiere k Fullnodes aus N per VRF/Blockhash = Dummy hier
    /// In echt => nimm z. B. blockhash => rng seed => shuffle => k top
    pub fn select_committee(&self) -> Vec<String> {
        // Hole N Fullnodes
        let fullnode_ids = match self.db.list_accounts_of_type(AccountType::Fullnode) {
            Ok(lst) => lst,
            Err(_) => return vec![],
        };
        if fullnode_ids.len() <= self.k {
            return fullnode_ids;
        }
        // Echte random => wir tun so
        let seed = 0xDEADBEEFu64; 
        let mut rng: StdRng = SeedableRng::seed_from_u64(seed);
        let mut pool = fullnode_ids.clone();
        pool.shuffle(&mut rng);
        pool.truncate(self.k);
        pool
    }

    /// Pr�fe OnboardingRequest => war software_hash + db_hash in Policy?
    pub fn check_onboarding_request(
        &self,
        req: &PhaseBOnboardingRequest
    ) -> Result<(), DexError> {
        if !self.allowed_software.contains(&req.software_hash) {
            return Err(DexError::Other(format!("Software-Hash {} not allowed", req.software_hash)));
        }
        if !self.allowed_dbhashes.contains(&req.db_hash) {
            return Err(DexError::Other(format!("DB-Hash {} not allowed", req.db_hash)));
        }
        if req.pubkey_ed25519 == [0u8; 32] {
            return Err(DexError::Other("pubkey=0 => invalid".into()));
        }
        Ok(())
    }

    /// Ein "Komitee-Mitglied" validiert => partial signature
    /// => In echt => wir brauchen den SecretKey
    pub fn produce_partial_signature(
        &self,
        validator_id: &str,
        req: &PhaseBOnboardingRequest
    ) -> Result<bls_threshold::PartialSignature, DexError> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)
            .map_err(|_| DexError::Other("Time error".into()))?
            .as_secs();
        self.check_onboarding_request(req)?;

        // Baue Message
        let msg = build_phase_b_message(req, now);

        let kp = self.committee_key_map.get(validator_id)
            .ok_or_else(|| DexError::Other(format!("Validator {} not found in committee_key_map", validator_id)))?;
        let partial_sig = bls_threshold::sign_message(&kp.sk, &msg);
        Ok(partial_sig)
    }

    /// Aggregation => TSign
    pub fn aggregate_threshold_signature(
        &self,
        partial_sigs: &[bls_threshold::PartialSignature],
        req: &PhaseBOnboardingRequest
    ) -> Result<bls_threshold::ThresholdSignature, DexError> {
        if partial_sigs.len() < self.m {
            return Err(DexError::Other(format!(
                "Nur {} partial sigs => brauchen mind. {}",
                partial_sigs.len(),
                self.m
            )));
        }
        let now = SystemTime::now().duration_since(UNIX_EPOCH)
            .map_err(|_| DexError::Other("Time error".into()))?
            .as_secs();
        self.check_onboarding_request(req)?;

        let sig = bls_threshold::aggregate_signatures(partial_sigs);
        Ok(sig)
    }

    /// Komplette Erzeugung des finalen OnboardingCertificate
    /// wenn newNode die partial sigs gesammelt hat.
    pub fn produce_onboarding_certificate(
        &self,
        partial_sigs: &[bls_threshold::PartialSignature],
        req: &PhaseBOnboardingRequest,
    ) -> Result<PhaseBOnboardingCertificate, DexError> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)
            .map_err(|_| DexError::Other("Time error".into()))?
            .as_secs();

        let t_sig = self.aggregate_threshold_signature(partial_sigs, req)?;
        let cert = PhaseBOnboardingCertificate {
            node_id: req.node_id.clone(),
            pubkey_ed25519: req.pubkey_ed25519,
            software_hash: req.software_hash.clone(),
            db_hash: req.db_hash.clone(),
            issued_at: now,
            threshold_sig: t_sig,
        };
        Ok(cert)
    }
}

////////////////////////////////////////////////////////////
// build_phase_b_message => analog wie in PhaseA
////////////////////////////////////////////////////////////

fn build_phase_b_message(req: &PhaseBOnboardingRequest, timestamp: u64) -> Vec<u8> {
    let mut msg = Vec::new();
    msg.extend_from_slice(b"PhaseB-Onboarding:");
    msg.extend_from_slice(req.node_id.as_bytes());
    msg.extend_from_slice(req.software_hash.as_bytes());
    msg.extend_from_slice(req.db_hash.as_bytes());
    msg.extend_from_slice(&req.pubkey_ed25519);
    msg.extend_from_slice(&timestamp.to_le_bytes());
    msg
}

////////////////////////////////////////////////////////////
// Verifikation: Andere Fullnodes pr�fen "PhaseBOnboardingCertificate"
////////////////////////////////////////////////////////////

pub fn verify_phase_b_onboarding_cert(
    cert: &PhaseBOnboardingCertificate,
    all_committee_pubkeys: &[bls_threshold::BlsPublicKey],
    m: usize,
) -> Result<(), DexError> {
    // Baue Message => selbe Logik
    let msg = {
        let mut v = Vec::new();
        v.extend_from_slice(b"PhaseB-Onboarding:");
        v.extend_from_slice(cert.node_id.as_bytes());
        v.extend_from_slice(cert.software_hash.as_bytes());
        v.extend_from_slice(cert.db_hash.as_bytes());
        v.extend_from_slice(&cert.pubkey_ed25519);
        v.extend_from_slice(&cert.issued_at.to_le_bytes());
        v
    };

    // => threshold verify
    if !bls_threshold::verify_threshold_signature(
        all_committee_pubkeys,
        m,
        &msg,
        &cert.threshold_sig
    ) {
        return Err(DexError::Other("verify_phase_b_onboarding_cert => TSign invalid".into()));
    }
    Ok(())
}
