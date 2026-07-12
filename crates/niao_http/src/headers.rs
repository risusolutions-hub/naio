//! HTTP header map (lowercase keys).

use std::collections::HashMap;

pub const MAX_HEADER_COUNT: usize = 100;
pub const MAX_HEADER_BYTES: usize = 8192;
pub const MAX_HEADER_LINE: usize = 8192;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeaderMap {
    entries: Vec<(String, String)>,
    index: HashMap<String, usize>,
}

impl HeaderMap {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn insert(&mut self, name: impl Into<String>, value: impl Into<String>) {
        let key = name.into().to_ascii_lowercase();
        let val = value.into();
        if let Some(&idx) = self.index.get(&key) {
            self.entries[idx].1 = val;
        } else {
            let idx = self.entries.len();
            self.entries.push((key.clone(), val));
            self.index.insert(key, idx);
        }
    }

    #[inline]
    pub fn get(&self, name: &str) -> Option<&str> {
        let key = name.to_ascii_lowercase();
        self.index
            .get(&key)
            .map(|&i| self.entries[i].1.as_str())
    }

    #[inline]
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(k, _)| k.as_str())
    }

    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn append_raw(&mut self, name: &str, value: &str) -> Result<(), String> {
        if self.len() >= MAX_HEADER_COUNT {
            return Err("too many headers".into());
        }
        self.insert(name, value);
        Ok(())
    }
}
