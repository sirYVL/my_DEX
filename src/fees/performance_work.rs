//////////////////////////////////////////////////////////
/// my_dex/src/fees/performance_work.rs
//////////////////////////////////////////////////////////
//
// 

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tracing::{info, warn, error};
use anyhow::{Result, anyhow};
use crate::error::DexError;
use crate::storage::db_layer::DexDB;
use crate::identity::accounts::{Account, AccountType};
use crate::fees::fee_pool::FeePool;

/// Definiert einen einzelnen Work-Nachweis: Node (user_id), Arbeitseinheiten, Zeitstempel etc.
#[derive(Debug, Clone)]
pub struct WorkEvent {
    pub node_id: String,
    pub timestamp: u64,           // UNIX-Sekunden
    pub work_units: u64,          // z. B. 5 Relay-Einheiten
    pub description: String,      // z. B. "Relayed 5 new orders"
}

/// Ein Eintrag in der DB: Aggregierter Score pro Node-ID seit letztem Reset
#[derive(Debug, Clone)]
pub struct NodeWorkScore {
    pub node_id: String,
    pub total_score: u64,
    pub last_update: u64,
}

/// Verwaltung der Work-Events und Work-Scores.
/// Hinterlegt in DexDB: "performance_work/score_{node_id}"
pub struct PerformanceWorkManager {
    db: Arc<Mutex<DexDB>>,
    // In-Memory-Puffer: NodeID -> NodeWorkScore
    in_memory_scores: Mutex<HashMap<String, NodeWorkScore>>,
}

impl PerformanceWorkManager {
    /// Erstellt einen Manager, der auf DexDB basiert.
    pub fn new(db: Arc<Mutex<DexDB>>) -> Self {
        Self {
            db,
            in_memory_scores: Mutex::new(HashMap::new()),
        }
    }

    /// F�gt einen WorkEvent hinzu und erh�ht den Score des betreffenden Nodes.
    pub fn record_work(&self, event: WorkEvent) -> Result<(), DexError> {
        let mut scores = self.in_memory_scores.lock().map_err(|_| DexError::Other("Poisoned lock".into()))?;

        let entry = scores.entry(event.node_id.clone()).or_insert_with(|| NodeWorkScore {
            node_id: event.node_id.clone(),
            total_score: 0,
            last_update: 0,
        });
        entry.total_score = entry.total_score.saturating_add(event.work_units);
        entry.last_update = event.timestamp;

        // (Optional) Man kann hier oder asynchron in die DB persistieren:
        self.store_node_score(entry.clone())?;

        info!("Leistungsbasiert: Node={} +{} work_units => total={}", entry.node_id, event.work_units, entry.total_score);
        Ok(())
    }

    /// L�dt den aggregierten Score einer Node aus der DB.
    pub fn load_node_score(&self, node_id: &str) -> Result<Option<NodeWorkScore>, DexError> {
        let key = format!("performance_work/score_{}", node_id);
        let lock = self.db.lock().map_err(|_| DexError::Other("DB lock poisoned".into()))?;
        let res = lock.load_struct::<NodeWorkScore>(&key)?;
        Ok(res)
    }

    /// Speichert einen NodeWorkScore in der DB.
    pub fn store_node_score(&self, score: NodeWorkScore) -> Result<(), DexError> {
        let key = format!("performance_work/score_{}", score.node_id);
        let lock = self.db.lock().map_err(|_| DexError::Other("DB lock poisoned".into()))?;
        lock.store_struct(&key, &score)?;
        Ok(())
    }

    /// Summiert alle Node-Scores (z. B. f�r die Fee-Verteilung).
    pub fn sum_all_scores(&self) -> Result<u64, DexError> {
        let lock = self.db.lock().map_err(|_| DexError::Other("DB lock poisoned".into()))?;
        let prefix = "performance_work/score_";

        let list = lock.list_structs_with_prefix::<NodeWorkScore>(prefix)?;
        let mut total: u64 = 0;
        for sc in list {
            total = total.saturating_add(sc.total_score);
        }
        Ok(total)
    }

    /// F�hrt eine periodische Abrechnung durch: 
    /// - Ermittelt total_score
    /// - Ermittelt, wer Fullnode-Account ist, und holt sich pro Node den Score
    /// - Rechnet anteilig vom FeePool (z. B. daily_fees) => sch�ttet an Dex-Balance aus
    pub fn distribute_rewards(&self, fee_pool: &FeePool) -> Result<(), DexError> {
        // 1) FeePool => wie viel ist im "globalen" daily_fees oder so?
        let daily_amount = fee_pool.current_daily_fees()?;
        if daily_amount <= 0.0 {
            info!("Keine Fees vorhanden => keine Verteilung");
            return Ok(());
        }

        // 2) Summe aller Scores:
        let total_score = self.sum_all_scores()?;
        if total_score == 0 {
            info!("Keine Work-Scores => keine Verteilung");
            return Ok(());
        }

        // 3) Verteile an alle Nodes (nur Fullnode?), proportional
        // => Man k�nnte AccountsManager hier heranziehen, 
        // => wir holen (node_id => account)
        let lock_db = self.db.lock().map_err(|_| DexError::Other("DB lock poisoned".into()))?;
        let prefix = "performance_work/score_";
        let scores = lock_db.list_structs_with_prefix::<NodeWorkScore>(prefix)?;

        for sc in scores {
            // Lad den Account => check account_type = Fullnode
            let acc_key = format!("accounts/{}", sc.node_id);
            let maybe_acc = lock_db.load_struct::<Account>(&acc_key)?;
            // Falls existiert und Fullnode
            if let Some(acc) = maybe_acc {
                if acc.account_type == AccountType::Fullnode {
                    let share_percent = sc.total_score as f64 / total_score as f64;
                    let node_reward = daily_amount * share_percent;
                    
                    // Gutschrift => an Dex-Balance => wir nehmen z. B. 1. Wallet des Accounts
                    let first_wallet = acc.wallet_ids.first().cloned();
                    if let Some(wid) = first_wallet {
                        // Add Dex-Balance
                        self.wallet_manager.add_dex_balance(&wid, node_reward)?;
                        info!("Leistungsbasiert: Node={} (acc={}) kriegt {} aus daily_fees ({} total, share={:.2}%)",
                            sc.node_id, acc.user_id, node_reward, daily_amount, share_percent*100.0
                        );
                    } else {
                        warn!("Node={} => kein Wallet in diesem Account => kann Rewards nicht auszahlen", sc.node_id);
                    }
                }
            }
        }

        // 4) FeePool => daily_fees auf 0 => da wir verteilt haben
        fee_pool.reset_daily_fees()?;
        info!("Leistungsbasiert: daily_fees={} wurden anteilig an {} Nodes verteilt", daily_amount, scores.len());
        Ok(())
    }
}
