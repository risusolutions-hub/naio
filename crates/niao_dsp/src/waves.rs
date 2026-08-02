//! Waveform generators: chirp, sawtooth, square, gausspulse.

use crate::error::{DspError, DspResult};
use std::f64::consts::PI;

pub fn chirp(t: &[f64], f0: f64, t1: f64, f1: f64, method: &str) -> DspResult<Vec<f64>> {
    if t1 == 0.0 {
        return Err(DspError::Param("t1 must be non-zero".into()));
    }
    let method = method.trim().to_ascii_lowercase();
    match method.as_str() {
        "linear" | "" => Ok(t
            .iter()
            .map(|&ti| {
                let k = (f1 - f0) / t1;
                (2.0 * PI * (f0 * ti + 0.5 * k * ti * ti)).sin()
            })
            .collect()),
        "quadratic" => Ok(t
            .iter()
            .map(|&ti| {
                let k = (f1 - f0) / (t1 * t1);
                (2.0 * PI * (f0 * ti + k * ti * ti * ti / 3.0)).sin()
            })
            .collect()),
        "logarithmic" => {
            if f0 <= 0.0 || f1 <= 0.0 {
                return Err(DspError::Param(
                    "logarithmic chirp requires f0 > 0 and f1 > 0".into(),
                ));
            }
            let k = (f1 / f0).ln() / t1;
            Ok(t.iter()
                .map(|&ti| {
                    let phase = 2.0 * PI * f0 * ((k * ti).exp() - 1.0) / k;
                    phase.sin()
                })
                .collect())
        }
        other => Err(DspError::Param(format!("unknown chirp method '{other}'"))),
    }
}

pub fn sawtooth(t: &[f64], width: f64) -> DspResult<Vec<f64>> {
    if !(0.0..=1.0).contains(&width) {
        return Err(DspError::Param("width must be in [0, 1]".into()));
    }
    Ok(t.iter()
        .map(|&ti| {
            let mut x = ti / (2.0 * PI);
            x -= x.floor();
            if x < width {
                if width.abs() < 1e-15 {
                    1.0
                } else {
                    2.0 * x / width - 1.0
                }
            } else if (1.0 - width).abs() < 1e-15 {
                -1.0
            } else {
                -2.0 * (x - width) / (1.0 - width) + 1.0
            }
        })
        .collect())
}

pub fn square(t: &[f64], duty: f64) -> DspResult<Vec<f64>> {
    if !(0.0..=1.0).contains(&duty) {
        return Err(DspError::Param("duty must be in [0, 1]".into()));
    }
    Ok(t.iter()
        .map(|&ti| {
            let mut x = ti / (2.0 * PI);
            x -= x.floor();
            if x < duty {
                1.0
            } else {
                -1.0
            }
        })
        .collect())
}

pub fn gausspulse(t: &[f64], fc: f64, bw: f64) -> DspResult<Vec<f64>> {
    if fc <= 0.0 {
        return Err(DspError::Param("fc must be > 0".into()));
    }
    if bw <= 0.0 || bw >= 1.0 {
        return Err(DspError::Param("bw must be in (0, 1)".into()));
    }
    let bwr = -6.0; // dB
    let ref_level = 10.0_f64.powf(bwr / 20.0);
    let a = -(PI * fc * bw).powi(2) / (4.0 * ref_level.ln());
    Ok(t.iter()
        .map(|&ti| (-a * ti * ti).exp() * (2.0 * PI * fc * ti).cos())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_duty() {
        let t = [0.0, PI * 0.25, PI, PI * 1.5];
        let y = square(&t, 0.5).unwrap();
        assert_eq!(y[0], 1.0);
        assert_eq!(y[2], -1.0);
    }
}
