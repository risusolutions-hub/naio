//! Elementwise operations.

use crate::array::NdArray;
use crate::error::NumResult;

pub fn add(a: &NdArray, b: &NdArray) -> NumResult<NdArray> {
    a.map_binary(b, |x, y| x + y)
}

pub fn sub(a: &NdArray, b: &NdArray) -> NumResult<NdArray> {
    a.map_binary(b, |x, y| x - y)
}

pub fn mul(a: &NdArray, b: &NdArray) -> NumResult<NdArray> {
    a.map_binary(b, |x, y| x * y)
}

pub fn div(a: &NdArray, b: &NdArray) -> NumResult<NdArray> {
    a.map_binary(b, |x, y| x / y)
}

pub fn pow(a: &NdArray, exp: f64) -> NumResult<NdArray> {
    a.map_unary(|x| x.powf(exp))
}

pub fn exp(a: &NdArray) -> NumResult<NdArray> {
    a.map_unary(f64::exp)
}

pub fn log(a: &NdArray) -> NumResult<NdArray> {
    a.map_unary(|x| x.ln())
}

pub fn sqrt(a: &NdArray) -> NumResult<NdArray> {
    a.map_unary(|x| x.sqrt())
}

pub fn abs(a: &NdArray) -> NumResult<NdArray> {
    a.map_unary(f64::abs)
}

pub fn sin(a: &NdArray) -> NumResult<NdArray> {
    a.map_unary(f64::sin)
}

pub fn cos(a: &NdArray) -> NumResult<NdArray> {
    a.map_unary(f64::cos)
}

pub fn tan(a: &NdArray) -> NumResult<NdArray> {
    a.map_unary(f64::tan)
}

pub fn clip(a: &NdArray, min: f64, max: f64) -> NumResult<NdArray> {
    a.map_unary(|x| x.clamp(min, max))
}

pub fn maximum(a: &NdArray, b: &NdArray) -> NumResult<NdArray> {
    a.map_binary(b, f64::max)
}

pub fn minimum(a: &NdArray, b: &NdArray) -> NumResult<NdArray> {
    a.map_binary(b, f64::min)
}

pub fn where_array(cond: &NdArray, x: &NdArray, y: &NdArray) -> NumResult<NdArray> {
    let shape =
        NdArray::broadcast_shapes(&NdArray::broadcast_shapes(&cond.shape, &x.shape)?, &y.shape)?;
    let c = cond.broadcast_to(&shape)?;
    let a = x.broadcast_to(&shape)?;
    let b = y.broadcast_to(&shape)?;
    let cv = c.to_vec();
    let av = a.to_vec();
    let bv = b.to_vec();
    let data: Vec<f64> = cv
        .iter()
        .zip(av.iter().zip(bv.iter()))
        .map(|(&c, (&a, &b))| if c != 0.0 { a } else { b })
        .collect();
    NdArray::from_vec(shape, data)
}
