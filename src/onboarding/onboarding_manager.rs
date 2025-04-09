////////////////////////////////////////////////////////////
// my_dex/src/onboarding/onboarding_manager.rs
////////////////////////////////////////////////////////////
//
// Dieses Modul kombiniert:
//  - Phase A (Gatekeeper-Admin) => "onboarding_mode = admin"
//  - Phase B (Zufalls-Komitee, M-of-k-Threshold) => "onboarding_mode = auto"
//
// Dabei nutzen wir threshold-crypto f�r echte Partial-Signaturen (statt Stubs).
// 
// Voraussetzungen in Cargo.toml:
//   threshold-crypto = "0.4"
//   rand = "0.8"
//   bincode = "1.3"
//   serde + features=["derive"]
//
// Dies ist ein zusammenh�ngendes, produktionsnahes Beispiel.
// Du musst es ggf. an deine CRDT/DB-Integration anpassen.
//
// (c) Dein DEX-Projekt
////////////////////////////////////////////////////////////

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use rand::{Rng, seq::SliceRandom, SeedableRng};
use rand::rngs::StdRng;
use serde::{Serialize, Deserialize};
use anyhow::{Result, anyhow};
use tracing::{info, warn, debug, error};
use threshold_crypto::{
    SecretKeySet, SecretKeyShare, PublicKeySet, SignatureShare, Signature,
};

use crate::error::DexError;

/// Repr�sentiert globale Einstellungen, z.?B. in CRDT oder On-Chain:
#[derive(Clone, Debug)]
pub struct GlobalConfig {
    pub onboarding_mode: String,  // "admin" oder "auto"
    pub min_fullnodes_for_auto: usize,
    pub committee_size_k: usize,
    pub threshold_m: usize,
}

/// Eine einfache Schnittstelle zu deinem DB-Layer:
/// Hier nur stichwortartig, du kannst es an DexDB anpassen.
pub trait CrdtDbInterface {
    fn load_global_config(&self) -> Result<GlobalConfig>;
    fn store_global_config(&self, cfg: &GlobalConfig) -> Result<()>;

    /// Liste aller (oder ID-Liste) Fullnodes
    fn list_fullnodes(&self) -> Result<Vec<NodeInfo>>;

    /// Node anlegen / updaten
    fn store_node_info(&self, node: &NodeInfo) -> Result<()>;
    fn load_node_info(&self, node_id: &str) -> Result<Option<NodeInfo>>;
}

/// Ein Node mit Onboarding-Infos
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeInfo {
    pub node_id: String,
    pub public_key: Vec<u8>,    // z. B. Ed25519-PK
    pub software_hash: Vec<u8>, // Docker-Image-Hash
    pub db_hash: Vec<u8>,
    pub is_fullnode: bool,
    pub onboarding_cert: Option<Vec<u8>>, // signiertes Zertifikat
}

/// Onboarding-Anfrage (wird von neuem Node bereitgestellt)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OnboardingRequest {
    pub node_id: String,
    pub software_hash: Vec<u8>,
    pub db_hash: Vec<u8>,
    pub public_key: Vec<u8>, // z. B. Ed25519-Pubkey
}

/// Das finale Onboarding-Zertifikat (Phase B, Threshold-Sig)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OnboardingCertificate {
    pub node_id: String,
    pub software_hash: Vec<u8>,
    pub db_hash: Vec<u8>,
    pub aggregated_sig: Vec<u8>,  // M-of-k threshold signature
    pub public_key_set: Vec<u8>,  // serialisiertes PublicKeySet
}

/// Der Gatekeeper (Phase A) unterschreibt ein Admin-Zertifikat
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdminOnboardingCertificate {
    pub node_id: String,
    pub software_hash: Vec<u8>,
    pub db_hash: Vec<u8>,
    pub admin_signature: Vec<u8>, // ECDSA / Ed25519 / etc.
}

/// Enth�lt Teil-Signaturen (Phase B). In Realita w�rdest du
/// P2P-Broadcasts mit diesen partial-sigs austauschen.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PartialSignatureMsg {
    pub validator_id: String,
    pub sig_share: Vec<u8>,
}

/// Der zentrale Manager, der Onboarding abwickelt
pub struct OnboardingManager {
    pub db: Arc<dyn CrdtDbInterface + Send + Sync>,

    // Gatekeeper-Admin-Schl�ssel (nur in Phase A relevant)
    pub admin_key: Vec<u8>,  // z. B. Ed25519-Secretkey

