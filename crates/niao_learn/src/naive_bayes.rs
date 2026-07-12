//! Naive Bayes classifiers.

use crate::error::{LearnError, LearnResult};
use crate::metrics::accuracy;
use crate::traits::{Estimator, Predictor, Scorer};
use crate::utils::{
    check_2d, check_xy, label_index, matrix_from, unique_labels, vector_from, y_as_vec,
};
use niao_num::NdArray;

#[derive(Clone, Debug, Default)]
pub struct GaussianNB {
    pub classes: Option<Vec<f64>>,
    pub theta: Option<Vec<f64>>, // n_classes * n_features
    pub var: Option<Vec<f64>>,
    pub class_prior: Option<Vec<f64>>,
    n_features: usize,
}

impl GaussianNB {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Estimator for GaussianNB {
    fn fit(&mut self, x: &NdArray, y: Option<&NdArray>) -> LearnResult<()> {
        let y = y.ok_or_else(|| LearnError::Error("y required".into()))?;
        let (n, d) = check_xy(x, y)?;
        let xv = x.to_vec();
        let yv = y_as_vec(y)?;
        let classes = unique_labels(&yv);
        let k = classes.len();
        let mut theta = vec![0.0; k * d];
        let mut var = vec![0.0; k * d];
        let mut counts = vec![0usize; k];
        for i in 0..n {
            let c = label_index(&classes, yv[i]).unwrap();
            counts[c] += 1;
            for j in 0..d {
                theta[c * d + j] += xv[i * d + j];
            }
        }
        for c in 0..k {
            let cnt = counts[c].max(1) as f64;
            for j in 0..d {
                theta[c * d + j] /= cnt;
            }
        }
        for i in 0..n {
            let c = label_index(&classes, yv[i]).unwrap();
            for j in 0..d {
                let diff = xv[i * d + j] - theta[c * d + j];
                var[c * d + j] += diff * diff;
            }
        }
        // sklearn uses var smoothing; ddof=0 then + epsilon
        let eps = 1e-9;
        for c in 0..k {
            let cnt = counts[c].max(1) as f64;
            for j in 0..d {
                var[c * d + j] = var[c * d + j] / cnt + eps;
            }
        }
        let prior: Vec<f64> = counts.iter().map(|&c| c as f64 / n as f64).collect();
        self.classes = Some(classes);
        self.theta = Some(theta);
        self.var = Some(var);
        self.class_prior = Some(prior);
        self.n_features = d;
        Ok(())
    }
}

impl Predictor for GaussianNB {
    fn predict(&self, x: &NdArray) -> LearnResult<NdArray> {
        let proba = self.predict_proba(x)?;
        let classes = self.classes.as_ref().unwrap();
        let (n, k) = (proba.shape[0], proba.shape[1]);
        let pv = proba.to_vec();
        let mut out = vec![0.0; n];
        for i in 0..n {
            let mut best = 0;
            let mut best_p = f64::NEG_INFINITY;
            for c in 0..k {
                if pv[i * k + c] > best_p {
                    best_p = pv[i * k + c];
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
            .ok_or_else(|| LearnError::NotFitted("GaussianNB not fitted".into()))?;
        let theta = self.theta.as_ref().unwrap();
        let var = self.var.as_ref().unwrap();
        let prior = self.class_prior.as_ref().unwrap();
        let (n, d) = check_2d(x, "X")?;
        let k = classes.len();
        let xv = x.to_vec();
        let mut out = vec![0.0; n * k];
        const LN_2PI: f64 = 1.8378770664093453;
        for i in 0..n {
            let mut logp = vec![0.0; k];
            for c in 0..k {
                let mut lp = prior[c].ln();
                for j in 0..d {
                    let v = var[c * d + j];
                    let diff = xv[i * d + j] - theta[c * d + j];
                    lp += -0.5 * (LN_2PI + v.ln() + diff * diff / v);
                }
                logp[c] = lp;
            }
            let m = logp.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let mut s = 0.0;
            for c in 0..k {
                out[i * k + c] = (logp[c] - m).exp();
                s += out[i * k + c];
            }
            for c in 0..k {
                out[i * k + c] /= s;
            }
        }
        matrix_from((n, k), out)
    }
}

impl Scorer for GaussianNB {
    fn score(&self, x: &NdArray, y: &NdArray) -> LearnResult<f64> {
        accuracy(y, &self.predict(x)?)
    }
}

#[derive(Clone, Debug)]
pub struct MultinomialNB {
    pub alpha: f64,
    pub classes: Option<Vec<f64>>,
    pub feature_log_prob: Option<Vec<f64>>,
    pub class_log_prior: Option<Vec<f64>>,
    n_features: usize,
}

impl Default for MultinomialNB {
    fn default() -> Self {
        Self {
            alpha: 1.0,
            classes: None,
            feature_log_prob: None,
            class_log_prior: None,
            n_features: 0,
        }
    }
}

impl MultinomialNB {
    pub fn new(alpha: f64) -> Self {
        Self {
            alpha,
            ..Default::default()
        }
    }
}

impl Estimator for MultinomialNB {
    fn fit(&mut self, x: &NdArray, y: Option<&NdArray>) -> LearnResult<()> {
        let y = y.ok_or_else(|| LearnError::Error("y required".into()))?;
        let (n, d) = check_xy(x, y)?;
        let xv = x.to_vec();
        let yv = y_as_vec(y)?;
        let classes = unique_labels(&yv);
        let k = classes.len();
        let mut fc = vec![0.0; k * d];
        let mut cc = vec![0.0; k];
        for i in 0..n {
            let c = label_index(&classes, yv[i]).unwrap();
            cc[c] += 1.0;
            for j in 0..d {
                fc[c * d + j] += xv[i * d + j];
            }
        }
        let mut flp = vec![0.0; k * d];
        for c in 0..k {
            let mut s = 0.0;
            for j in 0..d {
                s += fc[c * d + j] + self.alpha;
            }
            for j in 0..d {
                flp[c * d + j] = ((fc[c * d + j] + self.alpha) / s).ln();
            }
        }
        let clp: Vec<f64> = cc.iter().map(|&c| (c / n as f64).ln()).collect();
        self.classes = Some(classes);
        self.feature_log_prob = Some(flp);
        self.class_log_prior = Some(clp);
        self.n_features = d;
        Ok(())
    }
}

impl Predictor for MultinomialNB {
    fn predict(&self, x: &NdArray) -> LearnResult<NdArray> {
        let classes = self
            .classes
            .as_ref()
            .ok_or_else(|| LearnError::NotFitted("MultinomialNB not fitted".into()))?;
        let flp = self.feature_log_prob.as_ref().unwrap();
        let clp = self.class_log_prior.as_ref().unwrap();
        let (n, d) = check_2d(x, "X")?;
        let k = classes.len();
        let xv = x.to_vec();
        let mut out = vec![0.0; n];
        for i in 0..n {
            let mut best = 0;
            let mut best_lp = f64::NEG_INFINITY;
            for c in 0..k {
                let mut lp = clp[c];
                for j in 0..d {
                    lp += xv[i * d + j] * flp[c * d + j];
                }
                if lp > best_lp {
                    best_lp = lp;
                    best = c;
                }
            }
            out[i] = classes[best];
        }
        vector_from(out)
    }
}

impl Scorer for MultinomialNB {
    fn score(&self, x: &NdArray, y: &NdArray) -> LearnResult<f64> {
        accuracy(y, &self.predict(x)?)
    }
}

#[derive(Clone, Debug)]
pub struct BernoulliNB {
    pub alpha: f64,
    pub classes: Option<Vec<f64>>,
    pub feature_log_prob: Option<Vec<f64>>,
    pub class_log_prior: Option<Vec<f64>>,
    n_features: usize,
}

impl Default for BernoulliNB {
    fn default() -> Self {
        Self {
            alpha: 1.0,
            classes: None,
            feature_log_prob: None,
            class_log_prior: None,
            n_features: 0,
        }
    }
}

impl BernoulliNB {
    pub fn new(alpha: f64) -> Self {
        Self {
            alpha,
            ..Default::default()
        }
    }
}

impl Estimator for BernoulliNB {
    fn fit(&mut self, x: &NdArray, y: Option<&NdArray>) -> LearnResult<()> {
        let y = y.ok_or_else(|| LearnError::Error("y required".into()))?;
        let (n, d) = check_xy(x, y)?;
        let xv = x.to_vec();
        let yv = y_as_vec(y)?;
        let classes = unique_labels(&yv);
        let k = classes.len();
        let mut fc = vec![0.0; k * d];
        let mut cc = vec![0.0; k];
        for i in 0..n {
            let c = label_index(&classes, yv[i]).unwrap();
            cc[c] += 1.0;
            for j in 0..d {
                if xv[i * d + j] > 0.5 {
                    fc[c * d + j] += 1.0;
                }
            }
        }
        let mut flp = vec![0.0; k * d];
        for c in 0..k {
            for j in 0..d {
                let p = (fc[c * d + j] + self.alpha) / (cc[c] + 2.0 * self.alpha);
                flp[c * d + j] = p.ln();
            }
        }
        let clp: Vec<f64> = cc.iter().map(|&c| (c / n as f64).ln()).collect();
        self.classes = Some(classes);
        self.feature_log_prob = Some(flp);
        self.class_log_prior = Some(clp);
        self.n_features = d;
        Ok(())
    }
}

impl Predictor for BernoulliNB {
    fn predict(&self, x: &NdArray) -> LearnResult<NdArray> {
        let classes = self
            .classes
            .as_ref()
            .ok_or_else(|| LearnError::NotFitted("BernoulliNB not fitted".into()))?;
        let flp = self.feature_log_prob.as_ref().unwrap();
        let clp = self.class_log_prior.as_ref().unwrap();
        let (n, d) = check_2d(x, "X")?;
        let k = classes.len();
        let xv = x.to_vec();
        let mut out = vec![0.0; n];
        for i in 0..n {
            let mut best = 0;
            let mut best_lp = f64::NEG_INFINITY;
            for c in 0..k {
                let mut lp = clp[c];
                for j in 0..d {
                    let p = flp[c * d + j].exp();
                    if xv[i * d + j] > 0.5 {
                        lp += p.ln();
                    } else {
                        lp += (1.0 - p).ln();
                    }
                }
                if lp > best_lp {
                    best_lp = lp;
                    best = c;
                }
            }
            out[i] = classes[best];
        }
        vector_from(out)
    }
}

impl Scorer for BernoulliNB {
    fn score(&self, x: &NdArray, y: &NdArray) -> LearnResult<f64> {
        accuracy(y, &self.predict(x)?)
    }
}
