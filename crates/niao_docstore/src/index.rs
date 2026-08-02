//! Secondary indexes over scalar document fields.

use crate::value::{get_path, IndexKey};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Default, Clone)]
pub struct SecondaryIndex {
    /// field path → value key → doc ids
    map: HashMap<String, HashMap<IndexKey, HashSet<u64>>>,
}

impl SecondaryIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fields(&self) -> Vec<String> {
        let mut keys: Vec<_> = self.map.keys().cloned().collect();
        keys.sort();
        keys
    }

    pub fn has(&self, field: &str) -> bool {
        self.map.contains_key(field)
    }

    pub fn create(&mut self, field: &str, docs: &[(u64, &Value)]) {
        let mut bucket: HashMap<IndexKey, HashSet<u64>> = HashMap::new();
        for &(id, doc) in docs {
            if let Some(v) = get_path(doc, field) {
                if let Some(key) = IndexKey::from_value(v) {
                    bucket.entry(key).or_default().insert(id);
                }
            }
        }
        self.map.insert(field.to_string(), bucket);
    }

    pub fn drop(&mut self, field: &str) -> bool {
        self.map.remove(field).is_some()
    }

    pub fn insert_doc(&mut self, id: u64, doc: &Value) {
        for (field, bucket) in self.map.iter_mut() {
            if let Some(v) = get_path(doc, field) {
                if let Some(key) = IndexKey::from_value(v) {
                    bucket.entry(key).or_default().insert(id);
                }
            }
        }
    }

    pub fn remove_doc(&mut self, id: u64, doc: &Value) {
        for (field, bucket) in self.map.iter_mut() {
            if let Some(v) = get_path(doc, field) {
                if let Some(key) = IndexKey::from_value(v) {
                    if let Some(set) = bucket.get_mut(&key) {
                        set.remove(&id);
                        if set.is_empty() {
                            bucket.remove(&key);
                        }
                    }
                }
            }
        }
    }

    pub fn update_doc(&mut self, id: u64, old: &Value, new: &Value) {
        self.remove_doc(id, old);
        self.insert_doc(id, new);
    }

    /// Lookup ids for an equality on an indexed field.
    pub fn lookup_eq(&self, field: &str, value: &Value) -> Option<Vec<u64>> {
        let bucket = self.map.get(field)?;
        let key = IndexKey::from_value(value)?;
        let set = bucket.get(&key)?;
        let mut ids: Vec<_> = set.iter().copied().collect();
        ids.sort_unstable();
        Some(ids)
    }
}
