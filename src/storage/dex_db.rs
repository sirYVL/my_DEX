///////////////////////////////////////////////////////////////////////////
/// my_DEX/src/storage/dex_db.rs
///////////////////////////////////////////////////////////////////////////

//! DexDB implementiert eine persistente Speicherung via RocksDB
//! mit Column Families, sowie einen optionalen In-Memory-Fallback,
//! falls das Öffnen von RocksDB mehrfach fehlschlägt.
//!
//! Die API stellt einfache PUT/GET/DELETE-Methoden bereit. Je nach Bedarf
//! kannst du weitere Helpers ergänzen (z.B. list_keys_in_cf usw.).

use anyhow::{anyhow, Result};
use rocksdb::{
    DB, Options, ColumnFamilyDescriptor, ColumnFamily, Direction, IteratorMode
};
use std::path::Path;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// InMemory fallback (HashMap).
/// Wir verwenden Strings als Key, Vec<u8> als Wert.
#[derive(Default, Debug)]
pub struct InMemoryDb {
    pub store: HashMap<String, Vec<u8>>,
}

impl InMemoryDb {
    pub fn put(&mut self, key: &str, val: Vec<u8>) {
        self.store.insert(key.to_string(), val);
    }
    pub fn get(&self, key: &str) -> Option<&[u8]> {
        self.store.get(key).map(|v| &v[..])
    }
    pub fn delete(&mut self, key: &str) {
        self.store.remove(key);
    }
    /// Ein einfaches Iterator-Beispiel (optional)
    pub fn list_prefix(&self, prefix: &str) -> Vec<(String, Vec<u8>)> {
        self.store
            .iter()
            .filter(|(k, _v)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}

/// DexDB – umschließt RocksDB und optionalen InMemory-Fallback.
/// Für persistente Nutzung legen wir in der Regel ColumnFamilies an.
/// Wenn das Öffnen von RocksDB mehrfach scheitert, verwenden wir fallback_mem.
pub struct DexDB {
    /// Entweder Some(DB), oder None => fallback_mem nutzen.
    pub db: Option<Arc<DB>>,
    pub fallback_mem: Option<Arc<Mutex<InMemoryDb>>>,
    // hier könntest du z.B. ColumnFamily-Handles ablegen:
    pub default_cf: Option<ColumnFamily>,
    // ... weitere CFs, falls du willst
}

impl DexDB {
    /// Öffnet die RocksDB am Pfad path. Erzeugt ColumnFamilies, falls nötig.
    /// Gibt bei Erfolg Ok(DexDB { db: Some(...), fallback_mem: None }) zurück.
    pub fn open(path: &str) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);

        // Falls du mehr CFs willst, definierst du sie hier:
        let cfs = vec![
            ColumnFamilyDescriptor::new("default", Options::default()),
            // ColumnFamilyDescriptor::new("accounts", Options::default()),
            // ColumnFamilyDescriptor::new("wallets", Options::default()),
            // ...
        ];

        let db = DB::open_cf_descriptors(&opts, path, cfs)?;

        // Hole CF-Handles:
        let default_cf = db.cf_handle("default")
            .ok_or_else(|| anyhow!("CF default missing"))?;

        Ok(DexDB {
            db: Some(Arc::new(db)),
            fallback_mem: None,
            default_cf: Some(default_cf),
        })
    }

    /// Öffnet mit mehreren Retries. Falls alle fehlschlagen, wird ein
    /// InMemory-Fallback initialisiert, um den Node notfalls offline
    /// weiterlaufen zu lassen.
    pub fn open_with_retries(path: &str, max_retries: u32, backoff_secs: u64) -> Result<Self> {
        use std::thread;
        let mut attempts = 0;
        loop {
            attempts += 1;
            match Self::open(path) {
                Ok(db) => {
                    return Ok(db);
                },
                Err(e) => {
                    if attempts >= max_retries {
                        eprintln!("RocksDB öffnen fehlgeschlagen nach {} Versuchen: {:?}", attempts, e);
                        eprintln!("=> Fallback auf InMemoryDb!");
                        let mem = InMemoryDb::default();
                        return Ok(DexDB {
                            db: None,
                            fallback_mem: Some(Arc::new(Mutex::new(mem))),
                            default_cf: None,
                        });
                    } else {
                        eprintln!("RocksDB öffnen fehlgeschlagen (Versuch {}/{}): {:?}. Warte {}s.", attempts, max_retries, e, backoff_secs);
                        thread::sleep(std::time::Duration::from_secs(backoff_secs));
                    }
                }
            }
        }
    }

