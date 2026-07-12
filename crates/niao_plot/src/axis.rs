//! Axis engine: nice ticks, autoscale, linear/log transforms.

use crate::error::{PlotError, PlotResult};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scale {
    Linear,
    Log,
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub min: f64,
    pub max: f64,
}

impl Limits {
    pub fn new(min: f64, max: f64) -> Self {
        Self { min, max }
    }

    pub fn span(&self) -> f64 {
        (self.max - self.min).max(1e-12)
    }
}

/// Autoscale with 5% margin.
pub fn autoscale(data: &[f64]) -> PlotResult<Limits> {
    if data.is_empty() {
        return Err(PlotError::Empty("autoscale: empty data".into()));
    }
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for &v in data {
        if v.is_finite() {
            min = min.min(v);
            max = max.max(v);
        }
    }
    if !min.is_finite() || !max.is_finite() {
        return Ok(Limits::new(0.0, 1.0));
    }
    if (max - min).abs() < 1e-12 {
        let pad = max.abs().max(1.0) * 0.05;
        return Ok(Limits::new(min - pad, max + pad));
    }
    let margin = (max - min) * 0.05;
    Ok(Limits::new(min - margin, max + margin))
}

pub fn merge_limits(a: Limits, b: Limits) -> Limits {
    Limits::new(a.min.min(b.min), a.max.max(b.max))
}

/// "Nice" tick selection: 1/2/5 × 10^n (Wilkinson-style simplified).
pub fn nice_ticks(min: f64, max: f64, target: usize) -> Vec<f64> {
    if !min.is_finite() || !max.is_finite() || min >= max {
        return vec![0.0, 1.0];
    }
    let span = max - min;
    let raw_step = span / target.max(1) as f64;
    let mag = 10f64.powf(raw_step.log10().floor());
    let norm = raw_step / mag;
    let step = if norm < 1.5 {
        mag
    } else if norm < 3.0 {
        2.0 * mag
    } else if norm < 7.0 {
        5.0 * mag
    } else {
        10.0 * mag
    };
    let start = (min / step).floor() * step;
    let mut ticks = Vec::new();
    let mut v = start;
    while v <= max + step * 0.5 {
        if v >= min - step * 0.5 {
            ticks.push(round_tick(v));
        }
        v += step;
    }
    if ticks.is_empty() {
        ticks.push(min);
        ticks.push(max);
    }
    ticks
}

fn round_tick(v: f64) -> f64 {
    (v * 1_000_000.0).round() / 1_000_000.0
}

#[derive(Clone, Debug)]
pub struct Transform {
    pub limits: Limits,
    pub scale: Scale,
    pub plot_left: f64,
    pub plot_top: f64,
    pub plot_width: f64,
    pub plot_height: f64,
}

impl Transform {
    pub fn new(limits: Limits, scale: Scale, left: f64, top: f64, width: f64, height: f64) -> Self {
        Self {
            limits,
            scale,
            plot_left: left,
            plot_top: top,
            plot_width: width,
            plot_height: height,
        }
    }

    pub fn data_to_px_x(&self, x: f64) -> f64 {
        let t = self.norm_x(x);
        self.plot_left + t * self.plot_width
    }

    pub fn data_to_px_y(&self, y: f64) -> f64 {
        let t = self.norm_y(y);
        self.plot_top + (1.0 - t) * self.plot_height
    }

    fn norm_x(&self, x: f64) -> f64 {
        match self.scale {
            Scale::Linear => (x - self.limits.min) / self.limits.span(),
            Scale::Log => {
                let lx = x.max(1e-300).ln();
                let lmin = self.limits.min.max(1e-300).ln();
                let lmax = self.limits.max.max(1e-300).ln();
                (lx - lmin) / (lmax - lmin).max(1e-12)
            }
        }
    }

    fn norm_y(&self, y: f64) -> f64 {
        self.norm_x(y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nice_ticks_fixture() {
        let ticks = nice_ticks(0.0, 10.0, 5);
        assert!(ticks.first().unwrap() <= &0.0);
        assert!(ticks.last().unwrap() >= &10.0);
        assert!(ticks.windows(2).all(|w| w[1] > w[0]));
    }

    #[test]
    fn autoscale_margin() {
        let lim = autoscale(&[1.0, 2.0, 3.0, 4.0]).unwrap();
        assert!(lim.min < 1.0);
        assert!(lim.max > 4.0);
    }

    #[test]
    fn log_transform_px() {
        let tr = Transform::new(Limits::new(1.0, 100.0), Scale::Log, 50.0, 50.0, 300.0, 200.0);
        let px1 = tr.data_to_px_x(1.0);
        let px10 = tr.data_to_px_x(10.0);
        let px100 = tr.data_to_px_x(100.0);
        assert!((px1 - 50.0).abs() < 1e-6);
        assert!(px10 > px1 && px10 < px100);
        assert!((px100 - 350.0).abs() < 1e-6);
    }
}
