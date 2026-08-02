//! Linear algebra: decompositions, solves, norms.

use crate::array::NdArray;
use crate::error::{NumError, NumResult};
use niao_tensor::Tensor;

pub fn matmul(a: &NdArray, b: &NdArray) -> NumResult<NdArray> {
    if a.ndim() != 2 || b.ndim() != 2 {
        return Err(NumError::ShapeMismatch("matmul requires 2-D arrays".into()));
    }
    let (m, n) = (a.shape[0], a.shape[1]);
    let (n2, k) = (b.shape[0], b.shape[1]);
    if n != n2 {
        return Err(NumError::ShapeMismatch(format!(
            "matmul shape mismatch: ({m},{n}) x ({n2},{k})"
        )));
    }
    let av = a.to_vec();
    let bv = b.to_vec();
    let mut out = vec![0.0; m * k];
    for i in 0..m {
        for j in 0..k {
            let mut sum = 0.0;
            for t in 0..n {
                sum += av[i * n + t] * bv[t * k + j];
            }
            out[i * k + j] = sum;
        }
    }
    NdArray::from_vec(vec![m, k], out)
}

/// f32 matmul via niao_tensor GEMM (for large workloads / benchmarks).
pub fn matmul_tensor(a: &NdArray, b: &NdArray) -> NumResult<NdArray> {
    if a.ndim() != 2 || b.ndim() != 2 {
        return Err(NumError::ShapeMismatch("matmul requires 2-D arrays".into()));
    }
    let (m, n) = (a.shape[0], a.shape[1]);
    let (n2, k) = (b.shape[0], b.shape[1]);
    if n != n2 {
        return Err(NumError::ShapeMismatch(format!(
            "matmul shape mismatch: ({m},{n}) x ({n2},{k})"
        )));
    }
    let af: Vec<f32> = a.to_vec().iter().map(|&x| x as f32).collect();
    let bf: Vec<f32> = b.to_vec().iter().map(|&x| x as f32).collect();
    let ta = Tensor::from_float_slice(&[m, n], &af).map_err(|e| NumError::Error(e.to_string()))?;
    let tb = Tensor::from_float_slice(&[n, k], &bf).map_err(|e| NumError::Error(e.to_string()))?;
    let tc = ta.matmul(&tb).map_err(|e| NumError::Error(e.to_string()))?;
    let out: Vec<f64> = tc
        .to_cpu()
        .map_err(|e| NumError::Error(e.to_string()))?
        .iter()
        .map(|&x| x as f64)
        .collect();
    NdArray::from_vec(vec![m, k], out)
}

pub fn dot(a: &NdArray, b: &NdArray) -> NumResult<f64> {
    let av = a.to_vec();
    let bv = b.to_vec();
    if av.len() != bv.len() {
        return Err(NumError::ShapeMismatch("dot length mismatch".into()));
    }
    Ok(av.iter().zip(bv.iter()).map(|(&x, &y)| x * y).sum())
}

pub fn trace(a: &NdArray) -> NumResult<f64> {
    if a.ndim() != 2 || a.shape[0] != a.shape[1] {
        return Err(NumError::ShapeMismatch(
            "trace requires square 2-D array".into(),
        ));
    }
    let n = a.shape[0];
    let mut t = 0.0;
    for i in 0..n {
        t += a.index(&[i, i])?;
    }
    Ok(t)
}

pub fn norm(a: &NdArray, kind: NormKind) -> NumResult<f64> {
    let v = a.to_vec();
    Ok(match kind {
        NormKind::L1 => v.iter().map(|x| x.abs()).sum(),
        NormKind::L2 => v.iter().map(|x| x * x).sum::<f64>().sqrt(),
        NormKind::Inf => v.iter().map(|x| x.abs()).fold(0.0, f64::max),
        NormKind::Fro => {
            if a.ndim() == 2 {
                v.iter().map(|x| x * x).sum::<f64>().sqrt()
            } else {
                return Err(NumError::ShapeMismatch("fro norm requires 2-D".into()));
            }
        }
    })
}

