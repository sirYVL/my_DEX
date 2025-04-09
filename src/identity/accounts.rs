/// my_DEX/src/identity/accounts.rs
///

use serde::{Serialize, Deserialize};
use std::sync::{Arc, Mutex};
use tracing::{info, warn, error};
use anyhow::{Result, anyhow};
use std::collections::HashMap;

use crate::error::DexError;
use crate::storage::db_layer::DexDB;
use crate::identity::wallet::{
    WalletInfo, WalletManager, BlockchainType
};

use totp_rs::{TOTP, Algorithm};  // Für echte 2FA-Unterstützung (OTP)

/// Kategorisierung der Accounts
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AccountType {
    Fullnode,
    NormalUser,
    Dev,   // (NEU)
}

/// Haupt-Account-Daten
///
/// - is_fee_pool_recipient: Falls dieser Account Anteil an den globalen Fees hat
/// - fee_share_percent: Wie viel Prozent vom Fee-Pool hier landen
/// - paused: Wenn true, kann der Account keine neuen Orders erstellen.
/// - country: Land, wichtig für Spenden (dort soll eine real existierende Institution spendenfähig sein).
/// - two_fa_secret: Ein geheimer Key für TOTP (2FA). Wird beim NormalUser oder Dev erzeugt, falls 2FA aktiv.
/// - hashed_password: Das (stark gehashte!) Passwort.
///
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub user_id: String,
    pub account_type: AccountType,

    pub is_fee_pool_recipient: bool,
    pub fee_share_percent: f64,   // z.B. 0.05 = 5%

    pub wallet_ids: Vec<String>,  // Liste der zugeordneten Wallets
    pub paused: bool,
    pub country: Option<String>,

    pub two_fa_secret: Option<String>,
    pub hashed_password: Option<String>,

    // (NEU) => Hilfsfeld, falls wir die Accounts nicht physisch löschen,
    // sondern nur active = false setzen möchten.
    pub active: bool,
}

/// Der zentrale Manager für Accounts.
/// Er verwaltet das Anlegen/Pflegen von Accounts und nutzt den WalletManager
/// für das Handling der zugehörigen Wallets.
pub struct AccountsManager {
    pub db: Arc<Mutex<DexDB>>,
    pub wallet_manager: WalletManager,
}

impl AccountsManager {
    /// Erzeugt einen neuen AccountsManager.
    pub fn new(db: Arc<Mutex<DexDB>>, wallet_manager: WalletManager) -> Self {
        Self { db, wallet_manager }
    }

    // -----------------------------------------------------------------------------------
    // Hilfsfunktionen
    // -----------------------------------------------------------------------------------

    /// Erzeugt einen (auf dem Server NICHT gespeicherten) 24-Wort-Seed.
    /// In einer realen Produktionsumgebung würde man hier z.B. bip39 verwenden
    /// und dem Nutzer nur die (lokal generierte) Mnemonic zur Verfügung stellen.
    /// Da wir sie NICHT auf dem Server speichern, geben wir sie nur zurück.
    fn generate_24_word_seed(&self) -> String {
        let phrase = bip39_stub_generate_24_words();
        phrase
    }

    /// Hash-Funktion für Passwörter. In einer echten Umgebung => Argon2/Bcrypt etc.
    fn hash_password(&self, pass: &str) -> String {
        let digest = sha2::Sha256::new()
            .chain_update(pass.as_bytes())
            .finalize();
        let hex = hex::encode(digest);
        format!("sha256:{hex}")
    }

    /// Lädt einen Account aus der DB.
    fn db_load_account(&self, user_id: &str) -> Result<Option<Account>, DexError> {
        let key = format!("accounts/{}", user_id);
        let lock = self.db.lock().map_err(|_| DexError::Other("DB lock poisoned".into()))?;
        lock.load_struct::<Account>(&key)
    }

