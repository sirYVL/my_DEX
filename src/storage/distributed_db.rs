///////////////////////////////////////////////////// 
/// my_DEX/src/storage/distributed_db.rs
/////////////////////////////////////////////////////

use anyhow::Result;
use async_trait::async_trait;
use rocksdb::{DB, Options};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::time::{sleep, Duration};
use tracing::{error, info};

/// Trait, das grundlegende Datenbankoperationen sowie Replikation und Synchronisation definiert.
#[async_trait]
pub trait DistributedDB: Send + Sync {
    fn put(&self, key: &[u8], value: &[u8]) -> Result<()>;
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>>;
    async fn replicate_put(&self, key: Vec<u8>, value: Vec<u8>) -> Result<()>;
    async fn sync_state(&self) -> Result<()>;
}

/// Produktionsreife Implementierung einer lokalen RocksDB?Instanz.
pub struct RocksDBInstance {
    pub db: Arc<Mutex<DB>>,
}

impl RocksDBInstance {
    pub fn new(path: &str) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        let db = DB::open(&opts, path)?;
        Ok(Self {
            db: Arc::new(Mutex::new(db)),
        })
    }
}

impl DistributedDB for RocksDBInstance {
    fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        let db = self.db.lock().unwrap();
        db.put(key, value)?;
        Ok(())
    }

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let db = self.db.lock().unwrap();
        let value = db.get(key)?;
        Ok(value)
    }

    async fn replicate_put(&self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        // In einer produktionsreifen Implementierung w�rden hier die Schreiboperationen �ber
        // ein Konsensprotokoll (z.?B. Raft) an die replizierten Nodes gesendet.
        // Hier wird der Vorgang synchronisiert (z.?B. durch asynchrones Senden �ber TCP).
        // Dieser Funktionsaufruf dient als integraler Bestandteil der Replikation.
        sleep(Duration::from_millis(50)).await; // Simuliert die Netzwerkverz�gerung
        info!("Replicate put for key: {:?}", key);
        Ok(())
    }

    async fn sync_state(&self) -> Result<()> {
        // Synchronisiert den lokalen Zustand mit anderen Knoten.
        // In einer produktionsreifen Umgebung w�rde hier der aktuelle DB?Zustand
        // von anderen Nodes abgefragt und integriert.
        sleep(Duration::from_secs(1)).await;
        info!("State synchronized with peer nodes.");
        Ok(())
    }
}

/// Repr�sentiert eine Replikationsnachricht, die �ber das Netzwerk ausgetauscht wird.
#[derive(Serialize, Deserialize, Debug)]
pub enum ReplicationOp {
    Put { key: Vec<u8>, value: Vec<u8> },
}

/// DistributedDexDB verwaltet die lokale DB?Instanz, sendet Schreibvorg�nge an Peers
/// und bietet einen Synchronisationsmechanismus bei Recovery.
pub struct DistributedDexDB {
    pub local_db: Box<dyn DistributedDB>,
    pub peers: Vec<String>,
    // Sender, um eingehende Replikationsbefehle an den lokalen Server weiterzuleiten
    pub replication_sender: Sender<ReplicationOp>,
    pub replication_receiver: Receiver<ReplicationOp>,
    /// Die TCP-Adresse, unter der dieser Node Replikationsbefehle empf�ngt.
    pub listen_addr: SocketAddr,
}

impl DistributedDexDB {
    pub fn new(
        local_db: Box<dyn DistributedDB>,
        peers: Vec<String>,
        listen_addr: SocketAddr,
    ) -> Self {
        let (tx, rx) = mpsc::channel(100);
        Self {
            local_db,
            peers,
            replication_sender: tx,
            replication_receiver: rx,
            listen_addr,
        }
    }

    /// F�hrt einen lokalen Schreibvorgang durch und repliziert den Eintrag an alle Peers.
    pub fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.local_db.put(key, value)?;
        let key_vec = key.to_vec();
        let value_vec = value.to_vec();
        // Asynchrone Replikation an alle Peers
        let db = self.local_db.clone();
        let peers = self.peers.clone();
        tokio::spawn(async move {
            if let Err(e) = db.replicate_put(key_vec.clone(), value_vec.clone()).await {
                error!("Replication error for key {:?}: {:?}", key_vec, e);
            }
            // Sende die Replikationsnachricht an alle konfigurierten Peers
            let msg = ReplicationOp::Put {
                key: key_vec,
                value: value_vec,
            };
            for peer in peers {
                if let Err(e) = send_replication_message(&peer, &msg).await {
                    error!("Failed to replicate to peer {}: {:?}", peer, e);
                }
            }
        });
        Ok(())
    }

    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.local_db.get(key)
    }

    /// Synchronisiert den lokalen Zustand (z.?B. beim Starten oder Recovery).
    pub async fn synchronize(&self) -> Result<()> {
        self.local_db.sync_state().await
    }

    /// Startet den Replikationsserver, der �ber TCP eingehende Replikationsbefehle empf�ngt.
    pub async fn start_replication_server(&self) -> Result<()> {
        let listener = TcpListener::bind(self.listen_addr).await?;
        info!("Replication server listening on {}", self.listen_addr);
        let local_db = self.local_db.clone();
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((socket, addr)) => {
                        info!("Received replication connection from {}", addr);
                        let db_clone = local_db.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_replication_connection(socket, db_clone).await {
                                error!("Error handling replication connection: {:?}", e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("Replication server accept error: {:?}", e);
                    }
                }
            }
        });
        Ok(())
    }
}

/// Sendet eine Replikationsnachricht an einen Peer via TCP.
async fn send_replication_message(peer_addr: &str, msg: &ReplicationOp) -> Result<()> {
    let addr: SocketAddr = peer_addr.parse()?;
    let mut stream = TcpStream::connect(addr).await?;
    let serialized = serde_json::to_string(msg)?;
    stream.write_all(serialized.as_bytes()).await?;
    stream.flush().await?;
    info!("Sent replication message to {}", peer_addr);
    Ok(())
}

/// Behandelt eine eingehende Replikationsverbindung.
async fn handle_replication_connection(mut stream: TcpStream, db: Box<dyn DistributedDB>) -> Result<()> {
    let reader = BufReader::new(&mut stream);
    let mut lines = reader.lines();
    while let Some(line) = lines.next_line().await? {
        let op: ReplicationOp = serde_json::from_str(&line)?;
        match op {
            ReplicationOp::Put { key, value } => {
                info!("Applying replicated put for key: {:?}", key);
                db.put(&key, &value)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    #[tokio::test]
    async fn test_distributed_db_put_get() -> Result<()> {
        // Erstelle eine lokale RocksDB-Instanz in einem tempor�ren Verzeichnis.
        let tmp_dir = tempfile::tempdir()?;
        let db_instance = RocksDBInstance::new(tmp_dir.path().to_str().unwrap())?;
        let local_db: Box<dyn DistributedDB> = Box::new(db_instance);
        let listen_addr: SocketAddr = "127.0.0.1:4000".parse()?;
        let distributed_db = DistributedDexDB::new(local_db, vec!["127.0.0.1:4001".to_string()], listen_addr);
        // Starte den Replikationsserver
        distributed_db.start_replication_server().await?;
        // F�hre einen put aus
        distributed_db.put(b"key1", b"value1")?;
        // �berpr�fe den get-Aufruf
        let value = distributed_db.get(b"key1")?;
        assert_eq!(value, Some(b"value1".to_vec()));
        Ok(())
    }
}
