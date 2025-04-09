///////////////////////////////////////////////////////////
// my_dex/src/trading/trading_logic.rs
///////////////////////////////////////////////////////////
//
// Dieses Modul implementiert den Kernablauf von Kauf-/Verkaufsorders
// gemäß folgendem Ablauf:
//
//  1) Order-Placement:
//     - User wählt coin_to_sell, coin_to_buy, amount, price, side.
//     - Prüfe Dex-Balance (free) auf ausreichendes Guthaben.
//     - Lock die Menge => "locked_funds".
//     - Lege die Order im CRDT- oder LimitOrderbook an (verteilte Gossip-Logik).
//
//  2) Orderbuch-Anzeige:
//     - Alle offenen Orders ermitteln und zurückgeben (z. B. pro Market).
//     - UI/CLI kann daraus ein Orderbuch rendern.
//
//  3) Matching:
//     - MatchingEngine verknüpft Buy- und Sell-Orders; erstellt Trades.
//     - SettlementEngine => zieht Fee ab => Fee-Pool
//       (nur hier in der Settlement-Engine; NICHT in trading_logic!).
//
//  4) Gebühr-Vorschau (UI/CLI):
//     - "voraussichtliche Fee = volume * fee_rate" berechnen
//     - dem User anzeigen.
//
// Hinweis: Wir haben die Gebührenerhebung aus dieser Datei entfernt. 
//   D. h. im `run_matching()` findet keine Fee-Berechnung mehr statt. 
//   Stattdessen erfolgt das Einziehen der Fees ausschließlich in 
//   `advanced_settlement::SettlementEngineTrait::finalize_trade()`.

use std::sync::{Arc, Mutex};
use anyhow::Result;
use tracing::{info, debug, warn};
use crate::error::DexError;

// Accounts/Wallet (internes Dex-Guthaben):
use crate::identity::accounts::AccountsManager;
use crate::identity::wallet::WalletManager;
// CRDT/Limit-Orderbook:
use crate::dex_logic::orders::{OrderSide, Order as DexOrder}; 
use crate::dex_logic::limit_orderbook::{LimitOrderBook, LimitOrder, Side as LobSide};
use crate::dex_logic::crdt_orderbook::CrdtOrderBook; // Falls CRDT-Variante
// MatchingEngine:
use crate::matching_engine::MatchingEngine; // Beispiel: Müsste existieren
// SettlementEngine:
use crate::settlement::advanced_settlement::SettlementEngineTrait;
// FeePool behalten wir nur, falls wir z. B. für UI/CLI den fee_pool-Stand 
// abfragen wollen (oder get_fee_preview() Rechenhilfe). 
// Die eigentliche Fee-Buchung erfolgt NICHT mehr hier!
use crate::fees::fee_pool::FeePool;

/// Beschreibt eine eingehende "TradeOrderRequest", wie er vom User kommt:
pub struct TradeOrderRequest {
    pub user_id: String,
    pub coin_to_sell: String,
    pub coin_to_buy: String,
    pub amount: f64,
    pub price: f64,
    pub side: OrderSide, // Buy oder Sell
}

/// Der "TradingService" führt die vier Hauptschritte aus (ohne Fees).
pub struct TradingService {
    /// Manager, um Accounts (Dex-Balances) abzufragen & zu locken
    pub accounts_mgr: Arc<AccountsManager>,
    /// LimitOrderbook (oder CRDT-Orderbook)
    pub limit_orderbook: Arc<Mutex<LimitOrderBook>>,
    /// MatchingEngine => verknüpft Orders zu Trades
    pub matching_engine: Arc<Mutex<MatchingEngine>>,
    /// SettlementEngine => finalisiert Trades => Fee wird NUR hier eingezogen
    pub settlement_engine: Arc<Mutex<dyn SettlementEngineTrait>>,
    /// FeePool, falls wir z. B. den aktuellen Stand einsehen wollen
    /// (Die eigentliche Buchung passiert in `finalize_trade()`, nicht hier!)
    pub fee_pool: Arc<FeePool>,
    /// Default-Fee-Rate => nur für get_fee_preview()
    pub default_fee_rate: f64,
}

