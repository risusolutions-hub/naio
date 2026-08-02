//! Unified VFS dispatch: resolve URI → store + key, cross-scheme copy.

use crate::azure::AzureStore;
use crate::error::{BlobError, BlobResult};
use crate::gcs::GcsStore;
use crate::local::LocalStore;
use crate::memory::MemoryStore;
use crate::s3::S3Store;
use crate::store::{AzureOpts, BackendKind, Entry, GcsOpts, OpenMode, S3Opts, StoreArc};
use crate::uri::{self, BlobUri};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Open file buffer owned by the VFS layer.
pub struct OpenFile {
    pub store: StoreArc,
    pub key: String,
    pub mode: OpenMode,
    pub pos: u64,
    pub buf: Vec<u8>,
    pub dirty: bool,
}

impl OpenFile {
    pub fn size(&self) -> u64 {
        self.buf.len() as u64
    }

    pub fn tell(&self) -> u64 {
        self.pos
    }

    pub fn seek(&mut self, offset: i64, whence: i64) -> BlobResult<u64> {
        let base = match whence {
            0 => 0i64,
            1 => self.pos as i64,
            2 => self.buf.len() as i64,
            _ => return Err(BlobError::new("seek whence must be 0, 1, or 2")),
        };
        let neu = base.saturating_add(offset);
        if neu < 0 {
            return Err(BlobError::new("seek before start of file"));
        }
        self.pos = neu as u64;
        if self.pos > self.buf.len() as u64 {
            if self.mode.is_write() {
                self.buf.resize(self.pos as usize, 0);
            } else {
                self.pos = self.buf.len() as u64;
            }
        }
        Ok(self.pos)
    }

    pub fn read(&mut self, n: Option<usize>) -> BlobResult<Vec<u8>> {
        if self.mode.is_write() && self.mode != OpenMode::Append {
            // allow reading a write buffer after seek
        }
        let start = self.pos as usize;
        if start >= self.buf.len() {
            return Ok(Vec::new());
        }
        let end = match n {
            Some(n) => (start + n).min(self.buf.len()),
            None => self.buf.len(),
        };
        let out = self.buf[start..end].to_vec();
        self.pos = end as u64;
        Ok(out)
    }

    pub fn write(&mut self, data: &[u8]) -> BlobResult<u64> {
        if !self.mode.is_write() {
            return Err(BlobError::new("file not opened for writing"));
        }
        let start = self.pos as usize;
        let end = start + data.len();
        if end > self.buf.len() {
            self.buf.resize(end, 0);
        }
        self.buf[start..end].copy_from_slice(data);
        self.pos = end as u64;
        self.dirty = true;
        Ok(data.len() as u64)
    }

    pub fn flush(&mut self) -> BlobResult<()> {
        if !self.dirty {
            return Ok(());
        }
        if self.mode == OpenMode::Append {
            // For append we rewrite whole buffer (local has dedicated append path via open)
            self.store.write(&self.key, &self.buf, None)?;
        } else {
            self.store.write(&self.key, &self.buf, None)?;
        }
        self.dirty = false;
        Ok(())
    }
}

/// Filesystem handle: a store plus optional default prefix / bucket root URI.
#[derive(Clone)]
pub struct FsHandle {
    pub store: StoreArc,
    pub kind: BackendKind,
    /// Root URI string for display (e.g. `s3://bucket` or local root path).
    pub root_uri: String,
}

/// Resolve a URI into `(store, key)` using optional default credential opts.
pub struct Vfs {
    pub default_s3: Option<S3Opts>,
    pub default_azure: Option<AzureOpts>,
    pub default_gcs: Option<GcsOpts>,
}

impl Default for Vfs {
    fn default() -> Self {
        Self {
            default_s3: None,
            default_azure: None,
            default_gcs: None,
        }
    }
}

