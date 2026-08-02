//! Parallel batch diffing.

use crate::error::TextDiffError;
use crate::line;
use crate::matcher;
use crate::opts::{DiffOpts, Granularity};
use niao_parallel::map;

#[derive(Debug, Clone)]
pub struct DiffPair {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone)]
pub struct ParallelDiffResult {
    pub unified: Vec<String>,
    pub ratio: f64,
}

pub fn parallel_unified(
    pairs: &[DiffPair],
    opts: &DiffOpts,
    threads: usize,
) -> Result<Vec<Vec<String>>, TextDiffError> {
    Ok(map(pairs, threads, |p| {
        line::unified(&p.from, &p.to, opts).unwrap_or_default()
    }))
}

pub fn parallel_ratio(
    pairs: &[DiffPair],
    opts: &DiffOpts,
    threads: usize,
) -> Result<Vec<f64>, TextDiffError> {
    Ok(map(pairs, threads, |p| {
        matcher::ratio(&p.from, &p.to, opts, Granularity::Line).unwrap_or(0.0)
    }))
}

pub fn parallel_diff(
    pairs: &[DiffPair],
    opts: &DiffOpts,
    threads: usize,
) -> Result<Vec<ParallelDiffResult>, TextDiffError> {
    Ok(map(pairs, threads, |p| ParallelDiffResult {
        unified: line::unified(&p.from, &p.to, opts).unwrap_or_default(),
        ratio: matcher::ratio(&p.from, &p.to, opts, Granularity::Line).unwrap_or(0.0),
    }))
}
