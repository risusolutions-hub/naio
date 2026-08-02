//! Hash-based group-by aggregations.

use crate::dataframe::DataFrame;
use crate::error::{FrameError, FrameResult};
use crate::series::{ColumnData, CompositeKey, Series};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggOp {
    Sum,
    Mean,
    Min,
    Max,
    Count,
    Std,
    Var,
    Median,
    First,
    Last,
    NUnique,
}

impl AggOp {
    pub fn parse(s: &str) -> FrameResult<Self> {
        match s {
            "sum" => Ok(Self::Sum),
            "mean" => Ok(Self::Mean),
            "min" => Ok(Self::Min),
            "max" => Ok(Self::Max),
            "count" => Ok(Self::Count),
            "std" => Ok(Self::Std),
            "var" => Ok(Self::Var),
            "median" => Ok(Self::Median),
            "first" => Ok(Self::First),
            "last" => Ok(Self::Last),
            "n_unique" => Ok(Self::NUnique),
            other => Err(FrameError::Error(format!("unknown agg '{other}'"))),
        }
    }
}

pub struct GroupBy<'a> {
    frame: &'a DataFrame,
    keys: Vec<String>,
    /// Ordered unique keys → row indices (may be empty until materialized)
    groups: Vec<(CompositeKey, Vec<usize>)>,
    indices_ready: bool,
}

impl<'a> GroupBy<'a> {
    pub fn new(frame: &'a DataFrame, keys: &[&str]) -> FrameResult<Self> {
        if keys.is_empty() {
            return Err(FrameError::Error(
                "groupby requires at least one key".into(),
            ));
        }
        for k in keys {
            let _ = frame.get(k)?;
        }
        // Fast path: single i64/date key — defer index lists; store key order only.
        // Index vectors are filled lazily in `ensure_indices` when a general agg needs them.
        if keys.len() == 1 {
            let s = frame.get(keys[0])?;
            if let Some(vals) = s.as_i64_slice() {
                let n = vals.len();
                let mut map: HashMap<i64, usize> = HashMap::with_capacity((n / 8).max(16));
                let mut groups: Vec<(CompositeKey, Vec<usize>)> = Vec::new();
                let mut null_gi: Option<usize> = None;
                for (i, &k) in vals.iter().enumerate() {
                    if s.validity.is_null(i) {
                        if null_gi.is_none() {
                            null_gi = Some(groups.len());
                            groups.push((
                                CompositeKey(vec![crate::series::RowKey::Null]),
                                Vec::new(),
                            ));
                        }
                        continue;
                    }
                    if !map.contains_key(&k) {
                        let gi = groups.len();
                        map.insert(k, gi);
                        groups.push((
                            CompositeKey(vec![crate::series::RowKey::I64(k)]),
                            Vec::new(),
                        ));
                    }
                }
                return Ok(Self {
                    frame,
                    keys: keys.iter().map(|s| (*s).to_string()).collect(),
                    groups,
                    indices_ready: false,
                });
            }
        }

        let mut map: HashMap<CompositeKey, usize> = HashMap::with_capacity(frame.nrows() / 8 + 16);
        let mut groups: Vec<(CompositeKey, Vec<usize>)> = Vec::new();
        for i in 0..frame.nrows() {
            let key = frame.composite_key_at(keys, i)?;
            if let Some(&gi) = map.get(&key) {
                groups[gi].1.push(i);
            } else {
                let gi = groups.len();
                map.insert(key.clone(), gi);
                groups.push((key, vec![i]));
            }
        }
        Ok(Self {
            frame,
            keys: keys.iter().map(|s| (*s).to_string()).collect(),
            groups,
            indices_ready: true,
        })
    }

    pub fn group_count(&self) -> usize {
        self.groups.len()
    }

    fn materialize_indices(&mut self) -> FrameResult<()> {
        if self.indices_ready {
            return Ok(());
        }
        for g in &mut self.groups {
            g.1.clear();
        }
        let mut map: HashMap<CompositeKey, usize> = HashMap::with_capacity(self.groups.len());
        for (i, (k, _)) in self.groups.iter().enumerate() {
            map.insert(k.clone(), i);
        }
        for i in 0..self.frame.nrows() {
            let key = self
                .frame
                .composite_key_at(&self.keys.iter().map(|s| s.as_str()).collect::<Vec<_>>(), i)?;
            if let Some(&gi) = map.get(&key) {
                self.groups[gi].1.push(i);
            }
        }
        self.indices_ready = true;
        Ok(())
    }

    /// Fast path when every agg is sum or mean on the same numeric column set:
    /// still uses pre-built groups (correct ordering).
    pub fn agg(&mut self, aggs: &[(&str, AggOp)]) -> FrameResult<DataFrame> {
        if self.keys.len() == 1
            && aggs
                .iter()
                .all(|(_, op)| matches!(op, AggOp::Sum | AggOp::Mean))
            && !aggs.is_empty()
        {
            if let Ok(out) = self.agg_sum_mean_stream(aggs) {
                return Ok(out);
            }
        }
        self.materialize_indices()?;
        self.agg_general(aggs)
    }

