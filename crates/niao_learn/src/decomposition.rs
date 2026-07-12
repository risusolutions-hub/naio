//! Decomposition: PCA via covariance eigendecomposition (nnum).

use crate::error::{LearnError, LearnResult};
use crate::traits::{Estimator, Transformer};
use crate::utils::{check_2d, fix_component_sign, matrix_from, mean_axis0};
use niao_num::{eig_symmetric, NdArray};

#[derive(Clone, Debug)]
pub struct PCA {
    pub n_components: usize,
    pub components: Option<Vec<f64>>, // n_components * n_features
    pub explained_variance: Option<Vec<f64>>,
    pub mean: Option<Vec<f64>>,
    n_features: usize,
}

impl Default for PCA {
    fn default() -> Self {
        Self {
            n_components: 2,
            components: None,
            explained_variance: None,
            mean: None,
            n_features: 0,
        }
    }
}

impl PCA {
    pub fn new(n_components: usize) -> Self {
        Self {
            n_components,
            ..Default::default()
        }
    }
}

impl Estimator for PCA {
    fn fit(&mut self, x: &NdArray, _y: Option<&NdArray>) -> LearnResult<()> {
        let (n, d) = check_2d(x, "X")?;
        let k = self.n_components.min(n.min(d));
        if k == 0 {
            return Err(LearnError::Error("n_components must be > 0".into()));
        }
        let data = x.to_vec();
        let mean = mean_axis0(&data, n, d);
        // covariance (ddof=1) like sklearn PCA
        let mut cov = vec![0.0; d * d];
        for i in 0..n {
            for a in 0..d {
                let xa = data[i * d + a] - mean[a];
                for b in 0..d {
                    let xb = data[i * d + b] - mean[b];
                    cov[a * d + b] += xa * xb;
                }
            }
        }
        let denom = (n.saturating_sub(1)).max(1) as f64;
        for v in cov.iter_mut() {
            *v /= denom;
        }
        let cov_arr = matrix_from((d, d), cov)?;
        let eig = eig_symmetric(&cov_arr).map_err(|e| LearnError::Error(e.to_string()))?;
        let vals = eig.values.to_vec();
        let vecs = eig.vectors.to_vec(); // columns are eigenvectors
        let mut order: Vec<usize> = (0..d).collect();
        order.sort_by(|&a, &b| vals[b].partial_cmp(&vals[a]).unwrap());
        let mut comps = vec![0.0; k * d];
        let mut ev = vec![0.0; k];
        for (ci, &oi) in order.iter().take(k).enumerate() {
            for j in 0..d {
                comps[ci * d + j] = vecs[j * d + oi]; // column oi
            }
            fix_component_sign(&mut comps[ci * d..(ci + 1) * d]);
            ev[ci] = vals[oi].max(0.0);
        }
        self.components = Some(comps);
        self.explained_variance = Some(ev);
        self.mean = Some(mean);
        self.n_features = d;
        self.n_components = k;
        Ok(())
    }
}

impl Transformer for PCA {
    fn transform(&self, x: &NdArray) -> LearnResult<NdArray> {
        let comps = self
            .components
            .as_ref()
            .ok_or_else(|| LearnError::NotFitted("PCA not fitted".into()))?;
        let mean = self.mean.as_ref().unwrap();
        let (n, d) = check_2d(x, "X")?;
        if d != self.n_features {
            return Err(LearnError::Shape("feature count mismatch".into()));
        }
        let k = self.n_components;
        let data = x.to_vec();
        let mut out = vec![0.0; n * k];
        for i in 0..n {
            for c in 0..k {
                let mut s = 0.0;
                for j in 0..d {
                    s += (data[i * d + j] - mean[j]) * comps[c * d + j];
                }
                out[i * k + c] = s;
            }
        }
        matrix_from((n, k), out)
    }
}