impl TradingService {
    /// Konstruktor: Erzeugt eine TradingService-Instanz.
    pub fn new(
        accounts_mgr: Arc<AccountsManager>,
        limit_orderbook: Arc<Mutex<LimitOrderBook>>,
        matching_engine: Arc<Mutex<MatchingEngine>>,
        settlement_engine: Arc<Mutex<dyn SettlementEngineTrait>>,
        fee_pool: Arc<FeePool>,
        default_fee_rate: f64,
    ) -> Self {
        TradingService {
            accounts_mgr,
            limit_orderbook,
            matching_engine,
            settlement_engine,
            fee_pool,
            default_fee_rate,
        }
    }

    /// 1) Order-Placement (Lock Dex-Funds, dann ins Orderbuch).
    pub fn place_order(&self, req: &TradeOrderRequest) -> Result<(), DexError> {
        // Dex-Balance-Check
        let free_bal = self
            .accounts_mgr
            .check_free_balance(&req.user_id, &req.coin_to_sell)?;
        if free_bal < req.amount {
            return Err(DexError::Other(format!(
                "Nicht genug Guthaben: user={}, coin={}, free={}, needed={}",
                req.user_id, req.coin_to_sell, free_bal, req.amount
            )));
        }

        // Lock Dex Funds
        self.accounts_mgr
            .lock_funds(&req.user_id, &req.coin_to_sell, req.amount)?;

        // Erzeuge LimitOrder => side konvertieren
        let side = match req.side {
            OrderSide::Buy => LobSide::Buy,
            OrderSide::Sell => LobSide::Sell,
        };
        let new_lob_order = LimitOrder {
            order_id: format!("{}-{}", req.user_id, nanoid::nanoid!()),
            side,
            price: req.price,
            quantity: req.amount,
            user_id: req.user_id.clone(),
        };

        // Ins Orderbook
        {
            let mut lob = self.limit_orderbook.lock().unwrap();
            lob.insert_limit_order(new_lob_order);
        }

        info!(
            "place_order OK => user={}, side={:?}, amount={}, price={}",
            req.user_id, req.side, req.amount, req.price
        );
        Ok(())
    }

    /// 2) Orderbuch-Anzeige: wir sammeln ALLE Orders aus der LimitOrderBook-Struktur
    pub fn list_open_orders(&self) -> Vec<LimitOrder> {
        let lob = self.limit_orderbook.lock().unwrap();
        let mut all_orders = Vec::new();

        // Buy-MAP
        for (_price_key, price_level) in &lob.buy_map {
            for ord in &price_level.orders {
                all_orders.push(ord.clone());
            }
        }
        // Sell-MAP
        for (_price_key, price_level) in &lob.sell_map {
            for ord in &price_level.orders {
                all_orders.push(ord.clone());
            }
        }
        all_orders
    }

    /// 3) Matching: Rufen MatchingEngine => generiert Trades => rufen SettlementEngine => Fees etc. (NUR dort)
    ///
    /// ACHTUNG: KEINE Fee-Berechnung hier! Nur `finalize_trade()` in advanced_settlement
    pub fn run_matching(&self) -> Result<(), DexError> {
        let mut eng = self.matching_engine.lock().unwrap();
        let trades = eng.match_orders()?;

        let mut settle_eng = self.settlement_engine.lock().unwrap();
        for tr in trades {
            // KEINE Fee-Berechnung mehr hier. 
            // Nur finalize_trade => SettlementEngine => fees 
            settle_eng.finalize_trade(
                &tr.buyer_id,
                &tr.seller_id,
                tr.base_asset.clone(),
                tr.quote_asset.clone(),
                tr.base_amount,
                tr.quote_amount
            )?;
        }

        info!("run_matching => found {} trades => Settlement done", trades.len());
        Ok(())
    }

    /// 4) Gebühr-Vorschau: z. B. "Mögliche Fee = amount * default_fee_rate"
    ///    NICHT real einziehen => NUR als Info.
    pub fn get_fee_preview(&self, amount: f64) -> f64 {
        amount * self.default_fee_rate
    }
}
