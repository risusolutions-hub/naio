//! Total ordering for sorted-container keys (mirrors Niao numeric comparison rules).

use std::cmp::Ordering;

/// Comparable scalar used inside sorted containers.
#[derive(Clone, Debug, PartialEq)]
pub enum SortValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

impl SortValue {
    pub fn from_int(n: i64) -> Self {
        SortValue::Int(n)
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            SortValue::Int(n) => Some(*n),
            SortValue::Float(f)
                if f.fract() == 0.0 && *f >= i64::MIN as f64 && *f <= i64::MAX as f64 =>
            {
                Some(*f as i64)
            }
            _ => None,
        }
    }

    pub fn cmp_key(&self, other: &SortValue) -> Ordering {
        match (self, other) {
            (SortValue::Bool(a), SortValue::Bool(b)) => a.cmp(b),
            (SortValue::Int(a), SortValue::Int(b)) => a.cmp(b),
            (SortValue::Float(a), SortValue::Float(b)) => a.total_cmp(b),
            (SortValue::Int(a), SortValue::Float(b)) => (*a as f64).total_cmp(b),
            (SortValue::Float(a), SortValue::Int(b)) => a.total_cmp(&(*b as f64)),
            (SortValue::Str(a), SortValue::Str(b)) => a.cmp(b),
            _ => Ordering::Less, // mixed types — caller must reject
        }
    }

    pub fn same_type(&self, other: &SortValue) -> bool {
        matches!(
            (self, other),
            (SortValue::Bool(_), SortValue::Bool(_))
                | (SortValue::Int(_), SortValue::Int(_))
                | (SortValue::Float(_), SortValue::Float(_))
                | (SortValue::Str(_), SortValue::Str(_))
                | (SortValue::Int(_), SortValue::Float(_))
                | (SortValue::Float(_), SortValue::Int(_))
        )
    }
}

/// Slot key for multiset entries — `(value order, unique id)`.
#[derive(Clone, Debug, PartialEq)]
pub struct ListSlot {
    pub value: SortValue,
    pub id: u64,
}

impl PartialOrd for ListSlot {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for ListSlot {}

impl Ord for ListSlot {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.value.cmp_key(&other.value) {
            Ordering::Equal => self.id.cmp(&other.id),
            ord => ord,
        }
    }
}

/// Unique key for sets and dicts.
#[derive(Clone, Debug, PartialEq)]
pub struct SetKey(SortValue);

impl SetKey {
    pub fn new(v: SortValue) -> Self {
        SetKey(v)
    }

    pub fn value(&self) -> &SortValue {
        &self.0
    }
}

impl PartialOrd for SetKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for SetKey {}

impl Ord for SetKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp_key(&other.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_float_cross_compare() {
        let a = SortValue::Int(5);
        let b = SortValue::Float(5.0);
        assert_eq!(a.cmp_key(&b), Ordering::Equal);
    }
}