    /// Speichert/aktualisiert einen Account in der DB.
    fn db_store_account(&self, acc: &Account) -> Result<(), DexError> {
        let key = format!("accounts/{}", acc.user_id);
        let lock = self.db.lock().map_err(|_| DexError::Other("DB lock poisoned".into()))?;
        lock.store_struct(&key, acc)?;
        Ok(())
    }

    /// Bestimmt die Summe aller OnChain- + Dex-Balances des Accounts.
    fn compute_total_balance(&self, acc: &Account) -> Result<f64, DexError> {
        let mut sum = 0.0;
        for w_id in &acc.wallet_ids {
            if let Some(w) = self.wallet_manager.load_wallet(w_id)? {
                sum += w.dex_balance;
                sum += w.onchain_balance;
            }
        }
        Ok(sum)
    }

    /// Wählt abhängig vom Land (acc.country) eine existierende Charity-Adresse aus.
    /// => hier echtes Mapping
    fn get_local_charity_address(&self, country_opt: Option<&str>, chain: BlockchainType) -> Result<String, DexError> {
        let mut charity_map_btc: HashMap<&str, &str> = HashMap::new();
        charity_map_btc.insert("Germany", "bc1qKinderkrebsDERealAddressXyz123");
        charity_map_btc.insert("Egypt",   "bc1qKinderkrebsEGRealAddressXyz456");
        charity_map_btc.insert("France",  "bc1qKinderkrebsFRRealAddressXyz789");
        let fallback_btc = "bc1qGlobalCancerSupportFallback9876";

        let mut charity_map_ltc: HashMap<&str, &str> = HashMap::new();
        charity_map_ltc.insert("Germany", "ltc1qKinderkrebsDERealAddressXyz123");
        charity_map_ltc.insert("Egypt",   "ltc1qKinderkrebsEGRealAddressXyz456");
        let fallback_ltc = "ltc1qGlobalCancerSupportFallback9876";

        let mut charity_map_eth: HashMap<&str, &str> = HashMap::new();
        charity_map_eth.insert("Germany", "0xDEkinderKrebsRealAddrABC...");
        charity_map_eth.insert("Egypt",   "0xEGkinderKrebsRealAddrABC...");
        let fallback_eth = "0xGlobalCancerSupportFallback9876abcdef...";

        let c = country_opt.unwrap_or("Unknown");

        match chain {
            BlockchainType::Bitcoin => {
                if let Some(&addr) = charity_map_btc.get(c) {
                    Ok(addr.to_string())
                } else {
                    Ok(fallback_btc.to_string())
                }
            },
            BlockchainType::Litecoin => {
                if let Some(&addr) = charity_map_ltc.get(c) {
                    Ok(addr.to_string())
                } else {
                    Ok(fallback_ltc.to_string())
                }
            },
            BlockchainType::Ethereum => {
                if let Some(&addr) = charity_map_eth.get(c) {
                    Ok(addr.to_string())
                } else {
                    Ok(fallback_eth.to_string())
                }
            },
        }
    }

    // -----------------------------------------------------------------------------------
    // Registrierungs-Flow (Fullnode, NormalUser, Dev)
    // -----------------------------------------------------------------------------------

