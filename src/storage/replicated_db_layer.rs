///////////////////////////////////////////////////////////
// my_dex/src/storage/replicated_db_layer.rs
///////////////////////////////////////////////////////////
//
// Dieses Modul erweitert den lokalen RocksDB-Datenbank-Layer (mit In-Memory-Fallback)
// um Mechanismen zur Replikation und Synchronisation des Zustands.
// Es ermöglicht das Speichern, Laden und Auflisten von CRDT-Snapshots, die den Zustand
// der Node repräsentieren. Zusätzlich gibt es eine asynchrone Funktion, die als Basis für
// einen P2P-Gossip-Prozess dient, um den Zustand zwischen den Nodes kontinuierlich zu synchronisieren.
//
// Hinweis: In einer echten Produktionsumgebung würden Sie hier einen robusten,
// p2p-gestützten Replikationsmechanismus implementieren (ggf. über ein verteiltes DB-System),
// aber dieser Ansatz bildet eine solide Basis für einen vollständig dezentralen DEX.
///////////////////////////////////////////////////////////

use anyhow::Result;
use rocksdb::{DB, Options, Direction, IteratorMode};
use serde::{Serialize, Deserialize};
use bincode;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, debug, warn, error};

/// CRDT-Snapshot repräsentiert den Zustand der Datenbank.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CrdtSnapshot {
    pub version: u64,
    pub data: Vec<u8>,
}

/// Eine einfache In-Memory-Datenbank als Fallback.
#[derive(Default, Debug)]
pub struct InMemoryDb {
    store: HashMap<String, Vec<u8>>,
}

impl InMemoryDb {
    pub fn put(&mut self, key: &str, val: Vec<u8>) {
        self.store.insert(key.to_string(), val);
    }
    pub fn get(&self, key: &str) -> Option<&[u8]> {
        self.store.get(key).map(|v| &v[..])
    }
    pub fn list_prefix(&self, prefix: &str) -> Vec<(String, Vec<u8>)> {
        self.store.iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}

/// DexDB kapselt entweder eine RocksDB-Instanz oder einen In-Memory-Fallback.
pub struct DexDB {
    pub rocks: Option<DB>,
    pub fallback_mem: Option<Arc<Mutex<InMemoryDb>>>,

    // NEU => optional KademliaService, um beidseitig Snapshots zu verschicken
    pub kademlia: Option<Arc<Mutex<crate::kademlia::kademlia_service::KademliaService>>>,
}

impl DexDB {
    /// Öffnet eine RocksDB-Instanz am angegebenen Pfad.
    pub fn open(path: &str) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        let db = DB::open(&opts, path)?;
        info!("RocksDB geöffnet/erstellt an Pfad: {}", path);
        Ok(DexDB {
            rocks: Some(db),
            fallback_mem: None,
            kademlia: None,
        })
    }

