//! CSR sparse matrix for vectorizer output.

use crate::error::{NlpError, NlpResult};
use niao_num::NdArray;

/// Compressed sparse row matrix (scipy/sklearn CSR layout).
#[derive(Debug, Clone, PartialEq)]
pub struct CsrMatrix {
    pub n_rows: usize,
    pub n_cols: usize,
    pub indptr: Vec<usize>,
    pub indices: Vec<usize>,
    pub data: Vec<f64>,
}

impl CsrMatrix {
    pub fn new(n_rows: usize, n_cols: usize) -> Self {
        Self {
            n_rows,
            n_cols,
            indptr: vec![0; n_rows + 1],
            indices: Vec::new(),
            data: Vec::new(),
        }
    }

    pub fn nnz(&self) -> usize {
        self.data.len()
    }

    pub fn row_values(&self, row: usize) -> impl Iterator<Item = (usize, f64)> + '_ {
        let start = self.indptr[row];
        let end = self.indptr[row + 1];
        self.indices[start..end]
            .iter()
            .zip(self.data[start..end].iter())
            .map(|(&col, &val)| (col, val))
    }

    /// Dense row-major `[n_rows, n_cols]` matrix via niao_num.
    pub fn to_dense(&self) -> NlpResult<NdArray> {
        let mut data = vec![0.0f64; self.n_rows * self.n_cols];
        for r in 0..self.n_rows {
            for (c, v) in self.row_values(r) {
                data[r * self.n_cols + c] = v;
            }
        }
        NdArray::from_vec(vec![self.n_rows, self.n_cols], data)
            .map_err(|e| NlpError::Error(e.to_string()))
    }

    /// Alias for `to_dense` (nnum interop).
    pub fn to_nnum(&self) -> NlpResult<NdArray> {
        self.to_dense()
    }

    pub fn l2_normalize_rows(&mut self) {
        for r in 0..self.n_rows {
            let start = self.indptr[r];
            let end = self.indptr[r + 1];
            let mut norm_sq = 0.0;
            for i in start..end {
                norm_sq += self.data[i] * self.data[i];
            }
            if norm_sq > 0.0 {
                let inv = 1.0 / norm_sq.sqrt();
                for i in start..end {
                    self.data[i] *= inv;
                }
            }
        }
    }
}