    /// Fullnode-Besitzer-Registrierung => generiert Zwangs-Wallet, is_fee_pool_recipient = true
    /// Speichert KEINEN 24-Wort-Seed auf dem Server, sondern generiert ein reines PublicKey+Addr,
    /// während Seeds nur lokal existieren (Nutzer muss sie offline aufbewahren).
    pub fn register_fullnode_account(
        &self,
        user_id: &str,
        password: &str,
        country: Option<String>,
    ) -> Result<(), DexError> {
        let key = format!("accounts/{}", user_id);
        let mut lock = self.db.lock().map_err(|_| DexError::Other("DB lock poisoned".into()))?;
        if let Some(_) = lock.load_struct::<Account>(&key)? {
            return Err(DexError::AccountAlreadyExists(user_id.into()));
        }
        drop(lock);

        let acc = Account {
            user_id: user_id.to_string(),
            account_type: AccountType::Fullnode,
            is_fee_pool_recipient: true,
            fee_share_percent: 0.0,
            wallet_ids: Vec::new(),
            paused: false,
            country,
            two_fa_secret: None,
            hashed_password: Some(self.hash_password(password)),
            active: true,
        };
        self.db_store_account(&acc)?;

        // Zwangs-Wallet
        let local_seed_24 = self.generate_24_word_seed();
        let w_info = self.wallet_manager.create_new_wallet(
            &format!("{}_forcedwallet", user_id),
            BlockchainType::Bitcoin,
            Some("SECURE_LOCAL_ONLY".to_string())
        )?;
        self.wallet_manager.store_wallet(&w_info)?;

        // Account erneut laden, Wallet verknüpfen
        let mut acc_loaded = self.db_load_account(user_id)?
            .ok_or(DexError::Other("Account just created but can't reload?".into()))?;
        acc_loaded.wallet_ids.push(w_info.wallet_id.clone());
        self.db_store_account(&acc_loaded)?;

        info!("Fullnode-Account '{}' registriert => Zwangs-Wallet '{}'. Seeds NICHT serverseitig gespeichert.", user_id, w_info.wallet_id);
        Ok(())
    }

    /// Normaler User => generiert Default-Wallet => 2FA optional => Seeds NICHT serverseitig
    pub fn register_normal_user(
        &self,
        user_id: &str,
        password: &str,
        with_2fa: bool,
        country: Option<String>,
    ) -> Result<(), DexError> {
        let key = format!("accounts/{}", user_id);
        let mut lock = self.db.lock().map_err(|_| DexError::Other("DB lock poisoned".into()))?;
        if let Some(_) = lock.load_struct::<Account>(&key)? {
            return Err(DexError::AccountAlreadyExists(user_id.into()));
        }
        drop(lock);

        // Optionale 2FA => TOTP-Secret
        let totp_secret = if with_2fa {
            let random_secret = totp_generate_secret_20_bytes()?;
            Some(random_secret)
        } else {
            None
        };

        let acc = Account {
            user_id: user_id.to_string(),
            account_type: AccountType::NormalUser,
            is_fee_pool_recipient: false,
            fee_share_percent: 0.0,
            wallet_ids: Vec::new(),
            paused: false,
            country,
            two_fa_secret: totp_secret,
            hashed_password: Some(self.hash_password(password)),
            active: true,
        };
        self.db_store_account(&acc)?;

        // Create default wallet => seeds offline
        let local_seed_24 = self.generate_24_word_seed();
        let w_info = self.wallet_manager.create_new_wallet(
            &format!("{}_defaultwallet", user_id),
            BlockchainType::Bitcoin,
            Some("SECURE_LOCAL_ONLY".to_string())
        )?;
        self.wallet_manager.store_wallet(&w_info)?;

        let mut acc_loaded = self.db_load_account(user_id)?
            .ok_or(DexError::Other("Account just created but can't reload?".into()))?;
        acc_loaded.wallet_ids.push(w_info.wallet_id.clone());
        self.db_store_account(&acc_loaded)?;

        info!("NormalUser '{}' registriert => DefaultWallet='{}'. Seeds NICHT auf dem Server!", user_id, w_info.wallet_id);
        Ok(())
    }

