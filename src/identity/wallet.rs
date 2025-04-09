/////////////////////////////////////
/// my_DEX/src/identity/wallet.rs
/////////////////////////////////////

use serde::{Serialize, Deserialize};
use std::str::FromStr;
use tracing::{info, warn, error};
use anyhow::{Result, anyhow};
use crate::error::DexError;
use crate::storage::db_layer::DexDB;

use bitcoincore_rpc::{Auth, Client, RpcApi};
use bip39::{Language, Mnemonic, Seed};
use bitcoin::util::bip32::{
    ExtendedPrivKey, ExtendedPubKey, DerivationPath, ChildNumber
};
use bitcoin::Network as BTCNetwork;
use litecoin::Network as LTCNetwork;
use ethers::prelude::*;
use ethers::core::types::Address;

/// Beschreibt, für welche Blockchain (BTC/ETH/LTC) ein Wallet bestimmt ist.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BlockchainType {
    Bitcoin,
    Ethereum,
    Litecoin,
}

/// Ein Eintrag über ein Wallet, das in der Datenbank gespeichert wird.
///
/// Non-custodial:  
/// - Es werden nur öffentliche Informationen gespeichert (xpub / ETH-Address).  
/// - Keine Private Keys oder Seeds in der Dex-DB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletInfo {
    pub wallet_id: String,
    pub blockchain: BlockchainType,
    pub public_info: String,
    pub address: String,

    /// Ergebnis des On-Chain-Bestands (z. B. über RPC abgefragt).
    pub onchain_balance: f64,

    /// Off-Chain-Guthaben für interne DEX-Operationen.
    pub dex_balance: f64,
}

/// BTC-spezifische RPC-Konfiguration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitcoinRPCConfig {
    pub rpc_url: String,
    pub rpc_user: String,
    pub rpc_pass: String,
}

/// Litecoin-spezifische RPC-Konfiguration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LTCConfig {
    pub rpc_url: String,
    pub rpc_user: String,
    pub rpc_pass: String,
}

/// Ethereum-spezifische RPC-Konfiguration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ETHConfig {
    pub rpc_url: String,
}

/// WalletManager verwaltet das Erstellen, Laden und Aktualisieren von Wallets.
/// Private Seeds werden *nie* in der Dex-DB gespeichert. Stattdessen wird nur xpub oder Address gesichert.
#[derive(Debug, Clone)]
pub struct WalletManager {
    pub db: DexDB,
    pub btc_cfg: Option<BitcoinRPCConfig>,
    pub ltc_cfg: Option<LTCConfig>,
    pub eth_cfg: Option<ETHConfig>,
}

impl WalletManager {
    pub fn new(
        db: DexDB,
        btc_cfg: Option<BitcoinRPCConfig>,
        ltc_cfg: Option<LTCConfig>,
        eth_cfg: Option<ETHConfig>,
    ) -> Self {
        Self {
            db,
            btc_cfg,
            ltc_cfg,
            eth_cfg,
        }
    }

    // ------------------------------------------------------------------------
    // Hilfe: BTC / LTC xpub-Generierung
    // ------------------------------------------------------------------------

