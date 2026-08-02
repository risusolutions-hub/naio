//! Convolution and correlation.

use crate::error::{DspError, DspResult};
use crate::fft::{fft, ifft, Complex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConvMode {
    Full,
    Same,
    Valid,
}

impl ConvMode {
    pub fn parse(s: &str) -> DspResult<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "full" => Ok(Self::Full),
            "same" => Ok(Self::Same),
            "valid" => Ok(Self::Valid),
            other => Err(DspError::Param(format!("unknown mode '{other}'"))),
        }
    }
}

fn output_len(na: usize, nb: usize, mode: ConvMode) -> DspResult<usize> {
    match mode {
        ConvMode::Full => Ok(na
            .saturating_add(nb)
            .saturating_sub(1)
            .max(if na == 0 || nb == 0 { 0 } else { na + nb - 1 })),
        ConvMode::Same => Ok(na),
        ConvMode::Valid => {
            if nb > na {
                return Err(DspError::Length(
                    "valid mode requires len(a) >= len(b)".into(),
                ));
            }
            Ok(na + 1 - nb)
        }
    }
}

fn slice_mode(full: &[f64], na: usize, nb: usize, mode: ConvMode) -> DspResult<Vec<f64>> {
    let nfull = full.len();
    match mode {
        ConvMode::Full => Ok(full.to_vec()),
        ConvMode::Same => {
            let start = (nb.saturating_sub(1)) / 2;
            let out_len = na;
            if start + out_len > nfull {
                let mut v = vec![0.0; out_len];
                let copy = (nfull.saturating_sub(start)).min(out_len);
                v[..copy].copy_from_slice(&full[start..start + copy]);
                Ok(v)
            } else {
                Ok(full[start..start + out_len].to_vec())
            }
        }
        ConvMode::Valid => {
            let start = nb - 1;
            let out_len = output_len(na, nb, mode)?;
            Ok(full[start..start + out_len].to_vec())
        }
    }
}

/// Direct (time-domain) convolution.
pub fn convolve(a: &[f64], b: &[f64], mode: ConvMode) -> DspResult<Vec<f64>> {
    if a.is_empty() || b.is_empty() {
        return Ok(vec![]);
    }
    let na = a.len();
    let nb = b.len();
    let nfull = na + nb - 1;
    let mut full = vec![0.0; nfull];
    for i in 0..na {
        let ai = a[i];
        for j in 0..nb {
            full[i + j] += ai * b[j];
        }
    }
    slice_mode(&full, na, nb, mode)
}

/// Correlation: convolve(a, reverse(b)).
pub fn correlate(a: &[f64], b: &[f64], mode: ConvMode) -> DspResult<Vec<f64>> {
    let br: Vec<f64> = b.iter().rev().copied().collect();
    convolve(a, &br, mode)
}

fn next_pow2(n: usize) -> usize {
    n.next_power_of_two().max(1)
}

/// FFT-based convolution (O(N log N) for long signals).
pub fn fftconvolve(a: &[f64], b: &[f64], mode: ConvMode) -> DspResult<Vec<f64>> {
    if a.is_empty() || b.is_empty() {
        return Ok(vec![]);
    }
    // Prefer direct for short kernels / moderate products (FFT setup dominates).
    let product = a.len().saturating_mul(b.len());
    if b.len() <= 128 || product < 262_144 {
        return convolve(a, b, mode);
    }
    let na = a.len();
    let nb = b.len();
    let nfull = na + nb - 1;
    let nfft = next_pow2(nfull);
    let mut fa = vec![Complex::default(); nfft];
    let mut fb = vec![Complex::default(); nfft];
    for (i, &v) in a.iter().enumerate() {
        fa[i] = Complex::from_real(v);
    }
    for (i, &v) in b.iter().enumerate() {
        fb[i] = Complex::from_real(v);
    }
    let fa = fft(&fa);
    let fb = fft(&fb);
    let prod: Vec<Complex> = fa.into_iter().zip(fb).map(|(x, y)| x * y).collect();
    let inv = ifft(&prod);
    let full: Vec<f64> = inv.iter().take(nfull).map(|c| c.re).collect();
    slice_mode(&full, na, nb, mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convolve_impulse() {
        let a = [1.0, 2.0, 3.0];
        let b = [1.0];
        let y = convolve(&a, &b, ConvMode::Full).unwrap();
        assert_eq!(y, a);
    }

    #[test]
    fn fft_matches_direct() {
        let a: Vec<f64> = (0..64).map(|i| (i as f64 * 0.1).sin()).collect();
        let b = [0.25, 0.5, 0.25];
        let d = convolve(&a, &b, ConvMode::Same).unwrap();
        let f = fftconvolve(&a, &b, ConvMode::Same).unwrap();
        for (x, y) in d.iter().zip(f.iter()) {
            assert!((x - y).abs() < 1e-9);
        }
    }
}
