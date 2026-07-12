//! Axis-aware reductions.

use crate::array::NdArray;
use crate::error::{NumError, NumResult};

pub fn sum(a: &NdArray, axis: Option<usize>) -> NumResult<NdArray> {
    reduce(a, axis, 0.0, |acc, x| acc + x)
}

pub fn mean(a: &NdArray, axis: Option<usize>) -> NumResult<NdArray> {
    let s = sum(a, axis)?;
    let count = axis.map(|ax| a.shape[ax]).unwrap_or(a.len()) as f64;
    s.map_unary(|x| x / count)
}

pub fn var(a: &NdArray, axis: Option<usize>) -> NumResult<NdArray> {
    let m = mean(a, axis)?;
    let m_b = m.broadcast_to(&a.shape)?;
    let diff = a.map_binary(&m_b, |x, y| x - y)?;
    let sq = diff.map_unary(|x| x * x)?;
    mean(&sq, axis)
}

pub fn std(a: &NdArray, axis: Option<usize>) -> NumResult<NdArray> {
    var(a, axis)?.map_unary(|x| x.sqrt())
}

pub fn min(a: &NdArray, axis: Option<usize>) -> NumResult<NdArray> {
    reduce(a, axis, f64::INFINITY, f64::min)
}

pub fn max(a: &NdArray, axis: Option<usize>) -> NumResult<NdArray> {
    reduce(a, axis, f64::NEG_INFINITY, f64::max)
}

pub fn argmin(a: &NdArray, axis: Option<usize>) -> NumResult<NdArray> {
    argext(a, axis, true)
}

pub fn argmax(a: &NdArray, axis: Option<usize>) -> NumResult<NdArray> {
    argext(a, axis, false)
}

pub fn prod(a: &NdArray, axis: Option<usize>) -> NumResult<NdArray> {
    reduce(a, axis, 1.0, |acc, x| acc * x)
}

pub fn cumsum(a: &NdArray, axis: usize) -> NumResult<NdArray> {
    if axis >= a.ndim() {
        return Err(NumError::ShapeMismatch("axis out of range".into()));
    }
    let mut out = a.to_vec();
    let shape = a.shape.clone();
    let strides = crate::array::row_major_strides(&shape);
    let axis_stride = strides[axis] as usize;
    let outer: usize = shape[..axis].iter().product();
    let inner = shape[axis];
    let block = shape[axis + 1..].iter().product::<usize>().max(1);
    for o in 0..outer {
        for b in 0..block {
            let base = o * inner * block + b;
            for i in 1..inner {
                let prev = out[base + (i - 1) * axis_stride];
                let idx = base + i * axis_stride;
                out[idx] += prev;
            }
        }
    }
    NdArray::from_vec(shape, out)
}

fn reduce(
    a: &NdArray,
    axis: Option<usize>,
    init: f64,
    op: impl Fn(f64, f64) -> f64,
) -> NumResult<NdArray> {
    match axis {
        None => {
            let v = a.to_vec();
            let acc = v.iter().fold(init, |acc, &x| op(acc, x));
            NdArray::from_vec(vec![1], vec![acc])
        }
        Some(ax) => {
            if ax >= a.ndim() {
                return Err(NumError::ShapeMismatch("axis out of range".into()));
            }
            let mut out_shape = a.shape.clone();
            out_shape.remove(ax);
            if out_shape.is_empty() {
                out_shape.push(1);
            }
            let out_n: usize = out_shape.iter().product();
            let mut out = vec![init; out_n];
            let data = a.to_vec();
            let mut idx = vec![0usize; a.ndim()];
            for val in data {
                let mut out_pos = 0usize;
                let mut stride = 1usize;
                for (od, &dim) in out_shape.iter().enumerate().rev() {
                    let src_ax = if od < ax { od } else { od + 1 };
                    out_pos += idx[src_ax] * stride;
                    stride *= dim;
                }
                out[out_pos] = op(out[out_pos], val);
                advance(&a.shape, &mut idx);
            }
            NdArray::from_vec(out_shape, out)
        }
    }
}

fn argext(a: &NdArray, axis: Option<usize>, is_min: bool) -> NumResult<NdArray> {
    match axis {
        None => {
            let v = a.to_vec();
            let (idx, _) = if is_min {
                v.iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                    .unwrap()
            } else {
                v.iter()
                    .enumerate()
                    .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
                    .unwrap()
            };
            NdArray::from_vec(vec![1], vec![idx as f64])
        }
        Some(ax) => {
            if ax >= a.ndim() {
                return Err(NumError::ShapeMismatch("axis out of range".into()));
            }
            let mut out_shape = a.shape.clone();
            out_shape.remove(ax);
            if out_shape.is_empty() {
                out_shape.push(1);
            }
            let out_n: usize = out_shape.iter().product();
            let mut out = vec![0.0; out_n];
            let mut best = vec![if is_min { f64::INFINITY } else { f64::NEG_INFINITY }; out_n];
            let data = a.to_vec();
            let mut idx = vec![0usize; a.ndim()];
            for _ in 0..data.len() {
                let mut out_idx = 0usize;
                let mut stride = 1usize;
                for (d, &dim) in out_shape.iter().enumerate().rev() {
                    let src_d = if d < ax { d } else { d + 1 };
                    out_idx += (idx[src_d] % dim) * stride;
                    stride *= dim;
                }
                let val = data[linear_index(&a.shape, &idx)];
                let better = if is_min { val < best[out_idx] } else { val > best[out_idx] };
                if better {
                    best[out_idx] = val;
                    out[out_idx] = idx[ax] as f64;
                }
                advance(&a.shape, &mut idx);
            }
            NdArray::from_vec(out_shape, out)
        }
    }
}

fn linear_index(shape: &[usize], idx: &[usize]) -> usize {
    let mut pos = 0usize;
    let mut stride = 1usize;
    for (i, &dim) in shape.iter().enumerate().rev() {
        pos += (idx[shape.len() - 1 - i] % dim) * stride;
        stride *= dim;
    }
    pos
}

fn advance(shape: &[usize], idx: &mut [usize]) {
    for i in (0..shape.len()).rev() {
        idx[i] += 1;
        if idx[i] < shape[i] {
            return;
        }
        idx[i] = 0;
    }
}
