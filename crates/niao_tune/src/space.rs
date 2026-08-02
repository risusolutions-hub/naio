//! Search-space definitions, grid enumeration, and random sampling.

use crate::error::{TuneError, TuneResult};
use niao_rand::{Rng, SeedableRng, StdRng};
use std::collections::BTreeMap;

/// A single hyperparameter value.
#[derive(Clone, Debug, PartialEq)]
pub enum ParamValue {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
}

impl ParamValue {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Int(n) => Some(*n as f64),
            Self::Float(f) => Some(*f),
            _ => None,
        }
    }

    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::String(_) => "string",
            Self::Bool(_) => "bool",
        }
    }
}

/// One dimension of a search space.
#[derive(Clone, Debug)]
pub enum SpaceDim {
    /// Explicit grid values (also used for categorical choices).
    Grid(Vec<ParamValue>),
    Float {
        low: f64,
        high: f64,
        log: bool,
    },
    Int {
        low: i64,
        high: i64,
        log: bool,
    },
}

/// Ordered search space (stable key order for reproducible grids).
pub type SearchSpace = BTreeMap<String, SpaceDim>;

/// Count Cartesian product size for a grid-only space.
pub fn grid_size(space: &SearchSpace) -> TuneResult<usize> {
    if space.is_empty() {
        return Err(TuneError::EmptySpace);
    }
    let mut size = 1usize;
    for dim in space.values() {
        let n = match dim {
            SpaceDim::Grid(vals) => {
                if vals.is_empty() {
                    return Err(TuneError::InvalidSpace(
                        "grid dimension cannot be empty".into(),
                    ));
                }
                vals.len()
            }
            SpaceDim::Float { .. } | SpaceDim::Int { .. } => {
                return Err(TuneError::InvalidSpace(
                    "grid_size() requires explicit grid lists for every dimension".into(),
                ));
            }
        };
        size = size
            .checked_mul(n)
            .ok_or_else(|| TuneError::InvalidSpace("grid too large".into()))?;
    }
    Ok(size)
}

/// Enumerate all grid combinations in lexicographic key order.
pub fn grid_cartesian(space: &SearchSpace) -> TuneResult<Vec<BTreeMap<String, ParamValue>>> {
    if space.is_empty() {
        return Err(TuneError::EmptySpace);
    }
    let keys: Vec<String> = space.keys().cloned().collect();
    let mut grids: Vec<Vec<ParamValue>> = Vec::with_capacity(keys.len());
    for key in &keys {
        match space.get(key).unwrap() {
            SpaceDim::Grid(vals) => {
                if vals.is_empty() {
                    return Err(TuneError::InvalidSpace(format!(
                        "grid dimension '{key}' cannot be empty"
                    )));
                }
                grids.push(vals.clone());
            }
            SpaceDim::Float { .. } | SpaceDim::Int { .. } => {
                return Err(TuneError::InvalidSpace(format!(
                    "grid_cartesian() requires grid lists; '{key}' is a range dimension"
                )));
            }
        }
    }

    let mut out = Vec::new();
    let mut idx = vec![0usize; grids.len()];
    'outer: loop {
        let mut combo = BTreeMap::new();
        for (i, key) in keys.iter().enumerate() {
            combo.insert(key.clone(), grids[i][idx[i]].clone());
        }
        out.push(combo);

        for i in (0..idx.len()).rev() {
            idx[i] += 1;
            if idx[i] < grids[i].len() {
                continue 'outer;
            }
            idx[i] = 0;
        }
        break;
    }
    Ok(out)
}

