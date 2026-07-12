//! FFT: radix-2 Cooley-Tukey + Bluestein for arbitrary lengths.

use crate::array::NdArray;
use crate::error::{NumError, NumResult};
use std::f64::consts::PI;

#[derive(Clone, Copy)]
pub struct Complex {
    pub re: f64,
    pub im: f64,
}

impl Complex {
    pub fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }

    pub fn from_real(x: f64) -> Self {
        Self { re: x, im: 0.0 }
    }

    pub fn norm(&self) -> f64 {
        (self.re * self.re + self.im * self.im).sqrt()
    }
}

impl std::ops::Add for Complex {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.re + rhs.re, self.im + rhs.im)
    }
}

impl std::ops::Sub for Complex {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.re - rhs.re, self.im - rhs.im)
    }
}

impl std::ops::Mul for Complex {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self::new(
            self.re * rhs.re - self.im * rhs.im,
            self.re * rhs.im + self.im * rhs.re,
        )
    }
}

pub fn fft(a: &NdArray) -> NumResult<Vec<Complex>> {
    if a.ndim() != 1 {
        return Err(NumError::ShapeMismatch("fft requires 1-D array".into()));
    }
    let n = a.shape[0];
    let x: Vec<Complex> = a.to_vec().into_iter().map(Complex::from_real).collect();
    if n.is_power_of_two() {
        fft_radix2(&x)
    } else {
        fft_bluestein(&x, n)
    }
}

pub fn ifft(spectrum: &[Complex]) -> NumResult<Vec<Complex>> {
    let n = spectrum.len();
    if n == 0 {
        return Ok(vec![]);
    }
    let conj: Vec<Complex> = spectrum
        .iter()
        .map(|c| Complex::new(c.re, -c.im))
        .collect();
    let mut out = if n.is_power_of_two() {
        fft_radix2(&conj)?
    } else {
        fft_bluestein(&conj, n)?
    };
    let scale = 1.0 / n as f64;
    for c in &mut out {
        c.re *= scale;
        c.im = -c.im * scale;
    }
    Ok(out)
}

pub fn rfft(a: &NdArray) -> NumResult<Vec<Complex>> {
    let full = fft(a)?;
    let n = full.len();
    let half = n / 2 + 1;
    Ok(full[..half].to_vec())
}

pub fn fft2(a: &NdArray) -> NumResult<Vec<Vec<Complex>>> {
    if a.ndim() != 2 {
        return Err(NumError::ShapeMismatch("fft2 requires 2-D array".into()));
    }
    let rows = a.shape[0];
    let cols = a.shape[1];
    let mut out = vec![vec![Complex::new(0.0, 0.0); cols]; rows];
    for r in 0..rows {
        let row: Vec<f64> = (0..cols).map(|c| a.index(&[r, c]).unwrap()).collect();
        let row_arr = NdArray::from_vec(vec![cols], row)?;
        let freq = fft(&row_arr)?;
        out[r] = freq;
    }
    for c in 0..cols {
        let col: Vec<Complex> = (0..rows).map(|r| out[r][c]).collect();
        let col_arr_data: Vec<f64> = col.iter().map(|z| z.re).collect();
        let col_arr = NdArray::from_vec(vec![rows], col_arr_data)?;
        let freq_re = fft(&col_arr)?;
        let col_im: Vec<f64> = col.iter().map(|z| z.im).collect();
        let col_im_arr = NdArray::from_vec(vec![rows], col_im)?;
        let freq_im = fft(&col_im_arr)?;
        for r in 0..rows {
            out[r][c] = Complex::new(freq_re[r].re, freq_im[r].re);
        }
    }
    Ok(out)
}

fn fft_radix2(x: &[Complex]) -> NumResult<Vec<Complex>> {
    let n = x.len();
    if !n.is_power_of_two() {
        return Err(NumError::ShapeMismatch("radix-2 fft requires power-of-2 length".into()));
    }
    let mut a = x.to_vec();
    bit_reverse_permute(&mut a);
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * PI / len as f64;
        let wlen = Complex::new(ang.cos(), ang.sin());
        for i in (0..n).step_by(len) {
            let mut w = Complex::new(1.0, 0.0);
            for j in 0..len / 2 {
                let u = a[i + j];
                let v = a[i + j + len / 2] * w;
                a[i + j] = u + v;
                a[i + j + len / 2] = u - v;
                w = w * wlen;
            }
        }
        len *= 2;
    }
    Ok(a)
}

fn fft_bluestein(x: &[Complex], n: usize) -> NumResult<Vec<Complex>> {
    let m = next_pow2(2 * n - 1);
    let mut a = vec![Complex::new(0.0, 0.0); m];
    let mut b = vec![Complex::new(0.0, 0.0); m];
    for i in 0..n {
        let ang = -PI * (i * i) as f64 / n as f64;
        let chirp = Complex::new(ang.cos(), ang.sin());
        a[i] = x[i] * chirp;
        b[i] = chirp;
    }
    for i in 1..n {
        b[m - i] = b[i];
    }
    let fa = fft_radix2(&a)?;
    let fb = fft_radix2(&b)?;
    let mut fc: Vec<Complex> = fa.iter().zip(fb.iter()).map(|(&a, &b)| a * b).collect();
    let conv = ifft(&fc)?;
    let mut out = vec![Complex::new(0.0, 0.0); n];
    for i in 0..n {
        let ang = -PI * (i * i) as f64 / n as f64;
        let chirp = Complex::new(ang.cos(), ang.sin());
        out[i] = conv[i] * chirp;
    }
    Ok(out)
}

fn bit_reverse_permute(a: &mut [Complex]) {
    let n = a.len();
    let bits = n.trailing_zeros() as usize;
    for i in 0..n {
        let mut j = 0usize;
        for b in 0..bits {
            if (i >> b) & 1 == 1 {
                j |= 1 << (bits - 1 - b);
            }
        }
        if j > i {
            a.swap(i, j);
        }
    }
}

fn next_pow2(n: usize) -> usize {
    if n <= 1 {
        return 1;
    }
    let mut p = 1usize;
    while p < n {
        p <<= 1;
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::creation::from_slice;

    #[test]
    fn fft_impulse() {
        let a = from_slice(&[4], &[1.0, 0.0, 0.0, 0.0]).unwrap();
        let f = fft(&a).unwrap();
        for c in &f {
            assert!((c.re - 1.0).abs() < 1e-10);
            assert!(c.im.abs() < 1e-10);
        }
    }

    #[test]
    fn fft_roundtrip_pow2() {
        let a = from_slice(&[8], &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]).unwrap();
        let orig = a.to_vec();
        let spec = fft(&a).unwrap();
        let back = ifft(&spec).unwrap();
        for (i, &x) in orig.iter().enumerate() {
            assert!((back[i].re - x).abs() < 1e-10);
            assert!(back[i].im.abs() < 1e-10);
        }
    }

    #[test]
    fn fft_roundtrip_prime() {
        let a = from_slice(&[7], &[1.0, -1.0, 2.0, 0.5, 3.0, -2.0, 1.5]).unwrap();
        let orig = a.to_vec();
        let spec = fft(&a).unwrap();
        let back = ifft(&spec).unwrap();
        for (i, &x) in orig.iter().enumerate() {
            assert!((back[i].re - x).abs() < 1e-8);
        }
    }
}
