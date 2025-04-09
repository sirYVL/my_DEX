// my_dex/src/decentralized_order_book/settlement.rs

use std::collections::HashMap;
use crate::assets::{Asset, base_to_subunits, subunits_to_base};

/// Multi-Asset Settlement (Escrow).
/// Für jeden User und jedes Asset wird unterschieden zwischen `free` und `locked` (beides als u128).
#[derive(Debug)]
pub struct SettlementEngine {
    /// Struktur: user_id => (asset => (free, locked))
    balances: HashMap<String, HashMap<Asset, (u128, u128)>>,
}

impl SettlementEngine {
    pub fn new() -> Self {
        Self {
            balances: HashMap::new(),
        }
    }

    fn ensure_user(&mut self, user_id: &str) {
        self.balances.entry(user_id.to_string()).or_insert_with(HashMap::new);
    }

    /// Erstellt ein (leeres) Konto für den user.
    pub fn create_account(&mut self, user_id: &str) {
        self.ensure_user(user_id);
    }

    /// Zahlt base_amount (f64) in die freie Balance ein (z. B. initiale Gutschrift).
    pub fn deposit(&mut self, user_id: &str, asset: Asset, base_amount: f64) {
        self.ensure_user(user_id);
        let sub = base_to_subunits(&asset, base_amount);
        let asset_map = self.balances.get_mut(user_id).unwrap();
        let entry = asset_map.entry(asset).or_insert((0, 0));
        entry.0 += sub; // free += sub
    }

    /// Sperrt `base_amount` vom free-Konto => locked-Konto.
    /// Gibt `true` zurück, wenn genug `free` vorhanden war.
    pub fn lock_funds(&mut self, user_id: &str, asset: Asset, base_amount: f64) -> bool {
        self.ensure_user(user_id);
        let sub = base_to_subunits(&asset, base_amount);
        let asset_map = self.balances.get_mut(user_id).unwrap();
        let entry = asset_map.entry(asset).or_insert((0, 0));

        if entry.0 >= sub {
            entry.0 -= sub;
            entry.1 += sub;
            true
        } else {
            false
        }
    }

    /// Gibt `base_amount` aus dem locked-Bereich wieder frei,
    /// indem es zurück in den free-Bereich gebucht wird.
    pub fn release_funds(&mut self, user_id: &str, asset: Asset, base_amount: f64) -> bool {
        self.ensure_user(user_id);
        let sub = base_to_subunits(&asset, base_amount);
        let asset_map = self.balances.get_mut(user_id).unwrap();
        let entry = asset_map.entry(asset).or_insert((0, 0));

        if entry.1 >= sub {
            entry.1 -= sub;
            entry.0 += sub;
            true
        } else {
            false
        }
    }

    /// Finalisiert den Trade:
    /// - Buyer hat locked "quote_asset", Seller hat locked "base_asset".
    /// - Buyer bekommt `base_amount` an `base_asset` => free,
    /// - Seller bekommt `quote_amount` an `quote_asset` => free.
    ///
    /// `base_amount` und `quote_amount` sind f64 in "Basis"-Einheiten
    /// (z. B. 1.0 BTC, 30000.0 USDT).
    /// Wir konvertieren sie in subunits und ziehen sie vom locked-Bestand ab.
    pub fn finalize_trade(
        &mut self,
        buyer_id: &str,
        seller_id: &str,
        base_asset: Asset,
        quote_asset: Asset,
        base_amount: f64,
        quote_amount: f64,
    ) -> bool {
        self.ensure_user(buyer_id);
        self.ensure_user(seller_id);

        let base_sub = base_to_subunits(&base_asset, base_amount);
        let quote_sub = base_to_subunits(&quote_asset, quote_amount);

        let buyer_map = self.balances.get_mut(buyer_id).unwrap();
        let seller_map = self.balances.get_mut(seller_id).unwrap();

        // Buyer => locked quote_asset
        let buyer_entry = buyer_map.entry(quote_asset.clone()).or_insert((0, 0));
        // Seller => locked base_asset
        let seller_entry = seller_map.entry(base_asset.clone()).or_insert((0, 0));

        if buyer_entry.1 < quote_sub {
            return false;
        }
        if seller_entry.1 < base_sub {
            return false;
        }

        // locked abziehen
        buyer_entry.1 -= quote_sub;
        seller_entry.1 -= base_sub;

        // buyer kriegt base_asset in free
        let buyer_base = buyer_map.entry(base_asset).or_insert((0, 0));
        buyer_base.0 += base_sub;

        // seller kriegt quote_asset in free
        let seller_quote = seller_map.entry(quote_asset).or_insert((0, 0));
        seller_quote.0 += quote_sub;

        true
    }

    /// Gibt das gesamte Balancing aus (zu Debug-Zwecken).
    pub fn print_balances(&self) {
        println!("=== Settlement Balances ===");
        for (user, asset_map) in &self.balances {
            print!("User: {} => ", user);
            for (asset, (free, locked)) in asset_map {
                let free_base = subunits_to_base(asset, *free);
                let locked_base = subunits_to_base(asset, *locked);
                print!("[{}: free={}, locked={}], ", asset, free_base, locked_base);
            }
            println!();
        }
        println!("===========================");
    }
}