    fn agg_sum_mean_stream(&self, aggs: &[(&str, AggOp)]) -> FrameResult<DataFrame> {
        let key_name = &self.keys[0];
        let key_s = self.frame.get(key_name)?;
        let key_vals = key_s
            .as_i64_slice()
            .ok_or_else(|| FrameError::Dtype("stream agg needs i64 key".into()))?;

        let mut col_data: Vec<&[f64]> = Vec::with_capacity(aggs.len());
        let mut owned: Vec<Vec<f64>> = Vec::new();
        for &(name, _) in aggs {
            let s = self.frame.get(name)?;
            if let Some(slice) = s.as_f64_slice() {
                col_data.push(slice);
            } else {
                let v = s.to_f64_vec()?;
                owned.push(v);
                // placeholder; fixed below
                col_data.push(&[]);
            }
        }
        // Fix pointers into owned
        let mut owned_i = 0;
        for (i, &(name, _)) in aggs.iter().enumerate() {
            let s = self.frame.get(name)?;
            if s.as_f64_slice().is_none() {
                col_data[i] = owned[owned_i].as_slice();
                owned_i += 1;
            }
        }

        // key -> (group index)
        let mut map: HashMap<i64, usize> = HashMap::with_capacity(self.groups.len().max(16));
        let mut keys_out: Vec<i64> = Vec::new();
        let mut key_valid: Vec<bool> = Vec::new();
        let mut sums: Vec<Vec<f64>> = aggs.iter().map(|_| Vec::new()).collect();
        let mut counts: Vec<Vec<usize>> = aggs.iter().map(|_| Vec::new()).collect();

        let n = self.frame.nrows();
        for i in 0..n {
            let (k, valid) = if key_s.validity.is_null(i) {
                (0i64, false)
            } else {
                (key_vals[i], true)
            };
            // Skip null keys in streaming path (still counted in general path)
            if !valid {
                continue;
            }
            let gi = if let Some(&gi) = map.get(&k) {
                gi
            } else {
                let gi = keys_out.len();
                map.insert(k, gi);
                keys_out.push(k);
                key_valid.push(true);
                for c in 0..aggs.len() {
                    sums[c].push(0.0);
                    counts[c].push(0);
                }
                gi
            };
            for (c, _) in aggs.iter().enumerate() {
                let x = col_data[c][i];
                if !x.is_nan() {
                    sums[c][gi] += x;
                    counts[c][gi] += 1;
                }
            }
        }

        let mut out_cols = vec![Series::from_i64(key_name.clone(), keys_out)];
        for (c, &(name, op)) in aggs.iter().enumerate() {
            let vals: Vec<f64> = (0..sums[c].len())
                .map(|gi| {
                    let cnt = counts[c][gi];
                    if cnt == 0 {
                        f64::NAN
                    } else if matches!(op, AggOp::Mean) {
                        sums[c][gi] / cnt as f64
                    } else {
                        sums[c][gi]
                    }
                })
                .collect();
            let suffix = if matches!(op, AggOp::Mean) {
                "mean"
            } else {
                "sum"
            };
            out_cols.push(Series::from_f64(format!("{name}_{suffix}"), vals));
        }
        DataFrame::new(out_cols)
    }

    fn agg_general(&self, aggs: &[(&str, AggOp)]) -> FrameResult<DataFrame> {
        let ng = self.groups.len();
        let mut out_cols: Vec<Series> = Vec::new();

        // Reconstruct key columns from first row of each group
        for (ki, key_name) in self.keys.iter().enumerate() {
            let src = self.frame.get(key_name)?;
            let indices: Vec<usize> = self.groups.iter().map(|(_, idxs)| idxs[0]).collect();
            let mut col = src.take(&indices);
            // Preserve null keys from CompositeKey
            for (gi, (ck, _)) in self.groups.iter().enumerate() {
                if matches!(ck.0.get(ki), Some(crate::series::RowKey::Null)) {
                    col.validity.set_null(gi);
                }
            }
            col.name = key_name.clone();
            out_cols.push(col);
        }

        for &(col_name, op) in aggs {
            let src = self.frame.get(col_name)?;
            let series = match op {
                AggOp::Count => {
                    let counts: Vec<i64> = self
                        .groups
                        .iter()
                        .map(|(_, idxs)| idxs.len() as i64)
                        .collect();
                    Series::from_i64(format!("{col_name}_count"), counts)
                }
                AggOp::NUnique => {
                    let vals: Vec<i64> = self
                        .groups
                        .iter()
                        .map(|(_, idxs)| {
                            let mut seen = std::collections::HashSet::new();
                            for &i in idxs {
                                if src.validity.is_valid(i) {
                                    seen.insert(src.row_key(i));
                                }
                            }
                            seen.len() as i64
                        })
                        .collect();
                    Series::from_i64(format!("{col_name}_n_unique"), vals)
                }
                AggOp::First => {
                    let indices: Vec<usize> = self.groups.iter().map(|(_, idxs)| idxs[0]).collect();
                    let mut s = src.take(&indices);
                    s.name = format!("{col_name}_first");
                    s
                }
                AggOp::Last => {
                    let indices: Vec<usize> = self
                        .groups
                        .iter()
                        .map(|(_, idxs)| *idxs.last().unwrap())
                        .collect();
                    let mut s = src.take(&indices);
                    s.name = format!("{col_name}_last");
                    s
                }
                AggOp::Sum
                | AggOp::Mean
                | AggOp::Min
                | AggOp::Max
                | AggOp::Std
                | AggOp::Var
                | AggOp::Median => {
                    let mut out = Vec::with_capacity(ng);
                    let mut validity = crate::validity::Validity::all_valid(ng);
                    for (gi, (_, idxs)) in self.groups.iter().enumerate() {
                        match numeric_agg(src, idxs, op) {
                            Some(v) => out.push(v),
                            None => {
                                out.push(f64::NAN);
                                validity.set_null(gi);
                            }
                        }
                    }
                    let name = format!("{col_name}_{}", agg_suffix(op));
                    Series::from_f64(name, out).with_validity(validity)?
                }
            };
            out_cols.push(series);
        }
        DataFrame::new(out_cols)
    }
}

