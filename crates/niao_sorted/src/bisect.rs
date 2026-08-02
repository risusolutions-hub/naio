//! Bisect helpers on sorted slices (Python `bisect` parity).

use crate::key::SortValue;
use std::cmp::Ordering;

/// `bisect_left` on a sorted `i64` slice.
#[inline]
pub fn bisect_left_int(data: &[i64], x: i64) -> usize {
    data.partition_point(|&v| v < x)
}

/// `bisect_right` on a sorted `i64` slice.
#[inline]
pub fn bisect_right_int(data: &[i64], x: i64) -> usize {
    data.partition_point(|&v| v <= x)
}

/// `bisect_left` on a sorted generic slice.
pub fn bisect_left_values(data: &[SortValue], x: &SortValue) -> usize {
    data.iter()
        .position(|v| v.cmp_key(x) != Ordering::Less)
        .unwrap_or(data.len())
}

/// `bisect_right` on a sorted generic slice.
pub fn bisect_right_values(data: &[SortValue], x: &SortValue) -> usize {
    data.iter()
        .position(|v| v.cmp_key(x) == Ordering::Greater)
        .unwrap_or(data.len())
}

/// Insert `x` into sorted `i64` vec at the correct side.
pub fn insort_int(data: &mut Vec<i64>, x: i64, right: bool) {
    let pos = if right {
        bisect_right_int(data, x)
    } else {
        bisect_left_int(data, x)
    };
    data.insert(pos, x);
}

/// Insert into sorted generic vec.
pub fn insort_value(data: &mut Vec<SortValue>, x: SortValue, right: bool) {
    let pos = if right {
        bisect_right_values(data, &x)
    } else {
        bisect_left_values(data, &x)
    };
    data.insert(pos, x);
}

/// Nearest value at or below `x`.
pub fn nearest_left_int(data: &[i64], x: i64) -> Option<i64> {
    let pos = bisect_left_int(data, x);
    if pos == 0 {
        None
    } else {
        Some(data[pos - 1])
    }
}

/// Nearest value at or above `x`.
pub fn nearest_right_int(data: &[i64], x: i64) -> Option<i64> {
    data.get(bisect_left_int(data, x)).copied()
}

/// Closest value by absolute distance (ties prefer left).
pub fn nearest_int(data: &[i64], x: i64) -> Option<i64> {
    let left = nearest_left_int(data, x);
    let right = nearest_right_int(data, x);
    match (left, right) {
        (None, r) => r,
        (l, None) => l,
        (Some(l), Some(r)) => {
            let dl = x.saturating_sub(l).unsigned_abs();
            let dr = r.saturating_sub(x).unsigned_abs();
            if dr < dl {
                Some(r)
            } else {
                Some(l)
            }
        }
    }
}

pub fn nearest_left_values<'a>(data: &'a [SortValue], x: &SortValue) -> Option<&'a SortValue> {
    let pos = bisect_left_values(data, x);
    if pos == 0 {
        None
    } else {
        data.get(pos - 1)
    }
}

pub fn nearest_right_values<'a>(data: &'a [SortValue], x: &SortValue) -> Option<&'a SortValue> {
    data.get(bisect_left_values(data, x))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bisect_dupes() {
        let v = vec![1, 2, 2, 3];
        assert_eq!(bisect_left_int(&v, 2), 1);
        assert_eq!(bisect_right_int(&v, 2), 3);
    }
}
