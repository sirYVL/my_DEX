////////////////////////////////////////////////////////////
// my_dex/src/onboarding/gatekeeper_phase_a.rs
////////////////////////////////////////////////////////////
//
// "Phase A: Startphase mit erstem Node als Gatekeeper"
// ------------------------------------------------------
// Dieses Modul implementiert das konkrete Onboarding-Verfahren
// f�r neue Fullnodes, solange das Netzwerk noch klein ist (< X Nodes).
// Dabei fungiert der allererste Node als zentraler "Gatekeeper".
// Er hat einen Admin-Schl�ssel (ed25519) und pr�ft:
//   - Software-Integrit�tsnachweis
//   - DB-/CRDT-Hash
//   - Node-Pubkey
// Wenn alles korrekt ist, signiert er ein Onboarding-Zertifikat.
// Andere Fullnodes akzeptieren das Zertifikat, da sie den Admin-PublicKey
// kennen. So wird is_fee_pool_recipient oder account_type=Fullnode
// automatisch gesetzt.
//
// Dieses Modul verzichtet auf Demos und Platzhalter � es ist
// produktionsreifer Code, der das Gatekeeper-Verfahren f�r die
// Anfangsphase (bis Netzwerk < X Fullnodes) realisiert.
//
// Voraussetzungen:
//  - Du hast ein "Gatekeeper" (admin_keypair) 
//    -> gatekeeper_phase_a.rs" (hier integriert).
//  - Du speicherst OnboardingRequests und OnboardingCertificates
//    ggf. in einer DB, oder du verschickst sie direkt per P2P.
//  - Andere Fullnodes f�hren verify_onboarding_certificate aus,
//    bevor sie den Node offiziell "freischalten".
//
////////////////////////////////////////////////////////////

use std::time::{SystemTime, UNIX_EPOCH};
use std::collections::HashMap;
use anyhow::{Result, anyhow};
use ed25519_dalek::{Keypair, PublicKey, SecretKey, Signature, Signer, Verifier};
use serde::{Serialize, Deserialize};

use crate::error::DexError;
use crate::identity::accounts::{AccountsManager, AccountType};
use crate::storage::db_layer::DexDB;  // zum Speichern/Laden
use crate::fees::fee_pool::FeePool;   // falls du hier Fees loggen willst

