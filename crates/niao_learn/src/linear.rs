//! Linear models: OLS, Ridge, Lasso, ElasticNet.

use crate::error::{LearnError, LearnResult};
use crate::metrics::r2_score;
use crate::traits::{Estimator, Predictor, Scorer};
use crate::utils::{check_2d, check_xy, design_with_intercept, matrix_from, vector_from, y_as_vec};
use niao_num::{lstsq, matmul, solve, NdArray};

#[derive(Clone, Debug, Default)]
pub struct LinearRegression {
    pub fit_intercept: bool,
    pub coef: Option<Vec<f64>>,
    pub intercept: f64,
}

impl LinearRegression {
    pub fn new(fit_intercept: bool) -> Self {
        Self {
            fit_intercept,
            ..Default::default()
        }
    }
}

impl Estimator for LinearRegression {
    fn fit(&mut self, x: &NdArray, y: Option<&NdArray>) -> LearnResult<()> {
        let y = y.ok_or_else(|| LearnError::Error("y required".into()))?;
        let (n, d) = check_xy(x, y)?;
        let xv = x.to_vec();
        let yv = y_as_vec(y)?;
        if self.fit_intercept {
            let design = design_with_intercept(&xv, n, d);
            let a = matrix_from((n, d + 1), design)?;
            let b = matrix_from((n, 1), yv)?;
            let beta = lstsq(&a, &b).map_err(|e| LearnError::Error(e.to_string()))?;
            let bv = beta.to_vec();
            self.intercept = bv[0];
            self.coef = Some(bv[1..].to_vec());
        } else {
            let b = matrix_from((n, 1), yv)?;
            let beta = lstsq(x, &b).map_err(|e| LearnError::Error(e.to_string()))?;
            self.intercept = 0.0;
            self.coef = Some(beta.to_vec());
        }
        Ok(())
    }
}

impl Predictor for LinearRegression {
    fn predict(&self, x: &NdArray) -> LearnResult<NdArray> {
        let coef = self
            .coef
            .as_ref()
            .ok_or_else(|| LearnError::NotFitted("LinearRegression not fitted".into()))?;
        let (n, d) = check_2d(x, "X")?;
        if d != coef.len() {
            return Err(LearnError::Shape("feature count mismatch".into()));
        }
        let xv = x.to_vec();
        let mut out = vec![0.0; n];
        for i in 0..n {
            let mut s = self.intercept;
            for j in 0..d {
                s += xv[i * d + j] * coef[j];
            }
            out[i] = s;
        }
        vector_from(out)
    }
}

impl Scorer for LinearRegression {
    fn score(&self, x: &NdArray, y: &NdArray) -> LearnResult<f64> {
        let pred = self.predict(x)?;
        r2_score(y, &pred)
    }
}

#[derive(Clone, Debug)]
pub struct Ridge {
    pub alpha: f64,
    pub fit_intercept: bool,
    pub coef: Option<Vec<f64>>,
    pub intercept: f64,
}

impl Default for Ridge {
    fn default() -> Self {
        Self {
            alpha: 1.0,
            fit_intercept: true,
            coef: None,
            intercept: 0.0,
        }
    }
}

impl Ridge {
    pub fn new(alpha: f64, fit_intercept: bool) -> Self {
        Self {
            alpha,
            fit_intercept,
            ..Default::default()
        }
    }
}

impl Estimator for Ridge {
    fn fit(&mut self, x: &NdArray, y: Option<&NdArray>) -> LearnResult<()> {
        let y = y.ok_or_else(|| LearnError::Error("y required".into()))?;
        let (n, d) = check_xy(x, y)?;
        let mut xv = x.to_vec();
        let mut yv = y_as_vec(y)?;
        let mut x_mean = vec![0.0; d];
        let mut y_mean = 0.0;
        if self.fit_intercept {
            for i in 0..n {
                y_mean += yv[i];
                for j in 0..d {
                    x_mean[j] += xv[i * d + j];
                }
            }
            y_mean /= n as f64;
            for j in 0..d {
                x_mean[j] /= n as f64;
            }
            for i in 0..n {
                yv[i] -= y_mean;
                for j in 0..d {
                    xv[i * d + j] -= x_mean[j];
                }
            }
        }
        // (X'X + α I) β = X'y
        let x_arr = matrix_from((n, d), xv)?;
        let xt = x_arr
            .transpose()
            .map_err(|e| LearnError::Error(e.to_string()))?;
        let mut xtx = matmul(&xt, &x_arr).map_err(|e| LearnError::Error(e.to_string()))?;
        let mut g = xtx.to_vec();
        for i in 0..d {
            g[i * d + i] += self.alpha;
        }
        xtx = matrix_from((d, d), g)?;
        let y_arr = vector_from(yv)?;
        // X'y as column
        let xty = matmul(
            &xt,
            &y_arr
                .reshape(vec![n, 1])
                .map_err(|e| LearnError::Error(e.to_string()))?,
        )
        .map_err(|e| LearnError::Error(e.to_string()))?;
        let beta = solve(&xtx, &xty).map_err(|e| LearnError::Error(e.to_string()))?;
        let coef = beta.to_vec();
        if self.fit_intercept {
            let mut intercept = y_mean;
            for j in 0..d {
                intercept -= x_mean[j] * coef[j];
            }
            self.intercept = intercept;
        } else {
            self.intercept = 0.0;
        }
        self.coef = Some(coef);
        Ok(())
    }
}

