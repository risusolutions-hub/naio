//! Utilities: detrend, hilbert, medfilt, find_peaks, freqz.

use crate::error::{DspError, DspResult};
use crate::fft::{fft, ifft, Complex};
use std::f64::consts::PI;

pub fn detrend(x: &[f64], kind: &str) -> DspResult<Vec<f64>> {
    if x.is_empty() {
        return Ok(vec![]);
    }
    match kind.trim().to_ascii_lowercase().as_str() {
        "constant" | "c" | "mean" => {
            let m = x.iter().sum::<f64>() / x.len() as f64;
            Ok(x.iter().map(|v| v - m).collect())
        }
        "linear" | "l" | "" => {
            let n = x.len() as f64;
            let mut sx = 0.0;
            let mut sy = 0.0;
            let mut sxx = 0.0;
            let mut sxy = 0.0;
            for (i, &y) in x.iter().enumerate() {
                let xi = i as f64;
                sx += xi;
                sy += y;
                sxx += xi * xi;
                sxy += xi * y;
            }
            let den = n * sxx - sx * sx;
            let (a, b) = if den.abs() < 1e-30 {
                (0.0, sy / n)
            } else {
                let a = (n * sxy - sx * sy) / den;
                let b = (sy - a * sx) / n;
                (a, b)
            };
            Ok(x.iter()
                .enumerate()
                .map(|(i, &y)| y - (a * i as f64 + b))
                .collect())
        }
        other => Err(DspError::Param(format!("unknown detrend type '{other}'"))),
    }
}

/// Analytic signal via FFT Hilbert transform. Returns (re, im) = (x, Hilbert(x)).
pub fn hilbert(x: &[f64]) -> DspResult<(Vec<f64>, Vec<f64>)> {
    if x.is_empty() {
        return Ok((vec![], vec![]));
    }
    let n = x.len();
    let mut spectrum = fft(&x.iter().map(|&v| Complex::from_real(v)).collect::<Vec<_>>());
    // Double positive freqs, zero negative, keep DC/Nyquist
    let half = n / 2;
    for k in 1..half {
        spectrum[k] = spectrum[k] * 2.0;
    }
    if n % 2 == 0 {
        // Nyquist unchanged
        for k in (half + 1)..n {
            spectrum[k] = Complex::default();
        }
    } else {
        for k in (half + 1)..n {
            spectrum[k] = Complex::default();
        }
        spectrum[half] = spectrum[half] * 2.0;
    }
    let y = ifft(&spectrum);
    let re: Vec<f64> = y.iter().map(|c| c.re).collect();
    let im: Vec<f64> = y.iter().map(|c| c.im).collect();
    Ok((re, im))
}

pub fn medfilt(x: &[f64], kernel_size: usize) -> DspResult<Vec<f64>> {
    if kernel_size == 0 || kernel_size % 2 == 0 {
        return Err(DspError::Param(
            "kernel_size must be a positive odd integer".into(),
        ));
    }
    if x.is_empty() {
        return Ok(vec![]);
    }
    let r = kernel_size / 2;
    let mut out = vec![0.0; x.len()];
    let mut buf = vec![0.0; kernel_size];
    for i in 0..x.len() {
        for k in 0..kernel_size {
            let j = i as isize + k as isize - r as isize;
            let idx = if j < 0 {
                0
            } else if j as usize >= x.len() {
                x.len() - 1
            } else {
                j as usize
            };
            buf[k] = x[idx];
        }
        buf.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        out[i] = buf[r];
    }
    Ok(out)
}

#[derive(Clone, Debug)]
pub struct Peaks {
    pub peaks: Vec<usize>,
    pub heights: Vec<f64>,
}

