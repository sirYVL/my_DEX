// src/htlc/onchain_htlc.rs
//
// Rudimentäres On-Chain-HTLC mit rust-bitcoin
// => In echt: viel mehr Edge Cases, Fees, RBF etc.
//
// Bemerkung: Noch sehr abstrakt – in der Praxis bräuchtest du SigOps, SIGHASH, 
// korrekte Signaturen, Fee-Berechnungen, RBF-Handling, etc.

use anyhow::{Result, anyhow};
use tracing::{info, warn, instrument};
use bitcoin::{
    Script, blockdata::script::Builder, blockdata::opcodes::all::*,
    hashes::sha256, hashes::Hash, Transaction, TxIn, TxOut
};

#[derive(Debug)]
pub struct OnchainHtlc {
    pub redeem_script: Script,
    pub funded_tx: Transaction,
    pub hashlock: [u8; 32],
    pub timelock: u32,
}

impl OnchainHtlc {
    #[instrument(name="onchain_htlc_create")]
    pub fn create_htlc(preimage_hash: [u8; 32], locktime: u32) -> Self {
        // rudimentäre HTLC-Script-Konstruktion:
        let redeem_script = Builder::new()
            .push_opcode(OP_IF)                 // IF-Zweig => Redeem mit preimage
            .push_opcode(OP_SHA256)
            .push_slice(&preimage_hash)
            .push_opcode(OP_EQUALVERIFY)
            .push_opcode(OP_TRUE) // Pseudocode: hier könnte man CHECKSIG
            .push_opcode(OP_ELSE) // ELSE-Zweig => Refund via timelock
            .push_int(locktime as i64)
            .push_opcode(OP_CHECKLOCKTIMEVERIFY)
            .push_opcode(OP_DROP)
            .push_opcode(OP_TRUE) // Pseudocode: hier CHECKSIG
            .push_opcode(OP_ENDIF)
            .into_script();

        // Minimale "funded_tx" => Du würdest real UTXOs angeben, Fee, etc.
        let funded_tx = Transaction {
            version: 2,
            lock_time: 0,
            input: vec![TxIn::default()],
            output: vec![TxOut::default()],
        };

        OnchainHtlc {
            redeem_script,
            funded_tx,
            hashlock: preimage_hash,
            timelock: locktime,
        }
    }

    #[instrument(name="onchain_htlc_redeem", skip(self, preimage))]
    pub fn redeem_with_preimage(&mut self, preimage: &[u8]) -> Result<()> {
        // check => sha256(preimage) == self.hashlock
        let got = sha256::Hash::hash(preimage);
        if got[..] != self.hashlock[..] {
            return Err(anyhow!("Hash mismatch => can't redeem"));
        }
        info!("HTLC => redeem_with_preimage => IF-Pfad. Preimage ok.");
        Ok(())
    }

    #[instrument(name="onchain_htlc_refund", skip(self))]
    pub fn refund_after_timelock(&mut self, current_time: u32) -> Result<()> {
        // check => current_time >= self.timelock
        if current_time < self.timelock {
            return Err(anyhow!("Too early => can't refund"));
        }
        info!("HTLC => refund_after_timelock => ELSE-Pfad. Zeit abgelaufen => Refund ok.");
        Ok(())
    }
}