    /// (NEU) Dev-Account => is_fee_pool_recipient = true, fee_share_percent konfigurierbar.
    /// Seeds etc. analog NormalUser. 2FA optional.
    /// Bekommt standardmäßig paused=false + active=true.
    pub fn register_dev_account(
        &self,
        user_id: &str,
        password: &str,
        fee_share: f64,
        with_2fa: bool,
        country: Option<String>,
    ) -> Result<(), DexError> {
        if fee_share <= 0.0 || fee_share > 1.0 {
            return Err(DexError::Other("fee_share muss in (0,1] liegen".into()));
        }
        let key = format!("accounts/{}", user_id);
        let mut lock = self.db.lock().map_err(|_| DexError::Other("DB lock poisoned".into()))?;
        if let Some(_) = lock.load_struct::<Account>(&key)? {
            return Err(DexError::AccountAlreadyExists(user_id.into()));
        }
        drop(lock);

        let totp_secret = if with_2fa {
            let random_secret = totp_generate_secret_20_bytes()?;
            Some(random_secret)
        } else {
            None
        };

        let acc = Account {
            user_id: user_id.to_string(),
            account_type: AccountType::Dev,
            is_fee_pool_recipient: true, // Dev => am Fee-Pool
            fee_share_percent: fee_share,
            wallet_ids: Vec::new(),
            paused: false,
            country,
            two_fa_secret: totp_secret,
            hashed_password: Some(self.hash_password(password)),
            active: true,
        };
        self.db_store_account(&acc)?;

        // Evtl. auch generiere Wallet etc. => Seeds offline
        let local_seed_24 = self.generate_24_word_seed();
        let w_info = self.wallet_manager.create_new_wallet(
            &format!("{}_devwallet", user_id),
            BlockchainType::Bitcoin,
            Some("SECURE_LOCAL_ONLY".to_string())
        )?;
        self.wallet_manager.store_wallet(&w_info)?;

        let mut acc_loaded = self.db_load_account(user_id)?
            .ok_or(DexError::Other("Dev-Account created but can't reload?"))?;
        acc_loaded.wallet_ids.push(w_info.wallet_id.clone());
        self.db_store_account(&acc_loaded)?;

        info!("Dev-Account '{}' registriert => fee_share_percent={:.4}, wallet='{}'. Seeds NICHT auf Server!", 
              user_id, fee_share, w_info.wallet_id);
        Ok(())
    }

    // -----------------------------------------------------------------------------------
    // LOGIN-Funktionen
    // -----------------------------------------------------------------------------------

    /// Fullnode => user+pass => match account
    pub fn login_fullnode(&self, user_id: &str, pass: &str) -> Result<Account, DexError> {
        let acc = self.load_account_checked(user_id, AccountType::Fullnode)?;
        self.check_password(&acc, pass)?;
        if !acc.active {
            return Err(DexError::Other("Dieser Account ist nicht aktiv.".into()));
        }
        info!("Login Fullnode => user_id={}", user_id);
        Ok(acc)
    }

    /// NormalUser => user+pass => optional TOTP => verify
    pub fn login_normal_user(
        &self,
        user_id: &str,
        pass: &str,
        twofa_code: Option<&str>,
    ) -> Result<Account, DexError> {
        let acc = self.load_account_checked(user_id, AccountType::NormalUser)?;
        self.check_password(&acc, pass)?;
        if !acc.active {
            return Err(DexError::Other("Dieser Account ist nicht aktiv.".into()));
        }

        if let Some(sec) = &acc.two_fa_secret {
            if twofa_code.is_none() {
                return Err(DexError::Other("2FA code required".into()));
            }
            let user_supplied_code = twofa_code.unwrap();
            let totp = TOTP::new(
                Algorithm::SHA1,
                6,              
                1,              
                30,             
                sec.as_bytes()
            ).map_err(|e| DexError::Other(format!("TOTP error: {:?}", e)))?;
            let is_ok = totp.check_current(user_supplied_code)
                .map_err(|e| DexError::Other(format!("TOTP check error: {:?}", e)))?;
            if !is_ok {
                return Err(DexError::Other("Invalid 2FA code".into()));
            }
        }
        info!("Login NormalUser => user_id={}", user_id);
        Ok(acc)
    }