pub fn find_peaks(x: &[f64], height: Option<f64>, distance: Option<usize>) -> Peaks {
    if x.len() < 3 {
        return Peaks {
            peaks: vec![],
            heights: vec![],
        };
    }
    let mut candidates = Vec::new();
    for i in 1..x.len() - 1 {
        if x[i] > x[i - 1] && x[i] >= x[i + 1] {
            if let Some(h) = height {
                if x[i] < h {
                    continue;
                }
            }
            candidates.push(i);
        }
    }
    let dist = distance.unwrap_or(1).max(1);
    let mut kept = Vec::new();
    for &p in &candidates {
        if let Some(&last) = kept.last() {
            if p - last < dist {
                // keep taller
                if x[p] > x[last] {
                    kept.pop();
                    kept.push(p);
                }
                continue;
            }
        }
        kept.push(p);
    }
    let heights: Vec<f64> = kept.iter().map(|&i| x[i]).collect();
    Peaks {
        peaks: kept,
        heights,
    }
}

#[derive(Clone, Debug)]
pub struct FreqzResult {
    pub w: Vec<f64>,
    pub re: Vec<f64>,
    pub im: Vec<f64>,
}

pub fn freqz(b: &[f64], a: &[f64], wor_n: usize, fs: Option<f64>) -> DspResult<FreqzResult> {
    if b.is_empty() || a.is_empty() {
        return Err(DspError::Filter("b and a must be non-empty".into()));
    }
    if wor_n == 0 {
        return Err(DspError::Param("worN must be > 0".into()));
    }
    let mut w = Vec::with_capacity(wor_n);
    let mut re = Vec::with_capacity(wor_n);
    let mut im = Vec::with_capacity(wor_n);
    for k in 0..wor_n {
        let omega = PI * k as f64 / (wor_n.saturating_sub(1).max(1) as f64);
        let z_ang = -omega;
        let mut num = Complex::default();
        let mut den = Complex::default();
        for (n, &bn) in b.iter().enumerate() {
            let ang = z_ang * n as f64;
            num = num + Complex::new(bn * ang.cos(), bn * ang.sin());
        }
        for (n, &an) in a.iter().enumerate() {
            let ang = z_ang * n as f64;
            den = den + Complex::new(an * ang.cos(), an * ang.sin());
        }
        let den_ns = den.norm_sq();
        if den_ns < 1e-30 {
            re.push(f64::NAN);
            im.push(f64::NAN);
        } else {
            let h = num * Complex::new(den.re / den_ns, -den.im / den_ns);
            re.push(h.re);
            im.push(h.im);
        }
        if let Some(fs) = fs {
            w.push(omega / PI * (fs / 2.0));
        } else {
            w.push(omega);
        }
    }
    Ok(FreqzResult { w, re, im })
}

pub fn sosfreqz(sos: &[[f64; 6]], wor_n: usize, fs: Option<f64>) -> DspResult<FreqzResult> {
    if sos.is_empty() {
        return Err(DspError::Filter("sos must be non-empty".into()));
    }
    let mut acc_re = vec![1.0; wor_n];
    let mut acc_im = vec![0.0; wor_n];
    let mut w = vec![];
    for sec in sos {
        let r = freqz(&sec[0..3], &sec[3..6], wor_n, fs)?;
        if w.is_empty() {
            w = r.w;
        }
        for k in 0..wor_n {
            let a = Complex::new(acc_re[k], acc_im[k]);
            let b = Complex::new(r.re[k], r.im[k]);
            let p = a * b;
            acc_re[k] = p.re;
            acc_im[k] = p.im;
        }
    }
    Ok(FreqzResult {
        w,
        re: acc_re,
        im: acc_im,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detrend_constant() {
        let y = detrend(&[3.0, 3.0, 3.0], "constant").unwrap();
        assert!(y.iter().all(|v| v.abs() < 1e-12));
    }

    #[test]
    fn find_peaks_basic() {
        let x = [0.0, 1.0, 0.0, 2.0, 0.0];
        let p = find_peaks(&x, None, None);
        assert_eq!(p.peaks, vec![1, 3]);
    }
}
