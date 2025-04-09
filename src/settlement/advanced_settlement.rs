///////////////////////////////////////////////////////////
// my_dex/src/settlement/advanced_settlement.rs
///////////////////////////////////////////////////////////
//
// Ein "AdvancedSettlementEngine", das alle wichtigen Edge-Cases
// (Mehrfach-Assets, Atomic Swap, On-Chain-HTLC, Fee-Pool, Retry-Logik)
// abbildet.
//
// NEU (Sicherheitsupdate):
//   1) Negative/Null-Werte => Err(...) in finalize_trade, finalize_atomic_swap
//   2) Globaler Mutex => verhindert parallele Zugriffe in den finalize-Methoden
//
// Damit beseitigen wir triviale Schwachstellen ohne das Grunddesign zu ändern.
//

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn, error};
use anyhow::{Result, anyhow};

use crate::error::DexError;
use crate::security::security_validator::SecurityValidator;
use crate::fees::fee_pool::FeePool;
use crate::htlc::atomic_swap::{AtomicSwap, SwapState};
use crate::htlc::onchain_htlc::OnchainHtlc;
use crate::settlement::secured_settlement::SettlementEngineTrait;
use crate::storage::db_layer::DexDB;

// **NEU**: FeeConfig
use crate::settlement::fees_config::SettlementFees;

// NEU: Globaler Mutex => wir sperren finalize-Methoden
use lazy_static::lazy_static;

lazy_static! {
    static ref ENGINE_MUTEX: Mutex<()> = Mutex::new(());
}

/// Repräsentiert ein einfaches "Asset" – in einer echten Umsetzung
/// könntest du Asset::BTC, Asset::ETH, Asset::LTC, ERC20, usw. haben.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Asset {
    BTC,
    LTC,
    ETH,
    // ggf. mehr
}

///////////////////////////////////////////////////////////
// Trait: SettlementEngineTrait
///////////////////////////////////////////////////////////
pub trait SettlementEngineTrait: Send + Sync {
    /// Finalisiert normalen Off-Chain-Trade
    fn finalize_trade(
        &mut self,
        buyer: &str,
        seller: &str,
        base_asset: Asset,
        quote_asset: Asset,
        base_amount: f64,
        quote_amount: f64,
    ) -> Result<(), DexError>;

    /// Finalisiert einen Cross-Chain AtomicSwap
    fn finalize_atomic_swap(&mut self, swap_id: &str, swap: &mut AtomicSwap) -> Result<(), DexError>;

    /// Finalisiert On-Chain-HTLC
    fn finalize_onchain_htlc(&mut self, htlc_id: &str, htlc: &mut OnchainHtlc) -> Result<(), DexError>;
}

///////////////////////////////////////////////////////////
// AdvancedSettlementEngine => alle Off-Chain-Konten
///////////////////////////////////////////////////////////
#[derive(Clone, Debug)]
pub struct AdvancedSettlementEngine {
    /// user_id => (Asset => (free, locked))
    pub balances: Arc<Mutex<HashMap<String, HashMap<Asset, (f64, f64)>>>>,

    /// FeePool => Fees nur hier abgezogen
    pub fee_pool: Arc<FeePool>,

    /// DB-Retry
    pub max_retries: u32,
    pub retry_backoff: Duration,

    pub db: Arc<Mutex<DexDB>>,
    pub fees_config: SettlementFees,
}

impl AdvancedSettlementEngine {
    pub fn new(
        fee_pool: Arc<FeePool>,
        db: Arc<Mutex<DexDB>>,
        fees_config: SettlementFees,
    ) -> Self {
        Self {
            balances: Arc::new(Mutex::new(HashMap::new())),
            fee_pool,
            max_retries: 3,
            retry_backoff: Duration::from_millis(200),
            db,
            fees_config,
        }
    }

    /// Hilfsfunktion => Fees
    fn apply_fees(&self, user: &str, asset: &Asset, amount: f64, fee_percent: f64) {
        let fee_amt = amount * fee_percent;
        if fee_amt <= 0.0 {
            return;
        }
        let res = self.fee_pool.add_fees_in_asset(*asset, fee_amt);
        if let Err(e) = res {
            warn!("apply_fees => user={} => failed to add fee => err={:?}, ignoring", user, e);
        } else {
            debug!("apply_fees => user={} => fee_amt={:.8} asset={:?}", user, fee_amt, asset);
        }
    }
}

