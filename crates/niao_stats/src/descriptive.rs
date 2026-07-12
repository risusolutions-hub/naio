//! Descriptive statistics.

use crate::error::{StatsError, StatsResult};
use std::collections::HashMap;

pub fn mean(data: &[f64]) -> StatsResult<f64> {
    if data.is_empty() {
        return Err(StatsError::Error("mean of empty slice".into()));
    }
    Ok(data.iter().sum::<f64>() / data.len() as f64)
}

pub fn var(data: &[f64], ddof: usize) -> StatsResult<f64> {
    if data.len() <= ddof {
        return Err(StatsError::Error("var: insufficient data".into()));
    }
    let m = mean(data)?;
    let ss: f64 = data.iter().map(|x| (x - m).powi(2)).sum();
    Ok(ss / (data.len() - ddof) as f64)
}

pub fn std(data: &[f64], ddof: usize) -> StatsResult<f64> {
    Ok(var(data, ddof)?.sqrt())
}

pub fn median(data: &[f64]) -> StatsResult<f64> {
    if data.is_empty() {
        return Err(StatsError::Error("median of empty slice".into()));
    }
    let mut v = data.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    if n % 2 == 1 {
        Ok(v[n / 2])
    } else {
        Ok((v[n / 2 - 1] + v[n / 2]) / 2.0)
    }
}

pub fn mode(data: &[f64]) -> StatsResult<f64> {
    if data.is_empty() {
        return Err(StatsError::Error("mode of empty slice".into()));
    }
    let mut counts: HashMap<i64, usize> = HashMap::new();
    for &x in data {
        let key = x.to_bits() as i64;
        *counts.entry(key).or_insert(0) += 1;
    }
    let (best_bits, _) = counts
        .iter()
        .max_by_key(|(_, c)| *c)
        .ok_or_else(|| StatsError::Error("mode failed".into()))?;
    Ok(f64::from_bits(*best_bits as u64))
}

pub fn quantile(data: &[f64], q: f64) -> StatsResult<f64> {
    if data.is_empty() {
        return Err(StatsError::Error("quantile of empty slice".into()));
    }
    if q < 0.0 || q > 1.0 {
        return Err(StatsError::Domain("quantile q must be in [0,1]".into()));
    }
    let mut v = data.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len() as f64;
    let idx = q * (n - 1.0);
    let lo = idx.floor() as usize;
    let hi = idx.ceil() as usize;
    if lo == hi {
        Ok(v[lo])
    } else {
        let w = idx - lo as f64;
        Ok(v[lo] * (1.0 - w) + v[hi] * w)
    }
}

pub fn percentile(data: &[f64], p: f64) -> StatsResult<f64> {
    quantile(data, p / 100.0)
}

pub fn skew(data: &[f64], ddof: usize) -> StatsResult<f64> {
    let n = data.len();
    if n < 3 {
        return Err(StatsError::Error("skew requires n >= 3".into()));
    }
    let m = mean(data)?;
    let s = std(data, ddof)?;
    if s == 0.0 {
        return Ok(0.0);
    }
    let m3: f64 = data.iter().map(|x| ((x - m) / s).powi(3)).sum();
    let nf = n as f64;
    Ok(m3 / nf)
}

pub fn kurtosis(data: &[f64], ddof: usize) -> StatsResult<f64> {
    let n = data.len();
    if n < 4 {
        return Err(StatsError::Error("kurtosis requires n >= 4".into()));
    }
    let m = mean(data)?;
    let s = std(data, ddof)?;
    if s == 0.0 {
        return Ok(0.0);
    }
    let m4: f64 = data.iter().map(|x| ((x - m) / s).powi(4)).sum();
    let nf = n as f64;
    Ok(m4 / nf - 3.0) // excess kurtosis
}

pub fn min_val(data: &[f64]) -> StatsResult<f64> {
    data.iter()
        .copied()
        .reduce(f64::min)
        .ok_or_else(|| StatsError::Error("min of empty".into()))
}

pub fn max_val(data: &[f64]) -> StatsResult<f64> {
    data.iter()
        .copied()
        .reduce(f64::max)
        .ok_or_else(|| StatsError::Error("max of empty".into()))
}

pub fn iqr(data: &[f64]) -> StatsResult<f64> {
    Ok(quantile(data, 0.75)? - quantile(data, 0.25)?)
}

#[derive(Debug, Clone)]
pub struct DescribeResult {
    pub n: usize,
    pub mean: f64,
    pub std: f64,
    pub min: f64,
    pub q25: f64,
    pub median: f64,
    pub q75: f64,
    pub max: f64,
}

pub fn describe(data: &[f64]) -> StatsResult<DescribeResult> {
    Ok(DescribeResult {
        n: data.len(),
        mean: mean(data)?,
        std: std(data, 1)?,
        min: min_val(data)?,
        q25: quantile(data, 0.25)?,
        median: median(data)?,
        q75: quantile(data, 0.75)?,
        max: max_val(data)?,
    })
}

pub fn zscore(data: &[f64]) -> StatsResult<Vec<f64>> {
    let m = mean(data)?;
    let s = std(data, 0)?;
    if s == 0.0 {
        return Ok(vec![0.0; data.len()]);
    }
    Ok(data.iter().map(|x| (x - m) / s).collect())
}

pub fn trim_mean(data: &[f64], proportiontocut: f64) -> StatsResult<f64> {
    if proportiontocut < 0.0 || proportiontocut >= 0.5 {
        return Err(StatsError::Domain("proportiontocut must be in [0, 0.5)".into()));
    }
    let mut v = data.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let cut = (proportiontocut * v.len() as f64).floor() as usize;
    if cut * 2 >= v.len() {
        return Err(StatsError::Error("trim_mean: too much cut".into()));
    }
    mean(&v[cut..v.len() - cut])
}
