use crate::Value;
use std::collections::HashMap;

const SMALL_OBJECT_LIMIT: usize = 16;

/// Insertion-ordered object: linear scan below 16 keys, hash index above.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Object {
    pairs: Vec<(String, Value)>,
    index: Option<HashMap<String, usize>>,
}

impl Object {
    #[inline]
    pub fn new() -> Self {
        Self {
            pairs: Vec::new(),
            index: None,
        }
    }

    #[inline]
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            pairs: Vec::with_capacity(cap.min(SMALL_OBJECT_LIMIT)),
            index: None,
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.pairs.iter().map(|(k, v)| (k.as_str(), v))
    }

    #[inline]
    pub fn get(&self, key: &str) -> Option<&Value> {
        if let Some(idx) = &self.index {
            idx.get(key).map(|&i| &self.pairs[i].1)
        } else {
            self.pairs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v)
        }
    }

    #[inline]
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        if self.index.is_some() {
            if let Some(&i) = self.index.as_ref().and_then(|m| m.get(key)) {
                return Some(&mut self.pairs[i].1);
            }
            None
        } else {
            self.pairs
                .iter_mut()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v)
        }
    }

    pub fn insert(&mut self, key: String, value: Value) -> Option<Value> {
        if let Some(idx) = &mut self.index {
            if let Some(&i) = idx.get(&key) {
                return Some(std::mem::replace(&mut self.pairs[i].1, value));
            }
            let i = self.pairs.len();
            idx.insert(key.clone(), i);
            self.pairs.push((key, value));
            None
        } else if let Some(pos) = self.pairs.iter().position(|(k, _)| k == &key) {
            Some(std::mem::replace(&mut self.pairs[pos].1, value))
        } else {
            if self.pairs.len() >= SMALL_OBJECT_LIMIT {
                self.build_index();
                return self.insert(key, value);
            }
            self.pairs.push((key, value));
            None
        }
    }

    fn build_index(&mut self) {
        let mut map = HashMap::with_capacity(self.pairs.len());
        for (i, (k, _)) in self.pairs.iter().enumerate() {
            map.insert(k.clone(), i);
        }
        self.index = Some(map);
    }
}