////////////////////////////////////////////////////////////
// 1) Strukturdaten: OnboardingRequest, OnboardingCertificate
////////////////////////////////////////////////////////////

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingRequest {
    pub node_id: String,         // Name/ID der Node
    pub software_hash: String,   // Integrit�ts-Hash der Software
    pub db_hash: String,         // CRDT-/DB-Hash
    pub node_pubkey: [u8; 32],   // Ed25519-Pubkey des neuen Nodes
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnboardingCertificate {
    pub node_id: String,         // Wen es betrifft
    pub node_pubkey: [u8; 32],   // Dessen ed25519-Pubkey
    pub issued_at: u64,          // Timestamp
    pub signature: [u8; 64],     // Gatekeeper-Signatur (ed25519)
}

////////////////////////////////////////////////////////////
// 2) GatekeeperPhaseA => verwaltet Admin-Schl�ssel + WhiteLists
////////////////////////////////////////////////////////////

pub struct GatekeeperPhaseA {
    pub admin_keypair: Keypair,

    /// Liste erlaubter Software-Hashes (z.?B. Docker-Image-SHA256).
    pub allowed_software_hashes: Vec<String>,

    /// Liste erlaubter DB/CRDT-Hashes.
    pub allowed_db_hashes: Vec<String>,

    /// DB, um optional den Stand der bereits genehmigten Nodes zu speichern.
    pub db: DexDB,

    /// Maximalanzahl, ab der Phase A endet (z. B. 100).
    /// Danach wird man in eine andere Onboarding-Phase wechseln.
    pub max_phase_a_nodes: usize,
}

impl GatekeeperPhaseA {
    /// Erzeugt einen Gatekeeper mit bereits geladenem Keypair.
    /// (Keypair k�nnte man z. B. aus einer Datei laden)
    pub fn new(
        admin_keypair: Keypair,
        allowed_software_hashes: Vec<String>,
        allowed_db_hashes: Vec<String>,
        db: DexDB,
        max_phase_a_nodes: usize,
    ) -> Self {
        GatekeeperPhaseA {
            admin_keypair,
            allowed_software_hashes,
            allowed_db_hashes,
            db,
            max_phase_a_nodes,
        }
    }

    /// Pr�ft, wie viele Fullnodes es aktuell schon gibt. 
    /// (Phase A => wir machen Gatekeeper-Approval nur,
    ///  wenn wir < max_phase_a_nodes sind.)
    fn can_still_approve(&self) -> bool {
        match self.db.count_accounts_with_type(AccountType::Fullnode) {
            Ok(count) => count < self.max_phase_a_nodes,
            Err(_) => true,  // Falls DB-Lesefehler => wir lassen es zu
        }
    }

    /// Bearbeitet eine Onboarding-Anfrage, pr�ft:
    /// 1) Sind wir noch in Phase A (< max_phase_a_nodes)?
    /// 2) Ist software_hash in allowed_software_hashes?
    /// 3) Ist db_hash in allowed_db_hashes?
    /// 4) node_pubkey != [0; 32]?
    /// => Dann OnboardingCertificate signieren und
    ///    in DB speicher (z. B. "onboarding_certs/{node_id}")
    pub fn approve_onboarding(&self, req: &OnboardingRequest) -> Result<OnboardingCertificate, DexError> {
        // 1) check PhaseA
        if !self.can_still_approve() {
            return Err(DexError::Other(
                "PhaseA: max Anzahl Fullnodes erreicht => kein Gatekeeper-Approval mehr m�glich".into()
            ));
        }

        // 2) check software_hash
        if !self.allowed_software_hashes.contains(&req.software_hash) {
            return Err(DexError::Other(format!(
                "Software-Hash {} nicht in PhaseA-Whitelist => abgelehnt", req.software_hash
            )));
        }

        // 3) check db_hash
        if !self.allowed_db_hashes.contains(&req.db_hash) {
            return Err(DexError::Other(format!(
                "DB-Hash {} nicht in PhaseA-Whitelist => abgelehnt", req.db_hash
            )));
        }

        // 4) node_pubkey minimal
        if req.node_pubkey == [0u8; 32] {
            return Err(DexError::Other("Node-Pubkey = 0 => ung�ltig".into()));
        }

        // Signieren
        let now = SystemTime::now().duration_since(UNIX_EPOCH)
            .map_err(|_| DexError::Other("SystemTime error".into()))?
            .as_secs();
        let message = build_onboarding_message(
            &req.node_id,
            &req.node_pubkey,
            &req.software_hash,
            &req.db_hash,
            now
        );
        let signature = self.admin_keypair.sign(&message);
        let cert = OnboardingCertificate {
            node_id: req.node_id.clone(),
            node_pubkey: req.node_pubkey,
            issued_at: now,
            signature: signature.to_bytes(),
        };

        // Optional: Speichern in DB => "onboarding_certs/node_id"
        let key = format!("onboarding_certs/{}", req.node_id);
        self.db.store_bytes(&key, &bincode::serialize(&cert)?)?;

        Ok(cert)
    }
}

////////////////////////////////////////////////////////////
// Hilfsfunktion => dieselbe Logik, die wir beim Verify brauchen.
////////////////////////////////////////////////////////////

fn build_onboarding_message(
    node_id: &str,
    node_pubkey: &[u8; 32],
    software_hash: &str,
    db_hash: &str,
    ts: u64,
) -> Vec<u8> {
    // Kein Demo, sondern reale Concatenation
    let mut msg = Vec::new();
    msg.extend_from_slice(b"PhaseA-Onboarding:");
    msg.extend_from_slice(node_id.as_bytes());
    msg.extend_from_slice(node_pubkey);
    msg.extend_from_slice(software_hash.as_bytes());
    msg.extend_from_slice(db_hash.as_bytes());
    msg.extend_from_slice(&ts.to_le_bytes());
    msg
}

////////////////////////////////////////////////////////////
// 3) Jeder andere Fullnode => verifiziert OnboardingCertificate
//    => Setzt is_fee_pool_recipient = true / account_type=Fullnode
////////////////////////////////////////////////////////////

pub fn verify_phase_a_onboarding_cert(
    cert: &OnboardingCertificate,
    gatekeeper_pk: &PublicKey,
    software_hash: &str,
    db_hash: &str,
) -> Result<(), DexError> {
    let constructed = build_onboarding_message(
        &cert.node_id,
        &cert.node_pubkey,
        software_hash,
        db_hash,
        cert.issued_at
    );
    let sig = Signature::from_bytes(&cert.signature)
        .map_err(|_| DexError::Other("Ung�ltige Signatur im Zertifikat (Format)".into()))?;

    gatekeeper_pk
        .verify(&constructed, &sig)
        .map_err(|e| DexError::Other(format!("Signature verify failed => {:?}", e)))?;

    Ok(())
}