    /// Versucht, die Datenbank mit Retries zu öffnen. Bei Scheitern wird auf eine
    /// In-Memory-Datenbank zurückgegriffen.
    pub fn open_with_retries(path: &str, max_tries: u32, backoff_sec: u64) -> Result<Self> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            match Self::open(path) {
                Ok(db) => return Ok(db),
                Err(e) => {
                    warn!("DB-Öffnen fehlgeschlagen (Versuch {}/{}): {:?}", attempt, max_tries, e);
                    if attempt >= max_tries {
                        warn!("Maximale Versuche erreicht, Fallback auf In-Memory DB.");
                        return Ok(DexDB {
                            rocks: None,
                            fallback_mem: Some(Arc::new(Mutex::new(InMemoryDb::default()))),
                            kademlia: None,
                        });
                    }
                    thread::sleep(Duration::from_secs(backoff_sec));
                }
            }
        }
    }

    /// Speichert einen CRDT-Snapshot in der Datenbank.
    pub fn store_crdt_snapshot(&self, snapshot: &CrdtSnapshot) -> Result<()> {
        let key = format!("crdt_snapshot_v{}", snapshot.version);
        let encoded = bincode::serialize(snapshot)?;
        if let Some(rdb) = &self.rocks {
            rdb.put(key.as_bytes(), &encoded)?;
            debug!("Snapshot in RocksDB gespeichert: {}", key);
        } else if let Some(mem) = &self.fallback_mem {
            let mut lock = mem.lock().unwrap();
            lock.put(&key, encoded);
            debug!("Snapshot im InMemoryDB gespeichert: {}", key);
        }
        Ok(())
    }

    /// Lädt einen CRDT-Snapshot anhand der Versionsnummer.
    pub fn load_crdt_snapshot(&self, version: u64) -> Result<Option<CrdtSnapshot>> {
        let key = format!("crdt_snapshot_v{}", version);
        if let Some(rdb) = &self.rocks {
            match rdb.get(key.as_bytes())? {
                Some(bytes) => {
                    let snap: CrdtSnapshot = bincode::deserialize(&bytes)?;
                    Ok(Some(snap))
                },
                None => Ok(None),
            }
        } else if let Some(mem) = &self.fallback_mem {
            let lock = mem.lock().unwrap();
            if let Some(bytes) = lock.get(&key) {
                let snap: CrdtSnapshot = bincode::deserialize(bytes)?;
                Ok(Some(snap))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// Listet alle gespeicherten CRDT-Snapshots auf.
    pub fn list_crdt_snapshots(&self) -> Result<Vec<CrdtSnapshot>> {
        let mut out = Vec::new();
        let prefix = "crdt_snapshot_v";
        if let Some(rdb) = &self.rocks {
            let mode = IteratorMode::From(prefix.as_bytes(), Direction::Forward);
            for item in rdb.iterator(mode) {
                let (k, v) = item?;
                if !k.starts_with(prefix.as_bytes()) {
                    break;
                }
                let snap: CrdtSnapshot = bincode::deserialize(&v)?;
                out.push(snap);
            }
        } else if let Some(mem) = &self.fallback_mem {
            let lock = mem.lock().unwrap();
            for (k, v) in lock.list_prefix(prefix) {
                let snap: CrdtSnapshot = bincode::deserialize(&v)?;
                out.push(snap);
            }
        }
        debug!("Anzahl gefundener Snapshots: {}", out.len());
        Ok(out)
    }

    /// Synchronisiert den lokalen Zustand mit den Snapshots eines entfernten Nodes.
    pub fn sync_with_remote(&self, remote_snapshots: Vec<CrdtSnapshot>) -> Result<()> {
        if let Some(rdb) = &self.rocks {
            for snap in remote_snapshots {
                let key = format!("crdt_snapshot_v{}", snap.version);
                if rdb.get(key.as_bytes())?.is_none() {
                    rdb.put(key.as_bytes(), &bincode::serialize(&snap)?)?;
                    debug!("Remote Snapshot hinzugefügt: {}", key);
                } else {
                    debug!("Remote Snapshot existiert bereits: {}", key);
                }
            }
        } else if let Some(mem) = &self.fallback_mem {
            let mut lock = mem.lock().unwrap();
            for snap in remote_snapshots {
                let key = format!("crdt_snapshot_v{}", snap.version);
                if lock.get(&key).is_none() {
                    lock.put(&key, bincode::serialize(&snap)?);
                    debug!("Remote Snapshot hinzugefügt (Fallback): {}", key);
                }
            }
        }
        Ok(())
    }

    /// Extrahiert alle lokalen Snapshots, um sie an andere Nodes zu replizieren.
    pub fn replicate_state(&self) -> Result<Vec<CrdtSnapshot>> {
        let mut snapshots = Vec::new();
        let prefix = "crdt_snapshot_v";
        if let Some(rdb) = &self.rocks {
            let mode = IteratorMode::From(prefix.as_bytes(), Direction::Forward);
            for item in rdb.iterator(mode) {
                let (k, v) = item?;
                if !k.starts_with(prefix.as_bytes()) {
                    break;
                }
                let snap: CrdtSnapshot = bincode::deserialize(&v)?;
                snapshots.push(snap);
            }
        } else if let Some(mem) = &self.fallback_mem {
            let lock = mem.lock().unwrap();
            for (k, v) in lock.list_prefix(prefix) {
                let snap: CrdtSnapshot = bincode::deserialize(&v)?;
                snapshots.push(snap);
            }
        }
        Ok(snapshots)
    }

    /// Asynchrone Funktion, die periodisch den lokalen Zustand repliziert und
    /// mit entfernten Snapshots synchronisiert. Diese Funktion dient als Basis
    /// für einen p2p-Gossip-Mechanismus.
    pub async fn run_gossip_sync(&self) -> Result<()> {
        loop {
            // 1) Extrahiere lokale Snapshots
            let local_snapshots = self.replicate_state()?;

            // 2) In einer echten Implementierung würdest du normal
            // remote Snapshots vom Netzwerk empfangen => sync_with_remote(...)

            // NEU => beidseitige Synchronisierung:
            //    Wir schicken hier unsere Snapshots an Peers (über KademliaMessage::CrdtSnapshots)
            if let Some(ref kad_service) = self.kademlia {
                let kad = kad_service.lock().unwrap();
                // Wir holen z.B. die 20 nächsten Peers
                let peers = kad.table.find_closest(&kad.local_id, 20);
                for (_, addr) in peers {
                    let msg = crate::kademlia::kademlia_service::KademliaMessage::CrdtSnapshots(local_snapshots.clone());
                    kad.send_msg(addr, &msg);
                }
            } else {
                debug!("run_gossip_sync => kademlia is None => can't push local snapshots");
            }

            debug!("Gossip-Synchronisation abgeschlossen => nun 60s schlafen.");
            sleep(Duration::from_secs(60)).await;
        }
    }

    // NEU => set_kademlia, damit wir aus node_logic (oder main) dem DexDB die KademliaService referenzieren:
    pub fn set_kademlia(&mut self, kad: Arc<Mutex<crate::kademlia::kademlia_service::KademliaService>>) {
        self.kademlia = Some(kad);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_inmemory_db_put_get() {
        let mut db = InMemoryDb::default();
        db.put("key1", vec![1, 2, 3]);
        let value = db.get("key1");
        assert!(value.is_some());
        assert_eq!(value.unwrap(), &[1, 2, 3]);
    }
    
    #[test]
    fn test_sync_with_remote() {
        let snap = CrdtSnapshot {
            version: 1,
            data: vec![1, 2, 3, 4],
        };
        let mem_db = Arc::new(Mutex::new(InMemoryDb::default()));
        let dex_db = DexDB {
            rocks: None,
            fallback_mem: Some(mem_db.clone()),
            kademlia: None,
        };
        let remote_snapshots = vec![snap.clone()];
        assert!(dex_db.sync_with_remote(remote_snapshots).is_ok());
        let loaded = dex_db.load_crdt_snapshot(1).unwrap();
        assert!(loaded.is_some());
    }

    #[tokio::test]
    async fn test_run_gossip_sync() {
        let mem_db = Arc::new(Mutex::new(InMemoryDb::default()));
        let dex_db = DexDB {
            rocks: None,
            fallback_mem: Some(mem_db.clone()),
            kademlia: None, // im Test kein Kademlia
        };
        // Füge einen Snapshot hinzu, damit etwas synchronisiert wird.
        let snap = CrdtSnapshot {
            version: 1,
            data: vec![10, 20, 30],
        };
        dex_db.store_crdt_snapshot(&snap).unwrap();
        // Starte die Gossip-Synchronisation für eine kurze Zeit.
        tokio::spawn(async move {
            let _ = dex_db.run_gossip_sync().await;
        });
        sleep(Duration::from_secs(2)).await;
        // Testen, ob der Snapshot repliziert werden kann.
        let snapshots = dex_db.replicate_state().unwrap();
        assert!(!snapshots.is_empty());
    }
}
