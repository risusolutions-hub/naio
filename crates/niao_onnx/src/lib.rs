//! ONNX model loading and CPU inference for Niao (~onnxruntime subset).
//!
//! Pure-Rust CPU inference via [tract](https://github.com/sonos/tract).

mod error;
mod io_desc;
mod session;

pub use error::{OnnxError, OnnxResult};
pub use io_desc::IoDesc;
pub use session::{
    engine_version, inspect_bytes, inspect_path, load_bytes, load_path, OnnxSession, SessionOptions,
};
pub fn tensor_f32(shape: &[usize], data: &[f32]) -> OnnxResult<(Vec<usize>, Vec<f32>)> {
    let n: usize = shape.iter().product();
    if data.len() != n {
        return Err(OnnxError::SizeMismatch {
            name: "tensor".into(),
            expected: n,
            got: data.len(),
        });
    }
    Ok((shape.to_vec(), data.to_vec()))
}

/// Zero-filled float32 tensor with the given shape.
pub fn zeros_f32(shape: &[usize]) -> OnnxResult<(Vec<usize>, Vec<f32>)> {
    if shape.is_empty() {
        return Err(OnnxError::Empty);
    }
    let n: usize = shape.iter().product();
    if n == 0 {
        return Err(OnnxError::Empty);
    }
    Ok((shape.to_vec(), vec![0.0f32; n]))
}

/// Flatten a rank-2 batch of vectors into `[batch, feature]` row-major layout.
pub fn batch_from_rows(rows: &[Vec<f32>]) -> OnnxResult<(Vec<usize>, Vec<f32>)> {
    if rows.is_empty() {
        return Err(OnnxError::Empty);
    }
    let feat = rows[0].len();
    if feat == 0 {
        return Err(OnnxError::Empty);
    }
    for (i, row) in rows.iter().enumerate().skip(1) {
        if row.len() != feat {
            return Err(OnnxError::ShapeMismatch {
                name: "batch".into(),
                expected: format!("feature dim {feat}"),
                got: format!("row {i} has len {}", row.len()),
            });
        }
    }
    let mut flat = Vec::with_capacity(rows.len() * feat);
    for row in rows {
        flat.extend_from_slice(row);
    }
    Ok((vec![rows.len(), feat], flat))
}

/// Naive elementwise matmul for micro-benchmark baseline `[m,k] @ [k,n]`.
pub fn matmul_naive(a: &[f32], b: &[f32], m: usize, k: usize, n: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; m * n];
    for i in 0..m {
        for j in 0..n {
            let mut sum = 0.0f32;
            for t in 0..k {
                sum += a[i * k + t] * b[t * n + j];
            }
            out[i * n + j] = sum;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tensor_f32_ok() {
        let (s, d) = tensor_f32(&[2, 2], &[1.0, 2.0, 3.0, 4.0]).unwrap();
        assert_eq!(s, vec![2, 2]);
        assert_eq!(d.len(), 4);
    }

    #[test]
    fn batch_from_rows_ok() {
        let (s, d) = batch_from_rows(&[vec![1.0, 2.0], vec![3.0, 4.0]]).unwrap();
        assert_eq!(s, vec![2, 2]);
        assert_eq!(d, vec![1.0, 2.0, 3.0, 4.0]);
    }
}