    /// Erzeugt 24-Wort-Mnemonic, leitet Master-XPriv ab, 
    /// daraus xpub (m/44'/0'/0'/0) und dann eine Beispieladresse.
    /// Rückgabe: (mnemonic, xpub, address)
    fn generate_btc_xpub() -> Result<(String, String, String)> {
        // 1) 24-Wort: Mnemonic
        let mnemonic = Mnemonic::new_random(Language::English);
        let mnemonic_str = mnemonic.to_string();

        // 2) Seeds => ExtendedPrivKey
        let seed = Seed::new(&mnemonic, "");
        let xpriv = ExtendedPrivKey::new_master(BTCNetwork::Bitcoin, seed.as_bytes())
            .map_err(|e| anyhow!("new_master(BTC) error: {:?}", e))?;

        let derivation = DerivationPath::from_str("m/44'/0'/0'/0")
            .map_err(|e| anyhow!("bad derivation path BTC: {:?}", e))?;
        let cpriv = xpriv.derive_priv(&bitcoin::secp256k1::Secp256k1::new(), &derivation)
            .map_err(|e| anyhow!("BTC derive_priv: {:?}", e))?;

        let xpub = ExtendedPubKey::from_priv(&bitcoin::secp256k1::Secp256k1::new(), &cpriv);
        let xpub_str = xpub.to_string();

        // 3) Index0 => address
        let index_0 = cpriv.ckd_priv(
            &bitcoin::secp256k1::Secp256k1::new(),
            ChildNumber::Normal { index: 0 }
        ).map_err(|e| anyhow!("ckd_priv(BTC) error: {:?}", e))?;
        let pubkey0 = ExtendedPubKey::from_priv(&bitcoin::secp256k1::Secp256k1::new(), &index_0);
        let address_btc = bitcoin::Address::p2wpkh(
            &pubkey0.public_key,
            BTCNetwork::Bitcoin
        ).map_err(|e| anyhow!("p2wpkh(BTC) => {:?}", e))?;

        Ok((mnemonic_str, xpub_str, address_btc.to_string()))
    }

    /// Erzeugt LTC-xpub und address analog.
    fn generate_ltc_xpub() -> Result<(String, String, String)> {
        let mnemonic = Mnemonic::new_random(Language::English);
        let mnemonic_str = mnemonic.to_string();

        let seed = Seed::new(&mnemonic, "");
        let xpriv = ExtendedPrivKey::new_master(LTCNetwork::Litecoin, seed.as_bytes())
            .map_err(|e| anyhow!("new_master(LTC) error: {:?}", e))?;

        let derivation = DerivationPath::from_str("m/44'/2'/0'/0")
            .map_err(|e| anyhow!("bad LTC deriv path: {:?}", e))?;
        let cpriv = xpriv.derive_priv(&litecoin::secp256k1::Secp256k1::new(), &derivation)
            .map_err(|e| anyhow!("LTC derive_priv: {:?}", e))?;

        let xpub = ExtendedPubKey::from_priv(&litecoin::secp256k1::Secp256k1::new(), &cpriv);
        let xpub_str = xpub.to_string();

        let index_0 = cpriv.ckd_priv(
            &litecoin::secp256k1::Secp256k1::new(),
            ChildNumber::Normal { index: 0 }
        ).map_err(|e| anyhow!("ckd_priv(LTC): {:?}", e))?;
        let pubkey0 = ExtendedPubKey::from_priv(&litecoin::secp256k1::Secp256k1::new(), &index_0);
        let address_ltc = litecoin::Address::p2wpkh(
            &pubkey0.public_key,
            LTCNetwork::Litecoin
        ).map_err(|e| anyhow!("p2wpkh(LTC): {:?}", e))?;

        Ok((mnemonic_str, xpub_str, address_ltc.to_string()))
    }

    /// Generiert ETH-Account: (mnemonic, publicKeyHex, addressHex).
    /// Non-custodial => Seeds/PrivKeys existieren nur offline.
    fn generate_eth_account() -> Result<(String, String, String)> {
        let mnemonic = Mnemonic::new_random(Language::English);
        let mnemonic_str = mnemonic.to_string();

        let seed = Seed::new(&mnemonic, "");
        let privkey_bytes = &seed.as_bytes()[0..32];
        let wallet = LocalWallet::from_signing_key(privkey_bytes.into());

        let pubkey = wallet.signing_key.public_key();
        let pubkey_hex = format!("0x{}", hex::encode(pubkey.as_bytes()));

        let addr_hex = format!("{:?}", wallet.address());
        Ok((mnemonic_str, pubkey_hex, addr_hex))
    }

    fn derive_btc_address_from_xpub(xpub: &str, index: u32) -> Result<String> {
        let xpub_parsed = ExtendedPubKey::from_str(xpub)
            .map_err(|e| anyhow!("invalid BTC xpub: {:?}", e))?;
        let child = xpub_parsed.ckd_pub(
            &bitcoin::secp256k1::Secp256k1::new(),
            ChildNumber::Normal { index }
        )?;
        let addr = bitcoin::Address::p2wpkh(
            &child.public_key,
            BTCNetwork::Bitcoin
        )?;
        Ok(addr.to_string())
    }

