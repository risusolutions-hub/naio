//! Spectral analysis: STFT, spectrogram, Welch, periodogram.

use crate::error::{DspError, DspResult};
use crate::fft::{fft, ifft, Complex};
use crate::windows::{get_window, hann};

#[derive(Clone, Debug)]
pub struct StftResult {
    pub f: Vec<f64>,
    pub t: Vec<f64>,
    pub re: Vec<f64>,
    pub im: Vec<f64>,
    /// [n_freq, n_frames]
    pub shape: [usize; 2],
}

#[derive(Clone, Debug)]
pub struct SpecResult {
    pub f: Vec<f64>,
    pub t: Vec<f64>,
    pub sxx: Vec<f64>,
    pub shape: [usize; 2],
}

#[derive(Clone, Debug)]
pub struct PsdResult {
    pub f: Vec<f64>,
    pub pxx: Vec<f64>,
}

fn next_pow2(n: usize) -> usize {
    n.next_power_of_two().max(1)
}

pub struct SpectralOpts {
    pub fs: f64,
    pub window: String,
    pub nperseg: usize,
    pub noverlap: Option<usize>,
    pub nfft: Option<usize>,
}

impl Default for SpectralOpts {
    fn default() -> Self {
        Self {
            fs: 1.0,
            window: "hann".into(),
            nperseg: 256,
            noverlap: None,
            nfft: None,
        }
    }
}

fn frame_params(opts: &SpectralOpts, n: usize) -> DspResult<(usize, usize, usize, Vec<f64>)> {
    if opts.fs <= 0.0 {
        return Err(DspError::Param("fs must be > 0".into()));
    }
    let nperseg = opts.nperseg.min(n).max(1);
    let noverlap = opts.noverlap.unwrap_or(nperseg / 2);
    if noverlap >= nperseg {
        return Err(DspError::Param("noverlap must be < nperseg".into()));
    }
    let nfft = opts.nfft.unwrap_or(nperseg).max(nperseg);
    let nfft = next_pow2(nfft);
    let win = get_window(&opts.window, nperseg, true)?;
    Ok((nperseg, noverlap, nfft, win))
}

pub fn stft(x: &[f64], opts: &SpectralOpts) -> DspResult<StftResult> {
    if x.is_empty() {
        return Ok(StftResult {
            f: vec![],
            t: vec![],
            re: vec![],
            im: vec![],
            shape: [0, 0],
        });
    }
    let (nperseg, noverlap, nfft, win) = frame_params(opts, x.len())?;
    let step = nperseg - noverlap;
    let n_frames = if x.len() < nperseg {
        1
    } else {
        1 + (x.len() - nperseg) / step
    };
    let n_freq = nfft / 2 + 1;
    let mut re = vec![0.0; n_freq * n_frames];
    let mut im = vec![0.0; n_freq * n_frames];
    let mut t = Vec::with_capacity(n_frames);

    for frame in 0..n_frames {
        let start = frame * step;
        let mut buf = vec![Complex::default(); nfft];
        for i in 0..nperseg {
            let xi = if start + i < x.len() {
                x[start + i]
            } else {
                0.0
            };
            buf[i] = Complex::from_real(xi * win[i]);
        }
        let spectrum = fft(&buf);
        for k in 0..n_freq {
            let idx = k * n_frames + frame; // row-major freq-major
            re[idx] = spectrum[k].re;
            im[idx] = spectrum[k].im;
        }
        t.push((start + nperseg / 2) as f64 / opts.fs);
    }

    let f: Vec<f64> = (0..n_freq)
        .map(|k| k as f64 * opts.fs / nfft as f64)
        .collect();
    Ok(StftResult {
        f,
        t,
        re,
        im,
        shape: [n_freq, n_frames],
    })
}

pub fn istft(
    re: &[f64],
    im: &[f64],
    shape: [usize; 2],
    opts: &SpectralOpts,
) -> DspResult<Vec<f64>> {
    let (n_freq, n_frames) = (shape[0], shape[1]);
    if n_frames == 0 || n_freq == 0 {
        return Ok(vec![]);
    }
    if re.len() != n_freq * n_frames || im.len() != re.len() {
        return Err(DspError::Length("Zxx size does not match shape".into()));
    }
    let nperseg = opts.nperseg;
    let noverlap = opts.noverlap.unwrap_or(nperseg / 2);
    let step = nperseg - noverlap;
    let nfft = opts.nfft.unwrap_or(nperseg).max(nperseg);
    let nfft = next_pow2(nfft);
    let win = get_window(&opts.window, nperseg, true)?;
    let out_len = (n_frames - 1) * step + nperseg;
    let mut out = vec![0.0; out_len];
    let mut norm = vec![0.0; out_len];

    for frame in 0..n_frames {
        let mut buf = vec![Complex::default(); nfft];
        for k in 0..n_freq.min(nfft) {
            let idx = k * n_frames + frame;
            buf[k] = Complex::new(re[idx], im[idx]);
            if k > 0 && k < nfft - k {
                buf[nfft - k] = Complex::new(re[idx], -im[idx]);
            }
        }
        let y = ifft(&buf);
        let start = frame * step;
        for i in 0..nperseg {
            if start + i < out_len {
                out[start + i] += y[i].re * win[i];
                norm[start + i] += win[i] * win[i];
            }
        }
    }
    for i in 0..out_len {
        if norm[i] > 1e-12 {
            out[i] /= norm[i];
        }
    }
    Ok(out)
}

