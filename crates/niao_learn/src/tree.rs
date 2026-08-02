//! CART decision trees.

use crate::error::{LearnError, LearnResult};
use crate::metrics::{accuracy, r2_score};
use crate::traits::{Estimator, Predictor, Scorer};
use crate::utils::{check_2d, check_xy, unique_labels, vector_from, y_as_vec};
use niao_num::NdArray;

#[derive(Clone, Debug)]
enum Node {
    Leaf {
        value: f64,
        #[allow(dead_code)]
        class: usize,
    },
    Split {
        feature: usize,
        threshold: f64,
        left: Box<Node>,
        right: Box<Node>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Criterion {
    Gini,
    Entropy,
    Mse,
}

#[derive(Clone, Debug)]
pub struct DecisionTreeClassifier {
    pub max_depth: usize,
    pub min_samples_split: usize,
    pub min_samples_leaf: usize,
    pub criterion: Criterion,
    pub max_features: Option<usize>,
    root: Option<Node>,
    classes: Option<Vec<f64>>,
    pub(crate) n_features: usize,
    /// Optional feature subset for RF (indices into original features).
    #[allow(dead_code)]
    pub(crate) feature_indices: Option<Vec<usize>>,
}

impl Default for DecisionTreeClassifier {
    fn default() -> Self {
        Self {
            max_depth: usize::MAX,
            min_samples_split: 2,
            min_samples_leaf: 1,
            criterion: Criterion::Gini,
            max_features: None,
            root: None,
            classes: None,
            n_features: 0,
            feature_indices: None,
        }
    }
}

impl DecisionTreeClassifier {
    pub fn new(max_depth: usize) -> Self {
        Self {
            max_depth,
            ..Default::default()
        }
    }

    #[allow(dead_code)]
    pub(crate) fn with_feature_indices(mut self, feats: Vec<usize>) -> Self {
        self.feature_indices = Some(feats);
        self
    }

    /// Remap split feature indices from a reduced matrix back to original columns.
    pub(crate) fn remap_features(&mut self, map: &[usize]) {
        if let Some(ref mut root) = self.root {
            remap_node(root, map);
        }
        self.feature_indices = Some(map.to_vec());
    }
}

fn remap_node(node: &mut Node, map: &[usize]) {
    match node {
        Node::Leaf { .. } => {}
        Node::Split {
            feature,
            left,
            right,
            ..
        } => {
            *feature = map[*feature];
            remap_node(left, map);
            remap_node(right, map);
        }
    }
}

fn gini(y: &[f64], indices: &[usize], classes: &[f64]) -> f64 {
    if indices.is_empty() {
        return 0.0;
    }
    let n = indices.len() as f64;
    let mut imp = 1.0;
    for &c in classes {
        let cnt = indices
            .iter()
            .filter(|&&i| (y[i] - c).abs() < 1e-12)
            .count() as f64;
        let p = cnt / n;
        imp -= p * p;
    }
    imp
}

fn entropy(y: &[f64], indices: &[usize], classes: &[f64]) -> f64 {
    if indices.is_empty() {
        return 0.0;
    }
    let n = indices.len() as f64;
    let mut e = 0.0;
    for &c in classes {
        let cnt = indices
            .iter()
            .filter(|&&i| (y[i] - c).abs() < 1e-12)
            .count() as f64;
        if cnt > 0.0 {
            let p = cnt / n;
            e -= p * p.ln();
        }
    }
    e
}

fn impurity(crit: Criterion, y: &[f64], indices: &[usize], classes: &[f64]) -> f64 {
    match crit {
        Criterion::Gini => gini(y, indices, classes),
        Criterion::Entropy => entropy(y, indices, classes),
        Criterion::Mse => 0.0,
    }
}

fn majority(y: &[f64], indices: &[usize], classes: &[f64]) -> (f64, usize) {
    let mut best_i = 0;
    let mut best_c = 0usize;
    for (ci, &c) in classes.iter().enumerate() {
        let cnt = indices
            .iter()
            .filter(|&&i| (y[i] - c).abs() < 1e-12)
            .count();
        if cnt > best_c {
            best_c = cnt;
            best_i = ci;
        }
    }
    (classes[best_i], best_i)
}

fn build_clf(
    x: &[f64],
    y: &[f64],
    indices: &[usize],
    d: usize,
    depth: usize,
    max_depth: usize,
    min_split: usize,
    min_leaf: usize,
    criterion: Criterion,
    classes: &[f64],
    feature_pool: &[usize],
) -> Node {
    let n = indices.len();
    let (val, class) = majority(y, indices, classes);
    if n < min_split || depth >= max_depth || impurity(criterion, y, indices, classes) < 1e-12 {
        return Node::Leaf { value: val, class };
    }
    let parent_imp = impurity(criterion, y, indices, classes);
    let mut best_gain = 0.0f64;
    let mut best_feat = 0usize;
    let mut best_thr = 0.0f64;
    let mut best_left: Vec<usize> = Vec::new();
    let mut best_right: Vec<usize> = Vec::new();

    for &f in feature_pool {
        let mut vals: Vec<f64> = indices.iter().map(|&i| x[i * d + f]).collect();
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        vals.dedup_by(|a, b| (*a - *b).abs() < 1e-15);
        for w in vals.windows(2) {
            let thr = 0.5 * (w[0] + w[1]);
            let (left, right): (Vec<_>, Vec<_>) =
                indices.iter().copied().partition(|&i| x[i * d + f] <= thr);
            if left.len() < min_leaf || right.len() < min_leaf {
                continue;
            }
            let gain = parent_imp
                - left.len() as f64 / n as f64 * impurity(criterion, y, &left, classes)
                - right.len() as f64 / n as f64 * impurity(criterion, y, &right, classes);
            if gain > best_gain {
                best_gain = gain;
                best_feat = f;
                best_thr = thr;
                best_left = left;
                best_right = right;
            }
        }
    }
    if best_gain <= 1e-15 {
        return Node::Leaf { value: val, class };
    }
    Node::Split {
        feature: best_feat,
        threshold: best_thr,
        left: Box::new(build_clf(
            x,
            y,
            &best_left,
            d,
            depth + 1,
            max_depth,
            min_split,
            min_leaf,
            criterion,
            classes,
            feature_pool,
        )),
        right: Box::new(build_clf(
            x,
            y,
            &best_right,
            d,
            depth + 1,
            max_depth,
            min_split,
            min_leaf,
            criterion,
            classes,
            feature_pool,
        )),
    }
}

fn predict_node(node: &Node, row: &[f64]) -> f64 {
    match node {
        Node::Leaf { value, .. } => *value,
        Node::Split {
            feature,
            threshold,
            left,
            right,
        } => {
            if row[*feature] <= *threshold {
                predict_node(left, row)
            } else {
                predict_node(right, row)
            }
        }
    }
}

impl Estimator for DecisionTreeClassifier {
    fn fit(&mut self, x: &NdArray, y: Option<&NdArray>) -> LearnResult<()> {
        let y = y.ok_or_else(|| LearnError::Error("y required".into()))?;
        let (n, d) = check_xy(x, y)?;
        let xv = x.to_vec();
        let yv = y_as_vec(y)?;
        let classes = unique_labels(&yv);
        let indices: Vec<usize> = (0..n).collect();
        let pool: Vec<usize> = if let Some(ref feats) = self.feature_indices {
            feats.clone()
        } else if let Some(mf) = self.max_features {
            (0..d.min(mf)).collect()
        } else {
            (0..d).collect()
        };
        self.root = Some(build_clf(
            &xv,
            &yv,
            &indices,
            d,
            0,
            self.max_depth,
            self.min_samples_split,
            self.min_samples_leaf,
            self.criterion,
            &classes,
            &pool,
        ));
        self.classes = Some(classes);
        self.n_features = d;
        Ok(())
    }
}

impl Predictor for DecisionTreeClassifier {
    fn predict(&self, x: &NdArray) -> LearnResult<NdArray> {
        let root = self
            .root
            .as_ref()
            .ok_or_else(|| LearnError::NotFitted("DecisionTreeClassifier not fitted".into()))?;
        let (n, d) = check_2d(x, "X")?;
        if d != self.n_features {
            return Err(LearnError::Shape("feature count mismatch".into()));
        }
        let xv = x.to_vec();
        let mut out = vec![0.0; n];
        for i in 0..n {
            out[i] = predict_node(root, &xv[i * d..(i + 1) * d]);
        }
        vector_from(out)
    }
}

impl Scorer for DecisionTreeClassifier {
    fn score(&self, x: &NdArray, y: &NdArray) -> LearnResult<f64> {
        accuracy(y, &self.predict(x)?)
    }
}

#[derive(Clone, Debug)]
pub struct DecisionTreeRegressor {
    pub max_depth: usize,
    pub min_samples_split: usize,
    pub min_samples_leaf: usize,
    root: Option<Node>,
    pub(crate) n_features: usize,
}

impl Default for DecisionTreeRegressor {
    fn default() -> Self {
        Self {
            max_depth: usize::MAX,
            min_samples_split: 2,
            min_samples_leaf: 1,
            root: None,
            n_features: 0,
        }
    }
}

impl DecisionTreeRegressor {
    pub fn new(max_depth: usize) -> Self {
        Self {
            max_depth,
            ..Default::default()
        }
    }

    pub(crate) fn remap_features(&mut self, map: &[usize]) {
        if let Some(ref mut root) = self.root {
            remap_node(root, map);
        }
    }
}

fn mse_imp(y: &[f64], indices: &[usize]) -> f64 {
    if indices.is_empty() {
        return 0.0;
    }
    let mean: f64 = indices.iter().map(|&i| y[i]).sum::<f64>() / indices.len() as f64;
    indices
        .iter()
        .map(|&i| {
            let e = y[i] - mean;
            e * e
        })
        .sum::<f64>()
        / indices.len() as f64
}

fn mean_y(y: &[f64], indices: &[usize]) -> f64 {
    indices.iter().map(|&i| y[i]).sum::<f64>() / indices.len().max(1) as f64
}

fn build_reg(
    x: &[f64],
    y: &[f64],
    indices: &[usize],
    d: usize,
    depth: usize,
    max_depth: usize,
    min_split: usize,
    min_leaf: usize,
) -> Node {
    let n = indices.len();
    let val = mean_y(y, indices);
    if n < min_split || depth >= max_depth {
        return Node::Leaf {
            value: val,
            class: 0,
        };
    }
    let parent = mse_imp(y, indices);
    let mut best_gain = 0.0f64;
    let mut best_feat = 0usize;
    let mut best_thr = 0.0f64;
    let mut best_left = Vec::new();
    let mut best_right = Vec::new();
    for f in 0..d {
        let mut vals: Vec<f64> = indices.iter().map(|&i| x[i * d + f]).collect();
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        vals.dedup_by(|a, b| (*a - *b).abs() < 1e-15);
        for w in vals.windows(2) {
            let thr = 0.5 * (w[0] + w[1]);
            let (left, right): (Vec<_>, Vec<_>) =
                indices.iter().copied().partition(|&i| x[i * d + f] <= thr);
            if left.len() < min_leaf || right.len() < min_leaf {
                continue;
            }
            let gain = parent
                - left.len() as f64 / n as f64 * mse_imp(y, &left)
                - right.len() as f64 / n as f64 * mse_imp(y, &right);
            if gain > best_gain {
                best_gain = gain;
                best_feat = f;
                best_thr = thr;
                best_left = left;
                best_right = right;
            }
        }
    }
    if best_gain <= 1e-15 {
        return Node::Leaf {
            value: val,
            class: 0,
        };
    }
    Node::Split {
        feature: best_feat,
        threshold: best_thr,
        left: Box::new(build_reg(
            x,
            y,
            &best_left,
            d,
            depth + 1,
            max_depth,
            min_split,
            min_leaf,
        )),
        right: Box::new(build_reg(
            x,
            y,
            &best_right,
            d,
            depth + 1,
            max_depth,
            min_split,
            min_leaf,
        )),
    }
}

impl Estimator for DecisionTreeRegressor {
    fn fit(&mut self, x: &NdArray, y: Option<&NdArray>) -> LearnResult<()> {
        let y = y.ok_or_else(|| LearnError::Error("y required".into()))?;
        let (n, d) = check_xy(x, y)?;
        let xv = x.to_vec();
        let yv = y_as_vec(y)?;
        let indices: Vec<usize> = (0..n).collect();
        self.root = Some(build_reg(
            &xv,
            &yv,
            &indices,
            d,
            0,
            self.max_depth,
            self.min_samples_split,
            self.min_samples_leaf,
        ));
        self.n_features = d;
        Ok(())
    }
}

impl Predictor for DecisionTreeRegressor {
    fn predict(&self, x: &NdArray) -> LearnResult<NdArray> {
        let root = self
            .root
            .as_ref()
            .ok_or_else(|| LearnError::NotFitted("DecisionTreeRegressor not fitted".into()))?;
        let (n, d) = check_2d(x, "X")?;
        let xv = x.to_vec();
        let mut out = vec![0.0; n];
        for i in 0..n {
            out[i] = predict_node(root, &xv[i * d..(i + 1) * d]);
        }
        vector_from(out)
    }
}

impl Scorer for DecisionTreeRegressor {
    fn score(&self, x: &NdArray, y: &NdArray) -> LearnResult<f64> {
        r2_score(y, &self.predict(x)?)
    }
}
