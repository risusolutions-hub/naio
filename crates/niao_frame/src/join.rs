//! Hash joins (inner / left / right / outer) including null keys and many-to-many.

use crate::dataframe::{series_take_opt, DataFrame};
use crate::error::{FrameError, FrameResult};
use crate::series::{CompositeKey, Series};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinHow {
    Inner,
    Left,
    Right,
    Outer,
}

impl JoinHow {
    pub fn parse(s: &str) -> FrameResult<Self> {
        match s {
            "inner" => Ok(Self::Inner),
            "left" => Ok(Self::Left),
            "right" => Ok(Self::Right),
            "outer" => Ok(Self::Outer),
            other => Err(FrameError::Error(format!("unknown join how '{other}'"))),
        }
    }
}

pub fn join(
    left: &DataFrame,
    right: &DataFrame,
    on: &[&str],
    how: JoinHow,
) -> FrameResult<DataFrame> {
    if on.is_empty() {
        return Err(FrameError::Error("join requires at least one key".into()));
    }
    for k in on {
        let _ = left.get(k)?;
        let _ = right.get(k)?;
    }

    // Build right hash map: key → list of row indices
    let mut right_map: HashMap<CompositeKey, Vec<usize>> = HashMap::new();
    for i in 0..right.nrows() {
        let key = right.composite_key_at(on, i)?;
        right_map.entry(key).or_default().push(i);
    }

    let mut left_idx: Vec<Option<usize>> = Vec::new();
    let mut right_idx: Vec<Option<usize>> = Vec::new();
    let mut matched_right: HashSet<usize> = HashSet::new();

    for i in 0..left.nrows() {
        let key = left.composite_key_at(on, i)?;
        if let Some(rs) = right_map.get(&key) {
            for &r in rs {
                left_idx.push(Some(i));
                right_idx.push(Some(r));
                matched_right.insert(r);
            }
        } else if matches!(how, JoinHow::Left | JoinHow::Outer) {
            left_idx.push(Some(i));
            right_idx.push(None);
        }
    }

    if matches!(how, JoinHow::Right | JoinHow::Outer) {
        for j in 0..right.nrows() {
            if !matched_right.contains(&j) {
                left_idx.push(None);
                right_idx.push(Some(j));
            }
        }
    }

    if how == JoinHow::Inner {
        // already only matches
    }

    build_joined(left, right, on, &left_idx, &right_idx)
}

fn build_joined(
    left: &DataFrame,
    right: &DataFrame,
    on: &[&str],
    li: &[Option<usize>],
    ri: &[Option<usize>],
) -> FrameResult<DataFrame> {
    let on_set: HashSet<&str> = on.iter().copied().collect();
    let mut cols: Vec<Series> = Vec::new();

    // Join keys from left (or right when left is null)
    for &k in on {
        let ls = left.get(k)?;
        let rs = right.get(k)?;
        let n = li.len();
        let mut validity = crate::validity::Validity::all_valid(n);
        let data = match (&ls.data, &rs.data) {
            (crate::series::ColumnData::I64(_), _) | (crate::series::ColumnData::Date(_), _) => {
                let lv = ls.as_i64_slice().unwrap();
                let rv = rs.as_i64_slice().unwrap_or(&[]);
                let mut out = vec![0i64; n];
                for (j, (l, r)) in li.iter().zip(ri.iter()).enumerate() {
                    if let Some(i) = l {
                        if ls.validity.is_valid(*i) {
                            out[j] = lv[*i];
                        } else {
                            validity.set_null(j);
                        }
                    } else if let Some(i) = r {
                        if rs.validity.is_valid(*i) {
                            out[j] = rv[*i];
                        } else {
                            validity.set_null(j);
                        }
                    } else {
                        validity.set_null(j);
                    }
                }
                if matches!(ls.dtype(), crate::series::Dtype::Date) {
                    crate::series::ColumnData::Date(out)
                } else {
                    crate::series::ColumnData::I64(out)
                }
            }
            (crate::series::ColumnData::F64(lv), crate::series::ColumnData::F64(rv)) => {
                let mut out = vec![f64::NAN; n];
                for (j, (l, r)) in li.iter().zip(ri.iter()).enumerate() {
                    if let Some(i) = l {
                        if ls.validity.is_valid(*i) {
                            out[j] = lv[*i];
                        } else {
                            validity.set_null(j);
                        }
                    } else if let Some(i) = r {
                        if rs.validity.is_valid(*i) {
                            out[j] = rv[*i];
                        } else {
                            validity.set_null(j);
                        }
                    } else {
                        validity.set_null(j);
                    }
                }
                crate::series::ColumnData::F64(out)
            }
            (crate::series::ColumnData::Str(lv), crate::series::ColumnData::Str(rv)) => {
                let mut out = crate::series::StringColumn::new();
                for (j, (l, r)) in li.iter().zip(ri.iter()).enumerate() {
                    if let Some(i) = l {
                        if ls.validity.is_valid(*i) {
                            out.push(lv.get(*i));
                        } else {
                            out.push("");
                            validity.set_null(j);
                        }
                    } else if let Some(i) = r {
                        if rs.validity.is_valid(*i) {
                            out.push(rv.get(*i));
                        } else {
                            out.push("");
                            validity.set_null(j);
                        }
                    } else {
                        out.push("");
                        validity.set_null(j);
                    }
                }
                crate::series::ColumnData::Str(out)
            }
            (crate::series::ColumnData::Bool(lv), crate::series::ColumnData::Bool(rv)) => {
                let mut out = vec![false; n];
                for (j, (l, r)) in li.iter().zip(ri.iter()).enumerate() {
                    if let Some(i) = l {
                        if ls.validity.is_valid(*i) {
                            out[j] = lv[*i];
                        } else {
                            validity.set_null(j);
                        }
                    } else if let Some(i) = r {
                        if rs.validity.is_valid(*i) {
                            out[j] = rv[*i];
                        } else {
                            validity.set_null(j);
                        }
                    } else {
                        validity.set_null(j);
                    }
                }
                crate::series::ColumnData::Bool(out)
            }
            _ => {
                return Err(FrameError::Dtype(format!(
                    "join key '{k}' dtype mismatch between frames"
                )))
            }
        };
        cols.push(Series {
            name: k.to_string(),
            data,
            validity,
        });
    }

    for c in &left.columns {
        if on_set.contains(c.name.as_str()) {
            continue;
        }
        let mut s = series_take_opt(c, li);
        s.name = c.name.clone();
        cols.push(s);
    }

    for c in &right.columns {
        if on_set.contains(c.name.as_str()) {
            continue;
        }
        let mut s = series_take_opt(c, ri);
        if left.get(&c.name).is_ok() {
            s.name = format!("{}_right", c.name);
        }
        cols.push(s);
    }

    DataFrame::new(cols)
}
