// src/cross_chain_demo.rs
//
// Zeigt, wie man 2 Orders matched, AtomicSwap "aufbaut"
// und Fees abzieht, stark vereinfacht.

use crate::dex_logic::orders::{Order, Asset};
use crate::dex_logic::fees::{FeeDistribution, calc_fee_distribution};
use crate::dex_logic::htlc::{HTLC, AtomicSwap, SwapState};
use sha2::{Sha256, Digest};

use anyhow::{Result};

pub fn cross_chain_example() -> Result<()> {
    // Buyer: 0.10 BTC => will LTC
    // Seller: 10 LTC => will BTC
    let buyer_order = Order {
        order_id: "order-btc-ltc-1".to_string(),
        user_id: "buyerA".to_string(),
        asset_sell: Asset::BTC,
        asset_buy: Asset::LTC,
        amount_sell: 0.10,
        price: 100.0,
    };
    let seller_order = Order {
        order_id: "order-ltc-btc-1".to_string(),
        user_id: "sellerB".to_string(),
        asset_sell: Asset::LTC,
        asset_buy: Asset::BTC,
        amount_sell: 10.0,
        price: 0.01,
    };

    // Fees => 0.05% from buyer, 0.05% from seller => total 0.1%
    let buyer_fee = buyer_order.amount_sell * 0.0005; // 0.0005 = 0.05%
    let seller_fee = seller_order.amount_sell * 0.0005;

    let fee_dist = FeeDistribution::new();
    let bf = calc_fee_distribution(buyer_fee, &fee_dist);
    let sf = calc_fee_distribution(seller_fee, &fee_dist);

    println!("Buyer Fee: {:.8} BTC => Founder: {:.8}, Dev: {:.8}, Node: {:.8}",
        buyer_fee, bf.founder_fee, bf.dev_fee, bf.node_fee);
    println!("Seller Fee: {:.8} LTC => Founder: {:.8}, Dev: {:.8}, Node: {:.8}",
        seller_fee, sf.founder_fee, sf.dev_fee, sf.node_fee);

    // Erzeuge Preimage
    let preimage = b"mysecretstuff";
    let mut hasher = Sha256::new();
    hasher.update(preimage);
    let hashlock = hasher.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&hashlock[..32]);

    // Buyer legt (0.10 - buyer_fee) BTC in HTLC, Seller legt (10 - seller_fee) LTC
    let buyer_htlc = HTLC::new(Asset::BTC, buyer_order.amount_sell - buyer_fee, arr, 1000);
    let seller_htlc = HTLC::new(Asset::LTC, seller_order.amount_sell - seller_fee, arr, 500);

    let mut swap = AtomicSwap::new(buyer_htlc, seller_htlc);

    // Seller redeem (BTC-HTLC) => legt preimage frei
    swap.seller_redeem(preimage)?;
    println!("Seller hat BTC redeem ausgef�hrt => Swap State: {:?}", swap.state);

    // Buyer redeem LTC => kann selbes preimage nutzen
    swap.buyer_redeem()?;
    println!("Buyer hat LTC redeem ausgef�hrt => Swap State: {:?}", swap.state);

    Ok(())
}