    // In Phase B nutzen wir threshold_crypto:
    // => Du brauchst in Wirklichkeit "SecretKeyShare" je Node, nicht global.
    pub global_pk_set: Option<PublicKeySet>, // globales PublicKeySet
    // => Bei diesem Manager z. B. als "Test" der vollen SecretKeySet:
    // In Real: jedes Komitee-Mitglied hat nur sein `SecretKeyShare`.
    pub global_sk_set: Option<SecretKeySet>,

    // Eine Whitelist f�r software-hashes, db-hashes ...
    pub allowed_software_hashes: HashSet<Vec<u8>>,
    pub allowed_db_hashes: HashSet<Vec<u8>>,
}

impl OnboardingManager {
    /// Erzeugt ein OnboardingManager. 
    /// Du kannst optional einen "SecretKeySet::random(...)" hier anlegen
    /// oder aus DKG importieren.
    pub fn new(db: Arc<dyn CrdtDbInterface + Send + Sync>, admin_key: Vec<u8>) -> Self {
        OnboardingManager {
            db,
            admin_key,
            global_pk_set: None,
            global_sk_set: None,
            allowed_software_hashes: HashSet::new(),
            allowed_db_hashes: HashSet::new(),
        }
    }

    /// Beispielfunktion, um im Test-Setup ein SecretKeySet(2,5) zu erzeugen.
    /// In echt kommt das aus dem DKG.
    pub fn init_threshold_keys_for_test(&mut self, threshold: usize, total_shares: usize) {
        let sks = SecretKeySet::random(threshold, &mut rand::thread_rng());
        let pks = sks.public_keys();
        self.global_sk_set = Some(sks);
        self.global_pk_set = Some(pks);
        info!("Initiated threshold test-keys => t={}, n={}", threshold, total_shares);
    }

    /// Mode-Switch: Wenn fullnodes >= min_fullnodes_for_auto => onboarding_mode="auto"
    pub fn check_and_switch_mode(&self) -> Result<()> {
        let cfg = self.db.load_global_config()?;
        let nodes = self.db.list_fullnodes()?;
        if cfg.onboarding_mode == "admin" && nodes.len() >= cfg.min_fullnodes_for_auto {
            let mut new_cfg = cfg.clone();
            new_cfg.onboarding_mode = "auto";
            self.db.store_global_config(&new_cfg)?;
            info!("Switched to PhaseB => 'auto', fullnodes={}", nodes.len());
        }
        Ok(())
    }

    /// PHASE A => Gatekeeper signiert 
    /// => admin_signature -> AdminOnboardingCertificate
    pub fn gatekeeper_approve(
        &self,
        req: &OnboardingRequest
    ) -> Result<AdminOnboardingCertificate> {
        // checks
        let cfg = self.db.load_global_config()?;
        if cfg.onboarding_mode != "admin" {
            return Err(anyhow!("Currently not in Phase A => no admin-sig possible"));
        }
        // check policy
        if !self.allowed_software_hashes.contains(&req.software_hash) {
            return Err(anyhow!("software hash not allowed"));
        }
        if !self.allowed_db_hashes.contains(&req.db_hash) {
            return Err(anyhow!("db hash not allowed"));
        }
        // sign => wir faken => wir tun so, als ob wir admin_key + data = sign
        let mut sig_data = self.admin_key.clone();
        sig_data.extend_from_slice(&req.node_id.as_bytes());
        sig_data.extend_from_slice(&req.software_hash);
        sig_data.extend_from_slice(&req.db_hash);

        let cert = AdminOnboardingCertificate {
            node_id: req.node_id.clone(),
            software_hash: req.software_hash.clone(),
            db_hash: req.db_hash.clone(),
            admin_signature: sig_data,
        };
        Ok(cert)
    }

    /// PHASE A => Fullnodes pr�fen Admin-Zertifikat => wenn ok => set is_fullnode
    pub fn verify_and_accept_admin_certificate(
        &self,
        admin_cert: &AdminOnboardingCertificate
    ) -> Result<()> {
        let cfg = self.db.load_global_config()?;
        if cfg.onboarding_mode != "admin" {
            return Err(anyhow!("We're not in admin-phase => can't accept admin-sig!"));
        }
        // verify => wir tun so => check prefix
        if !admin_cert.admin_signature.starts_with(&self.admin_key) {
            return Err(anyhow!("Admin signature invalid => mismatch with stored admin_key"));
        }
        // => ok
        let new_node = NodeInfo {
            node_id: admin_cert.node_id.clone(),
            public_key: vec![], // or the same as in request
            software_hash: admin_cert.software_hash.clone(),
            db_hash: admin_cert.db_hash.clone(),
            is_fullnode: true,
            onboarding_cert: Some(bincode::serialize(admin_cert)?),
        };
        self.db.store_node_info(&new_node)?;
        Ok(())
    }

