//! Sorted dict — keys in sorted order (sortedcontainers `SortedDict` subset).

use crate::key::{SetKey, SortValue};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortError {
    IncompatibleTypes,
    NotFound,
    IndexOutOfBounds,
    Empty,
}

#[derive(Clone, Debug, Default)]
pub struct SortedDict {
    entries: BTreeMap<SetKey, SortValue>,
}

impl SortedDict {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_pairs(pairs: &[(SortValue, SortValue)]) -> Self {
        let mut d = Self::new();
        for (k, v) in pairs {
            d.set(k.clone(), v.clone());
        }
        d
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn set(&mut self, key: SortValue, value: SortValue) -> Option<SortValue> {
        self.entries.insert(SetKey::new(key), value)
    }

    pub fn get(&self, key: &SortValue) -> Option<SortValue> {
        self.entries.get(&SetKey::new(key.clone())).cloned()
    }

    pub fn contains_key(&self, key: &SortValue) -> bool {
        self.entries.contains_key(&SetKey::new(key.clone()))
    }

    pub fn remove(&mut self, key: &SortValue) -> Result<SortValue, SortError> {
        self.entries
            .remove(&SetKey::new(key.clone()))
            .ok_or(SortError::NotFound)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn keys(&self) -> Vec<SortValue> {
        self.entries.keys().map(|k| k.value().clone()).collect()
    }

    pub fn values(&self) -> Vec<SortValue> {
        self.entries.values().cloned().collect()
    }

    pub fn items(&self) -> Vec<(SortValue, SortValue)> {
        self.entries
            .iter()
            .map(|(k, v)| (k.value().clone(), v.clone()))
            .collect()
    }

    pub fn peekitem(&self, index: isize) -> Result<(SortValue, SortValue), SortError> {
        if self.is_empty() {
            return Err(SortError::Empty);
        }
        let len = self.len() as isize;
        let idx = if index < 0 { len + index } else { index };
        if idx < 0 || idx >= len {
            return Err(SortError::IndexOutOfBounds);
        }
        let (k, v) = self
            .entries
            .iter()
            .nth(idx as usize)
            .ok_or(SortError::IndexOutOfBounds)?;
        Ok((k.value().clone(), v.clone()))
    }

    pub fn bisect_left(&self, key: &SortValue) -> usize {
        self.entries
            .keys()
            .position(|k| k.value().cmp_key(key) != std::cmp::Ordering::Less)
            .unwrap_or(self.len())
    }

    pub fn bisect_right(&self, key: &SortValue) -> usize {
        self.entries
            .keys()
            .position(|k| k.value().cmp_key(key) == std::cmp::Ordering::Greater)
            .unwrap_or(self.len())
    }

    pub fn irange(
        &self,
        min: &SortValue,
        max: &SortValue,
        min_inclusive: bool,
        max_inclusive: bool,
    ) -> Vec<(SortValue, SortValue)> {
        let start = if min_inclusive {
            self.bisect_left(min)
        } else {
            self.bisect_right(min)
        };
        let end = if max_inclusive {
            self.bisect_right(max)
        } else {
            self.bisect_left(max)
        };
        if start >= end {
            return Vec::new();
        }
        self.entries
            .iter()
            .skip(start)
            .take(end.saturating_sub(start))
            .map(|(k, v)| (k.value().clone(), v.clone()))
            .collect()
    }

    pub fn min_key(&self) -> Result<SortValue, SortError> {
        self.entries
            .keys()
            .next()
            .map(|k| k.value().clone())
            .ok_or(SortError::Empty)
    }

    pub fn max_key(&self) -> Result<SortValue, SortError> {
        self.entries
            .keys()
            .next_back()
            .map(|k| k.value().clone())
            .ok_or(SortError::Empty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dict_order() {
        let mut d = SortedDict::new();
        d.set(SortValue::Int(3), SortValue::Str("c".into()));
        d.set(SortValue::Int(1), SortValue::Str("a".into()));
        assert_eq!(d.keys(), vec![SortValue::Int(1), SortValue::Int(3)]);
    }
}
