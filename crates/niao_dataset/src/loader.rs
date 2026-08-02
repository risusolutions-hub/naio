//! Batch iterator (DataLoader-style).

use rand::seq::SliceRandom;
use rand::SeedableRng;

/// Streaming batch cursor over row indices.
#[derive(Clone, Debug)]
pub struct BatchLoader {
    indices: Vec<usize>,
    cursor: usize,
    batch_size: usize,
    drop_last: bool,
}

impl BatchLoader {
    /// Create a loader over `n` rows.
    ///
    /// // >>> use niao_dataset::BatchLoader;
    /// // >>> let mut it = BatchLoader::new(10, 3, false, 0, false);
    /// // >>> assert_eq!(it.next_range(), Some((0, 3)));
    pub fn new(n: usize, batch_size: usize, shuffle: bool, seed: u64, drop_last: bool) -> Self {
        let mut indices: Vec<usize> = (0..n).collect();
        if shuffle && n > 1 {
            let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
            indices.shuffle(&mut rng);
        }
        let batch_size = batch_size.max(1);
        Self {
            indices,
            cursor: 0,
            batch_size,
            drop_last,
        }
    }

    /// Number of rows covered by this loader.
    #[inline]
    pub fn len(&self) -> usize {
        self.indices.len()
    }

    /// Batch size configured for this loader.
    #[inline]
    pub fn batch_size(&self) -> usize {
        self.batch_size
    }

    /// Whether another batch is available.
    pub fn has_next(&self) -> bool {
        if self.cursor >= self.indices.len() {
            return false;
        }
        let remaining = self.indices.len() - self.cursor;
        if remaining < self.batch_size {
            return !self.drop_last && remaining > 0;
        }
        true
    }

    /// Next batch as (start, end) slice bounds into `indices`.
    pub fn next_range(&mut self) -> Option<(usize, usize)> {
        if self.cursor >= self.indices.len() {
            return None;
        }
        let remaining = self.indices.len() - self.cursor;
        if remaining < self.batch_size {
            if self.drop_last {
                return None;
            }
            let start = self.cursor;
            self.cursor = self.indices.len();
            return Some((start, self.cursor));
        }
        let start = self.cursor;
        self.cursor += self.batch_size;
        Some((start, self.cursor))
    }

    /// Row indices for the next batch.
    pub fn next_indices(&mut self) -> Option<Vec<usize>> {
        self.next_range().map(|(s, e)| self.indices[s..e].to_vec())
    }

    /// Rewind to the first batch.
    pub fn reset(&mut self) {
        self.cursor = 0;
    }

    /// Underlying permutation (read-only view).
    pub fn indices(&self) -> &[usize] {
        &self.indices
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batches_cover_all_without_drop_last() {
        let mut loader = BatchLoader::new(10, 3, false, 0, false);
        let mut total = 0;
        while let Some(ix) = loader.next_indices() {
            total += ix.len();
        }
        assert_eq!(total, 10);
    }

    #[test]
    fn drop_last_skips_partial() {
        let mut loader = BatchLoader::new(10, 3, false, 0, true);
        let mut total = 0;
        while let Some(ix) = loader.next_indices() {
            assert_eq!(ix.len(), 3);
            total += ix.len();
        }
        assert_eq!(total, 9);
    }

    #[test]
    fn reset_replays() {
        let mut loader = BatchLoader::new(5, 2, false, 0, false);
        let first: Vec<_> = loader.next_indices().into_iter().collect();
        loader.reset();
        let second: Vec<_> = loader.next_indices().into_iter().collect();
        assert_eq!(first, second);
    }
}
