////////////////////////////////////////////////////////////
// my_dex/src/htlc/atomic_swap.rs
////////////////////////////////////////////////////////////
//
// Minimaler Off-Chain-Swap => vertrag "start_time + swap_timeout_sec" => fallback?
// Jetzt erweitert um buyer_asset / seller_asset und buyer_amount / seller_amount,
// damit die Settlement-Logik in advanced_settlement.rs dynamisch alle Asset-Paare
// verwenden kann.

use anyhow::{Result, anyhow};
use tracing::{info, warn, instrument};
use std::time::{SystemTime, UNIX_EPOCH};

// Bitte Asset aus advanced_settlement importieren:
use crate::settlement::advanced_settlement::Asset;

/// Diese Enums werden übernommen wie gehabt; 
/// du kannst sie "SwapState" nennen, wenn du möchtest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SwapPhase {
    Init,
    SellerRedeemed,
    BuyerRedeemed,
    Cancelled,
}

/// Minimale OffChain-Swap => wir nennen ihn jetzt `AtomicSwap`
/// (Du kannst den alten Namen "OffchainSwap" behalten, 
///  dann bitte in advanced_settlement.rs anpassen.)
#[derive(Clone, Debug)]
pub struct AtomicSwap {
    pub swap_id: String,
    pub start_time: u64,
    pub phase: SwapPhase,
    pub swap_timeout_sec: u64,

    /// Neue Felder: Buyer-/Seller-Assets und -Mengen
    /// Damit wir in advanced_settlement.rs dynamisch fees anwenden können.
    pub buyer_asset: Asset,
    pub seller_asset: Asset,
    pub buyer_amount: f64,
    pub seller_amount: f64,
}

impl AtomicSwap {
    #[instrument(name="swap_new")]
    pub fn new(
        swap_id: &str,
        swap_timeout_sec: u64,
        buyer_asset: Asset,
        seller_asset: Asset,
        buyer_amount: f64,
        seller_amount: f64,
    ) -> Self {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        Self {
            swap_id: swap_id.to_string(),
            start_time: now,
            phase: SwapPhase::Init,
            swap_timeout_sec,

            buyer_asset,
            seller_asset,
            buyer_amount,
            seller_amount,
        }
    }

    #[instrument(name="swap_check_timeout", skip(self))]
    pub fn check_timeout(&mut self) -> bool {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        if self.phase == SwapPhase::Init && now >= (self.start_time + self.swap_timeout_sec) {
            self.phase = SwapPhase::Cancelled;
            return true;
        }
        false
    }

    #[instrument(name="swap_seller_redeem", skip(self))]
    pub fn seller_redeem(&mut self) -> Result<()> {
        if self.phase != SwapPhase::Init {
            return Err(anyhow!("Swap not in init => can't seller redeem"));
        }
        self.phase = SwapPhase::SellerRedeemed;
        Ok(())
    }

    #[instrument(name="swap_buyer_redeem", skip(self))]
    pub fn buyer_redeem(&mut self) -> Result<()> {
        if self.phase != SwapPhase::SellerRedeemed {
            return Err(anyhow!("Seller not redeemed => can't buyer redeem"));
        }
        self.phase = SwapPhase::BuyerRedeemed;
        Ok(())
    }
}
