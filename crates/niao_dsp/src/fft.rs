//! Radix-2 Cooley–Tukey + Bluestein FFT for arbitrary lengths (pairs with nnum FFT).

use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, Default)]
pub struct Complex {
    pub re: f64,
    pub im: f64,
}

impl Complex {
    #[inline]
    pub const fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }

    #[inline]
    pub const fn from_real(x: f64) -> Self {
        Self { re: x, im: 0.0 }
    }

    #[inline]
    pub fn norm_sq(self) -> f64 {
        self.re * self.re + self.im * self.im
    }

    #[inline]
    pub fn abs(self) -> f64 {
        self.norm_sq().sqrt()
    }

    #[inline]
    pub fn conj(self) -> Self {
        Self {
            re: self.re,
            im: -self.im,
        }
    }
}

impl std::ops::Add for Complex {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self::new(self.re + rhs.re, self.im + rhs.im)
    }
}

impl std::ops::Sub for Complex {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.re - rhs.re, self.im - rhs.im)
    }
}

impl std::ops::Mul for Complex {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        Self::new(
            self.re * rhs.re - self.im * rhs.im,
            self.re * rhs.im + self.im * rhs.re,
        )
    }
}

impl std::ops::Mul<f64> for Complex {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: f64) -> Self {
        Self::new(self.re * rhs, self.im * rhs)
    }
}

fn bit_reverse(x: &mut [Complex]) {
    let n = x.len();
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            x.swap(i, j);
        }
    }
}

fn fft_radix2_inplace(x: &mut [Complex], inverse: bool) {
    let n = x.len();
    debug_assert!(n.is_power_of_two());
    bit_reverse(x);
    let sign = if inverse { 1.0 } else { -1.0 };
    let mut len = 2;
    while len <= n {
        let ang = sign * 2.0 * PI / len as f64;
        let wlen = Complex::new(ang.cos(), ang.sin());
        for i in (0..n).step_by(len) {
            let mut w = Complex::new(1.0, 0.0);
            for j in 0..len / 2 {
                let u = x[i + j];
                let v = x[i + j + len / 2] * w;
                x[i + j] = u + v;
                x[i + j + len / 2] = u - v;
                w = w * wlen;
            }
        }
        len <<= 1;
    }
    if inverse {
        let scale = 1.0 / n as f64;
        for c in x.iter_mut() {
            *c = *c * scale;
        }
    }
}

fn next_pow2(n: usize) -> usize {
    n.next_power_of_two().max(1)
}

fn fft_bluestein(x: &[Complex]) -> Vec<Complex> {
    let n = x.len();
    if n == 0 {
        return vec![];
    }
    let m = next_pow2(2 * n - 1);
    let mut a = vec![Complex::default(); m];
    let mut b = vec![Complex::default(); m];
    for i in 0..n {
        let angle = PI * (i * i) as f64 / n as f64;
        let w = Complex::new(angle.cos(), -angle.sin());
        a[i] = x[i] * w;
        b[i] = Complex::new(angle.cos(), angle.sin());
        if i > 0 {
            b[m - i] = b[i];
        }
    }
    fft_radix2_inplace(&mut a, false);
    fft_radix2_inplace(&mut b, false);
    for i in 0..m {
        a[i] = a[i] * b[i];
    }
    fft_radix2_inplace(&mut a, true);
    let mut out = vec![Complex::default(); n];
    for i in 0..n {
        let angle = PI * (i * i) as f64 / n as f64;
        let w = Complex::new(angle.cos(), -angle.sin());
        out[i] = a[i] * w;
    }
    out
}

pub fn fft(x: &[Complex]) -> Vec<Complex> {
    let n = x.len();
    if n == 0 {
        return vec![];
    }
    if n.is_power_of_two() {
        let mut buf = x.to_vec();
        fft_radix2_inplace(&mut buf, false);
        buf
    } else {
        fft_bluestein(x)
    }
}

pub fn ifft(x: &[Complex]) -> Vec<Complex> {
    let n = x.len();
    if n == 0 {
        return vec![];
    }
    if n.is_power_of_two() {
        let mut buf = x.to_vec();
        fft_radix2_inplace(&mut buf, true);
        buf
    } else {
        let conj: Vec<Complex> = x.iter().map(|c| c.conj()).collect();
        let mut y = fft_bluestein(&conj);
        let scale = 1.0 / n as f64;
        for c in &mut y {
            *c = Complex::new(c.re * scale, -c.im * scale);
        }
        y
    }
}

pub fn rfft(x: &[f64]) -> Vec<Complex> {
    let c: Vec<Complex> = x.iter().map(|&v| Complex::from_real(v)).collect();
    fft(&c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impulse_fft() {
        let x = [1.0, 0.0, 0.0, 0.0];
        let y = rfft(&x);
        assert!(y
            .iter()
            .all(|c| (c.re - 1.0).abs() < 1e-12 && c.im.abs() < 1e-12));
    }
}
