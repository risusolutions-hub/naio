//! Decision tree nodes and histogram-based tree builder.

use crate::binning::BinnedMatrix;
use crate::histogram::{
    best_split_on_histogram, build_histogram, leaf_weight, partition_rows, row_sums,
    FeatureHistogram, SplitCandidate,
};
use crate::params::{BoosterParams, GrowPolicy};
use std::cmp::Ordering;
use std::collections::BinaryHeap;

#[derive(Clone, Debug)]
pub enum TreeNode {
    Leaf {
        value: f64,
        cover: u32,
    },
    Split {
        feature: u16,
        bin: u8,
        default_left: bool,
        gain: f64,
        left: Box<TreeNode>,
        right: Box<TreeNode>,
    },
}

#[derive(Clone, Debug)]
pub struct Tree {
    pub root: TreeNode,
    pub num_leaves: usize,
}

impl Tree {
    pub fn predict_one(&self, data: &BinnedMatrix, row: usize) -> f64 {
        predict_node(&self.root, data, row)
    }

    pub fn predict(&self, data: &BinnedMatrix, out: &mut [f64]) {
        for (r, o) in out.iter_mut().enumerate().take(data.n_rows) {
            *o += self.predict_one(data, r);
        }
    }
}

fn predict_node(node: &TreeNode, data: &BinnedMatrix, row: usize) -> f64 {
    match node {
        TreeNode::Leaf { value, .. } => *value,
        TreeNode::Split {
            feature,
            bin,
            default_left,
            left,
            right,
            ..
        } => {
            let f = *feature as usize;
            let go_left = if data.is_missing(f, row) {
                *default_left
            } else {
                data.bin_at(f, row) <= *bin
            };
            if go_left {
                predict_node(left, data, row)
            } else {
                predict_node(right, data, row)
            }
        }
    }
}

#[derive(Clone)]
enum BuildNode {
    Leaf {
        rows: Vec<usize>,
        depth: usize,
    },
    Split {
        feature: u16,
        bin: u8,
        default_left: bool,
        gain: f64,
        left: usize,
        right: usize,
    },
}

struct ExpandCandidate {
    gain: f64,
    node_id: usize,
}

impl PartialEq for ExpandCandidate {
    fn eq(&self, other: &Self) -> bool {
        self.gain == other.gain
    }
}

impl Eq for ExpandCandidate {}

impl PartialOrd for ExpandCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ExpandCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.gain
            .partial_cmp(&other.gain)
            .unwrap_or(Ordering::Equal)
    }
}

/// Build one regression/class tree for the current gradient slice.
pub fn build_tree(
    data: &BinnedMatrix,
    grad: &[f64],
    hess: &[f64],
    params: &BoosterParams,
    feature_subset: &[usize],
    importance_gain: &mut [f64],
    importance_split: &mut [u32],
    importance_cover: &mut [u32],
) -> Tree {
    let n_rows = data.n_rows;
    let all_rows: Vec<usize> = (0..n_rows).collect();
    let max_bins = data
        .thresholds
        .iter()
        .map(|t| t.len().max(1))
        .max()
        .unwrap_or(1);

    let mut hist_bufs: Vec<FeatureHistogram> = (0..feature_subset.len())
        .map(|_| FeatureHistogram::new(max_bins))
        .collect();

    if params.grow_policy == GrowPolicy::DepthWise {
        let root = build_depthwise(
            data,
            grad,
            hess,
            &all_rows,
            0,
            params,
            feature_subset,
            &mut hist_bufs,
            importance_gain,
            importance_split,
            importance_cover,
        );
        let num_leaves = count_leaves(&root);
        return Tree { root, num_leaves };
    }

    let mut nodes = vec![BuildNode::Leaf {
        rows: all_rows,
        depth: 0,
    }];
    let mut heap = BinaryHeap::new();
    refresh_leaf_candidate(
        0,
        &mut nodes,
        data,
        grad,
        hess,
        params,
        feature_subset,
        &mut hist_bufs,
        &mut heap,
    );

    let mut num_leaves = 1usize;

    while num_leaves < params.max_leaves {
        let Some(cand) = heap.pop() else {
            break;
        };
        let BuildNode::Leaf { rows, depth } = nodes[cand.node_id].clone() else {
            continue;
        };
        if depth >= params.max_depth || rows.len() < params.min_data_in_leaf * 2 {
            continue;
        }

        let split = find_best_split(
            data,
            grad,
            hess,
            &rows,
            params,
            feature_subset,
            &mut hist_bufs,
        );
        let Some(split) = split else {
            continue;
        };
        if split.gain <= 0.0 {
            continue;
        }

        record_importance(&split, importance_gain, importance_split, importance_cover);

        let (lrows, rrows) = partition_rows(data, &rows, &split);
        let left_id = nodes.len();
        nodes.push(BuildNode::Leaf {
            rows: lrows,
            depth: depth + 1,
        });
        let right_id = nodes.len();
        nodes.push(BuildNode::Leaf {
            rows: rrows,
            depth: depth + 1,
        });

        nodes[cand.node_id] = BuildNode::Split {
            feature: split.feature as u16,
            bin: split.bin,
            default_left: split.default_left,
            gain: split.gain,
            left: left_id,
            right: right_id,
        };

        num_leaves += 1;
        refresh_leaf_candidate(
            left_id,
            &mut nodes,
            data,
            grad,
            hess,
            params,
            feature_subset,
            &mut hist_bufs,
            &mut heap,
        );
        refresh_leaf_candidate(
            right_id,
            &mut nodes,
            data,
            grad,
            hess,
            params,
            feature_subset,
            &mut hist_bufs,
            &mut heap,
        );
    }

    let root = finalize_node(0, &nodes, data, grad, hess, params);
    Tree { root, num_leaves }
}