#[derive(Debug, Clone, Copy)]
pub enum NormKind {
    L1,
    L2,
    Inf,
    Fro,
}

pub fn det(a: &NdArray) -> NumResult<f64> {
    let n = square_n(a)?;
    let (lu, piv, sign) = lu_decomp(&a.to_vec(), n)?;
    let mut d = sign as f64;
    for i in 0..n {
        d *= lu[i * n + i];
    }
    Ok(d)
}

pub fn solve(a: &NdArray, b: &NdArray) -> NumResult<NdArray> {
    let n = square_n(a)?;
    if b.ndim() != 2 || b.shape[0] != n {
        return Err(NumError::ShapeMismatch("solve shape mismatch".into()));
    }
    let nrhs = b.shape[1];
    let (lu, piv, _) = lu_decomp(&a.to_vec(), n)?;
    let mut x = b.to_vec();
    lu_solve(&lu, &piv, n, nrhs, &mut x)?;
    NdArray::from_vec(vec![n, nrhs], x)
}

pub fn inv(a: &NdArray) -> NumResult<NdArray> {
    let n = square_n(a)?;
    let eye = super::creation::eye(n)?;
    solve(a, &eye)
}

pub fn lstsq(a: &NdArray, b: &NdArray) -> NumResult<NdArray> {
    if a.ndim() != 2 || b.ndim() != 1 && b.ndim() != 2 {
        return Err(NumError::ShapeMismatch("lstsq shape mismatch".into()));
    }
    let (m, n) = (a.shape[0], a.shape[1]);
    let ata = matmul(&a.transpose()?, a)?;
    let atb = matmul(&a.transpose()?, b)?;
    solve(&ata, &atb)
}

pub fn qr(a: &NdArray) -> NumResult<(NdArray, NdArray)> {
    if a.ndim() != 2 {
        return Err(NumError::ShapeMismatch("qr requires 2-D array".into()));
    }
    let m = a.shape[0];
    let n = a.shape[1];
    let k = m.min(n);
    let mut q_cols: Vec<Vec<f64>> = Vec::with_capacity(k);
    let mut r = vec![0.0; k * n];

    for j in 0..n {
        let mut col: Vec<f64> = (0..m).map(|i| a.index(&[i, j]).unwrap()).collect();
        for (i, qi) in q_cols.iter().enumerate() {
            let dot: f64 = col.iter().zip(qi.iter()).map(|(x, y)| x * y).sum();
            if j < k {
                r[i * n + j] = dot;
            }
            for t in 0..m {
                col[t] -= dot * qi[t];
            }
        }
        if j < k {
            let norm: f64 = col.iter().map(|x| x * x).sum::<f64>().sqrt();
            if norm < 1e-15 {
                return Err(NumError::Singular("rank deficient matrix".into()));
            }
            r[j * n + j] = norm;
            for t in 0..m {
                col[t] /= norm;
            }
            q_cols.push(col);
        }
    }

    let mut q_data = vec![0.0; m * k];
    for (j, col) in q_cols.iter().enumerate() {
        for i in 0..m {
            q_data[i * k + j] = col[i];
        }
    }
    let q_arr = NdArray::from_vec(vec![m, k], q_data)?;
    let r_arr = NdArray::from_vec(vec![k, n], r)?;
    Ok((q_arr, r_arr))
}

pub fn cholesky(a: &NdArray) -> NumResult<NdArray> {
    let n = square_n(a)?;
    let mut l = vec![0.0; n * n];
    let av = a.to_vec();
    for i in 0..n {
        for j in 0..=i {
            let mut sum = av[i * n + j];
            for k in 0..j {
                sum -= l[i * n + k] * l[j * n + k];
            }
            if i == j {
                if sum <= 0.0 {
                    return Err(NumError::Singular("matrix is not positive definite".into()));
                }
                l[i * n + j] = sum.sqrt();
            } else {
                l[i * n + j] = sum / l[j * n + j];
            }
        }
    }
    NdArray::from_vec(vec![n, n], l)
}