    fn derive_ltc_address_from_xpub(xpub: &str, index: u32) -> Result<String> {
        let xpub_parsed = ExtendedPubKey::from_str(xpub)
            .map_err(|e| anyhow!("invalid LTC xpub: {:?}", e))?;
        let child = xpub_parsed.ckd_pub(
            &litecoin::secp256k1::Secp256k1::new(),
            ChildNumber::Normal { index }
        )?;
        let addr = litecoin::Address::p2wpkh(
            &child.public_key,
            LTCNetwork::Litecoin
        )?;
        Ok(addr.to_string())
    }

    // ----------------------------------------------------------------------------
    // create_new_wallet(...) => je nach Blockchain generieren/ableiten
    // ----------------------------------------------------------------------------

    /// Legt ein neues WalletInfo an, das nur xpub/Address etc. enthält.
    /// user_mnemonic_seed: Falls ein User eine existierende Info übergeben möchte,
    ///  z. B. xpub-String (BTC/LTC) oder "0xPub|0xAddress" für ETH.
    pub fn create_new_wallet(
        &self,
        wallet_id: &str,
        chain: BlockchainType,
        user_mnemonic_seed: Option<String>,
    ) -> Result<WalletInfo, DexError> {
        match chain {
            BlockchainType::Bitcoin => {
                if let Some(ref given_xpub) = user_mnemonic_seed {
                    let xp = given_xpub.clone();
                    let _test = ExtendedPubKey::from_str(&xp)
                        .map_err(|_| DexError::Other("Invalid BTC xpub".into()))?;
                    let addr_btc = Self::derive_btc_address_from_xpub(&xp, 0)
                        .map_err(|e| DexError::Other(format!("derive BTC: {:?}", e)))?;
                    let w = WalletInfo {
                        wallet_id: wallet_id.to_string(),
                        blockchain: BlockchainType::Bitcoin,
                        public_info: xp,
                        address: addr_btc,
                        onchain_balance: 0.0,
                        dex_balance: 0.0,
                    };
                    Ok(w)
                } else {
                    let (_, xpub, addr) = Self::generate_btc_xpub()
                        .map_err(|e| DexError::Other(format!("BTC gen error: {:?}", e)))?;
                    let w = WalletInfo {
                        wallet_id: wallet_id.to_string(),
                        blockchain: BlockchainType::Bitcoin,
                        public_info: xpub,
                        address: addr,
                        onchain_balance: 0.0,
                        dex_balance: 0.0,
                    };
                    Ok(w)
                }
            }
            BlockchainType::Litecoin => {
                if let Some(ref given_xpub) = user_mnemonic_seed {
                    let xp = given_xpub.clone();
                    let _test = ExtendedPubKey::from_str(&xp)
                        .map_err(|_| DexError::Other("Invalid LTC xpub".into()))?;
                    let addr_ltc = Self::derive_ltc_address_from_xpub(&xp, 0)
                        .map_err(|e| DexError::Other(format!("derive LTC: {:?}", e)))?;
                    let w = WalletInfo {
                        wallet_id: wallet_id.to_string(),
                        blockchain: BlockchainType::Litecoin,
                        public_info: xp,
                        address: addr_ltc,
                        onchain_balance: 0.0,
                        dex_balance: 0.0,
                    };
                    Ok(w)
                } else {
                    let (_, xpub, addr) = Self::generate_ltc_xpub()
                        .map_err(|e| DexError::Other(format!("LTC gen error: {:?}", e)))?;
                    let w = WalletInfo {
                        wallet_id: wallet_id.to_string(),
                        blockchain: BlockchainType::Litecoin,
                        public_info: xpub,
                        address: addr,
                        onchain_balance: 0.0,
                        dex_balance: 0.0,
                    };
                    Ok(w)
                }
            }
            BlockchainType::Ethereum => {
                if let Some(ref eth_combo) = user_mnemonic_seed {
                    // Erwarte "0xPubkey|0xAddress"
                    let splitted: Vec<_> = eth_combo.split('|').collect();
                    if splitted.len() != 2 {
                        return Err(DexError::Other("Expected '0xPub|0xAddr' for ETH".into()));
                    }
                    let pub_hex = splitted[0].to_string();
                    let addr = splitted[1].to_string();
                    if !addr.starts_with("0x") {
                        return Err(DexError::Other("ETH address must start with 0x".into()));
                    }
                    let w = WalletInfo {
                        wallet_id: wallet_id.to_string(),
                        blockchain: BlockchainType::Ethereum,
                        public_info: pub_hex,
                        address: addr,
                        onchain_balance: 0.0,
                        dex_balance: 0.0,
                    };
                    Ok(w)
                } else {
                    let (_, pub_hex, addr_hex) = Self::generate_eth_account()
                        .map_err(|e| DexError::Other(format!("ETH gen error: {:?}", e)))?;
                    let w = WalletInfo {
                        wallet_id: wallet_id.to_string(),
                        blockchain: BlockchainType::Ethereum,
                        public_info: pub_hex,
                        address: addr_hex,
                        onchain_balance: 0.0,
                        dex_balance: 0.0,
                    };
                    Ok(w)
                }
            }
        }
    }

