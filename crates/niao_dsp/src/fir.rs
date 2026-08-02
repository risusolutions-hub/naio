//! FIR filter design (window method).

use crate::error::{DspError, DspResult};
use crate::windows::{get_window, kaiser};
use std::f64::consts::PI;

fn normalize_cutoff(cutoff: f64, fs: f64) -> DspResult<f64> {
    if fs <= 0.0 {
        return Err(DspError::Param("fs must be > 0".into()));
    }
    let nyq = fs / 2.0;
    let fc = if cutoff > 1.0 { cutoff / nyq } else { cutoff };
    if !(0.0..=1.0).contains(&fc) {
        return Err(DspError::Param(
            "cutoff must be in (0, 1] relative to Nyquist (or Hz with fs)".into(),
        ));
    }
    Ok(fc)
}

/// Design FIR low/high/band-pass via windowed-sinc (`firwin`).
///
/// `cutoff` is a single frequency (low/high) or pair `[f1, f2]` encoded as
/// two values via `cutoffs` slice length 1 or 2.
pub fn firwin(
    numtaps: usize,
    cutoffs: &[f64],
    window: &str,
    pass_zero: bool,
    fs: f64,
) -> DspResult<Vec<f64>> {
    if numtaps == 0 {
        return Err(DspError::Param("numtaps must be > 0".into()));
    }
    if cutoffs.is_empty() || cutoffs.len() > 2 {
        return Err(DspError::Param(
            "cutoff must be one frequency or a band [lo, hi]".into(),
        ));
    }
    let mut fc: Vec<f64> = Vec::with_capacity(cutoffs.len());
    for &c in cutoffs {
        let n = normalize_cutoff(c, fs)?;
        if n <= 0.0 || n >= 1.0 {
            return Err(DspError::Param(
                "cutoff must be strictly between 0 and Nyquist".into(),
            ));
        }
        fc.push(n);
    }
    if fc.len() == 2 && fc[0] >= fc[1] {
        return Err(DspError::Param("band edges must be increasing".into()));
    }

    let win = if window.eq_ignore_ascii_case("kaiser") {
        kaiser(numtaps, 8.6)?
    } else {
        get_window(window, numtaps, true)?
    };

    let m = (numtaps - 1) as f64 / 2.0;
    let mut h = vec![0.0; numtaps];

    if fc.len() == 1 {
        let f = fc[0];
        for (n, hn) in h.iter_mut().enumerate() {
            let x = n as f64 - m;
            let sinc = if x.abs() < 1e-15 {
                2.0 * f
            } else {
                (2.0 * f * PI * x).sin() / (PI * x)
            };
            *hn = if pass_zero {
                sinc
            } else {
                // high-pass: spectral inversion
                if x.abs() < 1e-15 {
                    1.0 - sinc
                } else {
                    -sinc
                }
            };
            *hn *= win[n];
        }
    } else {
        // Band-pass (pass_zero=false) or band-stop (pass_zero=true)
        let (f1, f2) = (fc[0], fc[1]);
        for (n, hn) in h.iter_mut().enumerate() {
            let x = n as f64 - m;
            let s2 = if x.abs() < 1e-15 {
                2.0 * f2
            } else {
                (2.0 * f2 * PI * x).sin() / (PI * x)
            };
            let s1 = if x.abs() < 1e-15 {
                2.0 * f1
            } else {
                (2.0 * f1 * PI * x).sin() / (PI * x)
            };
            let band = s2 - s1;
            *hn = if pass_zero {
                // band-stop
                if x.abs() < 1e-15 {
                    1.0 - band
                } else {
                    -band
                }
            } else {
                band
            };
            *hn *= win[n];
        }
    }

    // Unity gain in passband (DC for lowpass/bandstop).
    let gain = if pass_zero || fc.len() == 1 && pass_zero {
        h.iter().sum::<f64>()
    } else if fc.len() == 1 && !pass_zero {
        // high-pass: gain at Nyquist ≈ sum (-1)^n h[n]
        h.iter()
            .enumerate()
            .map(|(i, &v)| if i % 2 == 0 { v } else { -v })
            .sum::<f64>()
    } else {
        // band-pass: approximate mid-band gain via sum h * cos(2π f_mid n)
        let fmid = (fc[0] + fc[1]) / 2.0;
        h.iter()
            .enumerate()
            .map(|(n, &v)| v * (2.0 * PI * fmid * (n as f64 - m)).cos())
            .sum::<f64>()
    };
    if gain.abs() > 1e-15 {
        for v in &mut h {
            *v /= gain;
        }
    }
    Ok(h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firwin_lowpass_sums_near_one() {
        let h = firwin(21, &[0.2], "hamming", true, 2.0).unwrap();
        let s: f64 = h.iter().sum();
        assert!((s - 1.0).abs() < 1e-9);
    }
}