impl Predictor for Ridge {
    fn predict(&self, x: &NdArray) -> LearnResult<NdArray> {
        let coef = self
            .coef
            .as_ref()
            .ok_or_else(|| LearnError::NotFitted("Ridge not fitted".into()))?;
        let (n, d) = check_2d(x, "X")?;
        let xv = x.to_vec();
        let mut out = vec![0.0; n];
        for i in 0..n {
            let mut s = self.intercept;
            for j in 0..d {
                s += xv[i * d + j] * coef[j];
            }
            out[i] = s;
        }
        vector_from(out)
    }
}

impl Scorer for Ridge {
    fn score(&self, x: &NdArray, y: &NdArray) -> LearnResult<f64> {
        r2_score(y, &self.predict(x)?)
    }
}

#[derive(Clone, Debug)]
pub struct Lasso {
    pub alpha: f64,
    pub fit_intercept: bool,
    pub max_iter: usize,
    pub tol: f64,
    pub coef: Option<Vec<f64>>,
    pub intercept: f64,
}

impl Default for Lasso {
    fn default() -> Self {
        Self {
            alpha: 1.0,
            fit_intercept: true,
            max_iter: 1000,
            tol: 1e-4,
            coef: None,
            intercept: 0.0,
        }
    }
}

impl Lasso {
    pub fn new(alpha: f64) -> Self {
        Self {
            alpha,
            ..Default::default()
        }
    }
}

fn soft_threshold(z: f64, gamma: f64) -> f64 {
    if z > gamma {
        z - gamma
    } else if z < -gamma {
        z + gamma
    } else {
        0.0
    }
}

fn coordinate_descent(
    x: &[f64],
    y: &[f64],
    n: usize,
    d: usize,
    alpha: f64,
    l1_ratio: f64,
    max_iter: usize,
    tol: f64,
) -> LearnResult<Vec<f64>> {
    // Elastic-net style: alpha * l1_ratio * |β| + 0.5 * alpha * (1-l1_ratio) * ||β||²
    let mut coef = vec![0.0; d];
    let mut residual = y.to_vec();
    let l1 = alpha * l1_ratio;
    let l2 = alpha * (1.0 - l1_ratio);
    for _ in 0..max_iter {
        let mut max_delta = 0.0f64;
        for j in 0..d {
            // add back contribution
            for i in 0..n {
                residual[i] += x[i * d + j] * coef[j];
            }
            let mut rho = 0.0;
            let mut norm = 0.0;
            for i in 0..n {
                let xij = x[i * d + j];
                rho += xij * residual[i];
                norm += xij * xij;
            }
            let new_c = soft_threshold(rho, l1 * n as f64) / (norm + l2 * n as f64);
            let delta = (new_c - coef[j]).abs();
            if delta > max_delta {
                max_delta = delta;
            }
            coef[j] = new_c;
            for i in 0..n {
                residual[i] -= x[i * d + j] * coef[j];
            }
        }
        if max_delta < tol {
            return Ok(coef);
        }
    }
    Ok(coef) // return best-effort; sklearn also often converges late
}

