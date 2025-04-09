///////////////////////////////////////////////////////////
// my_dex/src/matching_engine.rs
///////////////////////////////////////////////////////////
//
//  1) Order-Datenstrukturen & Enums:
//     - OrderType (Market, Limit, Stop, StopLimit)
//     - OrderSide (Buy, Sell)
//     - OrderStatus (Open, PartiallyFilled, Filled, Cancelled)
//     - Order (id, user, timestamp, side, order_type, quantity, filled, status)
//
//  2) Gebührenberechnung (FeeDistribution, FeeOutput, calculate_fee)
//
//  3) LimitOrderBook:
//     - add_order(...)
//     - match_orders(...) (führt Sortierung und Matching durch)
//
//  4) MatchingEngine (vereinigt mit Snippet-Code):
//     - new(...) => Erstellt MatchingEngine mit SecuredSettlement + optionalem GlobalSecurity
//     - place_order(...)
//     - match_orders(...) => Security-Audit (global_sec) + Matching
//     - process_trades(...) => SecurityValidate, Settlement, Audit-Log
//     - ring_sign_demo(...) => Beispielhafte Ring-Signatur mit global_sec
//     - check_expired_time_limited_orders(...) => Time-Limited Orders
//
//  5) SecurityValidator & Settlement-Integration
//
//  6) AtomicSwap & HTLC-Logik
//     - HTLC, AtomicSwap, Redeem/Refund
//
//  7) Demo-Funktionen:
//     - demo_matching_engine() => Beispiel-Orders platzieren & verarbeiten,
//       AtomicSwap-Demo.
///////////////////////////////////////////////////////////

use anyhow::{Result, anyhow};
use std::collections::{HashMap, VecDeque};
use std::cmp::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::{Arc, Mutex};

use tracing::{info, debug, warn, error};
use crate::error::DexError;
use crate::crdt_logic::Order;
use crate::metrics::ORDER_COUNT;
use crate::security::security_validator::{SecurityValidator, AdvancedSecurityValidator};
use crate::security::global_security_facade::GlobalSecuritySystem; // Neu für global_sec
use crate::settlement::secured_settlement::{
    SettlementEngineTrait,
    SettlementEngine,
    SecuredSettlementEngine
};
use crate::logging::enhanced_logging::{log_error, write_audit_log};

// Falls Sie das Modul time_limited_orders eingebunden haben
use crate::dex_logic::time_limited_orders::{
    TimeLimitedOrderManager, TimeLimitedOrderSide, TimeLimitedOrderType,
    TimeLimitedOrder, TimeLimitedStatus,
};

// ─────────────────────────────────────────────────────────
// Order-Typen (Market, Limit, etc.) + Status
// ─────────────────────────────────────────────────────────
#[derive(Clone, Debug, PartialEq)]
pub enum OrderType {
    Market,
    Limit(f64),
    Stop(f64),
    StopLimit { stop: f64, limit: f64 },
}

#[derive(Clone, Debug, PartialEq)]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Clone, Debug)]
pub enum OrderStatus {
    Open,
    PartiallyFilled,
    Filled,
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct OrderData {
    pub id: String,
    pub user_id: String,
    pub timestamp: u64,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub quantity: f64,
    pub filled: f64,
    pub status: OrderStatus,

    // Neu: Felder für Signatur (Beispiel)
    pub signature: Option<Vec<u8>>,
    pub public_key: Option<Vec<u8>>,
}

impl OrderData {
    pub fn new(
        id: &str,
        user_id: &str,
        side: OrderSide,
        order_type: OrderType,
        quantity: f64,
        timestamp: u64
    ) -> Self {
        Self {
            id: id.to_string(),
            user_id: user_id.to_string(),
            timestamp,
            side,
            order_type,
            quantity,
            filled: 0.0,
            status: OrderStatus::Open,
            signature: None,
            public_key: None,
        }
    }

    pub fn remaining(&self) -> f64 {
        self.quantity - self.filled
    }

    pub fn fill(&mut self, amount: f64) {
        self.filled += amount;
        if self.filled >= self.quantity {
            self.status = OrderStatus::Filled;
        } else {
            self.status = OrderStatus::PartiallyFilled;
        }
    }