    /// Speichert ein Wallet in der DB
    pub fn store_wallet(&self, w: &WalletInfo) -> Result<(), DexError> {
        let key = format!("wallets/{}", w.wallet_id);
        self.db.store_struct(&key, w)?;
        Ok(())
    }

    /// Lädt ein Wallet aus der DB
    pub fn load_wallet(&self, wallet_id: &str) -> Result<Option<WalletInfo>, DexError> {
        let key = format!("wallets/{}", wallet_id);
        self.db.load_struct::<WalletInfo>(&key)
    }

    // ----------------------------------------------------------------------------
    // OnChain-Balance + Senden
    // ----------------------------------------------------------------------------

    /// Aktualisiert den On-Chain-Bestand je nach Blockchain via RPC.
    pub fn update_onchain_balance(&self, w: &mut WalletInfo) -> Result<(), DexError> {
        match w.blockchain {
            BlockchainType::Bitcoin => {
                if let Some(cfg) = &self.btc_cfg {
                    let auth = Auth::UserPass(cfg.rpc_user.clone(), cfg.rpc_pass.clone());
                    let client = Client::new(cfg.rpc_url.clone(), auth)
                        .map_err(|e| DexError::Other(format!("BTC client init err: {:?}", e)))?;
                    let parsed = w.address.parse()
                        .map_err(|_| DexError::Other("BTC address parse err".into()))?;
                    let recv = client.get_received_by_address(parsed, Some(0))
                        .map_err(|e| DexError::Other(format!("BTC get_received_by_address: {:?}", e)))?;
                    w.onchain_balance = recv;
                    self.store_wallet(w)?;
                } else {
                    return Err(DexError::Other("No BTC config found".into()));
                }
            }
            BlockchainType::Litecoin => {
                if let Some(cfg) = &self.ltc_cfg {
                    let auth = Auth::UserPass(cfg.rpc_user.clone(), cfg.rpc_pass.clone());
                    let client = Client::new(cfg.rpc_url.clone(), auth)
                        .map_err(|e| DexError::Other(format!("LTC client init err: {:?}", e)))?;
                    let parsed = w.address.parse()
                        .map_err(|_| DexError::Other("LTC address parse err".into()))?;
                    let recv = client.get_received_by_address(parsed, Some(0))
                        .map_err(|e| DexError::Other(format!("LTC get_received_by_address: {:?}", e)))?;
                    w.onchain_balance = recv;
                    self.store_wallet(w)?;
                } else {
                    return Err(DexError::Other("No LTC config found".into()));
                }
            }
            BlockchainType::Ethereum => {
                if let Some(cfg) = &self.eth_cfg {
                    let provider = Provider::<Http>::try_from(cfg.rpc_url.clone())
                        .map_err(|e| DexError::Other(format!("ETH provider init err: {:?}", e)))?;
                    let addr = w.address.parse::<Address>()
                        .map_err(|_| DexError::Other("invalid ETH address".into()))?;
                    let balance_res = futures::executor::block_on(provider.get_balance(addr, None))
                        .map_err(|e| DexError::Other(format!("ETH get_balance error: {:?}", e)))?;
                    let bal_eth = ethers::utils::from_wei(balance_res, 18u32);
                    w.onchain_balance = bal_eth.to_string().parse().unwrap_or(0.0);
                    self.store_wallet(w)?;
                } else {
                    return Err(DexError::Other("No ETH config found".into()));
                }
            }
        }
        Ok(())
    }

