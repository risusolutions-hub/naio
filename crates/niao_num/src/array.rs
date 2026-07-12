//! N-dimensional array with shared-buffer views and broadcasting.

use crate::error::{NumError, NumResult};
use std::sync::Arc;

#[derive(Clone)]
pub struct NdArray {
    pub data: Arc<Vec<f64>>,
    pub shape: Vec<usize>,
    pub strides: Vec<isize>,
    pub offset: usize,
}

impl NdArray {
    pub fn from_vec(shape: Vec<usize>, data: Vec<f64>) -> NumResult<Self> {
        let n: usize = shape.iter().product();
        if data.len() != n {
            return Err(NumError::ShapeMismatch(format!(
                "data length {} does not match shape product {n}",
                data.len()
            )));
        }
        let strides = row_major_strides(&shape);
        Ok(Self {
            data: Arc::new(data),
            shape,
            strides,
            offset: 0,
        })
    }

    pub fn zeros(shape: &[usize]) -> NumResult<Self> {
        let n: usize = shape.iter().product();
        Self::from_vec(shape.to_vec(), vec![0.0; n])
    }

    pub fn ones(shape: &[usize]) -> NumResult<Self> {
        let n: usize = shape.iter().product();
        Self::from_vec(shape.to_vec(), vec![1.0; n])
    }

    pub fn len(&self) -> usize {
        self.shape.iter().product()
    }

    pub fn ndim(&self) -> usize {
        self.shape.len()
    }

    pub fn is_contiguous(&self) -> bool {
        if self.offset != 0 {
            return false;
        }
        let expected = row_major_strides(&self.shape);
        self.strides == expected
    }

    pub fn as_slice(&self) -> NumResult<&[f64]> {
        if !self.is_contiguous() {
            return Err(NumError::Error(
                "array is not contiguous; call to_vec() first".into(),
            ));
        }
        let n = self.len();
        Ok(&self.data[self.offset..self.offset + n])
    }

    pub fn to_vec(&self) -> Vec<f64> {
        let mut out = vec![0.0; self.len()];
        self.read_all(&mut out);
        out
    }

    pub fn read_all(&self, out: &mut [f64]) {
        let mut idx = vec![0usize; self.ndim()];
        for i in 0..out.len() {
            out[i] = self.get_linear(i, &idx);
            advance_indices(&self.shape, &mut idx);
        }
    }

    fn get_linear(&self, linear: usize, idx: &[usize]) -> f64 {
        let mut pos = self.offset;
        let mut rem = linear;
        for (d, &dim) in self.shape.iter().enumerate().rev() {
            let coord = rem % dim;
            rem /= dim;
            pos = ((pos as isize) + idx.get(d).copied().unwrap_or(coord) as isize * self.strides[d])
                as usize;
        }
        self.data[pos]
    }

    pub fn index(&self, indices: &[usize]) -> NumResult<f64> {
        if indices.len() != self.ndim() {
            return Err(NumError::ShapeMismatch("index rank mismatch".into()));
        }
        for (i, (&idx, &dim)) in indices.iter().zip(self.shape.iter()).enumerate() {
            if idx >= dim {
                return Err(NumError::ShapeMismatch(format!(
                    "index {idx} out of bounds for dimension {i} with size {dim}"
                )));
            }
        }
        let mut pos = self.offset as isize;
        for (i, &idx) in indices.iter().enumerate() {
            pos += self.strides[i] * idx as isize;
        }
        Ok(self.data[pos as usize])
    }

    pub fn reshape(&self, new_shape: Vec<usize>) -> NumResult<Self> {
        let n: usize = new_shape.iter().product();
        if n != self.len() {
            return Err(NumError::ShapeMismatch(format!(
                "cannot reshape {self:?} to {new_shape:?}"
            )));
        }
        if self.is_contiguous() {
            let strides = row_major_strides(&new_shape);
            Ok(Self {
                data: Arc::clone(&self.data),
                shape: new_shape,
                strides,
                offset: self.offset,
            })
        } else {
            Self::from_vec(new_shape, self.to_vec())
        }
    }