impl SettlementEngineTrait for AdvancedSettlementEngine {
    fn finalize_trade(
        &mut self,
        buyer: &str,
        seller: &str,
        base_asset: Asset,
        quote_asset: Asset,
        base_amount: f64,
        quote_amount: f64,
    ) -> Result<(), DexError> {
        // (A) => globaler Lock
        let _lock = ENGINE_MUTEX.lock().map_err(|_| DexError::Other("engine mutex poisoned".into()))?;

        // (1) Negative-/Nullwert-Prüfung
        if base_amount <= 0.0 {
            return Err(DexError::Other(format!("Invalid base_amount: {}", base_amount)));
        }
        if quote_amount <= 0.0 {
            return Err(DexError::Other(format!("Invalid quote_amount: {}", quote_amount)));
        }

        info!("finalize_trade => buyer={}, seller={}, base={:?}, quote={:?}, base_amt={}, quote_amt={}",
            buyer, seller, base_asset, quote_asset, base_amount, quote_amount
        );

        // (B) => Wir sperren balances => Race Condition in-memory fix
        let mut guard = self.balances.lock().map_err(|_| DexError::Other("balances mutex poisoned".into()))?;

        // Buyer => locked quote
        {
            let buyer_map = guard.entry(buyer.to_string()).or_insert_with(HashMap::new);
            let bal_quote = buyer_map.entry(quote_asset.clone()).or_insert((0.0, 0.0));
            if bal_quote.0 < quote_amount {
                return Err(DexError::Other(format!("Not enough free quote for buyer={}", buyer)));
            }
            bal_quote.0 -= quote_amount;
            bal_quote.1 += quote_amount;
        }

        // Seller => locked base
        {
            let seller_map = guard.entry(seller.to_string()).or_insert_with(HashMap::new);
            let bal_base = seller_map.entry(base_asset.clone()).or_insert((0.0, 0.0));
            if bal_base.0 < base_amount {
                return Err(DexError::Other(format!("Not enough free base for seller={}", seller)));
            }
            bal_base.0 -= base_amount;
            bal_base.1 += base_amount;
        }

        // Fees => standard_fee_rate
        let fee_percent = self.fees_config.standard_fee_rate;
        self.apply_fees(buyer, &quote_asset, quote_amount, fee_percent);
        self.apply_fees(seller, &base_asset, base_amount, fee_percent);

        // Release => buyer kriegt base, seller kriegt quote
        {
            let buyer_map = guard.entry(buyer.to_string()).or_insert_with(HashMap::new);
            let bal_base = buyer_map.entry(base_asset.clone()).or_insert((0.0, 0.0));
            if bal_base.1 < base_amount {
                return Err(DexError::Other(format!("Mismatch locked base for buyer={}", buyer)));
            }
            bal_base.1 -= base_amount;
            bal_base.0 += base_amount;

            let seller_map = guard.entry(seller.to_string()).or_insert_with(HashMap::new);
            let bal_quote = seller_map.entry(quote_asset.clone()).or_insert((0.0, 0.0));
            if bal_quote.1 < quote_amount {
                return Err(DexError::Other(format!("Mismatch locked quote for seller={}", seller)));
            }
            bal_quote.1 -= quote_amount;
            bal_quote.0 += quote_amount;
        }

        drop(guard); // balances-Lock freigeben

        // DB => Retry
        let mut attempt = 0;
        while attempt < self.max_retries {
            attempt += 1;
            let locked_db = self.db.lock().map_err(|_| DexError::Other("DB lock broken".into()))?;
            let store_res = locked_db.store_struct("settlement/balances", &*(self.balances.lock().unwrap()));
            drop(locked_db);
            if let Err(e) = store_res {
                warn!("DB store attempt {} => error={:?} => backoff", attempt, e);
                std::thread::sleep(self.retry_backoff);
            } else {
                debug!("DB store of balances => success on attempt {}", attempt);
                break;
            }
        }
        Ok(())
    }

