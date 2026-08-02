//! Loss objectives: gradients and hessians per boosting round.

use crate::error::{BoostError, BoostResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskKind {
    Regression,
    Binary,
    Multiclass,
}

pub trait Objective {
    fn kind(&self) -> TaskKind;
    fn num_output_trees(&self) -> usize;
    fn init_predictions(&self, labels: &[f64], n_rows: usize, out: &mut [f64]) -> BoostResult<()>;
    fn gradients(
        &self,
        labels: &[f64],
        preds: &[f64],
        grad: &mut [f64],
        hess: &mut [f64],
    ) -> BoostResult<()>;
}

/// Squared error (L2) regression.
#[derive(Clone, Copy, Debug, Default)]
pub struct SquaredError;

impl Objective for SquaredError {
    fn kind(&self) -> TaskKind {
        TaskKind::Regression
    }

    fn num_output_trees(&self) -> usize {
        1
    }

    fn init_predictions(&self, labels: &[f64], n_rows: usize, out: &mut [f64]) -> BoostResult<()> {
        if out.len() != n_rows {
            return Err(BoostError::Shape("init_predictions length mismatch".into()));
        }
        let mean = if labels.is_empty() {
            0.0
        } else {
            labels.iter().sum::<f64>() / labels.len() as f64
        };
        out.fill(mean);
        Ok(())
    }

    fn gradients(
        &self,
        labels: &[f64],
        preds: &[f64],
        grad: &mut [f64],
        hess: &mut [f64],
    ) -> BoostResult<()> {
        let n = labels.len();
        if preds.len() != n || grad.len() != n || hess.len() != n {
            return Err(BoostError::Shape("gradient buffers length mismatch".into()));
        }
        for i in 0..n {
            grad[i] = preds[i] - labels[i];
            hess[i] = 1.0;
        }
        Ok(())
    }
}

/// Binary logistic loss.
#[derive(Clone, Copy, Debug, Default)]
pub struct Logistic;

impl Logistic {
    #[inline]
    fn sigmoid(x: f64) -> f64 {
        if x >= 0.0 {
            let z = (-x).exp();
            1.0 / (1.0 + z)
        } else {
            let z = x.exp();
            z / (1.0 + z)
        }
    }
}

impl Objective for Logistic {
    fn kind(&self) -> TaskKind {
        TaskKind::Binary
    }

    fn num_output_trees(&self) -> usize {
        1
    }

    fn init_predictions(&self, labels: &[f64], n_rows: usize, out: &mut [f64]) -> BoostResult<()> {
        if out.len() != n_rows {
            return Err(BoostError::Shape("init_predictions length mismatch".into()));
        }
        let mut pos = 0.0;
        for &y in labels {
            if y > 0.5 {
                pos += 1.0;
            }
        }
        let p = (pos + 1.0) / (labels.len() as f64 + 2.0);
        let init = (p / (1.0 - p)).ln();
        out.fill(init);
        Ok(())
    }

    fn gradients(
        &self,
        labels: &[f64],
        preds: &[f64],
        grad: &mut [f64],
        hess: &mut [f64],
    ) -> BoostResult<()> {
        let n = labels.len();
        if preds.len() != n || grad.len() != n || hess.len() != n {
            return Err(BoostError::Shape("gradient buffers length mismatch".into()));
        }
        for i in 0..n {
            let p = Self::sigmoid(preds[i]);
            grad[i] = p - labels[i];
            hess[i] = (p * (1.0 - p)).max(1e-16);
        }
        Ok(())
    }
}

/// Softmax multiclass (one tree per class per round).
#[derive(Clone, Debug)]
pub struct SoftmaxMulticlass {
    pub num_class: usize,
}

impl SoftmaxMulticlass {
    pub fn new(num_class: usize) -> BoostResult<Self> {
        if num_class < 2 {
            return Err(BoostError::BadParam("num_class must be >= 2".into()));
        }
        Ok(Self { num_class })
    }

    fn softmax_probs(preds: &[f64], num_class: usize, row: usize, n_rows: usize, out: &mut [f64]) {
        let base = row * num_class;
        let mut maxv = f64::NEG_INFINITY;
        for c in 0..num_class {
            maxv = maxv.max(preds[base + c]);
        }
        let mut sum = 0.0;
        for c in 0..num_class {
            let v = (preds[base + c] - maxv).exp();
            out[c] = v;
            sum += v;
        }
        for c in 0..num_class {
            out[c] /= sum;
        }
    }
}

impl Objective for SoftmaxMulticlass {
    fn kind(&self) -> TaskKind {
        TaskKind::Multiclass
    }

    fn num_output_trees(&self) -> usize {
        self.num_class
    }

    fn init_predictions(&self, _labels: &[f64], n_rows: usize, out: &mut [f64]) -> BoostResult<()> {
        let need = n_rows * self.num_class;
        if out.len() != need {
            return Err(BoostError::Shape("init_predictions length mismatch".into()));
        }
        out.fill(0.0);
        Ok(())
    }

    fn gradients(
        &self,
        labels: &[f64],
        preds: &[f64],
        grad: &mut [f64],
        hess: &mut [f64],
    ) -> BoostResult<()> {
        let n = labels.len();
        let nc = self.num_class;
        if preds.len() != n * nc || grad.len() != n * nc || hess.len() != n * nc {
            return Err(BoostError::Shape("gradient buffers length mismatch".into()));
        }
        let mut probs = vec![0.0; nc];
        for i in 0..n {
            Self::softmax_probs(preds, nc, i, n, &mut probs);
            let yi = labels[i].round() as usize;
            let yi = yi.min(nc - 1);
            for c in 0..nc {
                let idx = i * nc + c;
                grad[idx] = probs[c] - if c == yi { 1.0 } else { 0.0 };
                hess[idx] = (probs[c] * (1.0 - probs[c])).max(1e-16);
            }
        }
        Ok(())
    }
}

/// Custom objective hook (grad + hess callback).
pub struct CustomObjective<F>
where
    F: Fn(&[f64], &[f64], &mut [f64], &mut [f64]) -> BoostResult<()> + Send + Sync,
{
    pub kind: TaskKind,
    pub n_trees: usize,
    pub init_fn: fn(&[f64], usize, &mut [f64]) -> BoostResult<()>,
    pub grad_fn: F,
}

impl<F> Objective for CustomObjective<F>
where
    F: Fn(&[f64], &[f64], &mut [f64], &mut [f64]) -> BoostResult<()> + Send + Sync,
{
    fn kind(&self) -> TaskKind {
        self.kind
    }

    fn num_output_trees(&self) -> usize {
        self.n_trees
    }

    fn init_predictions(&self, labels: &[f64], n_rows: usize, out: &mut [f64]) -> BoostResult<()> {
        (self.init_fn)(labels, n_rows, out)
    }

    fn gradients(
        &self,
        labels: &[f64],
        preds: &[f64],
        grad: &mut [f64],
        hess: &mut [f64],
    ) -> BoostResult<()> {
        (self.grad_fn)(labels, preds, grad, hess)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squared_error_grad() {
        let labels = vec![1.0, 2.0, 3.0];
        let preds = vec![0.5, 2.5, 2.0];
        let mut g = vec![0.0; 3];
        let mut h = vec![0.0; 3];
        SquaredError
            .gradients(&labels, &preds, &mut g, &mut h)
            .unwrap();
        assert!((g[0] - (-0.5)).abs() < 1e-12);
        assert!((g[1] - 0.5).abs() < 1e-12);
        assert!((g[2] - (-1.0)).abs() < 1e-12);
        assert!(h.iter().all(|&x| (x - 1.0).abs() < 1e-12));
    }
}