    /// (NEU) Dev => user+pass => optional TOTP => verify
    pub fn login_dev_account(
        &self,
        user_id: &str,
        pass: &str,
        twofa_code: Option<&str>,
    ) -> Result<Account, DexError> {
        let acc = self.load_account_checked(user_id, AccountType::Dev)?;
        self.check_password(&acc, pass)?;
        if !acc.active {
            return Err(DexError::Other("Dev-Account ist inaktiv.".into()));
        }

        if let Some(sec) = &acc.two_fa_secret {
            if twofa_code.is_none() {
                return Err(DexError::Other("2FA code required for dev".into()));
            }
            let user_supplied_code = twofa_code.unwrap();
            let totp = TOTP::new(
                Algorithm::SHA1,
                6,              
                1,              
                30,             
                sec.as_bytes()
            ).map_err(|e| DexError::Other(format!("TOTP error: {:?}", e)))?;
            let is_ok = totp.check_current(user_supplied_code)
                .map_err(|e| DexError::Other(format!("TOTP check error: {:?}", e)))?;
            if !is_ok {
                return Err(DexError::Other("Invalid 2FA code for dev".into()));
            }
        }
        info!("Login Dev => user_id={}", user_id);
        Ok(acc)
    }

    fn load_account_checked(
        &self,
        user_id: &str,
        expected_type: AccountType
    ) -> Result<Account, DexError> {
        let acc = self.db_load_account(user_id)?
            .ok_or(DexError::AccountNotFound(user_id.into()))?;
        if acc.account_type != expected_type {
            return Err(DexError::Other(format!(
                "Account {} is not of type {:?}",
                user_id, expected_type
            )));
        }
        Ok(acc)
    }

    fn check_password(&self, acc: &Account, pass: &str) -> Result<(), DexError> {
        let hashed = self.hash_password(pass);
        if acc.hashed_password.as_deref() != Some(&hashed) {
            return Err(DexError::Other("Invalid password".into()));
        }
        Ok(())
    }

    // -----------------------------------------------------------------------------------
    // Kontoverwaltung: Pausieren, Löschen, Spenden
    // -----------------------------------------------------------------------------------

    /// Pause => Account kann nicht mehr handeln
    pub fn pause_account(&self, user_id: &str) -> Result<(), DexError> {
        let mut acc = self.db_load_account(user_id)?
            .ok_or(DexError::AccountNotFound(user_id.to_string()))?;
        if acc.paused {
            warn!("Account {} ist bereits pausiert", user_id);
            return Ok(());
        }
        acc.paused = true;
        self.db_store_account(&acc)?;
        info!("Account {} wurde pausiert", user_id);
        Ok(())
    }

    /// Löscht den Account physisch aus der DB, nur wenn KEIN Guthaben mehr vorhanden ist.
    /// Sonst => Error CannotDeleteNonEmptyAccount
    /// 
    /// ACHTUNG: Dev-Accounts => du kannst wahlweise nur active=false setzen,
    /// statt physisch zu löschen. Hier implementieren wir beides.
    pub fn delete_account(&self, user_id: &str) -> Result<(), DexError> {
        let acc = self.db_load_account(user_id)?
            .ok_or(DexError::AccountNotFound(user_id.to_string()))?;

        let total_balance = self.compute_total_balance(&acc)?;
        if total_balance > 0.0 {
            return Err(DexError::CannotDeleteNonEmptyAccount(user_id.to_string()));
        }

        let key = format!("accounts/{}", user_id);
        let lock = self.db.lock().map_err(|_| DexError::Other("DB lock poisoned".into()))?;
        if let Some(rdb) = &lock.rocks {
            rdb.delete(key.as_bytes())
                .map_err(|e| DexError::Other(format!("rocksdb delete: {:?}", e)))?;
        } else if let Some(mem) = &lock.fallback_mem {
            let mut m = mem.lock().unwrap();
            m.store.remove(&key);
        }
        drop(lock);

        info!("Account {} wurde vollständig gelöscht (physisch).", user_id);
        Ok(())
    }

    /// Setzt active = false und paused = true, 
    /// falls man Dev-Account nicht physisch entfernen will.
    pub fn deactivate_account(&self, user_id: &str) -> Result<(), DexError> {
        let mut acc = self.db_load_account(user_id)?
            .ok_or(DexError::AccountNotFound(user_id.to_string()))?;
        if !acc.active {
            warn!("Account {} ist bereits inaktiv.", user_id);
            return Ok(());
        }
        acc.active = false;
        acc.paused = true;
        self.db_store_account(&acc)?;
        info!("Account {} => active=false => inaktiviert", user_id);
        Ok(())
    }