fn sample_dim(rng: &mut StdRng, dim: &SpaceDim) -> TuneResult<ParamValue> {
    match dim {
        SpaceDim::Grid(vals) => {
            if vals.is_empty() {
                return Err(TuneError::InvalidSpace(
                    "categorical/grid dimension cannot be empty".into(),
                ));
            }
            Ok(vals[rng.gen_range_usize(0, vals.len())].clone())
        }
        SpaceDim::Float { low, high, log } => {
            if !low.is_finite() || !high.is_finite() || low >= high {
                return Err(TuneError::InvalidSpace(format!(
                    "float range requires finite low < high, got {low}..{high}"
                )));
            }
            let v = if *log {
                let log_low = low.ln();
                let log_high = high.ln();
                (rng.gen_f64() * (log_high - log_low) + log_low).exp()
            } else {
                rng.gen_f64() * (high - low) + low
            };
            Ok(ParamValue::Float(v))
        }
        SpaceDim::Int { low, high, log } => {
            if low >= high {
                return Err(TuneError::InvalidSpace(format!(
                    "int range requires low < high, got {low}..{high}"
                )));
            }
            let v = if *log {
                let log_low = (*low as f64).ln();
                let log_high = (*high as f64).ln();
                (rng.gen_f64() * (log_high - log_low) + log_low)
                    .exp()
                    .round() as i64
            } else {
                rng.gen_range_i64(*low, *high + 1)
            };
            Ok(ParamValue::Int(v.clamp(*low, *high)))
        }
    }
}

/// Draw `n` independent random parameter sets from `space`.
pub fn sample_random(
    space: &SearchSpace,
    n: usize,
    seed: u64,
) -> TuneResult<Vec<BTreeMap<String, ParamValue>>> {
    if space.is_empty() {
        return Err(TuneError::EmptySpace);
    }
    if n == 0 {
        return Ok(Vec::new());
    }
    let mut rng = StdRng::seed_from_u64(seed);
    let keys: Vec<String> = space.keys().cloned().collect();
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let mut params = BTreeMap::new();
        for key in &keys {
            let val = sample_dim(&mut rng, space.get(key).unwrap())?;
            params.insert(key.clone(), val);
        }
        out.push(params);
    }
    Ok(out)
}

/// Validate a search space (non-empty, consistent ranges).
pub fn validate_space(space: &SearchSpace) -> TuneResult<()> {
    if space.is_empty() {
        return Err(TuneError::EmptySpace);
    }
    for (name, dim) in space {
        match dim {
            SpaceDim::Grid(vals) if vals.is_empty() => {
                return Err(TuneError::InvalidSpace(format!(
                    "dimension '{name}' has empty grid"
                )));
            }
            SpaceDim::Float { low, high, .. }
                if !low.is_finite() || !high.is_finite() || low >= high =>
            {
                return Err(TuneError::InvalidSpace(format!(
                    "dimension '{name}' has invalid float range"
                )));
            }
            SpaceDim::Int { low, high, .. } if low >= high => {
                return Err(TuneError::InvalidSpace(format!(
                    "dimension '{name}' has invalid int range"
                )));
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid_space() -> SearchSpace {
        let mut s = BTreeMap::new();
        s.insert(
            "lr".into(),
            SpaceDim::Grid(vec![ParamValue::Float(0.01), ParamValue::Float(0.1)]),
        );
        s.insert(
            "depth".into(),
            SpaceDim::Grid(vec![ParamValue::Int(3), ParamValue::Int(5)]),
        );
        s
    }

    #[test]
    fn grid_size_and_cartesian() {
        let s = grid_space();
        assert_eq!(grid_size(&s).unwrap(), 4);
        let combos = grid_cartesian(&s).unwrap();
        assert_eq!(combos.len(), 4);
    }

    #[test]
    fn random_sample_reproducible() {
        let mut s = BTreeMap::new();
        s.insert(
            "x".into(),
            SpaceDim::Float {
                low: 0.0,
                high: 1.0,
                log: false,
            },
        );
        let a = sample_random(&s, 5, 42).unwrap();
        let b = sample_random(&s, 5, 42).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn empty_space_errors() {
        assert!(matches!(
            grid_size(&BTreeMap::new()),
            Err(TuneError::EmptySpace)
        ));
    }
}