    // Neu: Dummy-Signatur-Prüfung
    pub fn verify_signature(&self) -> bool {
        if let (Some(sig), Some(pk)) = (&self.signature, &self.public_key) {
            // In echter Produktion => ed25519_dalek usw.
            // Hier nur: Wenn beides nicht leer => "valid"
            !sig.is_empty() && !pk.is_empty()
        } else {
            false
        }
    }
}

// ─────────────────────────────────────────────────────────
// Gebühr-Logik
// ─────────────────────────────────────────────────────────
#[derive(Clone, Debug)]
pub struct FeeDistribution {
    pub founder_percent: f64,
    pub dev_percent: f64,
    pub node_percent: f64,
}

impl FeeDistribution {
    pub fn new() -> Self {
        Self {
            founder_percent: 0.5,
            dev_percent: 0.3,
            node_percent: 0.2,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FeeOutput {
    pub founder_fee: f64,
    pub dev_fee: f64,
    pub node_fee: f64,
}

pub fn calculate_fee(total_fee: f64, distribution: &FeeDistribution) -> FeeOutput {
    FeeOutput {
        founder_fee: total_fee * distribution.founder_percent,
        dev_fee: total_fee * distribution.dev_percent,
        node_fee: total_fee * distribution.node_percent,
    }
}

// ─────────────────────────────────────────────────────────
// Limit Order Book
// ─────────────────────────────────────────────────────────
#[derive(Clone, Debug)]
pub struct LimitOrder {
    pub order: OrderData,
}

#[derive(Clone, Debug)]
pub struct LimitOrderBook {
    pub buy_orders: VecDeque<LimitOrder>,
    pub sell_orders: VecDeque<LimitOrder>,
}

impl LimitOrderBook {
    pub fn new() -> Self {
        Self {
            buy_orders: VecDeque::new(),
            sell_orders: VecDeque::new(),
        }
    }
    
    /// NEU: Anstelle des reinen "Warn" geben wir ein Result zurück,
    /// falls Signatur oder Menge ungültig.
    pub fn add_order(&mut self, order: OrderData) -> Result<(), DexError> {
        // 1) check quantity
        if order.quantity <= 0.0 {
            return Err(DexError::Other("Quantity <= 0 => invalid".into()));
        }
        // 2) sign
        if !order.verify_signature() {
            // wir loggen + return Err, damit der aufrufende Code es mitkriegt
            warn!("LimitOrderBook => add_order: Ungültige Signatur => abgelehnt, ID={}", order.id);
            return Err(DexError::Other("Ungültige Order-Signatur".into()));
        }
        // => insertion
        let lo = LimitOrder { order };
        match lo.order.side {
            OrderSide::Buy => self.buy_orders.push_back(lo),
            OrderSide::Sell => self.sell_orders.push_back(lo),
        }
        Ok(())
    }
    
    pub fn sort_orders(&mut self) {
        self.buy_orders
            .make_contiguous()
            .sort_by(|a, b| compare_orders(&a.order, &b.order, true));
        self.sell_orders
            .make_contiguous()
            .sort_by(|a, b| compare_orders(&a.order, &b.order, false));
    }
    
    pub fn match_orders(&mut self) -> Vec<(String, String, f64, f64)> {
        self.sort_orders();
        let mut trades = Vec::new();
        
        while let (Some(buy_lo), Some(sell_lo)) = (self.buy_orders.front_mut(), self.sell_orders.front_mut()) {
            let buy_order = &buy_lo.order;
            let sell_order = &sell_lo.order;
            
            // Abbruch, wenn Price nicht matcht
            if !price_match(buy_order, sell_order) {
                break;
            }
            
            // fill
            let fill_qty = buy_order.remaining().min(sell_order.remaining());
            let trade_price = match (&buy_order.order_type, &sell_order.order_type) {
                (OrderType::Market, OrderType::Market) => 0.0,
                (OrderType::Market, OrderType::Limit(p)) => *p,
                (OrderType::Limit(p), OrderType::Market) => *p,
                (OrderType::Limit(p1), OrderType::Limit(p2)) => (*p1 + *p2) / 2.0,
                (OrderType::Stop(_), _) | (_, OrderType::Stop(_)) => {
                    // Bei Stop in einer reinrassigen LimitEngine => default
                    // In echter Prod => extra Logik
                    0.0
                }
                (OrderType::StopLimit{..}, OrderType::StopLimit{..}) => {
                    // s. o. 
                    0.0
                }
            };

            {
                let buy_mut = &mut self.buy_orders.front_mut().unwrap().order;
                let sell_mut = &mut self.sell_orders.front_mut().unwrap().order;
                buy_mut.fill(fill_qty);
                sell_mut.fill(fill_qty);
            }

            trades.push((buy_order.id.clone(), sell_order.id.clone(), fill_qty, trade_price));

            // ggf. remove front if filled
            if self.buy_orders.front().unwrap().order.status == OrderStatus::Filled {
                self.buy_orders.pop_front();
            }
            if self.sell_orders.front().unwrap().order.status == OrderStatus::Filled {
                self.sell_orders.pop_front();
            }
        }
        trades
    }
}

fn compare_orders(a: &OrderData, b: &OrderData, is_buy: bool) -> Ordering {
    let a_market = matches!(a.order_type, OrderType::Market);
    let b_market = matches!(b.order_type, OrderType::Market);

    // Market-Orders zuerst
    if a_market && !b_market {
        return Ordering::Less;
    }
    if !a_market && b_market {
        return Ordering::Greater;
    }

    let price_a = order_price(a, is_buy);
    let price_b = order_price(b, is_buy);

    // Buy => absteigend sortieren, Sell => aufsteigend
    if is_buy {
        price_b.partial_cmp(&price_a).unwrap_or(Ordering::Equal)
    } else {
        price_a.partial_cmp(&price_b).unwrap_or(Ordering::Equal)
    }
}

fn order_price(o: &OrderData, is_buy: bool) -> f64 {
    match o.order_type {
        OrderType::Limit(px) => px,
        OrderType::Stop(px) => px,
        OrderType::StopLimit { stop: _, limit: px } => px,
        OrderType::Market => {
            if is_buy { f64::MAX } else { 0.0 }
        }
    }
}

fn price_match(buy: &OrderData, sell: &OrderData) -> bool {
    match (&buy.order_type, &sell.order_type) {
        (OrderType::Market, _) | (_, OrderType::Market) => true,
        (OrderType::Limit(pb), OrderType::Limit(ps)) => pb >= ps,
        (OrderType::Stop(pb),  OrderType::Stop(ps))  => pb >= ps,
        (OrderType::StopLimit{stop:_, limit:pb}, OrderType::StopLimit{stop:_, limit:ps}) => pb >= ps,
        _ => false,
    }
}

// ─────────────────────────────────────────────────────────
// MatchingEngine
// ─────────────────────────────────────────────────────────
pub struct MatchingEngine {
    pub order_book: LimitOrderBook,

    // SettlementEngine
    pub settlement: Box<dyn SettlementEngineTrait>,

    // AtomicSwaps
    pub swaps: Vec<AtomicSwap>,

    // Altes SecurityValidator
    pub advanced_security: Box<dyn SecurityValidator>,

    // TimeLimited Orders
    pub time_limited_manager: Option<TimeLimitedOrderManager>,

    // NEU: Optionales globales Security-System
    pub global_sec: Option<Arc<Mutex<GlobalSecuritySystem>>>,
}

impl MatchingEngine {
    /// Beispiel-Konstruktor ohne globale Security
    pub fn new() -> Self {
        let base_settlement = SettlementEngine::new();
        let secured_settlement = SecuredSettlementEngine::new(base_settlement, AdvancedSecurityValidator::new());
        Self {
            order_book: LimitOrderBook::new(),
            settlement: Box::new(secured_settlement),
            swaps: Vec::new(),
            advanced_security: Box::new(AdvancedSecurityValidator::new()),
            time_limited_manager: None,
            global_sec: None,
        }
    }

    /// Neuer Konstruktor mit optionalem GlobalSecuritySystem
    pub fn new_with_global_security(global_sec: Option<Arc<Mutex<GlobalSecuritySystem>>>) -> Self {
        let mut engine = Self::new();
        engine.global_sec = global_sec;
        engine
    }

    pub fn with_time_limited_manager(mut self, manager: TimeLimitedOrderManager) -> Self {
        self.time_limited_manager = Some(manager);
        self
    }

    /// Order platzieren (nun mit Checks):
    /// - Wir prüfen quantity
    /// - Wir übergeben an LimitOrderBook => signatur => Fehler, wenn invalid
    pub fn place_order(&mut self, order: OrderData) -> Result<(), DexError> {
        if order.quantity <= 0.0 {
            return Err(DexError::Other("Order quantity <= 0 => invalid".into()));
        }
        self.order_book.add_order(order)?;
        Ok(())
    }

    /// Vereinte Variante von match_orders():
    /// - Ruft ggf. Security Audit über global_sec auf
    /// - Führt das eigentliche Matching (bisheriger Code) durch
    /// - Liefert Liste an Trades zurück
    pub fn match_orders(&mut self) -> Result<Vec<(String, String, f64, f64)>, DexError> {
        // Falls global_sec vorhanden => z.B. Rate Limit / Audit
        if let Some(ref sec_arc) = self.global_sec {
            let sec = sec_arc.lock().unwrap();
            sec.audit_event("MatchingEngine => start match_orders");
        }

        // Dann reguläre Matching-Logik
        let trades = self.order_book.match_orders();
        Ok(trades)
    }

    /// Prozessiert die Trades => Security-Check, Settlement, Fees, Audit-Log
    pub fn process_trades(&mut self) -> Result<(), DexError> {
        // Time-Limited abgelaufene Orders
        if let Some(ref mut manager) = self.time_limited_manager {
            if let Err(e) = manager.check_and_handle_expired(&mut self.order_book) {
                warn!("Fehler bei check_and_handle_expired: {:?}", e);
            }
        }

        let trades = self.match_orders()?;
        for (buy_id, sell_id, qty, price) in trades {
            let trade_info = format!("Buy:{}; Sell:{}; Qty:{}; Price:{}", buy_id, sell_id, qty, price);

            debug!("Validiere Trade mit AdvancedSecurityValidator: {}", trade_info);
            if let Err(e) = self.advanced_security.validate_trade(&trade_info) {
                log_error(e);
                return Err(DexError::Other("Trade-Sicherheitsvalidierung fehlgeschlagen".into()));
            }

            let fee_total = qty * price * 0.001;
            let fee_output = calculate_fee(fee_total, &FeeDistribution::new());
            debug!("Trade => buy={}, sell={}, px={}, qty={}, fees={:?}",
                   buy_id, sell_id, price, qty, fee_output);

            if let Err(e) = self.settlement.finalize_trade(
                "buyer_id",
                "seller_id",
                "BTC",
                "USDT",
                qty,
                qty * price
            ) {
                log_error(e);
                return Err(DexError::Other("Settlement-Validierung fehlgeschlagen".into()));
            }

            write_audit_log(&format!(
                "Trade finalisiert: Buy:{}; Sell:{}; Qty:{}; Price:{}",
                buy_id, sell_id, qty, price
            ));
        }
        Ok(())
    }

    /// Explizit abgelaufene Time-Limited Orders prüfen (optional)
    pub fn check_expired_time_limited_orders(&mut self) -> Result<(), DexError> {
        if let Some(ref mut manager) = self.time_limited_manager {
            manager.check_and_handle_expired(&mut self.order_book)?;
        }
        Ok(())
    }

    /// Ring-Sign-Demo
    pub fn ring_sign_demo(&self, data: &[u8]) -> Result<Vec<u8>, DexError> {
        if let Some(ref sec_arc) = self.global_sec {
            let sec = sec_arc.lock().unwrap();
            let sig = sec.ring_sign_data(data)?;
            return Ok(sig);
        }
        Err(DexError::Other("No GlobalSecuritySystem set".into()))
    }
}

// ─────────────────────────────────────────────────────────
// SecurityValidator => trade-check
// ─────────────────────────────────────────────────────────
impl AdvancedSecurityValidator {
    pub fn validate_trade(&self, trade_info: &str) -> Result<(), DexError> {
        debug!("AdvancedSecurityValidator => trade check: {}", trade_info);
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────
// AtomicSwap etc.
// ─────────────────────────────────────────────────────────
#[derive(Clone, Debug, PartialEq)]
pub enum SwapState {
    Init,
    SellerRedeemed,
    BuyerRedeemed,
    Refunded,
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct HTLC {
    pub chain: String,
    pub amount: f64,
    pub hashlock: [u8; 32],
    pub timelock: u64,
    pub redeemed: bool,
    pub refunded: bool,
}

impl HTLC {
    pub fn new(chain: &str, amount: f64, hashlock: [u8; 32], timelock: u64) -> Self {
        Self {
            chain: chain.to_string(),
            amount,
            hashlock,
            timelock,
            redeemed: false,
            refunded: false,
        }
    }

    pub fn redeem(&mut self, preimage: &[u8]) -> Result<(), DexError> {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(preimage);
        let result = hasher.finalize();
        if result[..] != self.hashlock[..] {
            return Err(DexError::Other("Hashlock mismatch".into()));
        }
        if self.redeemed {
            return Err(DexError::Other("HTLC bereits eingelöst".into()));
        }
        self.redeemed = true;
        Ok(())
    }

    pub fn refund(&mut self, current_time: u64) -> Result<(), DexError> {
        if current_time < self.timelock {
            return Err(DexError::Other("HTLC noch nicht abgelaufen".into()));
        }
        if self.refunded {
            return Err(DexError::Other("HTLC bereits zurückerstattet".into()));
        }
        self.refunded = true;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct AtomicSwap {
    pub buyer_htlc: HTLC,
    pub seller_htlc: HTLC,
    pub state: SwapState,
    pub preimage: Option<Vec<u8>>,
}

impl AtomicSwap {
    pub fn new(buyer_htlc: HTLC, seller_htlc: HTLC) -> Self {
        Self {
            buyer_htlc,
            seller_htlc,
            state: SwapState::Init,
            preimage: None,
        }
    }

    pub fn seller_redeem(&mut self, preimage: &[u8]) -> Result<(), DexError> {
        if self.state != SwapState::Init {
            return Err(DexError::Other("Swap nicht im Init-Status".into()));
        }
        self.buyer_htlc.redeem(preimage)?;
        self.preimage = Some(preimage.to_vec());
        self.state = SwapState::SellerRedeemed;
        Ok(())
    }

    pub fn buyer_redeem(&mut self) -> Result<(), DexError> {
        if self.state != SwapState::SellerRedeemed {
            return Err(DexError::Other("Seller hat noch nicht eingelöst".into()));
        }
        if let Some(ref pre) = self.preimage {
            self.seller_htlc.redeem(pre)?;
            self.state = SwapState::BuyerRedeemed;
            Ok(())
        } else {
            Err(DexError::Other("Kein Preimage vorhanden".into()))
        }
    }

    pub fn refund(&mut self, current_time: u64) -> Result<(), DexError> {
        if self.state == SwapState::BuyerRedeemed {
            return Err(DexError::Other("Swap bereits abgeschlossen".into()));
        }
        self.buyer_htlc.refund(current_time)?;
        self.seller_htlc.refund(current_time)?;
        self.state = SwapState::Refunded;
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────
// Demo
// ─────────────────────────────────────────────────────────
#[allow(dead_code)]
pub fn demo_matching_engine() -> Result<(), DexError> {
    let mut engine = MatchingEngine::new();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

    // Beispiel-Orders
    let mut order1 = OrderData {
        id: "o1".to_string(),
        user_id: "Alice".to_string(),
        side: OrderSide::Buy,
        order_type: OrderType::Limit(100.0),
        quantity: 10.0,
        timestamp: now,
        filled: 0.0,
        status: OrderStatus::Open,
        signature: None,
        public_key: None,
    };
    let mut order2 = OrderData {
        id: "o2".to_string(),
        user_id: "Bob".to_string(),
        side: OrderSide::Sell,
        order_type: OrderType::Limit(99.0),
        quantity: 5.0,
        timestamp: now,
        filled: 0.0,
        status: OrderStatus::Open,
        signature: None,
        public_key: None,
    };

    // (Demo) sign them
    // In echter Prod => sign with ed25519, etc.
    order1.signature = Some(vec![1,2,3]);
    order1.public_key = Some(vec![9,9,9]);
    order2.signature = Some(vec![1,2,3]);
    order2.public_key = Some(vec![8,8,8]);

    // Insert
    engine.place_order(order1)?;
    engine.place_order(order2)?;

    // Check trades
    let trades = engine.match_orders()?;
    debug!("Vor process_trades => Trades={:?}", trades);

    // finalize trades
    engine.process_trades()?;

    // AtomicSwap
    let hashlock = [0u8; 32]; // real => hash of preimage
    let buyer_htlc = HTLC::new("BTC", 0.1, hashlock, now + 3600);
    let seller_htlc = HTLC::new("LTC", 10.0, hashlock, now + 1800);
    let mut swap = AtomicSwap::new(buyer_htlc, seller_htlc);

    let preimage = b"secret";
    swap.seller_redeem(preimage)?;
    swap.buyer_redeem()?;

    engine.swaps.push(swap);

    Ok(())
}
