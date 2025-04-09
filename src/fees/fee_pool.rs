///////////////////////////////////////////////////////////
// my_dex/src/fees/fee_pool.rs
///////////////////////////////////////////////////////////
//
// Hier wird Schritt 4) umgesetzt: Fees (Settlement-Engine + FeePool) 
// mit Sub-Aufteilung und Hintergrund-Task.
// 
// "FeePool" verwaltet:
//   - dev_pool
//   - nodes_pool
//   - total_fees (optional)
//   - recipients (fixe Empfänger, z.B. Founder) 
//
// Wir haben z.B. "add_fees(amount)", das die eingehenden Fees 
// in dev_pool und nodes_pool aufteilt. 
// 
// Die Verteilung an alle Empfänger geschieht durch 
// "distribute_dev_pool" bzw. "distribute_nodes_pool".
// Ein periodischer Task ("run_fee_distributor_task") ruft 
// z. B. "distribute_all" (dev + nodes) in einem definierten Intervall auf.

use std::sync::{Arc, Mutex};
use serde::{Serialize, Deserialize};
use tracing::{info, warn, debug};
use anyhow::Result;
use tokio::task::JoinHandle;
use tokio::time::{sleep, Duration};

use crate::error::DexError;
use crate::storage::db_layer::DexDB;
use crate::identity::accounts::{Account, AccountType};

/// Beschreibt einen Empfänger, der vom FeePool bedacht wird.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeeRecipient {
    pub user_id: String,
    pub fee_share_percent: f64,
}

/// Hauptzustand des FeePools.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FeePoolData {
    /// Optional: Bleibt hier, falls du den Gesamtpool 
    /// noch tracken willst.
    pub total_fees: f64,

    /// Pool für Dev/Erfinder/Geschäftsführung.
    pub dev_pool: f64,

    /// Pool für Fullnodes (oder \"Nodes\").
    pub nodes_pool: f64,

    /// Liste statischer Empfänger (Founder, Dev-Team, Partner).
    /// Fullnodes werden über \"auto_sync_fullnodes\" zugewiesen.
    pub recipients: Vec<FeeRecipient>,
}

/// Ein fester prozentualer Anteil, den alle Fullnodes zusammen 
/// an den Fees haben. Die Verteilung intern kann man aufsplitten 
/// in auto_sync_fullnodes().
const FULLNODES_POOL_PERCENT: f64 = 50.0;

/// HARDCODED: 30% (dev) / 70% (nodes) Splitting.
const DEV_PERCENT: f64 = 0.30;
const NODE_PERCENT: f64 = 0.70;

/// FeePool verwaltet sämtliche Fees und Empfänger.
#[derive(Debug, Clone)]
pub struct FeePool {
    db: Arc<Mutex<DexDB>>,
    pool_key: String,
}

impl FeePool {
    /// Erzeugt ein FeePool-Objekt, das in pool_key 
    /// (z. B. \"system_accounts/fee_pool\") persistiert.
    pub fn new(db: Arc<Mutex<DexDB>>, pool_key: &str) -> Self {
        Self {
            db,
            pool_key: pool_key.to_string(),
        }
    }

    /// Lädt den FeePool-Zustand oder erzeugt leeren, falls noch keiner existiert.
    fn load_fee_pool_data(&self) -> Result<FeePoolData, DexError> {
        let lock = self.db.lock().map_err(|_| DexError::Other("DB lock poisoned".into()))?;
        if let Some(fp) = lock.load_struct::<FeePoolData>(&self.pool_key)? {
            Ok(fp)
        } else {
            Ok(FeePoolData {
                total_fees: 0.0,
                dev_pool: 0.0,
                nodes_pool: 0.0,
                recipients: Vec::new(),
            })
        }
    }

    /// Speichert FeePoolData in DB.
    fn store_fee_pool_data(&self, data: &FeePoolData) -> Result<(), DexError> {
        let lock = self.db.lock().map_err(|_| DexError::Other("DB lock poisoned".into()))?;
        lock.store_struct(&self.pool_key, data)?;
        Ok(())
    }

    /// Addiert amount an Fees und splittet sie: 30% => dev_pool, 70% => nodes_pool.
    pub fn add_fees(&self, amount: f64) -> Result<(), DexError> {
        if amount <= 0.0 {
            return Err(DexError::Other(format!("fee amount <=0 => {amount}")));
        }
        let mut fp = self.load_fee_pool_data()?;

        // Optional: fp.total_fees += amount; (kann man belassen oder weglassen.)
        let dev_amt = amount * DEV_PERCENT;
        let node_amt = amount * NODE_PERCENT;

        fp.dev_pool += dev_amt;
        fp.nodes_pool += node_amt;

        self.store_fee_pool_data(&fp)?;
        debug!("add_fees({:.8}) => dev_pool += {:.8}, nodes_pool += {:.8}",
               amount, dev_amt, node_amt);
        Ok(())
    }

