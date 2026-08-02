//! Logistic regression (binary + multinomial) via L-BFGS / IRLS.

use crate::error::{LearnError, LearnResult};
use crate::metrics::accuracy;
use crate::traits::{Estimator, Predictor, Scorer};
use crate::utils::{
    check_2d, check_xy, label_index, matrix_from, sigmoid, softmax_inplace, unique_labels,
    vector_from, y_as_vec,
};
use niao_num::NdArray;
use niao_optim::{minimize, MinimizeMethod, MinimizeOptions};

#[derive(Clone, Debug)]
#[allow(non_snake_case)]
pub struct LogisticRegression {
    pub max_iter: usize,
    pub tol: f64,
    pub fit_intercept: bool,
    /// Inverse regularization strength (sklearn `C`).
    pub C: f64,
    pub classes: Option<Vec<f64>>,
    /// Binary: length d; multinomial: n_classes * d (row-major).
    pub coef: Option<Vec<f64>>,
    pub intercept: Option<Vec<f64>>,
}

impl Default for LogisticRegression {
    fn default() -> Self {
        Self {
            max_iter: 200,
            tol: 1e-6,
            fit_intercept: true,
            C: 1e12,
            classes: None,
            coef: None,
            intercept: None,
        }
    }
}

impl LogisticRegression {
    pub fn new() -> Self {
        Self::default()
    }

    fn n_features(&self) -> usize {
        let classes = self.classes.as_ref().unwrap();
        let coef = self.coef.as_ref().unwrap();
        if classes.len() <= 2 {
            coef.len()
        } else {
            coef.len() / classes.len()
        }
    }
}

fn fit_binary(
    x: &[f64],
    y01: &[f64],
    n: usize,
    d: usize,
    fit_intercept: bool,
    c_reg: f64,
    max_iter: usize,
) -> LearnResult<(Vec<f64>, f64)> {
    let dim = d + if fit_intercept { 1 } else { 0 };
    let x0 = vec![0.0; dim];
    let lam = 1.0 / c_reg;

    let fun = |w: &[f64], _g: &mut [f64]| -> f64 {
        let mut loss = 0.0;
        for i in 0..n {
            let mut z = if fit_intercept { w[0] } else { 0.0 };
            let off = if fit_intercept { 1 } else { 0 };
            for j in 0..d {
                z += x[i * d + j] * w[off + j];
            }
            let p = sigmoid(z);
            let yi = y01[i];
            loss -= yi * p.ln().max(-1e12) + (1.0 - yi) * (1.0 - p).ln().max(-1e12);
        }
        loss /= n as f64;
        for j in off_range(fit_intercept, dim) {
            loss += 0.5 * lam * w[j] * w[j];
        }
        loss
    };

    let jac = |w: &[f64], g: &mut [f64]| {
        for v in g.iter_mut() {
            *v = 0.0;
        }
        for i in 0..n {
            let mut z = if fit_intercept { w[0] } else { 0.0 };
            let off = if fit_intercept { 1 } else { 0 };
            for j in 0..d {
                z += x[i * d + j] * w[off + j];
            }
            let p = sigmoid(z);
            let err = p - y01[i];
            if fit_intercept {
                g[0] += err;
            }
            for j in 0..d {
                g[off + j] += err * x[i * d + j];
            }
        }
        for v in g.iter_mut() {
            *v /= n as f64;
        }
        for j in off_range(fit_intercept, dim) {
            g[j] += lam * w[j];
        }
    };

    let res = minimize(
        fun,
        &x0,
        MinimizeMethod::LBfgs,
        Some(jac),
        MinimizeOptions {
            max_iter,
            gtol: 1e-6,
            ftol: 1e-12,
            ..MinimizeOptions::default()
        },
    );
    if !res.success && res.nit < 2 {
        return Err(LearnError::NonConvergence(res.message));
    }
    let w = res.x;
    if fit_intercept {
        Ok((w[1..].to_vec(), w[0]))
    } else {
        Ok((w, 0.0))
    }
}

fn off_range(fit_intercept: bool, dim: usize) -> std::ops::Range<usize> {
    if fit_intercept {
        1..dim
    } else {
        0..dim
    }
}

