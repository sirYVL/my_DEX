////////////////////////////////////////////////////////////
// my_dex/src/fees/system_accounts.rs
////////////////////////////////////////////////////////////
//
// Dieses Modul implementiert System-Accounts (Erfinder, Entwickler usw.),
// deren Fee-Anteil zum globalen Fee-Pool beitragen kann. 
//
// Die Accounts werden in der DexDB unter Keys wie "system_account/<account_id>" gespeichert.
// Jeder Account kann eine Rolle, eine Wallet-Adresse, einen prozentualen Fee-Anteil und 
// einen Aktivstatus haben. 
//
// Hauptfunktionen:
//   - add_system_account(): legt einen neuen Account an
//   - set_account_inactive(): setzt is_active=false
//   - list_system_accounts(): listet alle (aktiv + inaktiv)
//   - total_active_share(): ermittelt die Summe aller fee_share_percent aktiver Accounts
//   - active_accounts(): liefert nur aktive Accounts
//   - distribute_fees(): verteilt einen Anteil an Fees an diese aktiven System-Accounts
//
////////////////////////////////////////////////////////////

use serde::{Serialize, Deserialize};
use anyhow::Result;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use tracing::{info, debug, warn};

use crate::error::DexError;
use crate::storage::replicated_db_layer::DexDB;

/// Rolle eines System-Accounts, z. B. Inventor, Developer, Founder etc.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SystemAccountRole {
    Inventor,
    Developer,
    Founder,
    Other(String),
}

/// Stellt einen Eintrag für einen System-Account dar:
///  - account_id: z. B. "dev/marcel"
///  - role: z. B. SystemAccountRole::Developer
///  - wallet_address: On-Chain-Adresse, an die man ggf. Fees auszahlen kann
///  - fee_share_percent: Anteil an den System-Fees (z. B. 0.05 = 5%)
///  - is_active: Ob dieser Account aktuell berücksichtigt wird
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SystemAccount {
    pub account_id: String,
    pub role: SystemAccountRole,
    pub wallet_address: String,
    pub fee_share_percent: f64,
    pub is_active: bool,
}

/// Der SystemAccountsManager verwaltet das Anlegen, Ändern und Auflisten
/// von System-Accounts in der DexDB.
pub struct SystemAccountsManager {
    db: Arc<Mutex<DexDB>>,
}

impl SystemAccountsManager {
    /// Erzeugt einen neuen SystemAccountsManager mit Verweis auf DexDB.
    pub fn new(db: Arc<Mutex<DexDB>>) -> Self {
        Self { db }
    }

    /// Fügt einen neuen System-Account hinzu und speichert ihn in der DB.
    /// Wenn schon vorhanden => Fehler.
    /// fee_share_percent darf z. B. max. 0.20 sein (rein exemplarisch).
    pub fn add_system_account(
        &self,
        account_id: &str,
        role: SystemAccountRole,
        wallet_address: &str,
        fee_share_percent: f64,
    ) -> Result<(), DexError> {
        if fee_share_percent < 0.0 || fee_share_percent > 0.2 {
            return Err(DexError::Other(format!(
                "fee_share_percent={} ist außerhalb des erlaubten Bereichs (0..0.2)",
                fee_share_percent
            )));
        }
        let key = format!("system_account/{}", account_id);
        let mut locked_db = self.db.lock().map_err(|_| DexError::Other("DB lock poisoned".into()))?;

        if locked_db.exists(&key)? {
            return Err(DexError::Other(format!(
                "SystemAccount '{}' existiert bereits", account_id
            )));
        }

        let sys_acc = SystemAccount {
            account_id: account_id.to_string(),
            role,
            wallet_address: wallet_address.to_string(),
            fee_share_percent,
            is_active: true,
        };
        let encoded = bincode::serialize(&sys_acc)
            .map_err(|e| DexError::Other(format!("Serialize error: {:?}", e)))?;

        locked_db.put(&key, &encoded)?;
        info!("SystemAccount '{}' angelegt: {:?}", account_id, sys_acc);
        Ok(())
    }