pub fn spectrogram(x: &[f64], opts: &SpectralOpts) -> DspResult<SpecResult> {
    let st = stft(x, opts)?;
    let n = st.shape[0] * st.shape[1];
    let mut sxx = vec![0.0; n];
    let scale = 1.0 / opts.fs;
    for i in 0..n {
        sxx[i] = (st.re[i] * st.re[i] + st.im[i] * st.im[i]) * scale;
    }
    Ok(SpecResult {
        f: st.f,
        t: st.t,
        sxx,
        shape: st.shape,
    })
}

pub fn periodogram(x: &[f64], opts: &SpectralOpts) -> DspResult<PsdResult> {
    if x.is_empty() {
        return Ok(PsdResult {
            f: vec![],
            pxx: vec![],
        });
    }
    let n = x.len();
    let nfft = opts.nfft.unwrap_or(n).max(n);
    let nfft = next_pow2(nfft);
    let win = if opts.window.is_empty() {
        hann(n)
    } else {
        get_window(&opts.window, n, true)?
    };
    let win_power: f64 = win.iter().map(|v| v * v).sum();
    let mut buf = vec![Complex::default(); nfft];
    for i in 0..n {
        buf[i] = Complex::from_real(x[i] * win[i]);
    }
    let spectrum = fft(&buf);
    let n_freq = nfft / 2 + 1;
    let scale = 1.0 / (opts.fs * win_power);
    let mut pxx = Vec::with_capacity(n_freq);
    for k in 0..n_freq {
        let mut p = spectrum[k].norm_sq() * scale;
        if k > 0 && k < nfft - k {
            p *= 2.0; // one-sided
        }
        pxx.push(p);
    }
    let f: Vec<f64> = (0..n_freq)
        .map(|k| k as f64 * opts.fs / nfft as f64)
        .collect();
    Ok(PsdResult { f, pxx })
}

pub fn welch(x: &[f64], opts: &SpectralOpts) -> DspResult<PsdResult> {
    if x.is_empty() {
        return Ok(PsdResult {
            f: vec![],
            pxx: vec![],
        });
    }
    let (nperseg, noverlap, nfft, win) = frame_params(opts, x.len())?;
    let step = nperseg - noverlap;
    let n_frames = if x.len() < nperseg {
        1
    } else {
        1 + (x.len() - nperseg) / step
    };
    let n_freq = nfft / 2 + 1;
    let win_power: f64 = win.iter().map(|v| v * v).sum();
    let mut acc = vec![0.0; n_freq];

    for frame in 0..n_frames {
        let start = frame * step;
        let mut buf = vec![Complex::default(); nfft];
        for i in 0..nperseg {
            let xi = if start + i < x.len() {
                x[start + i]
            } else {
                0.0
            };
            buf[i] = Complex::from_real(xi * win[i]);
        }
        let spectrum = fft(&buf);
        let scale = 1.0 / (opts.fs * win_power);
        for k in 0..n_freq {
            let mut p = spectrum[k].norm_sq() * scale;
            if k > 0 && k < nfft - k {
                p *= 2.0;
            }
            acc[k] += p;
        }
    }
    let inv = 1.0 / n_frames as f64;
    for v in &mut acc {
        *v *= inv;
    }
    let f: Vec<f64> = (0..n_freq)
        .map(|k| k as f64 * opts.fs / nfft as f64)
        .collect();
    Ok(PsdResult { f, pxx: acc })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn periodogram_sine_peak() {
        let fs = 128.0;
        let f0 = 16.0;
        let x: Vec<f64> = (0..256)
            .map(|i| (2.0 * std::f64::consts::PI * f0 * i as f64 / fs).sin())
            .collect();
        let opts = SpectralOpts {
            fs,
            window: "hann".into(),
            nperseg: 256,
            noverlap: None,
            nfft: Some(256),
        };
        let p = periodogram(&x, &opts).unwrap();
        let (imax, _) = p
            .pxx
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap();
        assert!((p.f[imax] - f0).abs() < fs / 256.0 * 1.5);
    }
}
