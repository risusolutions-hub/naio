//! Clustering: KMeans.

use crate::error::{LearnError, LearnResult};
use crate::traits::{Estimator, Predictor};
use crate::utils::{check_2d, matrix_from, squared_dist, vector_from};
use niao_num::NdArray;
use niao_rand::{Rng, SeedableRng, StdRng};

#[derive(Clone, Debug)]
pub struct KMeans {
    pub n_clusters: usize,
    pub max_iter: usize,
    pub tol: f64,
    pub random_state: u64,
    pub n_init: usize,
    pub cluster_centers: Option<Vec<f64>>,
    pub labels: Option<Vec<usize>>,
    pub inertia: f64,
    n_features: usize,
}

impl Default for KMeans {
    fn default() -> Self {
        Self {
            n_clusters: 8,
            max_iter: 300,
            tol: 1e-4,
            random_state: 42,
            n_init: 1,
            cluster_centers: None,
            labels: None,
            inertia: 0.0,
            n_features: 0,
        }
    }
}

impl KMeans {
    pub fn new(n_clusters: usize, random_state: u64) -> Self {
        Self {
            n_clusters,
            random_state,
            ..Default::default()
        }
    }
}

fn nearest(point: &[f64], centers: &[f64], k: usize, d: usize) -> (usize, f64) {
    let mut best = 0;
    let mut best_dist = f64::INFINITY;
    for c in 0..k {
        let dist = squared_dist(point, &centers[c * d..(c + 1) * d]);
        if dist < best_dist {
            best_dist = dist;
            best = c;
        }
    }
    (best, best_dist)
}

fn kmeans_pp_init(data: &[f64], n: usize, d: usize, k: usize, rng: &mut StdRng) -> Vec<f64> {
    let mut centers = Vec::with_capacity(k * d);
    let first = rng.gen_range_usize(0, n);
    centers.extend_from_slice(&data[first * d..(first + 1) * d]);
    let mut dists = vec![0.0; n];
    for _ in 1..k {
        let cur_k = centers.len() / d;
        let mut total = 0.0;
        for i in 0..n {
            let (_, dist) = nearest(&data[i * d..(i + 1) * d], &centers, cur_k, d);
            dists[i] = dist;
            total += dist;
        }
        let r = rng.gen_f64() * total;
        let mut acc = 0.0;
        let mut chosen = n - 1;
        for i in 0..n {
            acc += dists[i];
            if acc >= r {
                chosen = i;
                break;
            }
        }
        centers.extend_from_slice(&data[chosen * d..(chosen + 1) * d]);
    }
    centers
}

fn lloyd(
    data: &[f64],
    n: usize,
    d: usize,
    k: usize,
    mut centers: Vec<f64>,
    max_iter: usize,
    tol: f64,
    rng: &mut StdRng,
) -> (Vec<f64>, Vec<usize>, f64) {
    let mut labels = vec![0usize; n];
    for _ in 0..max_iter {
        for i in 0..n {
            labels[i] = nearest(&data[i * d..(i + 1) * d], &centers, k, d).0;
        }
        let mut new_centers = vec![0.0; k * d];
        let mut counts = vec![0usize; k];
        for i in 0..n {
            let c = labels[i];
            counts[c] += 1;
            for j in 0..d {
                new_centers[c * d + j] += data[i * d + j];
            }
        }
        for c in 0..k {
            if counts[c] == 0 {
                // reseed empty cluster
                let idx = rng.gen_range_usize(0, n);
                new_centers[c * d..(c + 1) * d]
                    .copy_from_slice(&data[idx * d..(idx + 1) * d]);
            } else {
                for j in 0..d {
                    new_centers[c * d + j] /= counts[c] as f64;
                }
            }
        }
        let mut shift = 0.0;
        for c in 0..k {
            shift += squared_dist(
                &centers[c * d..(c + 1) * d],
                &new_centers[c * d..(c + 1) * d],
            )
            .sqrt();
        }
        centers = new_centers;
        if shift < tol {
            break;
        }
    }
    let mut inertia = 0.0;
    for i in 0..n {
        let (lab, dist) = nearest(&data[i * d..(i + 1) * d], &centers, k, d);
        labels[i] = lab;
        inertia += dist;
    }
    (centers, labels, inertia)
}

impl Estimator for KMeans {
    fn fit(&mut self, x: &NdArray, _y: Option<&NdArray>) -> LearnResult<()> {
        let (n, d) = check_2d(x, "X")?;
        if self.n_clusters == 0 || self.n_clusters > n {
            return Err(LearnError::Error("invalid n_clusters".into()));
        }
        let data = x.to_vec();
        let mut best_inertia = f64::INFINITY;
        let mut best_centers = Vec::new();
        let mut best_labels = Vec::new();
        for init in 0..self.n_init.max(1) {
            let mut rng =
                StdRng::seed_from_u64(self.random_state.wrapping_add(init as u64 * 7919));
            let centers = kmeans_pp_init(&data, n, d, self.n_clusters, &mut rng);
            let (c, labels, inertia) = lloyd(
                &data,
                n,
                d,
                self.n_clusters,
                centers,
                self.max_iter,
                self.tol,
                &mut rng,
            );
            if inertia < best_inertia {
                best_inertia = inertia;
                best_centers = c;
                best_labels = labels;
            }
        }
        self.cluster_centers = Some(best_centers);
        self.labels = Some(best_labels);
        self.inertia = best_inertia;
        self.n_features = d;
        Ok(())
    }
}

impl Predictor for KMeans {
    fn predict(&self, x: &NdArray) -> LearnResult<NdArray> {
        let centers = self
            .cluster_centers
            .as_ref()
            .ok_or_else(|| LearnError::NotFitted("KMeans not fitted".into()))?;
        let (n, d) = check_2d(x, "X")?;
        if d != self.n_features {
            return Err(LearnError::Shape("feature count mismatch".into()));
        }
        let data = x.to_vec();
        let k = self.n_clusters;
        let mut out = vec![0.0; n];
        for i in 0..n {
            out[i] = nearest(&data[i * d..(i + 1) * d], centers, k, d).0 as f64;
        }
        vector_from(out)
    }
}

impl KMeans {
    pub fn cluster_centers_array(&self) -> LearnResult<NdArray> {
        let c = self
            .cluster_centers
            .as_ref()
            .ok_or_else(|| LearnError::NotFitted("KMeans not fitted".into()))?;
        matrix_from((self.n_clusters, self.n_features), c.clone())
    }
}
