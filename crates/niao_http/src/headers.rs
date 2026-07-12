//! HTTP header map (case-insensitive keys).

use crate::types::{HeaderName, HeaderValue};
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
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
            index: HashMap::with_capacity(capacity),
        }
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
    pub fn insert_typed(&mut self, name: HeaderName, value: HeaderValue) {
        let key = name.as_str().to_string();
        let val = value.to_str().unwrap_or("").to_string();
        if let Some(&idx) = self.index.get(&key) {
            self.entries[idx].1 = val;
        } else {
            let idx = self.entries.len();
            self.entries.push((key.clone(), val));
            self.index.insert(key, idx);
        }
    }

    #[inline]
    pub fn append(&mut self, name: impl Into<String>, value: impl Into<String>) {
        let key = name.into().to_ascii_lowercase();
        let val = value.into();
        if let Some(&idx) = self.index.get(&key) {
            let existing = &mut self.entries[idx].1;
            existing.push(',');
            existing.push(' ');
            existing.push_str(&val);
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
    pub fn get_typed(&self, name: &HeaderName) -> Option<&str> {
        self.get(name.as_str())
    }

    #[inline]
    pub fn contains_key(&self, name: &str) -> bool {
        self.index.contains_key(&name.to_ascii_lowercase())
    }

    #[inline]
    pub fn remove(&mut self, name: &str) -> Option<String> {
        let key = name.to_ascii_lowercase();
        let idx = self.index.remove(&key)?;
        let (_, value) = self.entries.remove(idx);
        for (_, slot) in self.index.iter_mut() {
            if *slot > idx {
                *slot -= 1;
            }
        }
        Some(value)
    }

    #[inline]
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(k, _)| k.as_str())
    }

    #[inline]
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.names()
    }

    #[inline]
    pub fn values(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(_, v)| v.as_str())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn header_case_insensitivity() {
        let mut map = HeaderMap::new();
        map.insert("Content-Type", "text/plain");
        assert_eq!(map.get("content-type"), Some("text/plain"));
        assert_eq!(map.get("CONTENT-TYPE"), Some("text/plain"));
        assert_eq!(map.get("Content-Type"), Some("text/plain"));
        assert!(map.contains_key("CoNtEnT-TyPe"));
    }

    #[test]
    fn insert_overwrites_case_insensitive() {
        let mut map = HeaderMap::new();
        map.insert("X-Test", "a");
        map.insert("x-test", "b");
        assert_eq!(map.len(), 1);
        assert_eq!(map.get("X-TEST"), Some("b"));
    }

    #[test]
    fn append_joins_values() {
        let mut map = HeaderMap::new();
        map.append("Accept", "text/html");
        map.append("accept", "application/json");
        assert_eq!(map.get("Accept"), Some("text/html, application/json"));
    }

    #[test]
    fn remove_case_insensitive() {
        let mut map = HeaderMap::new();
        map.insert("Host", "example.com");
        assert_eq!(map.remove("HOST"), Some("example.com".into()));
        assert!(map.is_empty());
    }

    #[test]
    fn typed_insert_lookup() {
        let mut map = HeaderMap::new();
        let name = HeaderName::from_str("Cache-Control").unwrap();
        let value = HeaderValue::from_str("no-cache").unwrap();
        map.insert_typed(name, value);
        assert_eq!(map.get_typed(&HeaderName::from_static("cache-control")), Some("no-cache"));
    }
}
