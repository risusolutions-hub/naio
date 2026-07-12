//! Vector helpers and finite-difference derivatives.

use crate::error::{OptimError, OptimResult};

pub const EPS: f64 = f64::EPSILON;

#[inline]
pub fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b.iter()).map(|(&x, &y)| x * y).sum()
}

#[inline]
pub fn norm2(v: &[f64]) -> f64 {
    dot(v, v).sqrt()
}

#[inline]
pub fn norm_inf(v: &[f64]) -> f64 {
    v.iter().map(|x| x.abs()).fold(0.0, f64::max)
}

#[inline]
pub fn axpy(a: f64, x: &[f64], y: &mut [f64]) {
    for (yi, &xi) in y.iter_mut().zip(x.iter()) {
        *yi += a * xi;
    }
}

#[inline]
pub fn scale(a: f64, x: &mut [f64]) {
    for xi in x.iter_mut() {
        *xi *= a;
    }
}

#[inline]
pub fn copy_from(src: &[f64], dst: &mut [f64]) {
    dst.copy_from_slice(src);
}

#[inline]
pub fn fd_step(xi: f64) -> f64 {
    EPS.sqrt() * xi.abs().max(1.0)
}

pub fn approx_fprime<F>(mut f: F, x: &[f64], grad: &mut [f64]) -> usize
where
    F: FnMut(&[f64], &mut [f64]) -> f64,
{
    let n = x.len();
    let mut x_lo = x.to_vec();
    let mut x_hi = x.to_vec();
    let mut nfev = 0usize;
    for i in 0..n {
        let h = fd_step(x[i]);
        x_lo[i] = x[i] - h;
        x_hi[i] = x[i] + h;
        let flo = f(&x_lo, grad);
        nfev += 1;
        let fhi = f(&x_hi, grad);
        nfev += 1;
        grad[i] = (fhi - flo) / (2.0 * h);
        x_lo[i] = x[i];
        x_hi[i] = x[i];
    }
    nfev
}

pub fn approx_jacobian<F>(mut f: F, x: &[f64], m: usize, jac: &mut [f64]) -> usize
where
    F: FnMut(&[f64], &mut [f64]),
{
    let n = x.len();
    let mut x_lo = x.to_vec();
    let mut x_hi = x.to_vec();
    let mut f_lo = vec![0.0; m];
    let mut f_hi = vec![0.0; m];
    let mut nfev = 0usize;
    for j in 0..n {
        let h = fd_step(x[j]);
        x_lo[j] = x[j] - h;
        x_hi[j] = x[j] + h;
        f(&x_lo, &mut f_lo);
        nfev += 1;
        f(&x_hi, &mut f_hi);
        nfev += 1;
        for i in 0..m {
            jac[i * n + j] = (f_hi[i] - f_lo[i]) / (2.0 * h);
        }
        x_lo[j] = x[j];
        x_hi[j] = x[j];
    }
    nfev
}

pub fn project_bounds(x: &mut [f64], lo: &[f64], hi: &[f64]) {
    for i in 0..x.len() {
        if x[i] < lo[i] {
            x[i] = lo[i];
        } else if x[i] > hi[i] {
            x[i] = hi[i];
        }
    }
}

pub fn validate_bounds(lo: &[f64], hi: &[f64]) -> OptimResult<()> {
    if lo.len() != hi.len() {
        return Err(OptimError::Error("bounds length mismatch".into()));
    }
    for i in 0..lo.len() {
        if lo[i] > hi[i] {
            return Err(OptimError::BadBounds(format!(
                "lower bound {lo} > upper bound {hi} at index {i}",
                lo = lo[i],
                hi = hi[i]
            )));
        }
    }
    Ok(())
}

pub fn mat_vec(m: usize, n: usize, a: &[f64], x: &[f64], out: &mut [f64]) {
    for i in 0..m {
        let row = &a[i * n..(i + 1) * n];
        out[i] = dot(row, x);
    }
}

pub fn mat_t_vec(m: usize, n: usize, a: &[f64], x: &[f64], out: &mut [f64]) {
    out.fill(0.0);
    for i in 0..m {
        let row = &a[i * n..(i + 1) * n];
        for j in 0..n {
            out[j] += row[j] * x[i];
        }
    }
}

pub fn outer(n: usize, a: &[f64], b: &[f64], out: &mut [f64]) {
    for i in 0..n {
        for j in 0..n {
            out[i * n + j] += a[i] * b[j];
        }
    }
}