fn fit_multinomial(
    x: &[f64],
    y_idx: &[usize],
    n: usize,
    d: usize,
    k: usize,
    fit_intercept: bool,
    c_reg: f64,
    max_iter: usize,
) -> LearnResult<(Vec<f64>, Vec<f64>)> {
    // parameters: k * (d + intercept?)
    let per = d + if fit_intercept { 1 } else { 0 };
    let dim = k * per;
    let x0 = vec![0.0; dim];
    let lam = 1.0 / c_reg;

    let fun = |w: &[f64], _g: &mut [f64]| -> f64 {
        let mut loss = 0.0;
        let mut logits = vec![0.0; k];
        for i in 0..n {
            for c in 0..k {
                let base = c * per;
                let mut z = if fit_intercept { w[base] } else { 0.0 };
                let off = if fit_intercept { 1 } else { 0 };
                for j in 0..d {
                    z += x[i * d + j] * w[base + off + j];
                }
                logits[c] = z;
            }
            softmax_inplace(&mut logits);
            loss -= logits[y_idx[i]].ln().max(-1e12);
        }
        loss /= n as f64;
        for c in 0..k {
            let base = c * per;
            for j in off_range(fit_intercept, per) {
                loss += 0.5 * lam * w[base + j] * w[base + j];
            }
        }
        loss
    };

    let jac = |w: &[f64], g: &mut [f64]| {
        for v in g.iter_mut() {
            *v = 0.0;
        }
        let mut logits = vec![0.0; k];
        for i in 0..n {
            for c in 0..k {
                let base = c * per;
                let mut z = if fit_intercept { w[base] } else { 0.0 };
                let off = if fit_intercept { 1 } else { 0 };
                for j in 0..d {
                    z += x[i * d + j] * w[base + off + j];
                }
                logits[c] = z;
            }
            softmax_inplace(&mut logits);
            for c in 0..k {
                let err = logits[c] - if c == y_idx[i] { 1.0 } else { 0.0 };
                let base = c * per;
                let off = if fit_intercept { 1 } else { 0 };
                if fit_intercept {
                    g[base] += err;
                }
                for j in 0..d {
                    g[base + off + j] += err * x[i * d + j];
                }
            }
        }
        for v in g.iter_mut() {
            *v /= n as f64;
        }
        for c in 0..k {
            let base = c * per;
            for j in off_range(fit_intercept, per) {
                g[base + j] += lam * w[base + j];
            }
        }
    };

    let res = minimize(
        fun,
        &x0,
        MinimizeMethod::LBfgs,
        Some(jac),
        MinimizeOptions {
            max_iter,
            gtol: 1e-5,
            ftol: 1e-12,
            ..MinimizeOptions::default()
        },
    );
    if !res.success && res.nit < 2 {
        return Err(LearnError::NonConvergence(res.message));
    }
    let w = res.x;
    let mut coef = vec![0.0; k * d];
    let mut intercept = vec![0.0; k];
    for c in 0..k {
        let base = c * per;
        if fit_intercept {
            intercept[c] = w[base];
            for j in 0..d {
                coef[c * d + j] = w[base + 1 + j];
            }
        } else {
            for j in 0..d {
                coef[c * d + j] = w[base + j];
            }
        }
    }
    Ok((coef, intercept))
}

impl Estimator for LogisticRegression {
    fn fit(&mut self, x: &NdArray, y: Option<&NdArray>) -> LearnResult<()> {
        let y = y.ok_or_else(|| LearnError::Error("y required".into()))?;
        let (n, d) = check_xy(x, y)?;
        let xv = x.to_vec();
        let yv = y_as_vec(y)?;
        let classes = unique_labels(&yv);
        if classes.len() < 2 {
            return Err(LearnError::Error("need at least 2 classes".into()));
        }
        if classes.len() == 2 {
            let y01: Vec<f64> = yv
                .iter()
                .map(|&v| {
                    if (v - classes[1]).abs() < 1e-12 {
                        1.0
                    } else {
                        0.0
                    }
                })
                .collect();
            let (coef, intercept) =
                fit_binary(&xv, &y01, n, d, self.fit_intercept, self.C, self.max_iter)?;
            self.classes = Some(classes);
            self.coef = Some(coef);
            self.intercept = Some(vec![intercept]);
        } else {
            let y_idx: Vec<usize> = yv
                .iter()
                .map(|&v| label_index(&classes, v).unwrap())
                .collect();
            let k = classes.len();
            let (coef, intercept) = fit_multinomial(
                &xv,
                &y_idx,
                n,
                d,
                k,
                self.fit_intercept,
                self.C,
                self.max_iter,
            )?;
            self.classes = Some(classes);
            self.coef = Some(coef);
            self.intercept = Some(intercept);
        }
        Ok(())
    }
}

impl Predictor for LogisticRegression {
    fn predict(&self, x: &NdArray) -> LearnResult<NdArray> {
        let proba = self.predict_proba(x)?;
        let classes = self.classes.as_ref().unwrap();
        let (n, k) = (proba.shape[0], proba.shape[1]);
        let pv = proba.to_vec();
        let mut out = vec![0.0; n];
        for i in 0..n {
            let mut best = 0usize;
            let mut best_p = f64::NEG_INFINITY;
            for c in 0..k {
                let p = pv[i * k + c];
                if p > best_p {
                    best_p = p;
                    best = c;
                }
            }
            out[i] = classes[best];
        }
        vector_from(out)
    }

    fn predict_proba(&self, x: &NdArray) -> LearnResult<NdArray> {
        let classes = self
            .classes
            .as_ref()
            .ok_or_else(|| LearnError::NotFitted("LogisticRegression not fitted".into()))?;
        let coef = self.coef.as_ref().unwrap();
        let intercept = self.intercept.as_ref().unwrap();
        let (n, d) = check_2d(x, "X")?;
        let xv = x.to_vec();
        if classes.len() == 2 {
            let mut out = vec![0.0; n * 2];
            for i in 0..n {
                let mut z = intercept[0];
                for j in 0..d {
                    z += xv[i * d + j] * coef[j];
                }
                let p1 = sigmoid(z);
                out[i * 2] = 1.0 - p1;
                out[i * 2 + 1] = p1;
            }
            matrix_from((n, 2), out)
        } else {
            let k = classes.len();
            let mut out = vec![0.0; n * k];
            for i in 0..n {
                for c in 0..k {
                    let mut z = intercept[c];
                    for j in 0..d {
                        z += xv[i * d + j] * coef[c * d + j];
                    }
                    out[i * k + c] = z;
                }
                softmax_inplace(&mut out[i * k..(i + 1) * k]);
            }
            matrix_from((n, k), out)
        }
    }
}

impl Scorer for LogisticRegression {
    fn score(&self, x: &NdArray, y: &NdArray) -> LearnResult<f64> {
        accuracy(y, &self.predict(x)?)
    }
}
