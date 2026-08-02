//! Audio playback via cpal.

use crate::error::{TtsError, TtsResult};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat, StreamConfig};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Play mono f32 PCM samples on the default output device (blocking until done).
pub fn play_samples(samples: &[f32], sample_rate: u32) -> TtsResult<()> {
    if samples.is_empty() {
        return Ok(());
    }
    if sample_rate == 0 {
        return Err(TtsError::Audio("sample rate must be positive".into()));
    }

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| TtsError::Audio("no default output device".into()))?;
    let config = pick_output_config(&device)?;
    let out_rate = config.config.sample_rate.0;
    let channels = config.config.channels as usize;

    let pcm: Vec<f32> = if out_rate == sample_rate {
        samples.to_vec()
    } else {
        resample_linear(samples, sample_rate, out_rate)
    };

    let pos: Arc<Mutex<usize>> = Arc::new(Mutex::new(0));
    let done: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
    let pos_cb = Arc::clone(&pos);
    let done_cb = Arc::clone(&done);
    let pcm_cb = pcm.clone();
    let frames = pcm.len();

    let stream = match config.sample_format {
        SampleFormat::F32 => device.build_output_stream(
            &config.config,
            move |out: &mut [f32], _| write_output(out, &pcm_cb, channels, &pos_cb, &done_cb),
            |e| eprintln!("ntts playback error: {e}"),
            None,
        ),
        SampleFormat::I16 => device.build_output_stream(
            &config.config,
            move |out: &mut [i16], _| {
                let mut fbuf = vec![0.0f32; out.len()];
                write_output(&mut fbuf, &pcm_cb, channels, &pos_cb, &done_cb);
                for (o, f) in out.iter_mut().zip(fbuf.iter()) {
                    *o = f.to_sample();
                }
            },
            |e| eprintln!("ntts playback error: {e}"),
            None,
        ),
        other => {
            return Err(TtsError::Audio(format!(
                "unsupported output sample format: {other:?}"
            )));
        }
    }
    .map_err(|e| TtsError::Audio(e.to_string()))?;

    stream.play().map_err(|e| TtsError::Audio(e.to_string()))?;

    let timeout = Duration::from_secs_f64(frames as f64 / out_rate as f64 + 2.0);
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if *done
            .lock()
            .map_err(|_| TtsError::Audio("lock poisoned".into()))?
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }

    drop(stream);
    Ok(())
}

struct OutputConfig {
    config: StreamConfig,
    sample_format: SampleFormat,
}

fn pick_output_config(device: &cpal::Device) -> TtsResult<OutputConfig> {
    let mut configs = device
        .supported_output_configs()
        .map_err(|e| TtsError::Audio(e.to_string()))?;
    let cfg = configs
        .next()
        .ok_or_else(|| TtsError::Audio("no supported output config".into()))?;
    let sample_format = cfg.sample_format();
    let config = cfg.with_max_sample_rate().config();
    Ok(OutputConfig {
        config,
        sample_format,
    })
}

fn write_output(
    out: &mut [f32],
    pcm: &[f32],
    channels: usize,
    pos: &Mutex<usize>,
    done: &Mutex<bool>,
) {
    let mut p = pos.lock().unwrap();
    let ch = channels.max(1);
    let frame_count = out.len() / ch;
    for frame in 0..frame_count {
        let sample = if *p < pcm.len() {
            let s = pcm[*p];
            *p += 1;
            s
        } else {
            if let Ok(mut d) = done.lock() {
                *d = true;
            }
            0.0
        };
        for c in 0..ch {
            out[frame * ch + c] = sample;
        }
    }
}

fn resample_linear(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if samples.is_empty() || from_rate == 0 || to_rate == 0 {
        return Vec::new();
    }
    if from_rate == to_rate {
        return samples.to_vec();
    }
    let out_len = ((samples.len() as u64) * to_rate as u64 / from_rate as u64) as usize;
    let mut out = Vec::with_capacity(out_len.max(1));
    let ratio = from_rate as f64 / to_rate as f64;
    for i in 0..out_len.max(1) {
        let src = i as f64 * ratio;
        let i0 = src.floor() as usize;
        let i1 = (i0 + 1).min(samples.len() - 1);
        let frac = (src - i0 as f64) as f32;
        out.push(samples[i0] * (1.0 - frac) + samples[i1] * frac);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_same() {
        let s = vec![0.0, 1.0];
        assert_eq!(resample_linear(&s, 22050, 22050), s);
    }
}
