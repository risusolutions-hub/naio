//! Array creation helpers.

use crate::array::NdArray;
use crate::error::{NumError, NumResult};
use niao_rand::{Rng, SeedableRng, Xoshiro256StarStar};

pub fn full(shape: &[usize], value: f64) -> NumResult<NdArray> {
    let n: usize = shape.iter().product();
    NdArray::from_vec(shape.to_vec(), vec![value; n])
}

pub fn eye(n: usize) -> NumResult<NdArray> {
    let mut data = vec![0.0; n * n];
    for i in 0..n {
        data[i * n + i] = 1.0;
    }
    NdArray::from_vec(vec![n, n], data)
}

pub fn arange(start: f64, stop: f64, step: f64) -> NumResult<NdArray> {
    if step == 0.0 {
        return Err(NumError::Error("arange step cannot be zero".into()));
    }
    let n = ((stop - start) / step).ceil() as usize;
    let data: Vec<f64> = (0..n).map(|i| start + i as f64 * step).collect();
    NdArray::from_vec(vec![n], data)
}

pub fn linspace(start: f64, stop: f64, n: usize) -> NumResult<NdArray> {
    if n == 0 {
        return NdArray::from_vec(vec![0], vec![]);
    }
    if n == 1 {
        return NdArray::from_vec(vec![1], vec![start]);
    }
    let step = (stop - start) / (n - 1) as f64;
    let data: Vec<f64> = (0..n).map(|i| start + i as f64 * step).collect();
    NdArray::from_vec(vec![n], data)
}

pub fn from_slice(shape: &[usize], data: &[f64]) -> NumResult<NdArray> {
    NdArray::from_vec(shape.to_vec(), data.to_vec())
}

pub fn rand(shape: &[usize], seed: u64) -> NumResult<NdArray> {
    let n: usize = shape.iter().product();
    let mut rng = Xoshiro256StarStar::seed_from_u64(seed);
    let data: Vec<f64> = (0..n).map(|_| rng.gen_f64()).collect();
    NdArray::from_vec(shape.to_vec(), data)
}

pub fn randn(shape: &[usize], seed: u64) -> NumResult<NdArray> {
    let n: usize = shape.iter().product();
    let mut rng = Xoshiro256StarStar::seed_from_u64(seed);
    let mut data = Vec::with_capacity(n);
    let mut spare = 0.0f64;
    let mut has_spare = false;
    for _ in 0..n {
        if has_spare {
            data.push(spare);
            has_spare = false;
            continue;
        }
        let u1 = rng.gen_f64().max(1e-15);
        let u2 = rng.gen_f64();
        let mag = (-2.0 * u1.ln()).sqrt();
        let z0 = mag * (2.0 * std::f64::consts::PI * u2).cos();
        let z1 = mag * (2.0 * std::f64::consts::PI * u2).sin();
        data.push(z0);
        spare = z1;
        has_spare = true;
    }
    if has_spare {
        data.pop();
    }
    NdArray::from_vec(shape.to_vec(), data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linspace_fixture() {
        let a = linspace(0.0, 1.0, 5).unwrap();
        let v = a.to_vec();
        assert!((v[0] - 0.0).abs() < 1e-12);
        assert!((v[1] - 0.25).abs() < 1e-12);
        assert!((v[2] - 0.5).abs() < 1e-12);
        assert!((v[3] - 0.75).abs() < 1e-12);
        assert!((v[4] - 1.0).abs() < 1e-12);
    }
}