    /// PHASE B => Komitee: 
    /// W�hle k fullnodes => jeder hat share => produce partial
    /// In echt => man broadcastet request => validator produce partial => P2P
    /// Hier: wir tun es lokal.
    pub fn committee_produce_partial(
        &self,
        local_share: &SecretKeyShare,
        local_share_idx: usize,
        req: &OnboardingRequest
    ) -> Result<SignatureShare> {
        let cfg = self.db.load_global_config()?;
        if cfg.onboarding_mode != "auto" {
            return Err(anyhow!("We're not in auto-phase => can't produce partial sig."));
        }
        // check policy:
        if !self.allowed_software_hashes.contains(&req.software_hash) {
            return Err(anyhow!("software hash not allowed"));
        }
        if !self.allowed_db_hashes.contains(&req.db_hash) {
            return Err(anyhow!("db hash not allowed"));
        }

        // build message:
        let message = build_phase_b_message(req)?;
        // sign
        let part = local_share.sign(message);
        debug!("Partial Sig => from share idx {} => len={}", local_share_idx, part.to_bytes().len());
        Ok(part)
    }

    /// Aggregation => M-of-k combine
    pub fn combine_partial_signatures(
        &self,
        partials: &[(usize, SignatureShare)],
        req: &OnboardingRequest
    ) -> Result<OnboardingCertificate> {
        let cfg = self.db.load_global_config()?;
        if cfg.onboarding_mode != "auto" {
            return Err(anyhow!("We're not in auto-phase => can't combine partial sigs."));
        }
        // must have at least threshold_m partials
        if partials.len() < cfg.threshold_m {
            return Err(anyhow!("Not enough partials => needed >= threshold_m"));
        }
        let msg = build_phase_b_message(req)?;
        let pkset = self.global_pk_set.as_ref()
            .ok_or_else(|| anyhow!("No global_pk_set in manager => can't combine?"))?;
        
        // combine
        let comb_sig = pkset.combine_signatures(partials)
            .map_err(|_| anyhow!("combine_signatures => mismatch or invalid shares"))?;
        // verify
        if !pkset.public_key().verify(&comb_sig, msg) {
            return Err(anyhow!("aggregated sig => verify fail!"));
        }
        // => success
        let ocert = OnboardingCertificate {
            node_id: req.node_id.clone(),
            software_hash: req.software_hash.clone(),
            db_hash: req.db_hash.clone(),
            aggregated_sig: comb_sig.to_bytes(),
            public_key_set: bincode::serialize(&pkset.commitment())?, 
        };
        Ok(ocert)
    }

    /// Jede Node => verify => if success => store is_fullnode
    pub fn verify_and_accept_onboarding_certificate(
        &self,
        cert: &OnboardingCertificate
    ) -> Result<()> {
        let cfg = self.db.load_global_config()?;
        if cfg.onboarding_mode != "auto" {
            return Err(anyhow!("We're not in auto-phase => can't verify phaseB cert."));
        }
        // reconstruct pkset
        let comm: threshold_crypto::Commitment = bincode::deserialize(&cert.public_key_set)?;
        let pkset = PublicKeySet::from(comm);
        let msg = {
            let mut data = vec![];
            data.extend_from_slice(b"PhaseB-Onboard|");
            data.extend_from_slice(&cert.node_id.as_bytes());
            data.extend_from_slice(&cert.software_hash);
            data.extend_from_slice(&cert.db_hash);
            data
        };
        let sig = Signature::from_bytes(&cert.aggregated_sig)
            .map_err(|_| anyhow!("invalid aggregated_sig bytes"))?;

        if !pkset.public_key().verify(&sig, msg) {
            return Err(anyhow!("Threshold signature verification failed => invalid cert"));
        }

        // => ok => store node
        let new_node = NodeInfo {
            node_id: cert.node_id.clone(),
            public_key: vec![], // in real => ...
            software_hash: cert.software_hash.clone(),
            db_hash: cert.db_hash.clone(),
            is_fullnode: true,
            onboarding_cert: Some(bincode::serialize(cert)?),
        };
        self.db.store_node_info(&new_node)?;
        info!("Node {} => now is_fullnode => Phase B certificate accepted", cert.node_id);
        Ok(())
    }
}

// Hilfsfunktion => build message (Phase B)
fn build_phase_b_message(req: &OnboardingRequest) -> Result<Vec<u8>> {
    let mut data = vec![];
    data.extend_from_slice(b"PhaseB-Onboard|");
    data.extend_from_slice(&req.node_id.as_bytes());
    data.extend_from_slice(&req.software_hash);
    data.extend_from_slice(&req.db_hash);
    Ok(data)
}
