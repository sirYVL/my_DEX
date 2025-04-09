// my_dex/src/dex_logic/htlc.rs
//
// HTLC-Logik + AtomicSwaps für Cross-Chain. 
// Jetzt mit tiefer Instrumentierung (#[instrument]) & Metrik-Zählern.
//
// NEU (Sicherheitsupdates):
// 1) Wir verhindern negative/Null-Beträge beim Erstellen eines HTLC/AtomicSwap.
// 2) Wir prüfen, ob der timelock (HTLC) bzw. max_sign_time (Swap) in der Vergangenheit liegt.
//    So kann kein Angreifer immediate-expired HTLC anlegen.

use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use anyhow::{Result, anyhow};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, debug, instrument};

use crate::dex_logic::orders::Asset;
use crate::metrics::{
    HTLC_REDEEM_COUNT, HTLC_REFUND_COUNT,
    SWAP_SELLER_REDEEM_COUNT, SWAP_BUYER_REDEEM_COUNT, SWAP_REFUND_COUNT
};

// Einfache HTLC-Struktur
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HTLC {
    pub chain: Asset,
    pub amount: f64,
    pub hashlock: [u8; 32],
    pub timelock: u64, // Unix-Zeit => ab hier refund
    pub redeemed: bool,
    pub refunded: bool,
}

impl HTLC {
    /// Erzeugt eine neue HTLC, prüft aber, ob amount>0 und timelock>JETZT.
    pub fn new(chain: Asset, amount: f64, preimage_hash: [u8; 32], timelock: u64) -> Result<Self> {
        if amount <= 0.0 {
            return Err(anyhow!("HTLC: amount must be positive"));
        }
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        if timelock <= now {
            return Err(anyhow!("HTLC: timelock is already in the past => can't create"));
        }

        Ok(Self {
            chain,
            amount,
            hashlock: preimage_hash,
            timelock,
            redeemed: false,
            refunded: false,
        })
    }

    pub fn hashlock_to_string(&self) -> String {
        hex::encode(self.hashlock)
    }

    fn is_expired(&self) -> Result<bool> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        Ok(now >= self.timelock)
    }

    #[instrument(name="htlc_redeem", skip(self, preimage))]
    pub fn redeem(&mut self, preimage: &[u8]) -> Result<()> {
        if self.is_expired()? {
            return Err(anyhow!("HTLC expired => can't redeem"));
        }
        let mut hasher = Sha256::new();
        hasher.update(preimage);
        let digest = hasher.finalize();
        if digest[..] != self.hashlock[..] {
            return Err(anyhow!("Hash mismatch => redeem failed"));
        }
        if self.redeemed {
            return Err(anyhow!("Already redeemed"));
        }
        self.redeemed = true;

        // Metrik
        HTLC_REDEEM_COUNT.inc();

        info!("HTLC redeemed: chain={:?}, amount={}, hashlock={}", 
            self.chain, self.amount, self.hashlock_to_string());
        Ok(())
    }

    #[instrument(name="htlc_refund", skip(self))]
    pub fn refund(&mut self) -> Result<()> {
        if !self.is_expired()? {
            return Err(anyhow!("HTLC not expired => can't refund"));
        }
        if self.refunded {
            return Err(anyhow!("Already refunded"));
        }
        self.refunded = true;

        // Metrik
        HTLC_REFUND_COUNT.inc();

        info!("HTLC refunded: chain={:?}, amount={}, hashlock={}", 
            self.chain, self.amount, self.hashlock_to_string());
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum SwapState {
    Init,
    SellerRedeemed,
    BuyerRedeemed,
    Refunded,
    Cancelled,
}

/// Repräsentiert den CrossChain-Swap
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AtomicSwap {
    pub buyer_htlc: HTLC,
    pub seller_htlc: HTLC,
    pub preimage: Option<Vec<u8>>,
    pub state: SwapState,

    pub creation_time: u64,
    pub max_sign_time: u64, // Zeitfenster, in dem Seller redeem muss
}

impl AtomicSwap {
    /// Erstellt ein AtomicSwap und prüft, ob max_sign_time > JETZT,
    /// um sofort ablaufende Swaps zu verhindern.
    pub fn new(
        buyer_htlc: HTLC,
        seller_htlc: HTLC,
        max_sign_time: u64
    ) -> Result<Self> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)?.as_secs();
        if max_sign_time <= now {
            return Err(anyhow!("AtomicSwap: max_sign_time is already in the past => can't create swap"));
        }

        Ok(AtomicSwap {
            buyer_htlc,
            seller_htlc,
            preimage: None,
            state: SwapState::Init,
            creation_time: now,
            max_sign_time,
        })
    }

    pub fn hashlock_to_string(&self) -> String {
        self.buyer_htlc.hashlock_to_string()
    }

    #[instrument(name="swap_seller_redeem", skip(self, preimage))]
    pub fn seller_redeem(&mut self, preimage: &[u8]) -> Result<()> {
        if self.state != SwapState::Init {
            return Err(anyhow!("Swap not in init => can't seller redeem"));
        }
        self.buyer_htlc.redeem(preimage)?;
        self.preimage = Some(preimage.to_vec());
        self.state = SwapState::SellerRedeemed;

        // Metrik
        SWAP_SELLER_REDEEM_COUNT.inc();

        info!("AtomicSwap => seller_redeem done, hashlock={}", self.hashlock_to_string());
        Ok(())
    }

    #[instrument(name="swap_buyer_redeem", skip(self))]
    pub fn buyer_redeem(&mut self) -> Result<()> {
        if self.state != SwapState::SellerRedeemed {
            return Err(anyhow!("Seller not redeemed => can't buyer redeem yet"));
        }
        let preimage = self.preimage.clone().ok_or_else(|| anyhow!("No preimage present"))?;
        self.seller_htlc.redeem(&preimage)?;
        self.state = SwapState::BuyerRedeemed;

        // Metrik
        SWAP_BUYER_REDEEM_COUNT.inc();

        info!("AtomicSwap => buyer_redeem done, hashlock={}", self.hashlock_to_string());
        Ok(())
    }

    #[instrument(name="swap_refund", skip(self))]
    pub fn refund(&mut self) -> Result<()> {
        if self.state == SwapState::BuyerRedeemed {
            return Err(anyhow!("Already completed => can't refund"));
        }
        let _ = self.buyer_htlc.refund();
        let _ = self.seller_htlc.refund();
        self.state = SwapState::Refunded;

        // Metrik
        SWAP_REFUND_COUNT.inc();

        info!("AtomicSwap => refund done, hashlock={}", self.hashlock_to_string());
        Ok(())
    }

    /// check_timeout => Falls wir in (Init/SellerRedeemed) und Zeit abgelaufen => Cancel
    #[instrument(name="swap_check_timeout", skip(self))]
    pub fn check_timeout(&mut self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        if (self.state == SwapState::Init || self.state == SwapState::SellerRedeemed)
            && now > self.max_sign_time
        {
            self.state = SwapState::Cancelled;
            info!("AtomicSwap => Cancelled due to max_sign_time. hashlock={}", self.hashlock_to_string());
        }
    }
}
