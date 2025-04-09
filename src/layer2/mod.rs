// my_dex/src/layer2/mod.rs

pub mod lightning;
pub mod atomic_swap;
pub mod delta_gossip;
pub mod watchtower;
pub mod fees;

use anyhow::Result;
use log::info;

pub struct Layer2DEX {
    pub lightning_node: lightning::LightningNode,
    pub atomic_swap: atomic_swap::AtomicSwap,
    pub fee_pool: fees::FeePool,
    pub delta_gossip: delta_gossip::DeltaGossip,
    pub watchtower_service: watchtower::Watchtower,
}

impl Layer2DEX {
    /// Initialisiert alle Layer?2-Komponenten mit den �bergebenen Parametern.
    pub fn new(
        fees_initial: u64,
        dev_share: u8,
        node_share: u8,
        gossip_addr: String,
        watchtower_interval: u64,
    ) -> Self {
        Self {
            lightning_node: lightning::LightningNode::new(),
            atomic_swap: atomic_swap::AtomicSwap::new(),
            fee_pool: fees::FeePool::new(fees_initial, dev_share, node_share),
            delta_gossip: delta_gossip::DeltaGossip::new(gossip_addr),
            watchtower_service: watchtower::Watchtower::new(watchtower_interval),
        }
    }
    
    /// F�hrt die Initialisierung aller Komponenten aus.
    pub async fn initialize(&self) -> Result<()> {
        self.lightning_node.discover_peers()?;
        self.lightning_node.manage_channels()?;
        self.lightning_node.process_payment()?;
        info!("Layer2DEX initialization complete.");
        Ok(())
    }
    
    /// Verarbeitet einen Trade, inklusive Delta-Update, Atomic Swap und Geb�hrenverteilung.
    pub async fn process_trade(&self, delta: &str) -> Result<()> {
        self.delta_gossip.send_delta("127.0.0.1:9001", delta).await?;
        
        use secp256k1::Secp256k1;
        use secp256k1::SecretKey;
        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_slice(&[1; 32])?;
        let public_key = secp256k1::PublicKey::from_secret_key(&secp, &secret_key);
        
        use sha2::{Sha256, Digest};
        let preimage = b"test";
        let mut hasher = Sha256::new();
        hasher.update(preimage);
        let hash_lock: [u8;32] = hasher.finalize().into();
        
        self.atomic_swap.initiate(secret_key, public_key, hash_lock, 3600)?;
        self.atomic_swap.complete(preimage)?;
        
        self.fee_pool.add_fee(10)?;
        self.fee_pool.distribute()?;
        Ok(())
    }
}
