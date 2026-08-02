//! Sorted multiset — int fast path plus generic B-tree backing.

use crate::bisect::{
    bisect_left_int, bisect_left_values, bisect_right_int, bisect_right_values, insort_int,
    nearest_int, nearest_left_int, nearest_right_int,
};
use crate::key::{ListSlot, SortValue};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default)]
enum ListInner {
    #[default]
    Empty,
    Int(Vec<i64>),
    Generic {
        slots: BTreeMap<ListSlot, SortValue>,
        next_id: u64,
    },
}

/// Sorted list allowing duplicate values (sortedcontainers `SortedList` subset).
#[derive(Clone, Debug, Default)]
pub struct SortedList {
    inner: ListInner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortError {
    IncompatibleTypes,
    NotFound,
    IndexOutOfBounds,
    Empty,
}

impl SortedList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_values(values: &[SortValue]) -> Result<Self, SortError> {
        let mut list = Self::new();
        for v in values {
            list.add(v.clone())?;
        }
        Ok(list)
    }

    pub fn from_ints(values: &[i64]) -> Self {
        let mut data = values.to_vec();
        data.sort_unstable();
        SortedList {
            inner: ListInner::Int(data),
        }
    }

    pub fn len(&self) -> usize {
        match &self.inner {
            ListInner::Empty => 0,
            ListInner::Int(v) => v.len(),
            ListInner::Generic { slots, .. } => slots.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn is_int_path(&self) -> bool {
        matches!(self.inner, ListInner::Int(_))
    }

    fn promote_to_generic(&mut self) {
        if let ListInner::Int(data) = std::mem::take(&mut self.inner) {
            let mut slots = BTreeMap::new();
            let mut next_id = 0u64;
            for n in data {
                let value = SortValue::Int(n);
                let slot = ListSlot {
                    value: value.clone(),
                    id: next_id,
                };
                next_id += 1;
                slots.insert(slot, value);
            }
            self.inner = ListInner::Generic { slots, next_id };
        }
    }

    pub fn add(&mut self, value: SortValue) -> Result<(), SortError> {
        match &mut self.inner {
            ListInner::Empty => {
                if let Some(n) = value.as_int() {
                    self.inner = ListInner::Int(vec![n]);
                } else {
                    let slot = ListSlot {
                        value: value.clone(),
                        id: 0,
                    };
                    self.inner = ListInner::Generic {
                        slots: BTreeMap::from([(slot, value)]),
                        next_id: 1,
                    };
                }
                Ok(())
            }
            ListInner::Int(data) => {
                if let Some(n) = value.as_int() {
                    insort_int(data, n, true);
                    Ok(())
                } else {
                    self.promote_to_generic();
                    self.add(value)
                }
            }
            ListInner::Generic { slots, next_id } => {
                let id = *next_id;
                *next_id += 1;
                let slot = ListSlot {
                    value: value.clone(),
                    id,
                };
                slots.insert(slot, value);
                Ok(())
            }
        }
    }

    pub fn add_many(&mut self, values: &[SortValue]) -> Result<(), SortError> {
        for v in values {
            self.add(v.clone())?;
        }
        Ok(())
    }

    pub fn bisect_left(&self, value: &SortValue) -> Result<usize, SortError> {
        match &self.inner {
            ListInner::Empty => Ok(0),
            ListInner::Int(data) => {
                let n = value.as_int().ok_or(SortError::IncompatibleTypes)?;
                Ok(bisect_left_int(data, n))
            }
            ListInner::Generic { slots, .. } => {
                let mut count = 0usize;
                for slot in slots.keys() {
                    if slot.value.cmp_key(value) != std::cmp::Ordering::Less {
                        break;
                    }
                    count += 1;
                }
                Ok(count)
            }
        }
    }

    pub fn bisect_right(&self, value: &SortValue) -> Result<usize, SortError> {
        match &self.inner {
            ListInner::Empty => Ok(0),
            ListInner::Int(data) => {
                let n = value.as_int().ok_or(SortError::IncompatibleTypes)?;
                Ok(bisect_right_int(data, n))
            }
            ListInner::Generic { slots, .. } => {
                let mut count = 0usize;
                for slot in slots.keys() {
                    if slot.value.cmp_key(value) == std::cmp::Ordering::Greater {
                        break;
                    }
                    count += 1;
                }
                Ok(count)
            }
        }
    }

    pub fn insort(&mut self, value: SortValue, right: bool) -> Result<(), SortError> {
        match &mut self.inner {
            ListInner::Empty => self.add(value),
            ListInner::Int(data) => {
                if let Some(n) = value.as_int() {
                    insort_int(data, n, right);
                    Ok(())
                } else {
                    self.promote_to_generic();
                    self.insort(value, right)
                }
            }
            ListInner::Generic { slots, next_id } => {
                let id = *next_id;
                *next_id += 1;
                let slot = ListSlot {
                    value: value.clone(),
                    id,
                };
                if right {
                    // insert after equal keys — higher id sorts later
                    slots.insert(slot, value);
                } else {
                    // for left insert among equals, use id before existing equal ids
                    // rebuild slot with id = min of equal range
                    let pos = slots
                        .keys()
                        .position(|s| s.value.cmp_key(&slot.value) != std::cmp::Ordering::Less)
                        .unwrap_or(slots.len());
                    let id = slots
                        .keys()
                        .nth(pos)
                        .filter(|s| s.value.cmp_key(&slot.value) == std::cmp::Ordering::Equal)
                        .map(|s| s.id.saturating_sub(1))
                        .unwrap_or(id);
                    let slot = ListSlot {
                        value: slot.value,
                        id,
                    };
                    slots.insert(slot, value);
                }
                Ok(())
            }
        }
    }

    pub fn get(&self, index: usize) -> Result<SortValue, SortError> {
        match &self.inner {
            ListInner::Empty => Err(SortError::IndexOutOfBounds),
            ListInner::Int(data) => data
                .get(index)
                .copied()
                .map(SortValue::Int)
                .ok_or(SortError::IndexOutOfBounds),
            ListInner::Generic { slots, .. } => slots
                .values()
                .nth(index)
                .cloned()
                .ok_or(SortError::IndexOutOfBounds),
        }
    }

    pub fn count(&self, value: &SortValue) -> Result<usize, SortError> {
        let left = self.bisect_left(value)?;
        let right = self.bisect_right(value)?;
        Ok(right.saturating_sub(left))
    }

    pub fn index(&self, value: &SortValue) -> Result<usize, SortError> {
        let left = self.bisect_left(value)?;
        if left >= self.len() {
            return Err(SortError::NotFound);
        }
        match &self.inner {
            ListInner::Int(data) => {
                let n = value.as_int().ok_or(SortError::IncompatibleTypes)?;
                if data.get(left) == Some(&n) {
                    Ok(left)
                } else {
                    Err(SortError::NotFound)
                }
            }
            ListInner::Generic { slots, .. } => {
                if slots
                    .values()
                    .nth(left)
                    .map(|v| v.cmp_key(value) == std::cmp::Ordering::Equal)
                    == Some(true)
                {
                    Ok(left)
                } else {
                    Err(SortError::NotFound)
                }
            }
            ListInner::Empty => Err(SortError::NotFound),
        }
    }

    pub fn discard_one(&mut self, value: &SortValue) -> Result<bool, SortError> {
        let idx = match self.index(value) {
            Ok(i) => i,
            Err(SortError::NotFound) => return Ok(false),
            Err(e) => return Err(e),
        };
        self.remove_at(idx)?;
        Ok(true)
    }

    pub fn remove_one(&mut self, value: &SortValue) -> Result<SortValue, SortError> {
        let idx = self.index(value)?;
        self.remove_at(idx)
    }

    pub fn remove_at(&mut self, index: usize) -> Result<SortValue, SortError> {
        match &mut self.inner {
            ListInner::Empty => Err(SortError::IndexOutOfBounds),
            ListInner::Int(data) => data
                .get(index)
                .copied()
                .map(|n| {
                    data.remove(index);
                    SortValue::Int(n)
                })
                .ok_or(SortError::IndexOutOfBounds),
            ListInner::Generic { slots, .. } => {
                let key = slots
                    .keys()
                    .nth(index)
                    .cloned()
                    .ok_or(SortError::IndexOutOfBounds)?;
                slots.remove(&key).ok_or(SortError::IndexOutOfBounds)
            }
        }
    }

    pub fn pop(&mut self, index: Option<usize>) -> Result<SortValue, SortError> {
        if self.is_empty() {
            return Err(SortError::Empty);
        }
        let idx = index.unwrap_or_else(|| self.len() - 1);
        self.remove_at(idx)
    }

    pub fn clear(&mut self) {
        self.inner = ListInner::Empty;
    }

    pub fn min(&self) -> Result<SortValue, SortError> {
        if self.is_empty() {
            return Err(SortError::Empty);
        }
        self.get(0)
    }

    pub fn max(&self) -> Result<SortValue, SortError> {
        if self.is_empty() {
            return Err(SortError::Empty);
        }
        self.get(self.len() - 1)
    }

    pub fn irange(
        &self,
        min: &SortValue,
        max: &SortValue,
        min_inclusive: bool,
        max_inclusive: bool,
    ) -> Result<Vec<SortValue>, SortError> {
        let start = if min_inclusive {
            self.bisect_left(min)?
        } else {
            self.bisect_right(min)?
        };
        let end = if max_inclusive {
            self.bisect_right(max)?
        } else {
            self.bisect_left(max)?
        };
        if start >= end {
            return Ok(Vec::new());
        }
        Ok((start..end).filter_map(|i| self.get(i).ok()).collect())
    }

    pub fn islice(&self, start: isize, stop: Option<isize>) -> Result<Vec<SortValue>, SortError> {
        let len = self.len() as isize;
        let start = if start < 0 { len + start } else { start };
        let stop = stop.map(|s| if s < 0 { len + s } else { s }).unwrap_or(len);
        if start < 0 || stop < start || start >= len {
            return Ok(Vec::new());
        }
        let start = start as usize;
        let stop = (stop as usize).min(self.len());
        Ok((start..stop).filter_map(|i| self.get(i).ok()).collect())
    }

    pub fn to_vec(&self) -> Vec<SortValue> {
        match &self.inner {
            ListInner::Empty => Vec::new(),
            ListInner::Int(data) => data.iter().copied().map(SortValue::Int).collect(),
            ListInner::Generic { slots, .. } => slots.values().cloned().collect(),
        }
    }

    pub fn nearest(&self, value: &SortValue, side: &str) -> Result<Option<SortValue>, SortError> {
        match side {
            "left" => self.nearest_left(value),
            "right" => self.nearest_right(value),
            _ => self.nearest_both(value),
        }
    }

    fn nearest_left(&self, value: &SortValue) -> Result<Option<SortValue>, SortError> {
        match &self.inner {
            ListInner::Empty => Ok(None),
            ListInner::Int(data) => {
                let n = value.as_int().ok_or(SortError::IncompatibleTypes)?;
                Ok(nearest_left_int(data, n).map(SortValue::Int))
            }
            ListInner::Generic { .. } => {
                let idx = self.bisect_left(value)?;
                if idx == 0 {
                    Ok(None)
                } else {
                    self.get(idx - 1).map(Some)
                }
            }
        }
    }

    fn nearest_right(&self, value: &SortValue) -> Result<Option<SortValue>, SortError> {
        match &self.inner {
            ListInner::Empty => Ok(None),
            ListInner::Int(data) => {
                let n = value.as_int().ok_or(SortError::IncompatibleTypes)?;
                Ok(nearest_right_int(data, n).map(SortValue::Int))
            }
            ListInner::Generic { .. } => {
                let idx = self.bisect_left(value)?;
                Ok(self.get(idx).ok())
            }
        }
    }

    fn nearest_both(&self, value: &SortValue) -> Result<Option<SortValue>, SortError> {
        match &self.inner {
            ListInner::Empty => Ok(None),
            ListInner::Int(data) => {
                let n = value.as_int().ok_or(SortError::IncompatibleTypes)?;
                Ok(nearest_int(data, n).map(SortValue::Int))
            }
            ListInner::Generic { .. } => {
                let left = self.nearest_left(value)?;
                let right = self.nearest_right(value)?;
                Ok(match (left, right) {
                    (None, r) => r,
                    (l, None) => l,
                    (Some(l), Some(r)) => {
                        // pick closer; ties prefer left
                        match l.cmp_key(&r) {
                            std::cmp::Ordering::Equal => Some(l),
                            _ => {
                                let dl = match (&l, value) {
                                    (SortValue::Int(a), SortValue::Int(b)) => {
                                        (a - b).unsigned_abs()
                                    }
                                    (SortValue::Float(a), SortValue::Float(b)) => {
                                        ((a - b).abs() as u64)
                                    }
                                    _ => 0,
                                };
                                let dr = match (&r, value) {
                                    (SortValue::Int(a), SortValue::Int(b)) => {
                                        (a - b).unsigned_abs()
                                    }
                                    (SortValue::Float(a), SortValue::Float(b)) => {
                                        ((a - b).abs() as u64)
                                    }
                                    _ => 0,
                                };
                                if dr < dl {
                                    Some(r)
                                } else {
                                    Some(l)
                                }
                            }
                        }
                    }
                })
            }
        }
    }
}

/// Standalone bisect on a sorted int array (no container handle).
pub fn bisect_left_sorted_ints(data: &[i64], x: i64) -> usize {
    bisect_left_int(data, x)
}

pub fn bisect_right_sorted_ints(data: &[i64], x: i64) -> usize {
    bisect_right_int(data, x)
}

pub fn bisect_left_sorted_values(data: &[SortValue], x: &SortValue) -> usize {
    bisect_left_values(data, x)
}

pub fn bisect_right_sorted_values(data: &[SortValue], x: &SortValue) -> usize {
    bisect_right_values(data, x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_list_basics() {
        let mut l = SortedList::from_ints(&[1, 3, 3, 7]);
        l.add(SortValue::Int(3)).unwrap();
        assert_eq!(l.len(), 5);
        assert_eq!(l.count(&SortValue::Int(3)).unwrap(), 3);
        assert_eq!(l.bisect_left(&SortValue::Int(3)).unwrap(), 1);
        assert_eq!(l.bisect_right(&SortValue::Int(3)).unwrap(), 4);
        let range = l
            .irange(&SortValue::Int(2), &SortValue::Int(5), true, true)
            .unwrap();
        assert_eq!(range.len(), 3);
    }

    #[test]
    fn pop_and_discard() {
        let mut l = SortedList::from_ints(&[1, 2, 2, 3]);
        assert!(l.discard_one(&SortValue::Int(2)).unwrap());
        assert_eq!(l.len(), 3);
        assert_eq!(l.pop(None).unwrap(), SortValue::Int(3));
    }
}
