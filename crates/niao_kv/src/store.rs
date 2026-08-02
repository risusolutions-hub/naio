//! Database and transaction wrappers over redb.

use crate::error::{KvError, KvResult};
use crate::scan::{prefix_end, ScanOptions, ScanPair};
use redb::{
    backends::InMemoryBackend, Database, ReadableDatabase, ReadableTable, ReadableTableMetadata,
    TableDefinition, TableHandle,
};
use std::collections::HashMap;
use std::ops::Bound;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Default table name used when callers omit `table`.
pub const DEFAULT_TABLE: &str = "main";

fn intern_name(name: &str) -> KvResult<&'static str> {
    if name.is_empty() {
        return Err(KvError::Invalid("table name must not be empty".into()));
    }
    static CACHE: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = cache.lock().map_err(|e| KvError::Store(e.to_string()))?;
    if let Some(s) = map.get(name) {
        return Ok(*s);
    }
    let leaked: &'static str = Box::leak(name.to_string().into_boxed_str());
    map.insert(name.to_string(), leaked);
    Ok(leaked)
}

fn table_def(name: &str) -> KvResult<TableDefinition<'static, &'static [u8], &'static [u8]>> {
    Ok(TableDefinition::new(intern_name(name)?))
}

/// Storage statistics snapshot.
#[derive(Debug, Clone)]
pub struct KvStats {
    pub tree_height: u32,
    pub allocated_pages: u64,
    pub leaf_pages: u64,
    pub branch_pages: u64,
    pub stored_bytes: u64,
    pub metadata_bytes: u64,
    pub fragmented_bytes: u64,
    pub page_size: u64,
}

/// Opened embedded database.
pub struct Store {
    db: Database,
    path: Option<PathBuf>,
}

impl Store {
    /// Open or create a file-backed database.
    pub fn open(path: impl AsRef<Path>, create: bool) -> KvResult<Self> {
        let path = path.as_ref();
        let db = if create || path.exists() {
            Database::create(path)?
        } else {
            Database::open(path)?
        };
        Ok(Store {
            db,
            path: Some(path.to_path_buf()),
        })
    }

    /// In-memory database (not durable across process exit).
    pub fn memory() -> KvResult<Self> {
        let db = Database::builder().create_with_backend(InMemoryBackend::new())?;
        Ok(Store { db, path: None })
    }

    /// Resolved file path, or `None` for memory stores.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Begin a read transaction (MVCC snapshot).
    pub fn begin_read(&self) -> KvResult<Txn> {
        Ok(Txn::Read(self.db.begin_read()?))
    }

    /// Begin a write transaction (exclusive writer).
    pub fn begin_write(&self) -> KvResult<Txn> {
        Ok(Txn::Write(Some(self.db.begin_write()?)))
    }

    /// Force a durability checkpoint (empty Immediate commit).
    pub fn sync(&self) -> KvResult<()> {
        let txn = self.db.begin_write()?;
        txn.commit()?;
        Ok(())
    }

    /// Database-wide storage stats.
    pub fn stats(&self) -> KvResult<KvStats> {
        let txn = self.db.begin_write()?;
        let s = txn.stats()?;
        let stats = KvStats {
            tree_height: s.tree_height(),
            allocated_pages: s.allocated_pages(),
            leaf_pages: s.leaf_pages(),
            branch_pages: s.branch_pages(),
            stored_bytes: s.stored_bytes(),
            metadata_bytes: s.metadata_bytes(),
            fragmented_bytes: s.fragmented_bytes(),
            page_size: s.page_size() as u64,
        };
        drop(txn);
        Ok(stats)
    }

    pub fn put(&self, table: &str, key: &[u8], value: &[u8]) -> KvResult<()> {
        let mut txn = self.begin_write()?;
        txn.put(table, key, value)?;
        txn.commit()
    }

    pub fn get(&self, table: &str, key: &[u8]) -> KvResult<Option<Vec<u8>>> {
        self.begin_read()?.get(table, key)
    }

    pub fn has(&self, table: &str, key: &[u8]) -> KvResult<bool> {
        Ok(self.get(table, key)?.is_some())
    }

    pub fn remove(&self, table: &str, key: &[u8]) -> KvResult<bool> {
        let mut txn = self.begin_write()?;
        let existed = txn.remove(table, key)?;
        txn.commit()?;
        Ok(existed)
    }

    pub fn clear(&self, table: &str) -> KvResult<u64> {
        let mut txn = self.begin_write()?;
        let n = txn.clear(table)?;
        txn.commit()?;
        Ok(n)
    }