    /// Markiert einen System-Account als inaktiv, was zur Folge hat,
    /// dass sein Fee-Anteil auf 0.0 gesetzt wird.
    pub fn set_account_inactive(&self, account_id: &str) -> Result<(), DexError> {
        let key = format!("system_account/{}", account_id);
        let mut locked_db = self.db.lock().map_err(|_| DexError::Other("DB lock poisoned".into()))?;

        let raw = locked_db.get(&key)?;
        let val = match raw {
            Some(bytes) => bytes,
            None => return Err(DexError::Other(format!(
                "SystemAccount '{}' nicht gefunden", account_id
            ))),
        };
        let mut sys_acc: SystemAccount = bincode::deserialize(&val)
            .map_err(|e| DexError::Other(format!("Deserialize error: {:?}", e)))?;

        sys_acc.is_active = false;
        sys_acc.fee_share_percent = 0.0;

        let encoded = bincode::serialize(&sys_acc)
            .map_err(|e| DexError::Other(format!("Serialize error: {:?}", e)))?;

        locked_db.put(&key, &encoded)?;
        info!("SystemAccount '{}' wurde deaktiviert", account_id);
        Ok(())
    }

    /// Gibt eine Liste aller SystemAccounts (aktiv und inaktiv) zurück.
    pub fn list_system_accounts(&self) -> Result<Vec<SystemAccount>, DexError> {
        let prefix = "system_account/";
        let locked_db = self.db.lock().map_err(|_| DexError::Other("DB lock poisoned".into()))?;
        let kv_pairs = locked_db.list_prefix(prefix)?;
        let mut out = Vec::new();

        for (_, val) in kv_pairs {
            let sys_acc: SystemAccount = bincode::deserialize(&val)
                .map_err(|e| DexError::Other(format!("Deserialize error: {:?}", e)))?;
            out.push(sys_acc);
        }
        Ok(out)
    }

    /// Ermittelt die Summe der fee_share_percent aller aktiven Accounts.
    /// Dies kann genutzt werden, um zu prüfen, ob die Summe zu hoch ist.
    pub fn total_active_share(&self) -> Result<f64, DexError> {
        let all = self.list_system_accounts()?;
        let sum: f64 = all
            .iter()
            .filter(|acc| acc.is_active)
            .map(|acc| acc.fee_share_percent)
            .sum();
        Ok(sum)
    }

    /// Liefert nur die aktiven System-Accounts.
    pub fn active_accounts(&self) -> Result<Vec<SystemAccount>, DexError> {
        let all = self.list_system_accounts()?;
        Ok(all.into_iter().filter(|acc| acc.is_active).collect())
    }

    /// Verteilt total_fee an alle aktiven System-Accounts proportional zu deren
    /// fee_share_percent. Die Summer aller aktiven Prozentsätze bestimmt den
    /// "System Fee Pool". Der verbleibende Rest (1 - sum_active) kann dann an andere
    /// Mechanismen gehen.
    ///
    /// Hier wird das Geld virtuell einem "fee_pool/system/<account_id>/<asset>"
    /// gutgeschrieben. Alternativ kannst du hier Dex-Guthaben verbuchen.
    pub fn distribute_fees(&self, total_fee: f64, asset: &str) -> Result<(), DexError> {
        if total_fee <= 0.0 {
            return Ok(()); 
        }
        let active = self.active_accounts()?;
        if active.is_empty() {
            debug!("Keine aktiven SystemAccounts => keine Verteilung");
            return Ok(());
        }
        let sum_share: f64 = active.iter().map(|a| a.fee_share_percent).sum();
        if sum_share <= 0.0 {
            debug!("Summe der fee_share_percent=0 => nichts zu verteilen");
            return Ok(());
        }
        // Der Teil, der an SystemAccounts geht:
        let system_fee_pool = total_fee * sum_share;
        debug!("Verteile {:.6} {asset} an {} System-Accounts", system_fee_pool, active.len());

        let mut locked_db = self.db.lock().map_err(|_| DexError::Other("DB lock poisoned".into()))?;
        for acc in &active {
            let ratio = acc.fee_share_percent / sum_share;
            let portion = system_fee_pool * ratio;
            let pool_key = format!("fee_pool/system/{}/{asset}", acc.account_id);

            let old_bytes = locked_db.get(&pool_key)?;
            let old_val: f64 = if let Some(b) = old_bytes {
                bincode::deserialize(&b).unwrap_or(0.0)
            } else {
                0.0
            };
            let new_val = old_val + portion;
            let enc = bincode::serialize(&new_val)
                .map_err(|e| DexError::Other(format!("Serialize error: {:?}", e)))?;

            locked_db.put(&pool_key, &enc)?;
            info!(
                "SystemAccount={} => +{:.6} {asset} => new pool balance={:.6}",
                acc.account_id,
                portion,
                new_val
            );
        }
        Ok(())
    }
}
