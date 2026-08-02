//! Local filesystem backend.

use crate::error::{BlobError, BlobResult};
use crate::store::{BackendKind, Entry, ObjectStore};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Local directory rooted at `root` (empty root = absolute paths as-is).
#[derive(Debug, Clone)]
pub struct LocalStore {
    pub root: PathBuf,
}

impl LocalStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn cwd() -> Self {
        Self {
            root: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        }
    }

    fn resolve(&self, key: &str) -> PathBuf {
        let p = Path::new(key);
        if p.is_absolute() || self.root.as_os_str().is_empty() {
            p.to_path_buf()
        } else {
            self.root.join(p)
        }
    }
}

fn mtime_secs(meta: &fs::Metadata) -> Option<i64> {
    meta.modified().ok().and_then(|t| {
        t.duration_since(SystemTime::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs() as i64)
    })
}

impl ObjectStore for LocalStore {
    fn kind(&self) -> BackendKind {
        BackendKind::Local
    }

    fn read(&self, key: &str) -> BlobResult<Vec<u8>> {
        let path = self.resolve(key);
        let mut f = File::open(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                BlobError::not_found(key)
            } else {
                BlobError::from(e)
            }
        })?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        Ok(buf)
    }

    fn write(&self, key: &str, data: &[u8], _content_type: Option<&str>) -> BlobResult<u64> {
        let path = self.resolve(key);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut f = File::create(&path)?;
        f.write_all(data)?;
        f.flush()?;
        Ok(data.len() as u64)
    }

    fn exists(&self, key: &str) -> BlobResult<bool> {
        Ok(self.resolve(key).exists())
    }

    fn info(&self, key: &str) -> BlobResult<Entry> {
        let path = self.resolve(key);
        let meta = fs::metadata(&path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                BlobError::not_found(key)
            } else {
                BlobError::from(e)
            }
        })?;
        let kind = if meta.is_dir() { "dir" } else { "file" };
        Ok(Entry {
            name: key.to_string(),
            kind: kind.into(),
            size: if meta.is_file() { meta.len() } else { 0 },
            mtime: mtime_secs(&meta),
        })
    }

    fn list(&self, prefix: &str, detail: bool) -> BlobResult<Vec<Entry>> {
        let path = self.resolve(prefix);
        if path.is_file() {
            let meta = fs::metadata(&path)?;
            return Ok(vec![Entry {
                name: prefix.to_string(),
                kind: "file".into(),
                size: meta.len(),
                mtime: if detail { mtime_secs(&meta) } else { None },
            }]);
        }
        if !path.exists() {
            return Err(BlobError::not_found(prefix));
        }
        let mut out = Vec::new();
        for ent in fs::read_dir(&path)? {
            let ent = ent?;
            let name = ent.file_name().to_string_lossy().into_owned();
            let full = if prefix.is_empty() || prefix == "." {
                name.clone()
            } else if prefix.ends_with('/') || prefix.ends_with('\\') {
                format!("{prefix}{name}")
            } else {
                format!("{prefix}/{name}")
            };
            let meta = ent.metadata()?;
            let kind = if meta.is_dir() { "dir" } else { "file" };
            out.push(Entry {
                name: full,
                kind: kind.into(),
                size: if meta.is_file() { meta.len() } else { 0 },
                mtime: if detail { mtime_secs(&meta) } else { None },
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    fn remove(&self, key: &str) -> BlobResult<()> {
        let path = self.resolve(key);
        if path.is_dir() {
            fs::remove_dir_all(&path)?;
        } else if path.exists() {
            fs::remove_file(&path)?;
        } else {
            return Err(BlobError::not_found(key));
        }
        Ok(())
    }

    fn mkdir(&self, key: &str) -> BlobResult<()> {
        fs::create_dir_all(self.resolve(key))?;
        Ok(())
    }

    fn copy(&self, src: &str, dst: &str) -> BlobResult<()> {
        let s = self.resolve(src);
        let d = self.resolve(dst);
        if let Some(parent) = d.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&s, &d)?;
        Ok(())
    }

    fn rename(&self, src: &str, dst: &str) -> BlobResult<()> {
        let s = self.resolve(src);
        let d = self.resolve(dst);
        if let Some(parent) = d.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&s, &d)?;
        Ok(())
    }
}

/// Append bytes to a local file (used by open mode `a`).
pub fn append_local(store: &LocalStore, key: &str, data: &[u8]) -> BlobResult<u64> {
    let path = store.resolve(key);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
    f.write_all(data)?;
    Ok(data.len() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn roundtrip() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("nblob_local_{stamp}"));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let store = LocalStore::new(&root);
        store.write("a/b.txt", b"hello", None).unwrap();
        assert_eq!(store.read("a/b.txt").unwrap(), b"hello");
        assert!(store.exists("a/b.txt").unwrap());
        let ents = store.list("a", false).unwrap();
        assert_eq!(ents.len(), 1);
        store.remove("a/b.txt").unwrap();
        assert!(!store.exists("a/b.txt").unwrap());
        let _ = fs::remove_dir_all(&root);
    }
}