    pub fn len(&self, table: &str) -> KvResult<u64> {
        self.begin_read()?.len(table)
    }

    pub fn scan(&self, table: &str, opts: &ScanOptions) -> KvResult<Vec<ScanPair>> {
        self.begin_read()?.scan(table, opts)
    }

    pub fn first(&self, table: &str) -> KvResult<Option<ScanPair>> {
        self.begin_read()?.first(table)
    }

    pub fn last(&self, table: &str) -> KvResult<Option<ScanPair>> {
        self.begin_read()?.last(table)
    }

    pub fn put_many(&self, table: &str, pairs: &[(Vec<u8>, Vec<u8>)]) -> KvResult<u64> {
        let mut txn = self.begin_write()?;
        let n = txn.put_many(table, pairs)?;
        txn.commit()?;
        Ok(n)
    }

    pub fn get_many(&self, table: &str, keys: &[Vec<u8>]) -> KvResult<Vec<Option<Vec<u8>>>> {
        self.begin_read()?.get_many(table, keys)
    }

    pub fn list_tables(&self) -> KvResult<Vec<String>> {
        self.begin_read()?.list_tables()
    }

    pub fn drop_table(&self, name: &str) -> KvResult<bool> {
        let mut txn = self.begin_write()?;
        let existed = txn.drop_table(name)?;
        txn.commit()?;
        Ok(existed)
    }
}

/// Read or write transaction handle.
pub enum Txn {
    Read(redb::ReadTransaction),
    Write(Option<redb::WriteTransaction>),
}

impl Txn {
    pub fn is_write(&self) -> bool {
        matches!(self, Txn::Write(_))
    }

    pub fn is_open(&self) -> bool {
        match self {
            Txn::Read(_) => true,
            Txn::Write(inner) => inner.is_some(),
        }
    }

    fn write_mut(&mut self) -> KvResult<&mut redb::WriteTransaction> {
        match self {
            Txn::Write(Some(t)) => Ok(t),
            Txn::Write(None) => Err(KvError::TxnClosed),
            Txn::Read(_) => Err(KvError::ReadOnly),
        }
    }

    pub fn put(&mut self, table: &str, key: &[u8], value: &[u8]) -> KvResult<()> {
        let def = table_def(table)?;
        let txn = self.write_mut()?;
        let mut t = txn.open_table(def)?;
        t.insert(key, value)?;
        Ok(())
    }

    pub fn get(&self, table: &str, key: &[u8]) -> KvResult<Option<Vec<u8>>> {
        let def = table_def(table)?;
        match self {
            Txn::Read(txn) => {
                let t = match txn.open_table(def) {
                    Ok(t) => t,
                    Err(redb::TableError::TableDoesNotExist(_)) => return Ok(None),
                    Err(e) => return Err(e.into()),
                };
                let got = t.get(key)?.map(|g| g.value().to_vec());
                Ok(got)
            }
            Txn::Write(Some(txn)) => {
                let t = txn.open_table(def)?;
                let got = t.get(key)?.map(|g| g.value().to_vec());
                Ok(got)
            }
            Txn::Write(None) => Err(KvError::TxnClosed),
        }
    }

    pub fn has(&self, table: &str, key: &[u8]) -> KvResult<bool> {
        Ok(self.get(table, key)?.is_some())
    }

