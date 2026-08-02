//! Resampling: FFT resample, polyphase, upfirdn, decimate.

use crate::convolve::{convolve, ConvMode};
use crate::error::{DspError, DspResult};
use crate::fft::{fft, ifft, Complex};
use crate::fir::firwin;
use crate::windows::hamming;

fn gcd(mut a: usize, mut b: usize) -> usize {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

/// FFT-based resampling to `num` samples (scipy.signal.resample).
pub fn resample(x: &[f64], num: usize) -> DspResult<Vec<f64>> {
    if num == 0 {
        return Ok(vec![]);
    }
    if x.is_empty() {
        return Err(DspError::Empty);
    }
    if x.len() == num {
        return Ok(x.to_vec());
    }
    let n = x.len();
    let x_c: Vec<Complex> = x.iter().map(|&v| Complex::from_real(v)).collect();
    let spectrum = fft(&x_c);
    let mut y_spec = vec![Complex::default(); num];
    let n_copy = n.min(num);
    let half = n_copy / 2;
    // Copy low positive freqs
    for i in 0..=half {
        if i < spectrum.len() && i < y_spec.len() {
            y_spec[i] = spectrum[i];
        }
    }
    // Copy negative freqs
    if n_copy > 1 {
        let n_neg = n_copy - half - 1;
        for i in 0..n_neg {
            let src = n - 1 - i;
            let dst = num - 1 - i;
            if src < spectrum.len() && dst < y_spec.len() {
                y_spec[dst] = spectrum[src];
            }
        }
    }
    // Nyquist bin scaling when both even
    if n % 2 == 0 && num > n {
        let nyq = n / 2;
        if nyq < spectrum.len() {
            y_spec[nyq] = spectrum[nyq] * 0.5;
            if num - nyq < y_spec.len() {
                y_spec[num - nyq] = spectrum[nyq] * 0.5;
            }
        }
    }
    let y = ifft(&y_spec);
    let scale = num as f64 / n as f64;
    Ok(y.iter().map(|c| c.re * scale).collect())
}

/// Upsample by `up`, FIR filter, downsample by `down`.
pub fn upfirdn(h: &[f64], x: &[f64], up: usize, down: usize) -> DspResult<Vec<f64>> {
    if up == 0 || down == 0 {
        return Err(DspError::Param("up and down must be >= 1".into()));
    }
    if h.is_empty() {
        return Err(DspError::Empty);
    }
    if x.is_empty() {
        return Ok(vec![]);
    }
    // Insert zeros
    let mut upsampled = vec![0.0; x.len() * up];
    for (i, &v) in x.iter().enumerate() {
        upsampled[i * up] = v;
    }
    let filtered = convolve(&upsampled, h, ConvMode::Full)?;
    // Compensate group delay: keep from center of filter
    let delay = (h.len() - 1) / 2;
    let mut out = Vec::with_capacity((filtered.len() + down - 1) / down);
    let mut i = delay;
    while i < filtered.len() {
        if (i - delay) % down == 0 {
            out.push(filtered[i]);
        }
        i += 1;
    }
    // Exact length for polyphase resample
    let expected = (x.len() * up + down - 1) / down;
    if out.len() > expected {
        out.truncate(expected);
    }
    Ok(out)
}

pub fn resample_poly(x: &[f64], up: usize, down: usize) -> DspResult<Vec<f64>> {
    if up == 0 || down == 0 {
        return Err(DspError::Param("up and down must be >= 1".into()));
    }
    if x.is_empty() {
        return Ok(vec![]);
    }
    let g = gcd(up, down);
    let up = up / g;
    let down = down / g;
    if up == 1 && down == 1 {
        return Ok(x.to_vec());
    }
    let max_rate = up.max(down);
    let cutoff = 1.0 / max_rate as f64;
    let numtaps = (10 * max_rate).max(31) | 1; // odd
    let h = firwin(numtaps, &[cutoff], "hamming", true, 2.0)?;
    // Scale for upsample gain
    let h: Vec<f64> = h.iter().map(|v| v * up as f64).collect();
    upfirdn(&h, x, up, down)
}

pub fn decimate(x: &[f64], q: usize, n: Option<usize>) -> DspResult<Vec<f64>> {
    if q == 0 {
        return Err(DspError::Param("q must be >= 1".into()));
    }
    if q == 1 {
        return Ok(x.to_vec());
    }
    let order = n.unwrap_or(8);
    let numtaps = (order * q).max(15) | 1;
    let h = firwin(numtaps, &[1.0 / q as f64], "hamming", true, 2.0)?;
    let y = convolve(x, &h, ConvMode::Same)?;
    Ok(y.iter().step_by(q).copied().collect())
}

#[allow(dead_code)]
fn _hamming_ref(m: usize) -> Vec<f64> {
    hamming(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_length() {
        let x: Vec<f64> = (0..100).map(|i| (i as f64 * 0.1).sin()).collect();
        let y = resample(&x, 50).unwrap();
        assert_eq!(y.len(), 50);
    }

    #[test]
    fn decimate_quarters() {
        let x: Vec<f64> = (0..40).map(|i| i as f64).collect();
        let y = decimate(&x, 4, Some(4)).unwrap();
        assert_eq!(y.len(), 10);
    }
}