pub struct SvdResult {
    pub u: NdArray,
    pub s: NdArray,
    pub vt: NdArray,
}

pub fn svd(a: &NdArray) -> NumResult<SvdResult> {
    if a.ndim() != 2 {
        return Err(NumError::ShapeMismatch("svd requires 2-D array".into()));
    }
    let m = a.shape[0];
    let n = a.shape[1];
    let mut u = vec![0.0; m * m];
    let mut v = vec![0.0; n * n];
    for i in 0..m {
        u[i * m + i] = 1.0;
    }
    for i in 0..n {
        v[i * n + i] = 1.0;
    }
    let mut work = a.to_vec();
    one_sided_jacobi_svd(&mut work, m, n, &mut u, &mut v)?;
    let mut s = vec![0.0; m.min(n)];
    for i in 0..s.len() {
        s[i] = work[i * n + i].abs();
    }
    Ok(SvdResult {
        u: NdArray::from_vec(vec![m, m], u)?,
        s: NdArray::from_vec(vec![s.len()], s)?,
        vt: NdArray::from_vec(vec![n, n], v)?,
    })
}

pub struct EigResult {
    pub values: NdArray,
    pub vectors: NdArray,
}

pub fn eig_symmetric(a: &NdArray) -> NumResult<EigResult> {
    let n = square_n(a)?;
    let av = a.to_vec();
    if !is_symmetric(&av, n, 1e-10) {
        return Err(NumError::Type(
            "eig in v1 requires a symmetric matrix".into(),
        ));
    }
    let mut v = vec![0.0; n * n];
    for i in 0..n {
        v[i * n + i] = 1.0;
    }
    let mut diag = av.clone();
    cyclic_jacobi(&mut diag, &mut v, n)?;
    let mut values = vec![0.0; n];
    for i in 0..n {
        values[i] = diag[i * n + i];
    }
    Ok(EigResult {
        values: NdArray::from_vec(vec![n], values)?,
        vectors: NdArray::from_vec(vec![n, n], v)?,
    })
}

pub fn pinv(a: &NdArray) -> NumResult<NdArray> {
    let svd = svd(a)?;
    let s = svd.s.to_vec();
    let mut s_inv = vec![0.0; s.len()];
    let eps = 1e-12 * s.iter().copied().fold(0.0f64, f64::max);
    for (i, &si) in s.iter().enumerate() {
        if si > eps {
            s_inv[i] = 1.0 / si;
        }
    }
    let rows = svd.vt.shape[0];
    let cols = svd.u.shape[0];
    let mut s_data = vec![0.0; rows * cols];
    let n = s.len();
    for i in 0..n {
        s_data[i * cols + i] = s_inv[i];
    }
    let s_mat = NdArray::from_vec(vec![rows, cols], s_data)?;
    let step1 = matmul(&svd.vt.transpose()?, &s_mat.transpose()?)?;
    matmul(&step1, &svd.u.transpose()?)
}

pub fn rank(a: &NdArray, tol: f64) -> NumResult<usize> {
    let s = svd(a)?.s.to_vec();
    Ok(s.iter().filter(|&&x| x > tol).count())
}

fn square_n(a: &NdArray) -> NumResult<usize> {
    if a.ndim() != 2 || a.shape[0] != a.shape[1] {
        return Err(NumError::ShapeMismatch(
            "operation requires square 2-D array".into(),
        ));
    }
    Ok(a.shape[0])
}

fn lu_decomp(a: &[f64], n: usize) -> NumResult<(Vec<f64>, Vec<usize>, i32)> {
    let mut lu = a.to_vec();
    let mut piv: Vec<usize> = (0..n).collect();
    let mut sign = 1i32;
    for k in 0..n {
        let mut piv_row = k;
        let mut max_val = lu[k * n + k].abs();
        for i in k + 1..n {
            let v = lu[i * n + k].abs();
            if v > max_val {
                max_val = v;
                piv_row = i;
            }
        }
        if max_val < 1e-15 {
            return Err(NumError::Singular("matrix is singular".into()));
        }
        if piv_row != k {
            for j in 0..n {
                lu.swap(k * n + j, piv_row * n + j);
            }
            piv.swap(k, piv_row);
            sign = -sign;
        }
        for i in k + 1..n {
            let factor = lu[i * n + k] / lu[k * n + k];
            lu[i * n + k] = factor;
            for j in k + 1..n {
                lu[i * n + j] -= factor * lu[k * n + j];
            }
        }
    }
    Ok((lu, piv, sign))
}