    pub fn transpose(&self) -> NumResult<Self> {
        if self.ndim() != 2 {
            return Err(NumError::ShapeMismatch(
                "transpose requires a 2-D array".into(),
            ));
        }
        let rows = self.shape[0];
        let cols = self.shape[1];
        let mut out = vec![0.0; rows * cols];
        for r in 0..rows {
            for c in 0..cols {
                out[c * rows + r] = self.index(&[r, c])?;
            }
        }
        Self::from_vec(vec![cols, rows], out)
    }

    pub fn broadcast_shapes(a: &[usize], b: &[usize]) -> NumResult<Vec<usize>> {
        let max_rank = a.len().max(b.len());
        let mut out = vec![1usize; max_rank];
        for i in 0..max_rank {
            let da = a.get(a.len().wrapping_sub(1 + i)).copied().unwrap_or(1);
            let db = b.get(b.len().wrapping_sub(1 + i)).copied().unwrap_or(1);
            if da != db && da != 1 && db != 1 {
                return Err(NumError::ShapeMismatch(format!(
                    "cannot broadcast shapes {a:?} and {b:?}"
                )));
            }
            out[max_rank - 1 - i] = da.max(db);
        }
        Ok(out)
    }

    pub fn broadcast_to(&self, target: &[usize]) -> NumResult<Self> {
        if self.shape == target {
            return Ok(self.clone());
        }
        let out_len: usize = target.iter().product();
        let mut out = vec![0.0; out_len];
        let mut idx = vec![0usize; target.len()];
        for i in 0..out_len {
            let src_idx = broadcast_index(&idx, &self.shape);
            out[i] = self.index(&src_idx)?;
            advance_indices(target, &mut idx);
        }
        Self::from_vec(target.to_vec(), out)
    }

    pub fn map_unary<F>(&self, f: F) -> NumResult<Self>
    where
        F: Fn(f64) -> f64,
    {
        let data: Vec<f64> = self.to_vec().into_iter().map(f).collect();
        Self::from_vec(self.shape.clone(), data)
    }

    pub fn map_binary(&self, other: &Self, f: impl Fn(f64, f64) -> f64) -> NumResult<Self> {
        let out_shape = Self::broadcast_shapes(&self.shape, &other.shape)?;
        let a = self.broadcast_to(&out_shape)?;
        let b = other.broadcast_to(&out_shape)?;
        let av = a.to_vec();
        let bv = b.to_vec();
        let data: Vec<f64> = av.iter().zip(bv.iter()).map(|(&x, &y)| f(x, y)).collect();
        Self::from_vec(out_shape, data)
    }
}

impl fmt::Debug for NdArray {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NdArray(shape={:?})", self.shape)
    }
}

use std::fmt;

pub fn row_major_strides(shape: &[usize]) -> Vec<isize> {
    let mut strides = vec![1isize; shape.len()];
    for i in (0..shape.len().saturating_sub(1)).rev() {
        strides[i] = strides[i + 1] * shape[i + 1] as isize;
    }
    strides
}

fn broadcast_index(target_idx: &[usize], self_shape: &[usize]) -> Vec<usize> {
    let rank_diff = target_idx.len() as isize - self_shape.len() as isize;
    self_shape
        .iter()
        .enumerate()
        .map(|(si, &dim)| {
            let ti = si as isize + rank_diff;
            if ti < 0 {
                0
            } else {
                let coord = target_idx[ti as usize];
                if dim == 1 { 0 } else { coord }
            }
        })
        .collect()
}

fn advance_indices(shape: &[usize], idx: &mut [usize]) {
    for i in (0..shape.len()).rev() {
        idx[i] += 1;
        if idx[i] < shape[i] {
            return;
        }
        idx[i] = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reshape_preserves_elements() {
        let a = NdArray::from_vec(vec![2, 3], (0..6).map(|x| x as f64).collect()).unwrap();
        let b = a.reshape(vec![3, 2]).unwrap();
        assert_eq!(b.shape, vec![3, 2]);
        assert_eq!(b.to_vec(), (0..6).map(|x| x as f64).collect::<Vec<_>>());
    }
}
