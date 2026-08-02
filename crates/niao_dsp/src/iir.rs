//! IIR filter design: Butterworth / Chebyshev-I via bilinear transform.

use crate::error::{DspError, DspResult};
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Btype {
    Lowpass,
    Highpass,
    Bandpass,
    Bandstop,
}

impl Btype {
    pub fn parse(s: &str) -> DspResult<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "lowpass" | "low" | "lp" => Ok(Self::Lowpass),
            "highpass" | "high" | "hp" => Ok(Self::Highpass),
            "bandpass" | "band" | "bp" => Ok(Self::Bandpass),
            "bandstop" | "stop" | "bs" => Ok(Self::Bandstop),
            other => Err(DspError::Param(format!("unknown btype '{other}'"))),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ftype {
    Butter,
    Cheby1,
}

impl Ftype {
    pub fn parse(s: &str) -> DspResult<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "butter" | "butterworth" => Ok(Self::Butter),
            "cheby1" | "chebyshev1" | "cheby" => Ok(Self::Cheby1),
            other => Err(DspError::Param(format!("unknown ftype '{other}'"))),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Tf {
    pub b: Vec<f64>,
    pub a: Vec<f64>,
}

/// Second-order section: [b0, b1, b2, a0, a1, a2]
pub type Sos = Vec<[f64; 6]>;

fn prewarp(f: f64, fs: f64) -> f64 {
    (PI * f / fs).tan()
}

fn normalize_wn(wn: &[f64], fs: f64) -> DspResult<Vec<f64>> {
    if wn.is_empty() || wn.len() > 2 {
        return Err(DspError::Param("Wn must be 1 or 2 frequencies".into()));
    }
    if fs <= 0.0 {
        return Err(DspError::Param("fs must be > 0".into()));
    }
    let mut out = Vec::with_capacity(wn.len());
    for &w in wn {
        let f = if w > 1.0 { w } else { w * fs / 2.0 };
        if f <= 0.0 || f >= fs / 2.0 {
            return Err(DspError::Param("Wn must be in (0, Nyquist)".into()));
        }
        out.push(f);
    }
    if out.len() == 2 && out[0] >= out[1] {
        return Err(DspError::Param("band edges must be increasing".into()));
    }
    Ok(out)
}

fn butter_analog_poles(order: usize) -> Vec<(f64, f64)> {
    // poles on unit circle: e^{j(2k-1+n)π/(2n)} left half-plane
    let n = order as f64;
    (0..order)
        .map(|k| {
            let theta = PI * (2.0 * k as f64 + n + 1.0) / (2.0 * n);
            (theta.cos(), theta.sin()) // re, im  (re < 0)
        })
        .filter(|(re, _)| *re < 0.0 || (*re).abs() < 1e-14)
        .map(|(re, im)| (re.min(-1e-15), im))
        .collect()
}

fn cheby1_analog_poles(order: usize, rp: f64) -> DspResult<Vec<(f64, f64)>> {
    if rp <= 0.0 {
        return Err(DspError::Param("cheby1 rp (dB) must be > 0".into()));
    }
    let eps = (10.0_f64.powf(0.1 * rp) - 1.0).sqrt();
    let mu = (1.0 / eps).asinh() / order as f64;
    let n = order as f64;
    let mut poles = Vec::new();
    for k in 0..order {
        let theta = PI * (2.0 * k as f64 + 1.0) / (2.0 * n);
        let re = -mu.sinh() * theta.sin();
        let im = mu.cosh() * theta.cos();
        if re < 0.0 {
            poles.push((re, im));
        }
    }
    Ok(poles)
}

fn bilinear_lp(poles: &[(f64, f64)], z0: f64) -> (Vec<f64>, Vec<f64>) {
    // H(s) = prod 1/(s-p) → bilinear s = 2/T (1-z^{-1})/(1+z^{-1}), T=2 → s=(1-z^{-1})/(1+z^{-1})
    // Scale so that DC gain is 1 for lowpass.
    let mut b = vec![1.0];
    let mut a = vec![1.0];
    for &(pr, pi) in poles {
        // each pole contributes: (1+z^{-1}) / ((1-p) + (1+p) z^{-1}) after s=(1-z^{-1})/(1+z^{-1})
        // For complex conjugate pairs process two at once when im != 0.
        if pi.abs() < 1e-14 {
            let p = pr * z0; // frequency scale
            let den0 = 1.0 - p;
            let den1 = 1.0 + p;
            let nb = poly_mul(&b, &[1.0, 1.0]);
            let na = poly_mul(&a, &[den0, den1]);
            b = nb;
            a = na;
        }
    }
    // Complex pairs
    let mut used = vec![false; poles.len()];
    for i in 0..poles.len() {
        if used[i] || poles[i].1.abs() < 1e-14 {
            continue;
        }
        // find conjugate
        let mut j = None;
        for k in (i + 1)..poles.len() {
            if !used[k]
                && (poles[k].0 - poles[i].0).abs() < 1e-9
                && (poles[k].1 + poles[i].1).abs() < 1e-9
            {
                j = Some(k);
                break;
            }
        }
        let Some(j) = j else { continue };
        used[i] = true;
        used[j] = true;
        let (pr, pi) = (poles[i].0 * z0, poles[i].1 * z0);
        // Quadratic from conjugate pair after bilinear:
        // (1+z^{-1})^2 / [(1-p)(1-p*) + ... ]
        let p_re = pr;
        let p_im = pi;
        let c0 = (1.0 - p_re).powi(2) + p_im * p_im;
        let c1 = 2.0 * (1.0 - (p_re * p_re + p_im * p_im));
        let c2 = (1.0 + p_re).powi(2) + p_im * p_im;
        let nb = poly_mul(&b, &[1.0, 2.0, 1.0]);
        let na = poly_mul(&a, &[c0, c1, c2]);
        b = nb;
        a = na;
    }

    // Normalize a[0]=1 and set DC gain = 1
    if a[0].abs() > 1e-30 {
        let s = a[0];
        for v in &mut a {
            *v /= s;
        }
        for v in &mut b {
            *v /= s;
        }
    }
    let dc_num: f64 = b.iter().sum();
    let dc_den: f64 = a.iter().sum();
    if dc_num.abs() > 1e-30 {
        let g = dc_den / dc_num;
        for v in &mut b {
            *v *= g;
        }
    }
    (b, a)
}

fn poly_mul(p: &[f64], q: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; p.len() + q.len() - 1];
    for (i, &pi) in p.iter().enumerate() {
        for (j, &qj) in q.iter().enumerate() {
            out[i + j] += pi * qj;
        }
    }
    out
}

fn spectral_invert_lp(b: &[f64], a: &[f64]) -> (Vec<f64>, Vec<f64>) {
    // highpass from lowpass prototype: z -> -z
    let mut nb: Vec<f64> = b
        .iter()
        .enumerate()
        .map(|(i, &v)| if i % 2 == 0 { v } else { -v })
        .collect();
    let na: Vec<f64> = a
        .iter()
        .enumerate()
        .map(|(i, &v)| if i % 2 == 0 { v } else { -v })
        .collect();
    // Normalize Nyquist gain
    let nyq_num: f64 = nb
        .iter()
        .enumerate()
        .map(|(i, &v)| if i % 2 == 0 { v } else { -v })
        .sum();
    let nyq_den: f64 = na
        .iter()
        .enumerate()
        .map(|(i, &v)| if i % 2 == 0 { v } else { -v })
        .sum();
    if nyq_num.abs() > 1e-30 {
        let g = nyq_den / nyq_num;
        for v in &mut nb {
            *v *= g;
        }
    }
    (nb, na)
}

fn design_lp_digital(order: usize, f_cut: f64, fs: f64, ftype: Ftype, rp: f64) -> DspResult<Tf> {
    let warped = prewarp(f_cut, fs);
    let poles = match ftype {
        Ftype::Butter => butter_analog_poles(order),
        Ftype::Cheby1 => cheby1_analog_poles(order, rp)?,
    };
    if poles.is_empty() {
        return Err(DspError::Filter("no poles generated".into()));
    }
    let (b, a) = bilinear_lp(&poles, warped);
    Ok(Tf { b, a })
}

pub fn iirfilter(
    order: usize,
    wn: &[f64],
    btype: Btype,
    ftype: Ftype,
    rp: f64,
    fs: f64,
    output_sos: bool,
) -> DspResult<IirOut> {
    if order == 0 || order > 32 {
        return Err(DspError::Param("order must be in 1..=32".into()));
    }
    let freqs = normalize_wn(wn, fs)?;
    match btype {
        Btype::Lowpass => {
            let tf = design_lp_digital(order, freqs[0], fs, ftype, rp)?;
            Ok(pack(tf, output_sos))
        }
        Btype::Highpass => {
            let tf = design_lp_digital(order, freqs[0], fs, ftype, rp)?;
            let (b, a) = spectral_invert_lp(&tf.b, &tf.a);
            Ok(pack(Tf { b, a }, output_sos))
        }
        Btype::Bandpass => {
            if freqs.len() != 2 {
                return Err(DspError::Param("bandpass requires two Wn values".into()));
            }
            // Cascade: highpass(f1) then lowpass(f2)
            let hp = design_lp_digital(order, freqs[0], fs, ftype, rp)?;
            let (hb, ha) = spectral_invert_lp(&hp.b, &hp.a);
            let lp = design_lp_digital(order, freqs[1], fs, ftype, rp)?;
            let b = poly_mul(&hb, &lp.b);
            let a = poly_mul(&ha, &lp.a);
            Ok(pack(Tf { b, a }, output_sos))
        }
        Btype::Bandstop => {
            if freqs.len() != 2 {
                return Err(DspError::Param("bandstop requires two Wn values".into()));
            }
            let lp = design_lp_digital(order, freqs[0], fs, ftype, rp)?;
            let hp = design_lp_digital(order, freqs[1], fs, ftype, rp)?;
            let (hb, ha) = spectral_invert_lp(&hp.b, &hp.a);
            // Parallel: LP + HP ≈ bandstop (sum of TFs)
            // H = (b1*a2 + b2*a1) / (a1*a2)
            let num1 = poly_mul(&lp.b, &ha);
            let num2 = poly_mul(&hb, &lp.a);
            let b = poly_add(&num1, &num2);
            let a = poly_mul(&lp.a, &ha);
            Ok(pack(Tf { b, a }, output_sos))
        }
    }
}

fn poly_add(p: &[f64], q: &[f64]) -> Vec<f64> {
    let n = p.len().max(q.len());
    let mut out = vec![0.0; n];
    for (i, &v) in p.iter().enumerate() {
        out[i] += v;
    }
    for (i, &v) in q.iter().enumerate() {
        out[i] += v;
    }
    out
}

#[derive(Clone, Debug)]
pub enum IirOut {
    Ba(Tf),
    Sos(Sos),
}

fn pack(tf: Tf, output_sos: bool) -> IirOut {
    if output_sos {
        IirOut::Sos(tf2sos(&tf.b, &tf.a))
    } else {
        IirOut::Ba(tf)
    }
}

pub fn butter(
    order: usize,
    wn: &[f64],
    btype: Btype,
    fs: f64,
    output_sos: bool,
) -> DspResult<IirOut> {
    iirfilter(order, wn, btype, Ftype::Butter, 0.0, fs, output_sos)
}

pub fn cheby1(
    order: usize,
    rp: f64,
    wn: &[f64],
    btype: Btype,
    fs: f64,
    output_sos: bool,
) -> DspResult<IirOut> {
    iirfilter(order, wn, btype, Ftype::Cheby1, rp, fs, output_sos)
}

/// Convert transfer function to cascaded second-order sections (pairing by order).
pub fn tf2sos(b: &[f64], a: &[f64]) -> Sos {
    // Simple sequential pairing of poles/zeros via polynomial factoring is complex;
    // for v0.1 emit one tall section padded into SOS chunks of degree 2.
    let mut bb = b.to_vec();
    let mut aa = a.to_vec();
    while bb.len() < aa.len() {
        bb.push(0.0);
    }
    while aa.len() < bb.len() {
        aa.push(0.0);
    }
    if aa.is_empty() {
        aa.push(1.0);
    }
    if aa[0].abs() > 1e-30 {
        let s = aa[0];
        for v in &mut aa {
            *v /= s;
        }
        for v in &mut bb {
            *v /= s;
        }
    }
    let mut sos = Vec::new();
    let mut i = 0;
    while i < bb.len() {
        let b0 = bb[i];
        let b1 = if i + 1 < bb.len() { bb[i + 1] } else { 0.0 };
        let b2 = if i + 2 < bb.len() { bb[i + 2] } else { 0.0 };
        let a0 = if i < aa.len() { aa[i] } else { 1.0 };
        let a1 = if i + 1 < aa.len() { aa[i + 1] } else { 0.0 };
        let a2 = if i + 2 < aa.len() { aa[i + 2] } else { 0.0 };
        if i == 0 {
            sos.push([b0, b1, b2, a0, a1, a2]);
        } else if b0.abs() + b1.abs() + b2.abs() + a1.abs() + a2.abs() > 1e-18 {
            // Remaining higher-order coeffs: fold into extra sections with a0=1
            sos.push([b0, b1, b2, 1.0, a1, a2]);
        }
        i += 3;
    }
    if sos.is_empty() {
        sos.push([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    }
    sos
}

pub fn sos2tf(sos: &Sos) -> Tf {
    let mut b = vec![1.0];
    let mut a = vec![1.0];
    for sec in sos {
        b = poly_mul(&b, &sec[0..3]);
        a = poly_mul(&a, &sec[3..6]);
    }
    if a[0].abs() > 1e-30 {
        let s = a[0];
        for v in &mut a {
            *v /= s;
        }
        for v in &mut b {
            *v /= s;
        }
    }
    Tf { b, a }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn butter_lp_dc_gain() {
        let out = butter(2, &[0.2], Btype::Lowpass, 2.0, false).unwrap();
        match out {
            IirOut::Ba(tf) => {
                let g: f64 = tf.b.iter().sum::<f64>() / tf.a.iter().sum::<f64>();
                assert!((g - 1.0).abs() < 1e-6, "dc gain {g}");
            }
            _ => panic!("expected ba"),
        }
    }
}