fn lu_solve(lu: &[f64], piv: &[usize], n: usize, nrhs: usize, b: &mut [f64]) -> NumResult<()> {
    for rhs in 0..nrhs {
        for i in 0..n {
            if piv[i] != i {
                b.swap(i * nrhs + rhs, piv[i] * nrhs + rhs);
            }
        }
        for i in 0..n {
            for j in 0..i {
                b[i * nrhs + rhs] -= lu[i * n + j] * b[j * nrhs + rhs];
            }
        }
        for i in (0..n).rev() {
            for j in i + 1..n {
                b[i * nrhs + rhs] -= lu[i * n + j] * b[j * nrhs + rhs];
            }
            b[i * nrhs + rhs] /= lu[i * n + i];
        }
    }
    Ok(())
}

fn is_symmetric(a: &[f64], n: usize, tol: f64) -> bool {
    for i in 0..n {
        for j in i + 1..n {
            if (a[i * n + j] - a[j * n + i]).abs() > tol {
                return false;
            }
        }
    }
    true
}

fn cyclic_jacobi(a: &mut [f64], v: &mut [f64], n: usize) -> NumResult<()> {
    const MAX_SWEEPS: usize = 50;
    const TOL: f64 = 1e-12;
    for _ in 0..MAX_SWEEPS {
        let mut off = 0.0;
        for i in 0..n {
            for j in i + 1..n {
                off += a[i * n + j] * a[i * n + j];
            }
        }
        if off < TOL {
            return Ok(());
        }
        for p in 0..n {
            for q in p + 1..n {
                let apq = a[p * n + q];
                if apq.abs() < 1e-15 {
                    continue;
                }
                let app = a[p * n + p];
                let aqq = a[q * n + q];
                let tau = (aqq - app) / (2.0 * apq);
                let t = if tau >= 0.0 {
                    1.0 / (tau + (1.0 + tau * tau).sqrt())
                } else {
                    -1.0 / (-tau + (1.0 + tau * tau).sqrt())
                };
                let c = 1.0 / (1.0 + t * t).sqrt();
                let s = t * c;
                jacobi_rotate(a, v, n, p, q, c, s);
            }
        }
    }
    Err(NumError::NonConvergence(
        "symmetric eig did not converge".into(),
    ))
}

fn jacobi_rotate(a: &mut [f64], v: &mut [f64], n: usize, p: usize, q: usize, c: f64, s: f64) {
    let app = a[p * n + p];
    let aqq = a[q * n + q];
    let apq = a[p * n + q];
    a[p * n + p] = c * c * app - 2.0 * s * c * apq + s * s * aqq;
    a[q * n + q] = s * s * app + 2.0 * s * c * apq + c * c * aqq;
    a[p * n + q] = 0.0;
    a[q * n + p] = 0.0;
    for k in 0..n {
        if k == p || k == q {
            continue;
        }
        let akp = a[k * n + p];
        let akq = a[k * n + q];
        a[k * n + p] = c * akp - s * akq;
        a[p * n + k] = a[k * n + p];
        a[k * n + q] = s * akp + c * akq;
        a[q * n + k] = a[k * n + q];
    }
    for k in 0..n {
        let vkp = v[k * n + p];
        let vkq = v[k * n + q];
        v[k * n + p] = c * vkp - s * vkq;
        v[k * n + q] = s * vkp + c * vkq;
    }
}