    /// Aktueller dev_pool-Betrag
    pub fn current_dev_pool(&self) -> Result<f64, DexError> {
        let fp = self.load_fee_pool_data()?;
        Ok(fp.dev_pool)
    }

    /// Aktueller nodes_pool-Betrag
    pub fn current_nodes_pool(&self) -> Result<f64, DexError> {
        let fp = self.load_fee_pool_data()?;
        Ok(fp.nodes_pool)
    }

    /// Listet alle recipients (exklusive Fullnodes).
    pub fn list_recipients(&self) -> Result<Vec<FeeRecipient>, DexError> {
        let fp = self.load_fee_pool_data()?;
        Ok(fp.recipients)
    }

    /// Upsert eines Empfängers. 
    /// Fullnodes -> auto_sync_fullnodes().
    pub fn upsert_recipient(&self, user_id: &str, percent: f64) -> Result<(), DexError> {
        if percent <= 0.0 {
            return Err(DexError::Other(format!("fee_share_percent must >0 => {percent}")));
        }
        let mut fp = self.load_fee_pool_data()?;
        if let Some(r) = fp.recipients.iter_mut().find(|r| r.user_id == user_id) {
            r.fee_share_percent = percent;
        } else {
            fp.recipients.push(FeeRecipient {
                user_id: user_id.to_string(),
                fee_share_percent: percent,
            });
        }
        self.store_fee_pool_data(&fp)?;
        info!("Upserted recipient => user_id={user_id}, newPercent={percent}");
        Ok(())
    }

    /// Entfernt Empfänger (keine Fullnodes).
    pub fn remove_recipient(&self, user_id: &str) -> Result<(), DexError> {
        let mut fp = self.load_fee_pool_data()?;
        let old_len = fp.recipients.len();
        fp.recipients.retain(|r| r.user_id != user_id);
        let new_len = fp.recipients.len();
        if new_len < old_len {
            self.store_fee_pool_data(&fp)?;
            info!("Removed fee recipient => user_id={user_id}");
        } else {
            warn!("No fee recipient found with user_id={user_id}");
        }
        Ok(())
    }

    /// Ermittelt alle Fullnode-Accounts und vergibt FULLNODES_POOL_PERCENT anteilig.
    /// D. h. wir entfernen alle Fullnodes aus recipients und setzen sie neu 
    /// mit fee_share_percent = (FULLNODES_POOL_PERCENT / #found).
    pub fn auto_sync_fullnodes(&self) -> Result<(), DexError> {
        let lock = self.db.lock().map_err(|_| DexError::Other("DB lock poisoned".into()))?;
        let all_keys = lock.list_prefix("accounts/");
        drop(lock);

        let mut fullnode_ids = Vec::new();
        for (k, _) in all_keys {
            let maybe_acc = self.db.lock().unwrap().load_struct::<Account>(&k)?;
            if let Some(acc) = maybe_acc {
                if acc.account_type == AccountType::Fullnode && acc.is_fee_pool_recipient {
                    fullnode_ids.push(acc.user_id.clone());
                }
            }
        }

        let mut fp = self.load_fee_pool_data()?;
        // Entferne vorhandene Fullnodes
        let old_len = fp.recipients.len();
        fp.recipients.retain(|r| !fullnode_ids.contains(&r.user_id));
        let removed = old_len - fp.recipients.len();
        if removed > 0 {
            debug!("Removed {removed} old Fullnode recipients");
        }

        if fullnode_ids.is_empty() {
            self.store_fee_pool_data(&fp)?;
            info!("auto_sync_fullnodes => no fullnodes => done");
            return Ok(());
        }
        let share_each = FULLNODES_POOL_PERCENT / (fullnode_ids.len() as f64);
        for f_id in fullnode_ids {
            fp.recipients.push(FeeRecipient {
                user_id: f_id,
                fee_share_percent: share_each,
            });
        }
        self.store_fee_pool_data(&fp)?;
        info!("auto_sync_fullnodes => assigned {FULLNODES_POOL_PERCENT}% among them => done");
        Ok(())
    }

    /// Verteilt dev_pool an alle NICHT-Fullnode recipients 
    /// (hier check per Summation) => dev_pool=0 afterwards.
    pub fn distribute_dev_pool(&self) -> Result<(), DexError> {
        let mut fp = self.load_fee_pool_data()?;
        let dev_total = fp.dev_pool;
        if dev_total <= 0.0 {
            debug!("distribute_dev_pool => dev_pool=0 => skip");
            return Ok(());
        }
        let dev_recipients: Vec<_> = fp.recipients
            .iter()
            .filter(|r| r.fee_share_percent < FULLNODES_POOL_PERCENT)
            .collect();
        let sum_perc: f64 = dev_recipients.iter().map(|r| r.fee_share_percent).sum();
        if sum_perc <= 0.0 {
            warn!("No dev recipients => dev_pool=0 => done");
            fp.dev_pool = 0.0;
            self.store_fee_pool_data(&fp)?;
            return Ok(());
        }
        for r in dev_recipients {
            let ratio = r.fee_share_percent / sum_perc;
            let portion = dev_total * ratio;
            self.credit_user_dex_balance(&r.user_id, portion)?;
            info!("DEV user={} => +{:.8} => ratio={:.2}%, dev_pool={:.8}",
                  r.user_id, portion, r.fee_share_percent, dev_total);
        }
        fp.dev_pool = 0.0;
        self.store_fee_pool_data(&fp)?;
        info!("dev_pool => 0 after distributing total={:.8}", dev_total);
        Ok(())
    }

