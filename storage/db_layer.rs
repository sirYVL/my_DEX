// src/storage/db_layer.rs
//
// Datenbank-Layer mit Retry-Logik & Fallback (InMemoryDb).
// So kannst du DB-Open robust behandeln, Crash verhindern.

use anyhow::{Result, anyhow};
use rocksdb::{DB, Options, Direction, IteratorMode};
use serde::{Serialize, Deserialize};
use bincode;
use tracing::{info, debug, warn, instrument};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Minimale InMemory-DB => Fallback
#[derive(Default, Debug)]
pub struct InMemoryDb {
    store: std::collections::HashMap<String, Vec<u8>>,
}

impl InMemoryDb {
    fn put(&mut self, key: &str, val: Vec<u8>) {
        self.store.insert(key.to_string(), val);
    }
    fn get(&self, key: &str) -> Option<&[u8]> {
        self.store.get(key).map(|v| &v[..])
    }
    fn list_prefix(&self, prefix: &str) -> Vec<(String, Vec<u8>)> {
        let mut out = Vec::new();
        for (k, v) in &self.store {
            if k.starts_with(prefix) {
                out.push((k.clone(), v.clone()));
            }
        }
        out
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CrdtSnapshot {
    pub version: u64,
    pub data: Vec<u8>,
}

pub struct DexDB {
    pub rocks: Option<DB>,
    pub fallback_mem: Option<Arc<Mutex<InMemoryDb>>>,
}

impl DexDB {
    #[instrument(name="db_open", skip(path))]
    pub fn open(path: &str) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);

        let db = DB::open(&opts, path)
            .map_err(|e| anyhow!("RocksDB open error: {:?}", e))?;

        info!("DexDB: RocksDB open/created at path={}", path);

        Ok(DexDB {
            rocks: Some(db),
            fallback_mem: None,
        })
    }

    #[instrument(name="db_open_with_retries", skip(path, max_tries, backoff_sec))]
    pub fn open_with_retries(path: &str, max_tries: u32, backoff_sec: u64) -> Result<Self> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            match Self::open(path) {
                Ok(db) => {
                    return Ok(db);
                }
                Err(e) => {
                    warn!("DB open failed (attempt {}/{}): {:?}", attempt, max_tries, e);
                    if attempt >= max_tries {
                        warn!("Max DB attempts reached => fallback to in-memory DB!");
                        let mem = InMemoryDb::default();
                        return Ok(DexDB {
                            rocks: None,
                            fallback_mem: Some(Arc::new(Mutex::new(mem))),
                        });
                    } else {
                        thread::sleep(Duration::from_secs(backoff_sec));
                    }
                }
            }
        }
    }

    #[instrument(name="db_store_crdt_snapshot", skip(self, snapshot))]
    pub fn store_crdt_snapshot(&self, snapshot: &CrdtSnapshot) -> Result<()> {
        let key = format!("crdt_snapshot_v{}", snapshot.version);
        let encoded = bincode::serialize(snapshot)
            .map_err(|e| anyhow!("Serialize error: {:?}", e))?;

        if let Some(rdb) = &self.rocks {
            rdb.put(key.as_bytes(), &encoded)
                .map_err(|e| anyhow!("RocksDB put error: {:?}", e))?;
            debug!("store_crdt_snapshot => RocksDB key={}", key);
        } else if let Some(mem) = &self.fallback_mem {
            let mut lock = mem.lock().unwrap();
            lock.put(&key, encoded);
            debug!("store_crdt_snapshot => fallback_mem key={}", key);
        }
        Ok(())
    }

    #[instrument(name="db_load_crdt_snapshot", skip(self))]
    pub fn load_crdt_snapshot(&self, version: u64) -> Result<Option<CrdtSnapshot>> {
        let key = format!("crdt_snapshot_v{}", version);

        if let Some(rdb) = &self.rocks {
            match rdb.get(key.as_bytes()) {
                Ok(Some(bytes)) => {
                    let snap: CrdtSnapshot = bincode::deserialize(&bytes)
                        .map_err(|e| anyhow!("Deserialize error: {:?}", e))?;
                    return Ok(Some(snap));
                }
                Ok(None) => return Ok(None),
                Err(e) => return Err(anyhow!("RocksDB get error: {:?}", e)),
            }
        } else if let Some(mem) = &self.fallback_mem {
            let lock = mem.lock().unwrap();
            if let Some(bytes) = lock.get(&key) {
                let snap: CrdtSnapshot = bincode::deserialize(bytes)
                    .map_err(|e| anyhow!("Deserialize mem error: {:?}", e))?;
                return Ok(Some(snap));
            } else {
                return Ok(None);
            }
        }
        Ok(None)
    }

    #[instrument(name="db_list_crdt_snapshots", skip(self))]
    pub fn list_crdt_snapshots(&self) -> Result<Vec<CrdtSnapshot>> {
        let prefix = "crdt_snapshot_v";
        let mut out = Vec::new();

        if let Some(rdb) = &self.rocks {
            let mode = IteratorMode::From(prefix.as_bytes(), Direction::Forward);
            for item in rdb.iterator(mode) {
                let (k, v) = item.map_err(|e| anyhow!("iterator error: {:?}", e))?;
                if !k.starts_with(prefix.as_bytes()) {
                    break;
                }
                let snap: CrdtSnapshot = bincode::deserialize(&v)
                    .map_err(|e| anyhow!("deserialize in list: {:?}", e))?;
                out.push(snap);
            }
        } else if let Some(mem) = &self.fallback_mem {
            let lock = mem.lock().unwrap();
            let items = lock.list_prefix(prefix);
            for (k, v) in items {
                let snap: CrdtSnapshot = bincode::deserialize(&v)
                    .map_err(|e| anyhow!("deserialize fallback mem: {:?}", e))?;
                out.push(snap);
            }
        }

        debug!("list_crdt_snapshots => found {} snapshots", out.len());
        Ok(out)
    }
}
