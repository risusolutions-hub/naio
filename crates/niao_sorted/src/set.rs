//! Sorted set — unique values in order (sortedcontainers `SortedSet` subset).

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
pub struct SortedSet {
    entries: BTreeMap<SetKey, SortValue>,
}

impl SortedSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_values(values: &[SortValue]) -> Self {
        let mut s = Self::new();
        for v in values {
            let _ = s.add(v.clone());
        }
        s
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn add(&mut self, value: SortValue) -> bool {
        let key = SetKey::new(value.clone());
        self.entries.insert(key, value).is_none()
    }

    pub fn add_many(&mut self, values: &[SortValue]) {
        for v in values {
            self.add(v.clone());
        }
    }

    pub fn contains(&self, value: &SortValue) -> bool {
        self.entries.contains_key(&SetKey::new(value.clone()))
    }

    pub fn discard(&mut self, value: &SortValue) -> bool {
        self.entries.remove(&SetKey::new(value.clone())).is_some()
    }

    pub fn remove(&mut self, value: &SortValue) -> Result<SortValue, SortError> {
        self.entries
            .remove(&SetKey::new(value.clone()))
            .ok_or(SortError::NotFound)
    }

    pub fn pop(&mut self, index: Option<usize>) -> Result<SortValue, SortError> {
        if self.is_empty() {
            return Err(SortError::Empty);
        }
        let idx = index.unwrap_or(self.len() - 1);
        let key = self
            .entries
            .keys()
            .nth(idx)
            .cloned()
            .ok_or(SortError::IndexOutOfBounds)?;
        self.entries.remove(&key).ok_or(SortError::IndexOutOfBounds)
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn bisect_left(&self, value: &SortValue) -> usize {
        self.entries
            .keys()
            .position(|k| k.value().cmp_key(value) != std::cmp::Ordering::Less)
            .unwrap_or(self.len())
    }

    pub fn bisect_right(&self, value: &SortValue) -> usize {
        self.entries
            .keys()
            .position(|k| k.value().cmp_key(value) == std::cmp::Ordering::Greater)
            .unwrap_or(self.len())
    }

    pub fn get(&self, index: usize) -> Result<SortValue, SortError> {
        self.entries
            .values()
            .nth(index)
            .cloned()
            .ok_or(SortError::IndexOutOfBounds)
    }

    pub fn index(&self, value: &SortValue) -> Result<usize, SortError> {
        let pos = self.bisect_left(value);
        if pos >= self.len() {
            return Err(SortError::NotFound);
        }
        if self
            .entries
            .values()
            .nth(pos)
            .map(|v| v.cmp_key(value) == std::cmp::Ordering::Equal)
            == Some(true)
        {
            Ok(pos)
        } else {
            Err(SortError::NotFound)
        }
    }

    pub fn min(&self) -> Result<SortValue, SortError> {
        self.entries
            .values()
            .next()
            .cloned()
            .ok_or(SortError::Empty)
    }

    pub fn max(&self) -> Result<SortValue, SortError> {
        self.entries
            .values()
            .next_back()
            .cloned()
            .ok_or(SortError::Empty)
    }

    pub fn irange(
        &self,
        min: &SortValue,
        max: &SortValue,
        min_inclusive: bool,
        max_inclusive: bool,
    ) -> Vec<SortValue> {
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
            .values()
            .skip(start)
            .take(end.saturating_sub(start))
            .cloned()
            .collect()
    }

    pub fn islice(&self, start: isize, stop: Option<isize>) -> Vec<SortValue> {
        let len = self.len() as isize;
        let start = if start < 0 { len + start } else { start };
        let stop = stop.map(|s| if s < 0 { len + s } else { s }).unwrap_or(len);
        if start < 0 || stop < start || start >= len {
            return Vec::new();
        }
        let start = start as usize;
        let stop = (stop as usize).min(self.len());
        self.entries
            .values()
            .skip(start)
            .take(stop.saturating_sub(start))
            .cloned()
            .collect()
    }

    pub fn to_vec(&self) -> Vec<SortValue> {
        self.entries.values().cloned().collect()
    }

    pub fn nearest(&self, value: &SortValue, side: &str) -> Option<SortValue> {
        match side {
            "left" => {
                let idx = self.bisect_left(value);
                if idx == 0 {
                    None
                } else {
                    self.get(idx - 1).ok()
                }
            }
            "right" => self.get(self.bisect_left(value)).ok(),
            _ => {
                let left = self.nearest(value, "left");
                let right = self.nearest(value, "right");
                match (left, right) {
                    (None, r) => r,
                    (l, None) => l,
                    (Some(l), Some(r)) => Some(l),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_unique() {
        let mut s = SortedSet::new();
        assert!(s.add(SortValue::Int(2)));
        assert!(!s.add(SortValue::Int(2)));
        assert_eq!(s.len(), 1);
    }
}