    pub fn remove(&mut self, table: &str, key: &[u8]) -> KvResult<bool> {
        let def = table_def(table)?;
        let txn = self.write_mut()?;
        let mut t = match txn.open_table(def) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(false),
            Err(e) => return Err(e.into()),
        };
        let existed = t.remove(key)?.is_some();
        Ok(existed)
    }

    pub fn clear(&mut self, table: &str) -> KvResult<u64> {
        let def = table_def(table)?;
        let txn = self.write_mut()?;
        let mut t = match txn.open_table(def) {
            Ok(t) => t,
            Err(redb::TableError::TableDoesNotExist(_)) => return Ok(0),
            Err(e) => return Err(e.into()),
        };
        let n = t.len()?;
        let keys: Vec<Vec<u8>> = t
            .iter()?
            .map(|r| r.map(|(k, _)| k.value().to_vec()))
            .collect::<Result<Vec<_>, _>>()?;
        for k in &keys {
            t.remove(k.as_slice())?;
        }
        Ok(n)
    }

    pub fn len(&self, table: &str) -> KvResult<u64> {
        let def = table_def(table)?;
        match self {
            Txn::Read(txn) => {
                let t = match txn.open_table(def) {
                    Ok(t) => t,
                    Err(redb::TableError::TableDoesNotExist(_)) => return Ok(0),
                    Err(e) => return Err(e.into()),
                };
                Ok(t.len()?)
            }
            Txn::Write(Some(txn)) => {
                let t = txn.open_table(def)?;
                Ok(t.len()?)
            }
            Txn::Write(None) => Err(KvError::TxnClosed),
        }
    }

    pub fn put_many(&mut self, table: &str, pairs: &[(Vec<u8>, Vec<u8>)]) -> KvResult<u64> {
        let def = table_def(table)?;
        let txn = self.write_mut()?;
        let mut t = txn.open_table(def)?;
        for (k, v) in pairs {
            t.insert(k.as_slice(), v.as_slice())?;
        }
        Ok(pairs.len() as u64)
    }

    pub fn get_many(&self, table: &str, keys: &[Vec<u8>]) -> KvResult<Vec<Option<Vec<u8>>>> {
        let mut out = Vec::with_capacity(keys.len());
        for k in keys {
            out.push(self.get(table, k)?);
        }
        Ok(out)
    }

    pub fn first(&self, table: &str) -> KvResult<Option<ScanPair>> {
        let opts = ScanOptions {
            limit: Some(1),
            reverse: false,
            ..ScanOptions::default()
        };
        let mut pairs = self.scan(table, &opts)?;
        Ok(if pairs.is_empty() {
            None
        } else {
            Some(pairs.remove(0))
        })
    }

    pub fn last(&self, table: &str) -> KvResult<Option<ScanPair>> {
        let opts = ScanOptions {
            limit: Some(1),
            reverse: true,
            ..ScanOptions::default()
        };
        let mut pairs = self.scan(table, &opts)?;
        Ok(if pairs.is_empty() {
            None
        } else {
            Some(pairs.remove(0))
        })
    }

    pub fn list_tables(&self) -> KvResult<Vec<String>> {
        match self {
            Txn::Read(txn) => {
                let mut names: Vec<String> =
                    txn.list_tables()?.map(|h| h.name().to_string()).collect();
                names.sort();
                Ok(names)
            }
            Txn::Write(Some(txn)) => {
                let mut names: Vec<String> =
                    txn.list_tables()?.map(|h| h.name().to_string()).collect();
                names.sort();
                Ok(names)
            }
            Txn::Write(None) => Err(KvError::TxnClosed),
        }
    }

    pub fn drop_table(&mut self, name: &str) -> KvResult<bool> {
        let def = table_def(name)?;
        let txn = self.write_mut()?;
        Ok(txn.delete_table(def)?)
    }

    pub fn commit(&mut self) -> KvResult<()> {
        match self {
            Txn::Write(inner) => {
                let txn = inner.take().ok_or(KvError::TxnClosed)?;
                txn.commit()?;
                Ok(())
            }
            Txn::Read(_) => Err(KvError::Invalid(
                "cannot commit a read transaction; close/drop it instead".into(),
            )),
        }
    }

    pub fn abort(&mut self) -> KvResult<()> {
        match self {
            Txn::Write(inner) => {
                if let Some(txn) = inner.take() {
                    txn.abort()?;
                }
                Ok(())
            }
            Txn::Read(_) => Ok(()),
        }
    }

    pub fn scan(&self, table: &str, opts: &ScanOptions) -> KvResult<Vec<ScanPair>> {
        let def = table_def(table)?;
        let (start, end, end_inclusive) = resolve_bounds(opts);
        match self {
            Txn::Read(txn) => {
                let t = match txn.open_table(def) {
                    Ok(t) => t,
                    Err(redb::TableError::TableDoesNotExist(_)) => return Ok(Vec::new()),
                    Err(e) => return Err(e.into()),
                };
                collect_bytes_range(&t, start.as_deref(), end.as_deref(), end_inclusive, opts)
            }
            Txn::Write(Some(txn)) => {
                let t = txn.open_table(def)?;
                collect_bytes_range(&t, start.as_deref(), end.as_deref(), end_inclusive, opts)
            }
            Txn::Write(None) => Err(KvError::TxnClosed),
        }
    }
}