impl Vfs {
    pub fn resolve_store(&self, uri: &BlobUri) -> BlobResult<(StoreArc, String)> {
        match uri.scheme.as_str() {
            "" | "file" => {
                let store: StoreArc = Arc::new(LocalStore::new(PathBuf::new()));
                Ok((store, uri.path.clone()))
            }
            "memory" => {
                let store: StoreArc = Arc::new(MemoryStore::named(&uri.netloc));
                Ok((store, uri.path.clone()))
            }
            "s3" => {
                let opts = self.default_s3.clone().ok_or_else(|| {
                    BlobError::auth("S3 credentials required (nblob.s3 / fs opts)")
                })?;
                let store: StoreArc = Arc::new(S3Store::new(opts, uri.netloc.clone()));
                Ok((store, uri.path.clone()))
            }
            "az" | "abfs" => {
                let opts = self.default_azure.clone().ok_or_else(|| {
                    BlobError::auth("Azure credentials required (nblob.azure / fs opts)")
                })?;
                let (account_container, key) = split_azure_netloc(&uri.netloc, &uri.path)?;
                let (account, container) = account_container;
                let mut o = opts;
                o.account = account;
                let store: StoreArc = Arc::new(AzureStore::new(o, container));
                Ok((store, key))
            }
            "gs" => {
                let opts = self.default_gcs.clone().ok_or_else(|| {
                    BlobError::auth("GCS credentials required (nblob.gcs / fs opts)")
                })?;
                let store: StoreArc = Arc::new(GcsStore::new(opts, uri.netloc.clone()));
                Ok((store, uri.path.clone()))
            }
            other => Err(BlobError::unsupported(other)),
        }
    }

    pub fn read_uri(&self, uri: &str) -> BlobResult<Vec<u8>> {
        let u = uri::parse(uri)?;
        let (store, key) = self.resolve_store(&u)?;
        store.read(&key)
    }

    pub fn write_uri(&self, uri: &str, data: &[u8], content_type: Option<&str>) -> BlobResult<u64> {
        let u = uri::parse(uri)?;
        let (store, key) = self.resolve_store(&u)?;
        store.write(&key, data, content_type)
    }

    pub fn exists_uri(&self, uri: &str) -> BlobResult<bool> {
        let u = uri::parse(uri)?;
        let (store, key) = self.resolve_store(&u)?;
        store.exists(&key)
    }

    pub fn info_uri(&self, uri: &str) -> BlobResult<Entry> {
        let u = uri::parse(uri)?;
        let (store, key) = self.resolve_store(&u)?;
        store.info(&key)
    }

    pub fn list_uri(&self, uri: &str, detail: bool) -> BlobResult<Vec<Entry>> {
        let u = uri::parse(uri)?;
        let (store, key) = self.resolve_store(&u)?;
        store.list(&key, detail)
    }

    pub fn remove_uri(&self, uri: &str) -> BlobResult<()> {
        let u = uri::parse(uri)?;
        let (store, key) = self.resolve_store(&u)?;
        store.remove(&key)
    }

    pub fn mkdir_uri(&self, uri: &str) -> BlobResult<()> {
        let u = uri::parse(uri)?;
        let (store, key) = self.resolve_store(&u)?;
        store.mkdir(&key)
    }

    pub fn copy_uri(&self, src: &str, dst: &str) -> BlobResult<()> {
        let su = uri::parse(src)?;
        let du = uri::parse(dst)?;
        let (ss, sk) = self.resolve_store(&su)?;
        let (ds, dk) = self.resolve_store(&du)?;
        if Arc::ptr_eq(&ss, &ds) || (ss.kind() == ds.kind() && same_backend_identity(&su, &du)) {
            // Prefer store-native copy when same logical store
            if ss.kind() == ds.kind() && su.scheme == du.scheme && su.netloc == du.netloc {
                return ss.copy(&sk, &dk);
            }
        }
        let data = ss.read(&sk)?;
        ds.write(&dk, &data, None)?;
        Ok(())
    }

    pub fn move_uri(&self, src: &str, dst: &str) -> BlobResult<()> {
        self.copy_uri(src, dst)?;
        self.remove_uri(src)?;
        Ok(())
    }

    pub fn open_uri(&self, uri: &str, mode: OpenMode) -> BlobResult<OpenFile> {
        let u = uri::parse(uri)?;
        let (store, key) = self.resolve_store(&u)?;
        let buf = match mode {
            OpenMode::Read => store.read(&key)?,
            OpenMode::Write => Vec::new(),
            OpenMode::Append => store.read(&key).unwrap_or_default(),
        };
        let pos = if mode == OpenMode::Append {
            buf.len() as u64
        } else {
            0
        };
        Ok(OpenFile {
            store,
            key,
            mode,
            pos,
            buf,
            dirty: mode == OpenMode::Write,
        })
    }
}

fn same_backend_identity(a: &BlobUri, b: &BlobUri) -> bool {
    a.scheme == b.scheme && a.netloc == b.netloc
}