fn agg_suffix(op: AggOp) -> &'static str {
    match op {
        AggOp::Sum => "sum",
        AggOp::Mean => "mean",
        AggOp::Min => "min",
        AggOp::Max => "max",
        AggOp::Std => "std",
        AggOp::Var => "var",
        AggOp::Median => "median",
        _ => "agg",
    }
}

fn collect_f64(src: &Series, idxs: &[usize]) -> Vec<f64> {
    let mut vals = Vec::with_capacity(idxs.len());
    match &src.data {
        ColumnData::F64(v) => {
            for &i in idxs {
                if src.validity.is_valid(i) && !v[i].is_nan() {
                    vals.push(v[i]);
                }
            }
        }
        ColumnData::I64(v) | ColumnData::Date(v) => {
            for &i in idxs {
                if src.validity.is_valid(i) {
                    vals.push(v[i] as f64);
                }
            }
        }
        ColumnData::Bool(v) => {
            for &i in idxs {
                if src.validity.is_valid(i) {
                    vals.push(if v[i] { 1.0 } else { 0.0 });
                }
            }
        }
        ColumnData::Str(_) => {}
    }
    vals
}

#[inline]
fn sum_count_f64(src: &Series, idxs: &[usize]) -> (f64, usize) {
    let mut sum = 0.0;
    let mut cnt = 0usize;
    match &src.data {
        ColumnData::F64(v) => {
            for &i in idxs {
                if src.validity.is_valid(i) {
                    let x = v[i];
                    if !x.is_nan() {
                        sum += x;
                        cnt += 1;
                    }
                }
            }
        }
        ColumnData::I64(v) | ColumnData::Date(v) => {
            for &i in idxs {
                if src.validity.is_valid(i) {
                    sum += v[i] as f64;
                    cnt += 1;
                }
            }
        }
        ColumnData::Bool(v) => {
            for &i in idxs {
                if src.validity.is_valid(i) {
                    sum += if v[i] { 1.0 } else { 0.0 };
                    cnt += 1;
                }
            }
        }
        ColumnData::Str(_) => {}
    }
    (sum, cnt)
}

fn numeric_agg(src: &Series, idxs: &[usize], op: AggOp) -> Option<f64> {
    match op {
        AggOp::Sum => {
            let (sum, cnt) = sum_count_f64(src, idxs);
            if cnt == 0 {
                None
            } else {
                Some(sum)
            }
        }
        AggOp::Mean => {
            let (sum, cnt) = sum_count_f64(src, idxs);
            if cnt == 0 {
                None
            } else {
                Some(sum / cnt as f64)
            }
        }
        AggOp::Min | AggOp::Max | AggOp::Var | AggOp::Std | AggOp::Median => {
            let mut vals = collect_f64(src, idxs);
            if vals.is_empty() {
                return None;
            }
            match op {
                AggOp::Min => vals.iter().cloned().reduce(f64::min),
                AggOp::Max => vals.iter().cloned().reduce(f64::max),
                AggOp::Var => {
                    let n = vals.len() as f64;
                    if n < 2.0 {
                        return Some(0.0);
                    }
                    let mean = vals.iter().sum::<f64>() / n;
                    let ss: f64 = vals.iter().map(|x| (x - mean).powi(2)).sum();
                    Some(ss / (n - 1.0))
                }
                AggOp::Std => {
                    let n = vals.len() as f64;
                    if n < 2.0 {
                        return Some(0.0);
                    }
                    let mean = vals.iter().sum::<f64>() / n;
                    let ss: f64 = vals.iter().map(|x| (x - mean).powi(2)).sum();
                    Some((ss / (n - 1.0)).sqrt())
                }
                AggOp::Median => {
                    vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
                    let m = vals.len();
                    if m % 2 == 1 {
                        Some(vals[m / 2])
                    } else {
                        Some((vals[m / 2 - 1] + vals[m / 2]) / 2.0)
                    }
                }
                _ => None,
            }
        }
        _ => None,
    }
}