    /// Sendet amount OnChain, abgezogen von w.onchain_balance.
    /// BTC/LTC => sendtoaddress (RPC).
    /// ETH => local Key sign? => Minimales Stub => TODO
    pub fn send_onchain(&self, w: &mut WalletInfo, to_addr: &str, amount: f64) -> Result<(), DexError> {
        if w.onchain_balance < amount {
            return Err(DexError::Other(format!(
                "Not enough onchain balance in wallet '{}'", w.wallet_id
            )));
        }
        match w.blockchain {
            BlockchainType::Bitcoin => {
                if let Some(cfg) = &self.btc_cfg {
                    let auth = Auth::UserPass(cfg.rpc_user.clone(), cfg.rpc_pass.clone());
                    let client = Client::new(cfg.rpc_url.clone(), auth)
                        .map_err(|e| DexError::Other(format!("BTC client init err: {:?}", e)))?;
                    let parsed_addr = to_addr.parse()
                        .map_err(|_| DexError::Other("Bad BTC address to send".into()))?;
                    let _txid = client.send_to_address(
                        parsed_addr, amount,
                        None, None, None, None, None, None
                    ).map_err(|e| DexError::Other(format!("BTC send_to_address: {:?}", e)))?;
                    w.onchain_balance -= amount;
                    self.store_wallet(w)?;
                } else {
                    return Err(DexError::Other("No BTC config found".into()));
                }
            }
            BlockchainType::Litecoin => {
                if let Some(cfg) = &self.ltc_cfg {
                    let auth = Auth::UserPass(cfg.rpc_user.clone(), cfg.rpc_pass.clone());
                    let client = Client::new(cfg.rpc_url.clone(), auth)
                        .map_err(|e| DexError::Other(format!("LTC client init err: {:?}", e)))?;
                    let parsed_addr = to_addr.parse()
                        .map_err(|_| DexError::Other("Bad LTC address to send".into()))?;
                    let _txid = client.send_to_address(
                        parsed_addr, amount,
                        None, None, None, None, None, None
                    ).map_err(|e| DexError::Other(format!("LTC send_to_address: {:?}", e)))?;
                    w.onchain_balance -= amount;
                    self.store_wallet(w)?;
                } else {
                    return Err(DexError::Other("No LTC config found".into()));
                }
            }
            BlockchainType::Ethereum => {
                if let Some(cfg) = &self.eth_cfg {
                    let provider = Provider::<Http>::try_from(cfg.rpc_url.clone())
                        .map_err(|e| DexError::Other(format!("ETH provider init err: {:?}", e)))?;
                    // Non-custodial => wir bräuchten local Key => sign => ...
                    // Minimales NotImplemented
                    return Err(DexError::Other("ETH send not yet implemented local-key-based.".into()));
                } else {
                    return Err(DexError::Other("No ETH config found".into()));
                }
            }
        }
        Ok(())
    }

    /// Erhöht Dex-Guthaben
    pub fn add_dex_balance(&self, wallet_id: &str, amount: f64) -> Result<(), DexError> {
        let mut w = self.load_wallet(wallet_id)?
            .ok_or(DexError::WalletNotFound(wallet_id.to_string()))?;
        w.dex_balance += amount;
        self.store_wallet(&w)?;
        Ok(())
    }

    /// Verringert Dex-Guthaben
    pub fn sub_dex_balance(&self, wallet_id: &str, amount: f64) -> Result<(), DexError> {
        let mut w = self.load_wallet(wallet_id)?
            .ok_or(DexError::WalletNotFound(wallet_id.to_string()))?;
        if w.dex_balance < amount {
            return Err(DexError::Other("Not enough dex_balance".into()));
        }
        w.dex_balance -= amount;
        self.store_wallet(&w)?;
        Ok(())
    }
}