fn split_azure_netloc(netloc: &str, path: &str) -> BlobResult<((String, String), String)> {
    // netloc is "account/container"
    let mut parts = netloc.splitn(2, '/');
    let account = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| BlobError::invalid_uri(netloc))?
        .to_string();
    let container = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| BlobError::new("azure URI needs account/container"))?
        .to_string();
    Ok(((account, container), path.to_string()))
}

/// Build an FsHandle from factory helpers.
pub fn fs_local(root: Option<&str>) -> FsHandle {
    let root_path = root
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let root_uri = root_path.to_string_lossy().into_owned();
    FsHandle {
        store: Arc::new(LocalStore::new(root_path)),
        kind: BackendKind::Local,
        root_uri,
    }
}

pub fn fs_memory(name: Option<&str>) -> FsHandle {
    let store = match name {
        Some(n) => MemoryStore::named(n),
        None => MemoryStore::ephemeral(),
    };
    let root_uri = format!("memory://{}", store.name());
    FsHandle {
        store: Arc::new(store),
        kind: BackendKind::Memory,
        root_uri,
    }
}

pub fn fs_s3(opts: S3Opts, bucket: Option<&str>) -> BlobResult<FsHandle> {
    let bucket = bucket
        .map(|s| s.to_string())
        .or_else(|| opts.default_bucket.clone())
        .ok_or_else(|| BlobError::new("s3 fs requires bucket"))?;
    let root_uri = format!("s3://{bucket}");
    Ok(FsHandle {
        store: Arc::new(S3Store::new(opts, bucket)),
        kind: BackendKind::S3,
        root_uri,
    })
}

pub fn fs_azure(opts: AzureOpts, container: Option<&str>) -> BlobResult<FsHandle> {
    let container = container
        .map(|s| s.to_string())
        .or_else(|| opts.default_container.clone())
        .ok_or_else(|| BlobError::new("azure fs requires container"))?;
    let root_uri = format!("az://{}/{}", opts.account, container);
    Ok(FsHandle {
        store: Arc::new(AzureStore::new(opts, container)),
        kind: BackendKind::Azure,
        root_uri,
    })
}

pub fn fs_gcs(opts: GcsOpts, bucket: Option<&str>) -> BlobResult<FsHandle> {
    let bucket = bucket
        .map(|s| s.to_string())
        .or_else(|| opts.default_bucket.clone())
        .ok_or_else(|| BlobError::new("gcs fs requires bucket"))?;
    let root_uri = format!("gs://{bucket}");
    Ok(FsHandle {
        store: Arc::new(GcsStore::new(opts, bucket)),
        kind: BackendKind::Gcs,
        root_uri,
    })
}

pub fn fs_from_uri(vfs: &Vfs, uri: &str) -> BlobResult<FsHandle> {
    let u = uri::parse(uri)?;
    match u.scheme.as_str() {
        "" | "file" => Ok(fs_local(Some(&u.path))),
        "memory" => Ok(fs_memory(Some(&u.netloc))),
        "s3" => {
            let opts = vfs
                .default_s3
                .clone()
                .ok_or_else(|| BlobError::auth("S3 credentials required"))?;
            fs_s3(opts, Some(&u.netloc))
        }
        "az" | "abfs" => {
            let opts = vfs
                .default_azure
                .clone()
                .ok_or_else(|| BlobError::auth("Azure credentials required"))?;
            let ((account, container), _) = split_azure_netloc(&u.netloc, &u.path)?;
            let mut o = opts;
            o.account = account;
            fs_azure(o, Some(&container))
        }
        "gs" => {
            let opts = vfs
                .default_gcs
                .clone()
                .ok_or_else(|| BlobError::auth("GCS credentials required"))?;
            fs_gcs(opts, Some(&u.netloc))
        }
        other => Err(BlobError::unsupported(other)),
    }
}

/// Thread-safe global VFS defaults (credentials).
pub fn global_vfs() -> &'static Mutex<Vfs> {
    static V: std::sync::OnceLock<Mutex<Vfs>> = std::sync::OnceLock::new();
    V.get_or_init(|| Mutex::new(Vfs::default()))
}

/// Append helper for open(mode=a) on non-local stores (read+concat+write).
pub fn append_bytes(store: &StoreArc, key: &str, data: &[u8]) -> BlobResult<u64> {
    let mut existing = store.read(key).unwrap_or_default();
    existing.extend_from_slice(data);
    store.write(key, &existing, None)?;
    Ok(data.len() as u64)
}