impl Estimator for Lasso {
    fn fit(&mut self, x: &NdArray, y: Option<&NdArray>) -> LearnResult<()> {
        let y = y.ok_or_else(|| LearnError::Error("y required".into()))?;
        let (n, d) = check_xy(x, y)?;
        let mut xv = x.to_vec();
        let mut yv = y_as_vec(y)?;
        let mut x_mean = vec![0.0; d];
        let mut y_mean = 0.0;
        if self.fit_intercept {
            for i in 0..n {
                y_mean += yv[i];
                for j in 0..d {
                    x_mean[j] += xv[i * d + j];
                }
            }
            y_mean /= n as f64;
            for j in 0..d {
                x_mean[j] /= n as f64;
            }
            for i in 0..n {
                yv[i] -= y_mean;
                for j in 0..d {
                    xv[i * d + j] -= x_mean[j];
                }
            }
        }
        let coef = coordinate_descent(&xv, &yv, n, d, self.alpha, 1.0, self.max_iter, self.tol)?;
        if self.fit_intercept {
            let mut intercept = y_mean;
            for j in 0..d {
                intercept -= x_mean[j] * coef[j];
            }
            self.intercept = intercept;
        } else {
            self.intercept = 0.0;
        }
        self.coef = Some(coef);
        Ok(())
    }
}

impl Predictor for Lasso {
    fn predict(&self, x: &NdArray) -> LearnResult<NdArray> {
        let coef = self
            .coef
            .as_ref()
            .ok_or_else(|| LearnError::NotFitted("Lasso not fitted".into()))?;
        let (n, d) = check_2d(x, "X")?;
        let xv = x.to_vec();
        let mut out = vec![0.0; n];
        for i in 0..n {
            let mut s = self.intercept;
            for j in 0..d {
                s += xv[i * d + j] * coef[j];
            }
            out[i] = s;
        }
        vector_from(out)
    }
}

impl Scorer for Lasso {
    fn score(&self, x: &NdArray, y: &NdArray) -> LearnResult<f64> {
        r2_score(y, &self.predict(x)?)
    }
}

#[derive(Clone, Debug)]
pub struct ElasticNet {
    pub alpha: f64,
    pub l1_ratio: f64,
    pub fit_intercept: bool,
    pub max_iter: usize,
    pub tol: f64,
    pub coef: Option<Vec<f64>>,
    pub intercept: f64,
}

impl Default for ElasticNet {
    fn default() -> Self {
        Self {
            alpha: 1.0,
            l1_ratio: 0.5,
            fit_intercept: true,
            max_iter: 1000,
            tol: 1e-4,
            coef: None,
            intercept: 0.0,
        }
    }
}

impl ElasticNet {
    pub fn new(alpha: f64, l1_ratio: f64) -> Self {
        Self {
            alpha,
            l1_ratio,
            ..Default::default()
        }
    }
}

impl Estimator for ElasticNet {
    fn fit(&mut self, x: &NdArray, y: Option<&NdArray>) -> LearnResult<()> {
        let y = y.ok_or_else(|| LearnError::Error("y required".into()))?;
        let (n, d) = check_xy(x, y)?;
        let mut xv = x.to_vec();
        let mut yv = y_as_vec(y)?;
        let mut x_mean = vec![0.0; d];
        let mut y_mean = 0.0;
        if self.fit_intercept {
            for i in 0..n {
                y_mean += yv[i];
                for j in 0..d {
                    x_mean[j] += xv[i * d + j];
                }
            }
            y_mean /= n as f64;
            for j in 0..d {
                x_mean[j] /= n as f64;
            }
            for i in 0..n {
                yv[i] -= y_mean;
                for j in 0..d {
                    xv[i * d + j] -= x_mean[j];
                }
            }
        }
        let coef = coordinate_descent(
            &xv,
            &yv,
            n,
            d,
            self.alpha,
            self.l1_ratio,
            self.max_iter,
            self.tol,
        )?;
        if self.fit_intercept {
            let mut intercept = y_mean;
            for j in 0..d {
                intercept -= x_mean[j] * coef[j];
            }
            self.intercept = intercept;
        } else {
            self.intercept = 0.0;
        }
        self.coef = Some(coef);
        Ok(())
    }
}

impl Predictor for ElasticNet {
    fn predict(&self, x: &NdArray) -> LearnResult<NdArray> {
        let coef = self
            .coef
            .as_ref()
            .ok_or_else(|| LearnError::NotFitted("ElasticNet not fitted".into()))?;
        let (n, d) = check_2d(x, "X")?;
        let xv = x.to_vec();
        let mut out = vec![0.0; n];
        for i in 0..n {
            let mut s = self.intercept;
            for j in 0..d {
                s += xv[i * d + j] * coef[j];
            }
            out[i] = s;
        }
        vector_from(out)
    }
}

impl Scorer for ElasticNet {
    fn score(&self, x: &NdArray, y: &NdArray) -> LearnResult<f64> {
        r2_score(y, &self.predict(x)?)
    }
}