    /// Put – schreibt in die DB (oder fallback).
    /// - cf_name => z. B. "default"
    /// - key & val => Bytes
    pub fn put(&self, cf_name: &str, key: &[u8], val: &[u8]) -> Result<()> {
        if let Some(db) = &self.db {
            if let Some(cf) = db.cf_handle(cf_name) {
                db.put_cf(cf, key, val)?;
            } else {
                return Err(anyhow!("ColumnFamily not found: {}", cf_name));
            }
        } else if let Some(mem) = &self.fallback_mem {
            let mut lock = mem.lock().unwrap();
            let composed_key = format!("{}|{}", cf_name, String::from_utf8_lossy(key));
            lock.put(&composed_key, val.to_vec());
        }
        Ok(())
    }

    /// Get
    pub fn get(&self, cf_name: &str, key: &[u8]) -> Result<Option<Vec<u8>>> {
        if let Some(db) = &self.db {
            if let Some(cf) = db.cf_handle(cf_name) {
                match db.get_cf(cf, key)? {
                    Some(bytes) => Ok(Some(bytes.to_vec())),
                    None => Ok(None),
                }
            } else {
                Err(anyhow!("ColumnFamily not found: {}", cf_name))
            }
        } else if let Some(mem) = &self.fallback_mem {
            let lock = mem.lock().unwrap();
            let composed_key = format!("{}|{}", cf_name, String::from_utf8_lossy(key));
            match lock.get(&composed_key) {
                Some(bytes) => Ok(Some(bytes.to_vec())),
                None => Ok(None),
            }
        } else {
            Ok(None)
        }
    }

    /// Delete
    pub fn delete(&self, cf_name: &str, key: &[u8]) -> Result<()> {
        if let Some(db) = &self.db {
            if let Some(cf) = db.cf_handle(cf_name) {
                db.delete_cf(cf, key)?;
            } else {
                return Err(anyhow!("ColumnFamily not found: {}", cf_name));
            }
        } else if let Some(mem) = &self.fallback_mem {
            let mut lock = mem.lock().unwrap();
            let composed_key = format!("{}|{}", cf_name, String::from_utf8_lossy(key));
            lock.delete(&composed_key);
        }
        Ok(())
    }

    /// Optionale Auflistung aller Schlüssel in einer CF (Beispiel):
    pub fn list_keys_in_cf(&self, cf_name: &str) -> Result<Vec<Vec<u8>>> {
        let mut result = Vec::new();
        if let Some(db) = &self.db {
            if let Some(cf) = db.cf_handle(cf_name) {
                let iter_mode = IteratorMode::Start; // ab Start
                let iter = db.iterator_cf(cf, iter_mode)?;
                for item_res in iter {
                    let (key_bytes, _val_bytes) = item_res?;
                    result.push(key_bytes.to_vec());
                }
            } else {
                return Err(anyhow!("ColumnFamily not found: {}", cf_name));
            }
        } else if let Some(mem) = &self.fallback_mem {
            let lock = mem.lock().unwrap();
            for (k, _v) in &lock.store {
                if let Some(idx) = k.find('|') {
                    let (cf_part, key_part) = k.split_at(idx);
                    if cf_part == cf_name {
                        // skip '|'
                        let real_key = &key_part[1..];
                        result.push(real_key.as_bytes().to_vec());
                    }
                }
            }
        }
        Ok(result)
    }
}
