///////////////////////////////////////////////////////////////////////////
/// my_DEX/src/storage/db_layer.rs
///////////////////////////////////////////////////////////////////////////

use anyhow::{Result, anyhow};
use rocksdb::{DB, Options, Direction, IteratorMode};
use serde::{Serialize, de::DeserializeOwned};
use tracing::{info, debug, warn, instrument};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::error::DexError;

#[derive(Default, Debug)]
pub struct InMemoryDb {
    pub store: std::collections::HashMap<String, Vec<u8>>,
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
    fn list_keys(&self) -> Vec<String> {
        self.store.keys().cloned().collect()
    }
}

#[derive(Debug)]
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

    /// Lesevorgang (generisch)
    pub fn load_struct<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>, DexError> {
        if let Some(rdb) = &self.rocks {
            match rdb.get(key.as_bytes()) {
                Ok(Some(bytes)) => {
                    let val: T = bincode::deserialize(&bytes)
                        .map_err(|e| DexError::Other(format!("deserialize error: {:?}", e)))?;
                    Ok(Some(val))
                }
                Ok(None) => Ok(None),
                Err(e) => Err(DexError::Other(format!("rocksdb get error: {:?}", e))),
            }
        } else if let Some(mem) = &self.fallback_mem {
            let lock = mem.lock().unwrap();
            if let Some(bytes) = lock.get(key) {
                let val: T = bincode::deserialize(bytes)
                    .map_err(|e| DexError::Other(format!("deserialize mem error: {:?}", e)))?;
                Ok(Some(val))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// Schreibvorgang (generisch)
    pub fn store_struct<T: Serialize>(&self, key: &str, val: &T) -> Result<(), DexError> {
        let encoded = bincode::serialize(val)
            .map_err(|e| DexError::Other(format!("serialize: {:?}", e)))?;
        if let Some(rdb) = &self.rocks {
            rdb.put(key.as_bytes(), encoded)
                .map_err(|e| DexError::Other(format!("rocksdb put: {:?}", e)))?;
        } else if let Some(mem) = &self.fallback_mem {
            let mut lock = mem.lock().unwrap();
            lock.put(key, encoded);
        }
        Ok(())
    }

    /// Key-Liste mit Prefix
    pub fn list_keys_with_prefix(&self, prefix: &str) -> Result<Vec<String>, DexError> {
        let mut out = Vec::new();
        if let Some(rdb) = &self.rocks {
            let mode = IteratorMode::From(prefix.as_bytes(), Direction::Forward);
            for item in rdb.iterator(mode) {
                let (k, _v) = item.map_err(|e| DexError::Other(format!("iterator error: {:?}", e)))?;
                if !k.starts_with(prefix.as_bytes()) {
                    break;
                }
                out.push(String::from_utf8_lossy(&k).to_string());
            }
        } else if let Some(mem) = &self.fallback_mem {
            let lock = mem.lock().unwrap();
            for (k, _v) in lock.store.iter() {
                if k.starts_with(prefix) {
                    out.push(k.clone());
                }
            }
        }
        Ok(out)
    }
}
