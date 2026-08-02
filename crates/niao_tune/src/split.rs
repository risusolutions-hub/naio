//! Train/test and k-fold index splits (ntune-compatible contract).

use crate::error::{TuneError, TuneResult};
use niao_rand::{Rng, SeedableRng, StdRng};

/// Index split returned by `train_test_split`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexSplit {
    pub train: Vec<usize>,
    pub test: Vec<usize>,
}

/// One fold of cross-validation indices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoldSplit {
    pub train: Vec<usize>,
    pub test: Vec<usize>,
}

/// Shuffle-split `n` samples into train and test index lists.
///
/// `test_size` is a fraction in `(0, 1)`; at least one train and one test index are kept.
pub fn train_test_split_indices(n: usize, test_size: f64, seed: u64) -> TuneResult<IndexSplit> {
    if n == 0 {
        return Err(TuneError::InvalidSplit("n must be > 0".into()));
    }
    if !(test_size > 0.0 && test_size < 1.0) {
        return Err(TuneError::InvalidSplit(
            "test_size must be in (0, 1)".into(),
        ));
    }
    let n_test = ((n as f64) * test_size).round() as usize;
    let n_test = n_test.clamp(1, n.saturating_sub(1));

    let mut idx: Vec<usize> = (0..n).collect();
    fisher_yates(&mut idx, seed);

    Ok(IndexSplit {
        train: idx[n_test..].to_vec(),
        test: idx[..n_test].to_vec(),
    })
}

/// K-fold cross-validation index splits.
pub fn kfold_indices(
    n: usize,
    n_splits: usize,
    shuffle: bool,
    seed: u64,
) -> TuneResult<Vec<FoldSplit>> {
    if n == 0 {
        return Err(TuneError::InvalidSplit("n must be > 0".into()));
    }
    if n_splits < 2 || n_splits > n {
        return Err(TuneError::InvalidSplit(format!(
            "n_splits must be in [2, {n}], got {n_splits}"
        )));
    }

    let mut idx: Vec<usize> = (0..n).collect();
    if shuffle {
        fisher_yates(&mut idx, seed);
    }

    let base = n / n_splits;
    let rem = n % n_splits;
    let fold_sizes: Vec<usize> = (0..n_splits)
        .map(|i| base + if i < rem { 1 } else { 0 })
        .collect();

    let mut folds = Vec::with_capacity(n_splits);
    let mut start = 0usize;
    for &fs in &fold_sizes {
        let test: Vec<usize> = idx[start..start + fs].to_vec();
        let mut train = Vec::with_capacity(n - fs);
        train.extend_from_slice(&idx[..start]);
        train.extend_from_slice(&idx[start + fs..]);
        folds.push(FoldSplit { train, test });
        start += fs;
    }
    Ok(folds)
}

fn fisher_yates(idx: &mut [usize], seed: u64) {
    let n = idx.len();
    if n <= 1 {
        return;
    }
    let mut rng = StdRng::seed_from_u64(seed);
    for i in (1..n).rev() {
        let j = rng.gen_range_usize(0, i + 1);
        idx.swap(i, j);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_covers_all_indices() {
        let s = train_test_split_indices(10, 0.2, 7).unwrap();
        let mut all = s.train.clone();
        all.extend_from_slice(&s.test);
        all.sort_unstable();
        assert_eq!(all, (0..10).collect::<Vec<_>>());
        assert!(!s.train.is_empty() && !s.test.is_empty());
    }

    #[test]
    fn kfold_partition() {
        let folds = kfold_indices(9, 3, false, 0).unwrap();
        assert_eq!(folds.len(), 3);
        for fold in &folds {
            assert_eq!(fold.train.len() + fold.test.len(), 9);
        }
    }
}
