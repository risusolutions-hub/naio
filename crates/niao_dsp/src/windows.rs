//! Window functions (~scipy.signal.windows).

use crate::error::{DspError, DspResult};
use std::f64::consts::PI;

pub fn boxcar(m: usize) -> Vec<f64> {
    vec![1.0; m]
}

pub fn hann(m: usize) -> Vec<f64> {
    if m == 0 {
        return vec![];
    }
    if m == 1 {
        return vec![1.0];
    }
    (0..m)
        .map(|n| 0.5 - 0.5 * (2.0 * PI * n as f64 / (m - 1) as f64).cos())
        .collect()
}

pub fn hamming(m: usize) -> Vec<f64> {
    if m == 0 {
        return vec![];
    }
    if m == 1 {
        return vec![1.0];
    }
    (0..m)
        .map(|n| 0.54 - 0.46 * (2.0 * PI * n as f64 / (m - 1) as f64).cos())
        .collect()
}

pub fn blackman(m: usize) -> Vec<f64> {
    if m == 0 {
        return vec![];
    }
    if m == 1 {
        return vec![1.0];
    }
    let den = (m - 1) as f64;
    (0..m)
        .map(|n| {
            let x = 2.0 * PI * n as f64 / den;
            0.42 - 0.5 * x.cos() + 0.08 * (2.0 * x).cos()
        })
        .collect()
}

pub fn bartlett(m: usize) -> Vec<f64> {
    if m == 0 {
        return vec![];
    }
    if m == 1 {
        return vec![1.0];
    }
    let den = (m - 1) as f64;
    (0..m)
        .map(|n| {
            let t = 2.0 * n as f64 / den;
            if n as f64 <= den / 2.0 {
                t
            } else {
                2.0 - t
            }
        })
        .collect()
}

pub fn kaiser(m: usize, beta: f64) -> DspResult<Vec<f64>> {
    if beta < 0.0 {
        return Err(DspError::Param("kaiser beta must be >= 0".into()));
    }
    if m == 0 {
        return Ok(vec![]);
    }
    if m == 1 {
        return Ok(vec![1.0]);
    }
    let i0_beta = bessel_i0(beta);
    let den = (m - 1) as f64 / 2.0;
    Ok((0..m)
        .map(|n| {
            let t = (n as f64 - den) / den;
            bessel_i0(beta * (1.0 - t * t).max(0.0).sqrt()) / i0_beta
        })
        .collect())
}

pub fn tukey(m: usize, alpha: f64) -> DspResult<Vec<f64>> {
    if !(0.0..=1.0).contains(&alpha) {
        return Err(DspError::Param("tukey alpha must be in [0, 1]".into()));
    }
    if m == 0 {
        return Ok(vec![]);
    }
    if m == 1 || alpha == 0.0 {
        return Ok(vec![1.0; m]);
    }
    if (alpha - 1.0).abs() < 1e-15 {
        return Ok(hann(m));
    }
    let width = (alpha * (m - 1) as f64 / 2.0).floor() as usize;
    let mut w = vec![1.0; m];
    for n in 0..width {
        let x = PI * n as f64 / (alpha * (m - 1) as f64 / 2.0);
        w[n] = 0.5 * (1.0 + x.cos());
        w[m - 1 - n] = w[n];
    }
    Ok(w)
}

/// Modified Bessel function I0 (series).
fn bessel_i0(x: f64) -> f64 {
    let ax = x.abs();
    if ax < 3.75 {
        let y = (x / 3.75).powi(2);
        1.0 + y
            * (3.5156229
                + y * (3.0899424
                    + y * (1.2067492 + y * (0.2659732 + y * (0.0360768 + y * 0.0045813)))))
    } else {
        let y = 3.75 / ax;
        (ax.exp() / ax.sqrt())
            * (0.39894228
                + y * (0.01328592
                    + y * (0.00225319
                        + y * (-0.00157565
                            + y * (0.00916281
                                + y * (-0.02057706
                                    + y * (0.02635537 + y * (-0.01647633 + y * 0.00392377))))))))
    }
}

pub fn get_window(name: &str, nx: usize, _fftbins: bool) -> DspResult<Vec<f64>> {
    if nx == 0 {
        return Ok(vec![]);
    }
    let n = name.trim().to_ascii_lowercase();
    match n.as_str() {
        "boxcar" | "rect" | "rectangular" => Ok(boxcar(nx)),
        "hann" | "hanning" => Ok(hann(nx)),
        "hamming" => Ok(hamming(nx)),
        "blackman" => Ok(blackman(nx)),
        "bartlett" | "triang" | "triangular" => Ok(bartlett(nx)),
        "kaiser" => kaiser(nx, 8.6),
        "tukey" => tukey(nx, 0.5),
        other => Err(DspError::Param(format!("unknown window '{other}'"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hann_symmetric_peak() {
        let w = hann(5);
        assert!((w[2] - 1.0).abs() < 1e-12);
        assert!((w[0] - w[4]).abs() < 1e-12);
    }
}