fn refresh_leaf_candidate(
    node_id: usize,
    nodes: &mut [BuildNode],
    data: &BinnedMatrix,
    grad: &[f64],
    hess: &[f64],
    params: &BoosterParams,
    feature_subset: &[usize],
    hist_bufs: &mut [FeatureHistogram],
    heap: &mut BinaryHeap<ExpandCandidate>,
) {
    let BuildNode::Leaf { rows, depth } = &nodes[node_id] else {
        return;
    };
    if *depth >= params.max_depth || rows.len() < params.min_data_in_leaf * 2 {
        return;
    }
    let split = find_best_split(data, grad, hess, rows, params, feature_subset, hist_bufs);
    if let Some(ref sp) = split {
        if sp.gain > 0.0 {
            heap.push(ExpandCandidate {
                gain: sp.gain,
                node_id,
            });
        }
    }
}

fn finalize_node(
    id: usize,
    nodes: &[BuildNode],
    data: &BinnedMatrix,
    grad: &[f64],
    hess: &[f64],
    params: &BoosterParams,
) -> TreeNode {
    match &nodes[id] {
        BuildNode::Leaf { rows, .. } => {
            let (g, h, c) = row_sums(rows, grad, hess);
            TreeNode::Leaf {
                value: leaf_weight(g, h, params.lambda_l2, params.alpha_l1),
                cover: c,
            }
        }
        BuildNode::Split {
            feature,
            bin,
            default_left,
            gain,
            left,
            right,
        } => TreeNode::Split {
            feature: *feature,
            bin: *bin,
            default_left: *default_left,
            gain: *gain,
            left: Box::new(finalize_node(*left, nodes, data, grad, hess, params)),
            right: Box::new(finalize_node(*right, nodes, data, grad, hess, params)),
        },
    }
}

fn build_depthwise(
    data: &BinnedMatrix,
    grad: &[f64],
    hess: &[f64],
    rows: &[usize],
    depth: usize,
    params: &BoosterParams,
    feature_subset: &[usize],
    hist_bufs: &mut [FeatureHistogram],
    importance_gain: &mut [f64],
    importance_split: &mut [u32],
    importance_cover: &mut [u32],
) -> TreeNode {
    let (g, h, c) = row_sums(rows, grad, hess);
    if depth >= params.max_depth || rows.len() < params.min_data_in_leaf * 2 {
        return TreeNode::Leaf {
            value: leaf_weight(g, h, params.lambda_l2, params.alpha_l1),
            cover: c,
        };
    }
    let split = find_best_split(data, grad, hess, rows, params, feature_subset, hist_bufs);
    let Some(split) = split else {
        return TreeNode::Leaf {
            value: leaf_weight(g, h, params.lambda_l2, params.alpha_l1),
            cover: c,
        };
    };
    record_importance(&split, importance_gain, importance_split, importance_cover);
    let (lrows, rrows) = partition_rows(data, rows, &split);
    TreeNode::Split {
        feature: split.feature as u16,
        bin: split.bin,
        default_left: split.default_left,
        gain: split.gain,
        left: Box::new(build_depthwise(
            data,
            grad,
            hess,
            &lrows,
            depth + 1,
            params,
            feature_subset,
            hist_bufs,
            importance_gain,
            importance_split,
            importance_cover,
        )),
        right: Box::new(build_depthwise(
            data,
            grad,
            hess,
            &rrows,
            depth + 1,
            params,
            feature_subset,
            hist_bufs,
            importance_gain,
            importance_split,
            importance_cover,
        )),
    }
}

fn count_leaves(node: &TreeNode) -> usize {
    match node {
        TreeNode::Leaf { .. } => 1,
        TreeNode::Split { left, right, .. } => count_leaves(left) + count_leaves(right),
    }
}

fn find_best_split(
    data: &BinnedMatrix,
    grad: &[f64],
    hess: &[f64],
    rows: &[usize],
    params: &BoosterParams,
    feature_subset: &[usize],
    hist_bufs: &mut [FeatureHistogram],
) -> Option<SplitCandidate> {
    let mut best: Option<SplitCandidate> = None;
    for (i, &feat) in feature_subset.iter().enumerate() {
        build_histogram(data, feat, rows, grad, hess, &mut hist_bufs[i]);
        if let Some(cand) = best_split_on_histogram(&hist_bufs[i], params, feat) {
            if best.as_ref().map_or(true, |b| cand.gain > b.gain) {
                best = Some(cand);
            }
        }
    }
    best
}

fn record_importance(
    split: &SplitCandidate,
    gain: &mut [f64],
    splits: &mut [u32],
    cover: &mut [u32],
) {
    let f = split.feature;
    if f < gain.len() {
        gain[f] += split.gain;
        splits[f] += 1;
        cover[f] += split.left_count + split.right_count;
    }
}