    /// Verteilt nodes_pool auf Fullnode-Recipients => je auto_sync_fullnodes
    /// und setzt nodes_pool=0.
    pub fn distribute_nodes_pool(&self) -> Result<(), DexError> {
        // Erst Fullnodes updaten
        self.auto_sync_fullnodes()?;

        let mut fp = self.load_fee_pool_data()?;
        let node_total = fp.nodes_pool;
        if node_total <= 0.0 {
            debug!("nodes_pool=0 => skip");
            return Ok(());
        }
        // Fullnode => fee_share == FULLNODES_POOL_PERCENT / n => wir matchen 
        let fulls: Vec<_> = fp.recipients.iter()
            .filter(|r| (r.fee_share_percent - (FULLNODES_POOL_PERCENT / 1.0)).abs() < 0.0001 
                    || (r.fee_share_percent - FULLNODES_POOL_PERCENT).abs() < 0.0001)
            .collect();
        // Oder du scannst DB => Variation
        if fulls.is_empty() {
            warn!("No fullnode recipients => nodes_pool=0 => done");
            fp.nodes_pool = 0.0;
            self.store_fee_pool_data(&fp)?;
            return Ok(());
        }
        let count_fn = fulls.len() as f64;
        let portion_each = node_total / count_fn;
        for r in fulls {
            self.credit_user_dex_balance(&r.user_id, portion_each)?;
            info!("Fullnode user={} => portion={:.8} => from node_pool={:.8}", 
                  r.user_id, portion_each, node_total);
        }
        fp.nodes_pool = 0.0;
        self.store_fee_pool_data(&fp)?;
        info!("node_pool => 0 after distributing total={:.8}", node_total);
        Ok(())
    }

    /// Ruft distribute_dev_pool + distribute_nodes_pool auf, 
    /// um den gesamten \"dev_pool\" und \"nodes_pool\" zu verteilen.
    /// Falls du \"total_fees\" gesondert verteilen willst, 
    /// könntest du das hier ebenfalls tun.
    pub fn distribute_all(&self) -> Result<(), DexError> {
        self.distribute_dev_pool()?;
        self.distribute_nodes_pool()?;
        Ok(())
    }

    /// Bucht portion auf das Dex-Balance des erstbesten Wallets dieses Users.
    fn credit_user_dex_balance(&self, user_id: &str, portion: f64) -> Result<(), DexError> {
        if portion <= 0.0 { return Ok(()); }

        let lock = self.db.lock().map_err(|_| DexError::Other("DB lock poisoned".into()))?;
        let key = format!("accounts/{}", user_id);
        let maybe_acc = lock.load_struct::<Account>(&key)?;
        let acc = match maybe_acc {
            Some(a) => a,
            None => {
                warn!("credit_user_dex_balance => user={} not found => skip portion={}", user_id, portion);
                return Ok(());
            }
        };
        if acc.wallet_ids.is_empty() {
            warn!("User={} has no wallet => ignoring portion={:.8}", user_id, portion);
            return Ok(());
        }
        let w_id = &acc.wallet_ids[0];
        let wkey = format!("wallets/{}", w_id);
        let mut maybe_w = lock.load_struct::<crate::identity::wallet::WalletInfo>(&wkey)?;
        if let Some(mut w) = maybe_w {
            w.dex_balance += portion;
            lock.store_struct(&wkey, &w)?;
            info!("User={} => credited +{:.8} => wallet={}", user_id, portion, w.wallet_id);
        } else {
            warn!("Wallet={} for user={} not found => skipping portion", w_id, user_id);
        }
        Ok(())
    }
}

/// Startet eine Hintergrund-Task, die in einem festen Intervall 
/// (z. B. alle 60 Sekunden) fee_pool.distribute_all() aufruft.
/// Du kannst diese Task in main.rs oder wo immer du willst starten:
///
/// rust
/// let fee_pool = FeePool::new(db.clone(), "system_accounts/fee_pool");
/// fee_pool.start_fee_distribution_task(Duration::from_secs(60));
/// 
pub fn start_fee_distribution_task(fee_pool: FeePool, interval: Duration) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if let Err(e) = fee_pool.distribute_all() {
                warn!("Fee distribution error: {:?}", e);
            }
            sleep(interval).await;
        }
    })
}