fn one_sided_jacobi_svd(
    a: &mut [f64],
    m: usize,
    n: usize,
    u: &mut [f64],
    v: &mut [f64],
) -> NumResult<()> {
    const MAX_SWEEPS: usize = 60;
    const TOL: f64 = 1e-12;
    let p = m.min(n);
    for _ in 0..MAX_SWEEPS {
        let mut converged = true;
        for i in 0..p {
            for j in i + 1..p {
                let mut alpha = 0.0;
                let mut beta = 0.0;
                let mut gamma = 0.0;
                for k in 0..n {
                    let aik = a[i * n + k];
                    let ajk = a[j * n + k];
                    alpha += aik * aik;
                    beta += ajk * ajk;
                    gamma += aik * ajk;
                }
                if gamma.abs() < TOL * (alpha * beta).sqrt() {
                    continue;
                }
                converged = false;
                let zeta = (beta - alpha) / (2.0 * gamma);
                let t = if zeta >= 0.0 {
                    1.0 / (zeta + (1.0 + zeta * zeta).sqrt())
                } else {
                    -1.0 / (-zeta + (1.0 + zeta * zeta).sqrt())
                };
                let c = 1.0 / (1.0 + t * t).sqrt();
                let s = t * c;
                for k in 0..n {
                    let aik = a[i * n + k];
                    let ajk = a[j * n + k];
                    a[i * n + k] = c * aik - s * ajk;
                    a[j * n + k] = s * aik + c * ajk;
                }
                for k in 0..m {
                    let uki = u[k * m + i];
                    let ukj = u[k * m + j];
                    u[k * m + i] = c * uki - s * ukj;
                    u[k * m + j] = s * uki + c * ukj;
                }
            }
        }
        if converged {
            return Ok(());
        }
    }
    Err(NumError::NonConvergence("svd did not converge".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::creation::from_slice;

    fn approx_eq(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() < tol
    }

    #[test]
    fn det_inv_fixture() {
        let a = from_slice(&[2, 2], &[4.0, 7.0, 2.0, 6.0]).unwrap();
        let d = det(&a).unwrap();
        assert!(approx_eq(d, 10.0, 1e-10));
        let inv_a = inv(&a).unwrap();
        let expected = from_slice(&[2, 2], &[0.6, -0.7, -0.2, 0.4]).unwrap();
        for i in 0..2 {
            for j in 0..2 {
                assert!(approx_eq(
                    inv_a.index(&[i, j]).unwrap(),
                    expected.index(&[i, j]).unwrap(),
                    1e-10
                ));
            }
        }
    }

    #[test]
    fn solve_residual() {
        let a = from_slice(&[2, 2], &[3.0, 1.0, 1.0, 2.0]).unwrap();
        let b = from_slice(&[2, 1], &[9.0, 8.0]).unwrap();
        let x = solve(&a, &b).unwrap();
        let ax = matmul(&a, &x).unwrap();
        let bv = b.to_vec();
        let axv = ax.to_vec();
        let res: f64 = bv
            .iter()
            .zip(axv.iter())
            .map(|(&bi, &axi)| (bi - axi).powi(2))
            .sum::<f64>()
            .sqrt();
        assert!(res < 1e-10);
    }

    #[test]
    fn symmetric_eig_fixture() {
        let a = from_slice(&[3, 3], &[4.0, 1.0, 2.0, 1.0, 3.0, 0.0, 2.0, 0.0, 2.0]).unwrap();
        let e = eig_symmetric(&a).unwrap();
        let mut vals = e.values.to_vec();
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        // numpy.linalg.eigvalsh reference
        assert!(approx_eq(vals[0], 0.63853123, 1e-6));
        assert!(approx_eq(vals[1], 2.83255081, 1e-6));
        assert!(approx_eq(vals[2], 5.52891796, 1e-6));
    }

    #[test]
    fn qr_reconstruct() {
        let a = from_slice(&[3, 2], &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]).unwrap();
        let (q, r) = qr(&a).unwrap();
        let qr = matmul(&q, &r).unwrap();
        let av = a.to_vec();
        let qrv = qr.to_vec();
        for (&x, &y) in av.iter().zip(qrv.iter()) {
            assert!((x - y).abs() < 1e-10);
        }
    }
}