fn resolve_bounds(opts: &ScanOptions) -> (Option<Vec<u8>>, Option<Vec<u8>>, bool) {
    let mut start = opts.start.clone();
    let mut end = opts.end.clone();
    let mut end_inclusive = opts.end_inclusive;

    if let Some(ref prefix) = opts.prefix {
        let p_end = prefix_end(prefix);
        start = Some(match start {
            Some(s) if s.as_slice() > prefix.as_slice() => s,
            _ => prefix.clone(),
        });
        match (end.clone(), p_end) {
            (Some(e), Some(pe)) => {
                if e.as_slice() < pe.as_slice() || (e.as_slice() == pe.as_slice() && !end_inclusive)
                {
                    end = Some(e);
                } else {
                    end = Some(pe);
                    end_inclusive = false;
                }
            }
            (None, Some(pe)) => {
                end = Some(pe);
                end_inclusive = false;
            }
            (Some(e), None) => end = Some(e),
            (None, None) => {}
        }
    }

    (start, end, end_inclusive)
}

fn collect_bytes_range<T>(
    table: &T,
    start: Option<&[u8]>,
    end: Option<&[u8]>,
    end_inclusive: bool,
    opts: &ScanOptions,
) -> KvResult<Vec<ScanPair>>
where
    T: ReadableTable<&'static [u8], &'static [u8]>,
{
    let lower: Bound<&[u8]> = match start {
        Some(s) => Bound::Included(s),
        None => Bound::Unbounded,
    };
    let upper: Bound<&[u8]> = match end {
        Some(e) if end_inclusive => Bound::Included(e),
        Some(e) => Bound::Excluded(e),
        None => Bound::Unbounded,
    };

    let mut pairs: Vec<ScanPair> = Vec::new();
    for item in table.range::<&[u8]>((lower, upper))? {
        let (k, v) = item?;
        pairs.push(ScanPair {
            key: k.value().to_vec(),
            value: v.value().to_vec(),
        });
    }

    if opts.reverse {
        pairs.reverse();
    }
    if let Some(limit) = opts.limit {
        pairs.truncate(limit);
    }
    Ok(pairs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("niao_kv_{name}_{nanos}.redb"))
    }

    #[test]
    fn memory_put_get() {
        let store = Store::memory().unwrap();
        store.put(DEFAULT_TABLE, b"a", b"1").unwrap();
        assert_eq!(store.get(DEFAULT_TABLE, b"a").unwrap().unwrap(), b"1");
        assert!(store.get(DEFAULT_TABLE, b"missing").unwrap().is_none());
    }

    #[test]
    fn file_persist_and_prefix() {
        let path = tmp_path("persist");
        let _ = std::fs::remove_file(&path);
        {
            let store = Store::open(&path, true).unwrap();
            store.put(DEFAULT_TABLE, b"user:1", b"alice").unwrap();
            store.put(DEFAULT_TABLE, b"user:2", b"bob").unwrap();
            store.put(DEFAULT_TABLE, b"z", b"other").unwrap();
        }
        let store = Store::open(&path, false).unwrap();
        let opts = ScanOptions {
            prefix: Some(b"user:".to_vec()),
            ..ScanOptions::default()
        };
        let pairs = store.scan(DEFAULT_TABLE, &opts).unwrap();
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].key, b"user:1");
        assert_eq!(pairs[1].value, b"bob");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn snapshot_isolation() {
        let store = Store::memory().unwrap();
        store.put(DEFAULT_TABLE, b"k", b"v1").unwrap();
        let snap = store.begin_read().unwrap();
        store.put(DEFAULT_TABLE, b"k", b"v2").unwrap();
        assert_eq!(snap.get(DEFAULT_TABLE, b"k").unwrap().unwrap(), b"v1");
        assert_eq!(store.get(DEFAULT_TABLE, b"k").unwrap().unwrap(), b"v2");
    }

    #[test]
    fn write_txn_abort() {
        let store = Store::memory().unwrap();
        let mut txn = store.begin_write().unwrap();
        txn.put(DEFAULT_TABLE, b"x", b"1").unwrap();
        txn.abort().unwrap();
        assert!(store.get(DEFAULT_TABLE, b"x").unwrap().is_none());
    }

    #[test]
    fn named_tables() {
        let store = Store::memory().unwrap();
        store.put("a", b"k", b"1").unwrap();
        store.put("b", b"k", b"2").unwrap();
        assert_eq!(store.get("a", b"k").unwrap().unwrap(), b"1");
        assert_eq!(store.get("b", b"k").unwrap().unwrap(), b"2");
        let tables = store.list_tables().unwrap();
        assert!(tables.contains(&"a".to_string()));
        assert!(tables.contains(&"b".to_string()));
        assert!(store.drop_table("a").unwrap());
        assert!(store.get("a", b"k").unwrap().is_none());
    }
}
