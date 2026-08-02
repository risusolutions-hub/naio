//! `niao_kv` — embedded ordered key-value store (ACID, prefix scans, snapshots).
//!
//! Thin wrapper over [`redb`]: byte keys/values, named tables, read snapshots,
//! and write transactions. The Niao VM binding lives in `niao_runtime::nkv`.

mod error;
mod scan;
mod store;

pub use error::{KvError, KvResult};
pub use scan::{prefix_end, ScanOptions, ScanPair};
pub use store::{KvStats, Store, Txn, DEFAULT_TABLE};
