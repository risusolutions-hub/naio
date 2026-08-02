//! In-memory object store (great for tests; `memory://` scheme).

use crate::error::{BlobError, BlobResult};
use crate::store::{BackendKind, Entry, ObjectStore};
use std::collections::BTreeMap;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Default)]
struct MemState {
    /// Full key → bytes. Directory markers end with `/`.
    objects: BTreeMap<String, Vec<u8>>,
}

/// Process-wide named memory roots so `memory://name/...` shares state.
static ROOTS: std::sync::OnceLock<RwLock<BTreeMap<String, std::sync::Arc<RwLock<MemState>>>>> =
    std::sync::OnceLock::new();

fn roots() -> &'static RwLock<BTreeMap<String, std::sync::Arc<RwLock<MemState>>>> {
    ROOTS.get_or_init(|| RwLock::new(BTreeMap::new()))
}

#[derive(Debug, Clone)]
pub struct MemoryStore {
    name: String,
    state: std::sync::Arc<RwLock<MemState>>,
}

impl MemoryStore {
    pub fn named(name: impl Into<String>) -> Self {
        let name = name.into();
        let key = if name.is_empty() {
            "_default".into()
        } else {
            name.clone()
        };
        let state = {
            let mut map = roots().write().expect("memory roots lock");
            map.entry(key.clone())
                .or_insert_with(|| std::sync::Arc::new(RwLock::new(MemState::default())))
                .clone()
        };
        Self { name: key, state }
    }

    pub fn ephemeral() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        Self::named(format!("_ephemeral_{stamp}"))
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn clear(&self) {
        let mut g = self.state.write().expect("mem lock");
        g.objects.clear();
    }
}

fn normalize_key(key: &str) -> String {
    key.trim_matches('/').to_string()
}

impl ObjectStore for MemoryStore {
    fn kind(&self) -> BackendKind {
        BackendKind::Memory
    }

    fn read(&self, key: &str) -> BlobResult<Vec<u8>> {
        let k = normalize_key(key);
        let g = self.state.read().expect("mem lock");
        g.objects
            .get(&k)
            .cloned()
            .ok_or_else(|| BlobError::not_found(key))
    }

    fn write(&self, key: &str, data: &[u8], _content_type: Option<&str>) -> BlobResult<u64> {
        let k = normalize_key(key);
        if k.is_empty() {
            return Err(BlobError::new("empty key"));
        }
        // Ensure parent dir markers exist
        let mut g = self.state.write().expect("mem lock");
        let mut prefix = String::new();
        for part in k.split('/') {
            if part.is_empty() {
                continue;
            }
            if !prefix.is_empty() {
                let dir = format!("{prefix}/");
                g.objects.entry(dir).or_insert_with(Vec::new);
                prefix.push('/');
            }
            prefix.push_str(part);
        }
        g.objects.insert(k, data.to_vec());
        Ok(data.len() as u64)
    }

    fn exists(&self, key: &str) -> BlobResult<bool> {
        let k = normalize_key(key);
        let g = self.state.read().expect("mem lock");
        Ok(g.objects.contains_key(&k) || g.objects.contains_key(&format!("{k}/")))
    }

    fn info(&self, key: &str) -> BlobResult<Entry> {
        let k = normalize_key(key);
        let g = self.state.read().expect("mem lock");
        if let Some(data) = g.objects.get(&k) {
            return Ok(Entry {
                name: k,
                kind: "file".into(),
                size: data.len() as u64,
                mtime: None,
            });
        }
        let dir = format!("{k}/");
        if g.objects.contains_key(&dir) || g.objects.keys().any(|x| x.starts_with(&dir) || *x == k)
        {
            return Ok(Entry {
                name: k,
                kind: "dir".into(),
                size: 0,
                mtime: None,
            });
        }
        Err(BlobError::not_found(key))
    }

    fn list(&self, prefix: &str, _detail: bool) -> BlobResult<Vec<Entry>> {
        let prefix = normalize_key(prefix);
        let g = self.state.read().expect("mem lock");
        let mut names = BTreeMap::<String, Entry>::new();
        let needle = if prefix.is_empty() {
            String::new()
        } else {
            format!("{prefix}/")
        };
        for (k, data) in g.objects.iter() {
            if k.ends_with('/') {
                continue;
            }
            let rel = if prefix.is_empty() {
                k.as_str()
            } else if k.starts_with(&needle) {
                &k[needle.len()..]
            } else if k == &prefix {
                names.insert(
                    k.clone(),
                    Entry {
                        name: k.clone(),
                        kind: "file".into(),
                        size: data.len() as u64,
                        mtime: None,
                    },
                );
                continue;
            } else {
                continue;
            };
            if rel.is_empty() {
                continue;
            }
            if let Some(slash) = rel.find('/') {
                let child = &rel[..slash];
                let full = if prefix.is_empty() {
                    child.to_string()
                } else {
                    format!("{prefix}/{child}")
                };
                names.entry(full.clone()).or_insert(Entry {
                    name: full,
                    kind: "dir".into(),
                    size: 0,
                    mtime: None,
                });
            } else {
                let full = if prefix.is_empty() {
                    rel.to_string()
                } else {
                    format!("{prefix}/{rel}")
                };
                names.insert(
                    full.clone(),
                    Entry {
                        name: full,
                        kind: "file".into(),
                        size: data.len() as u64,
                        mtime: None,
                    },
                );
            }
        }
        Ok(names.into_values().collect())
    }

    fn remove(&self, key: &str) -> BlobResult<()> {
        let k = normalize_key(key);
        let mut g = self.state.write().expect("mem lock");
        let mut removed = g.objects.remove(&k).is_some();
        let prefix = format!("{k}/");
        let keys: Vec<String> = g
            .objects
            .keys()
            .filter(|x| x.starts_with(&prefix) || *x == &format!("{k}/"))
            .cloned()
            .collect();
        for key in keys {
            g.objects.remove(&key);
            removed = true;
        }
        if !removed {
            return Err(BlobError::not_found(key));
        }
        Ok(())
    }

    fn mkdir(&self, key: &str) -> BlobResult<()> {
        let k = normalize_key(key);
        let mut g = self.state.write().expect("mem lock");
        g.objects.entry(format!("{k}/")).or_insert_with(Vec::new);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_crud() {
        let s = MemoryStore::ephemeral();
        s.write("docs/a.txt", b"one", None).unwrap();
        s.write("docs/b.txt", b"two", None).unwrap();
        assert_eq!(s.read("docs/a.txt").unwrap(), b"one");
        let ls = s.list("docs", false).unwrap();
        assert_eq!(ls.len(), 2);
        s.remove("docs/a.txt").unwrap();
        assert!(!s.exists("docs/a.txt").unwrap());
    }
}