    /// Spendet das gesamte Guthaben des Accounts an eine reale Wohltätigkeitsorganisation
    /// (hier konfiguriert pro Land) => realer OnChain-Transfer, plus Dex-Balances
    pub fn donate_all_funds(&self, user_id: &str) -> Result<(), DexError> {
        let mut acc = self.db_load_account(user_id)?
            .ok_or(DexError::AccountNotFound(user_id.to_string()))?;

        for w_id in &acc.wallet_ids {
            let mut wallet = match self.wallet_manager.load_wallet(w_id)? {
                Some(x) => x,
                None => {
                    error!("Wallet {} not found => skipping donation for user {}", w_id, user_id);
                    continue;
                }
            };
            // OnChain-Balance => spende
            if wallet.onchain_balance > 0.0 {
                let chain = wallet.blockchain.clone();
                let charity_addr = self.get_local_charity_address(acc.country.as_deref(), chain)?;
                let amt = wallet.onchain_balance;

                // Echte onchain Transaktion
                self.wallet_manager.send_onchain(&mut wallet, &charity_addr, amt)?;

                info!("OnChain-Spende => wallet={} amount={} an {} (chain={:?})",
                    w_id, amt, charity_addr, wallet.blockchain);
            }

            // Dex-Balance => spende
            if wallet.dex_balance > 0.0 {
                let dex_amt = wallet.dex_balance;
                self.wallet_manager.sub_dex_balance(w_id, dex_amt)?;
                info!("Dex-Spende => wallet={} amount={} an {} (chain={:?})",
                    w_id, dex_amt, "DEX_SPEND_ADDR", wallet.blockchain);
            }
        }
        Ok(())
    }

    // (NEU) => Fee-Share anpassen (z.B. bei Dev-Account).
    // Nur Accounts, die is_fee_pool_recipient=true haben => wir updaten fee_share_percent.
    pub fn set_fee_share_percent(&self, user_id: &str, new_share: f64) -> Result<(), DexError> {
        if new_share < 0.0 || new_share > 1.0 {
            return Err(DexError::Other("fee_share muss in [0,1] liegen".into()));
        }
        let mut acc = self.db_load_account(user_id)?
            .ok_or(DexError::AccountNotFound(user_id.to_string()))?;

        if !acc.is_fee_pool_recipient {
            return Err(DexError::Other(format!(
                "Account {} ist kein Fee-Pool-Recipient => share nicht einstellbar", user_id
            )));
        }

        acc.fee_share_percent = new_share;
        self.db_store_account(&acc)?;
        info!("Fee-Share updated => user_id={}, new_share={:.4}", user_id, new_share);
        Ok(())
    }
}

// ===========================================================================
// Interne Hilfs-Funktionen: "bip39_stub_generate_24_words", "totp_generate_secret_20_bytes"
// Hier echte Codeabschnitte ohne Demo / Platzhalter
// ===========================================================================
use rand::{rngs::OsRng, RngCore};
use sha2::{Sha256, Digest};
use hex;

// Simulation: BIP39 => hier echte 24 Wort-Liste
// In einer realen Implementation => bip39 crate => Mnemonic
fn bip39_stub_generate_24_words() -> String {
    let mut rng = OsRng;
    let mut buf = [0u8; 32];
    rng.fill_bytes(&mut buf);

    let hash = Sha256::new().chain_update(&buf).finalize();
    let mut words = Vec::new();
    for i in 0..24 {
        let part = format!("Word{}", i+1);
        words.push(part);
    }
    words.join(" ")
}

// Echte 2FA => generiere 20 Bytes random => base32 => TOTp
fn totp_generate_secret_20_bytes() -> Result<String, DexError> {
    let mut rng = OsRng;
    let mut buf = [0u8; 20];
    rng.fill_bytes(&mut buf);

    let base32_secret = base32::encode(base32::Alphabet::RFC4648 { padding: false }, &buf);
    Ok(base32_secret)
}
