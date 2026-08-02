//! nnum — numpy + scipy.linalg + scipy.fft for Niao.

pub mod array;
pub mod creation;
pub mod elementwise;
pub mod error;
pub mod fft;
pub mod linalg;
pub mod reductions;

pub use array::NdArray;
pub use creation::{arange, eye, from_slice, full, linspace, rand, randn};
pub use elementwise::{
    abs, add, clip, cos, div, exp, log, maximum, minimum, mul, pow, sin, sqrt, sub, tan,
    where_array,
};
pub use error::{NumError, NumResult};
pub use fft::{fft, fft2, ifft, rfft, Complex};
pub use linalg::{
    cholesky, det, dot, eig_symmetric, inv, lstsq, matmul, norm, pinv, qr, rank, solve, svd, trace,
    EigResult, NormKind, SvdResult,
};
pub use reductions::{argmax, argmin, cumsum, max, mean, min, prod, std, sum, var};

pub fn zeros_arr(shape: &[usize]) -> NumResult<NdArray> {
    NdArray::zeros(shape)
}

pub fn ones_arr(shape: &[usize]) -> NumResult<NdArray> {
    NdArray::ones(shape)
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn elementwise_broadcast() {
        let a = from_slice(&[2, 3], &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]).unwrap();
        let b = from_slice(&[3], &[10.0, 20.0, 30.0]).unwrap();
        let c = add(&a, &b).unwrap();
        assert_eq!(c.shape, vec![2, 3]);
        assert!((c.index(&[0, 0]).unwrap() - 11.0).abs() < 1e-12);
        assert!((c.index(&[1, 2]).unwrap() - 36.0).abs() < 1e-12);
    }

    #[test]
    fn reductions_sum_mean() {
        let a = from_slice(&[2, 2], &[1.0, 2.0, 3.0, 4.0]).unwrap();
        let s = sum(&a, None).unwrap();
        assert!((s.index(&[0]).unwrap() - 10.0).abs() < 1e-12);
        let m = mean(&a, Some(0)).unwrap();
        assert!((m.index(&[0]).unwrap() - 2.0).abs() < 1e-12);
        assert!((m.index(&[1]).unwrap() - 3.0).abs() < 1e-12);
    }
}