    fn finalize_atomic_swap(&mut self, swap_id: &str, swap: &mut AtomicSwap) -> Result<(), DexError> {
        let _lock = ENGINE_MUTEX.lock().map_err(|_| DexError::Other("engine mutex poisoned".into()))?;

        info!("finalize_atomic_swap => swap_id={}", swap_id);

        // 1) Negative checks => buyer_htlc.amount, seller_htlc.amount
        if swap.buyer_htlc.amount <= 0.0 {
            return Err(DexError::Other(format!("Invalid buyer_htlc.amount in swap {}", swap_id)));
        }
        if swap.seller_htlc.amount <= 0.0 {
            return Err(DexError::Other(format!("Invalid seller_htlc.amount in swap {}", swap_id)));
        }

        // 2) Timeout
        let now_sec = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
        if now_sec > swap.max_sign_time {
            warn!("AtomicSwap {} => TIMEOUT => do refund!", swap_id);
            swap.refund()?; // => evtl. Balances
            return Ok(());
        }

        // 3) check state
        if swap.state == SwapState::Cancelled || swap.state == SwapState::Refunded {
            return Err(DexError::Other(format!("AtomicSwap {} => invalid state={:?}", swap_id, swap.state)));
        }
        if swap.state == SwapState::Init {
            return Err(DexError::Other("Seller not redeemed => can't finalize swap".into()));
        }

        // 4) Fees => atomic_swap_fee_rate
        let fee_percent = self.fees_config.atomic_swap_fee_rate;
        self.apply_fees("buyer-of-swap", &swap.buyer_asset, swap.buyer_htlc.amount, fee_percent);
        self.apply_fees("seller-of-swap", &swap.seller_asset, swap.seller_htlc.amount, fee_percent);

        // 5) => Hier kein balances-lock-Freigabe => falls du wanted to update self.balances, tu es hier
        info!("AtomicSwap => final => buyer has {:?}, seller has {:?} => done, state={:?}",
              swap.seller_asset, swap.buyer_asset, swap.state);
        Ok(())
    }

    fn finalize_onchain_htlc(&mut self, htlc_id: &str, htlc: &mut OnchainHtlc) -> Result<(), DexError> {
        let _lock = ENGINE_MUTEX.lock().map_err(|_| DexError::Other("engine mutex poisoned".into()))?;

        info!("finalize_onchain_htlc => htlc_id={}", htlc_id);
        // => hier kein negativity check, da OnchainHtlc nicht storage of amounts?
        // => if needed, do partial lock/unlock of balances
        if htlc.redeemed {
            debug!("HTLC {} => redeemed => credited user => done", htlc_id);
        } else if htlc.refunded {
            debug!("HTLC {} => refunded => no off-chain distribution => done", htlc_id);
        } else {
            return Err(DexError::Other("HTLC not redeemed or refunded => can't finalize_onchain_htlc".into()));
        }
        Ok(())
    }
}

///////////////////////////////////////////////////////////
// SecuredSettlementEngine => Decorator
///////////////////////////////////////////////////////////
pub struct SecuredSettlementEngine<E: SettlementEngineTrait, S: SecurityValidator> {
    pub inner: E,
    pub validator: S,
}

impl<E: SettlementEngineTrait, S: SecurityValidator> SecuredSettlementEngine<E, S> {
    pub fn new(inner: E, validator: S) -> Self {
        Self { inner, validator }
    }
}

impl<E: SettlementEngineTrait, S: SecurityValidator> SettlementEngineTrait for SecuredSettlementEngine<E, S> {
    fn finalize_trade(
        &mut self,
        buyer: &str,
        seller: &str,
        base_asset: Asset,
        quote_asset: Asset,
        base_amount: f64,
        quote_amount: f64,
    ) -> Result<(), DexError> {
        let info_str = format!("Trade => buyer={}, seller={}, base={:?}, quote={:?}, amtB={}, amtQ={}",
                               buyer, seller, base_asset, quote_asset, base_amount, quote_amount);
        self.validator.validate_settlement(&info_str)?;
        self.inner.finalize_trade(buyer, seller, base_asset, quote_asset, base_amount, quote_amount)
    }

    fn finalize_atomic_swap(&mut self, swap_id: &str, swap: &mut AtomicSwap) -> Result<(), DexError> {
        let info_str = format!("AtomicSwap => ID={}", swap_id);
        self.validator.validate_settlement(&info_str)?;
        self.inner.finalize_atomic_swap(swap_id, swap)
    }

    fn finalize_onchain_htlc(&mut self, htlc_id: &str, htlc: &mut OnchainHtlc) -> Result<(), DexError> {
        let info_str = format!("OnChain-HTLC => ID={}", htlc_id);
        self.validator.validate_settlement(&info_str)?;
        self.inner.finalize_onchain_htlc(htlc_id, htlc)
    }
}
