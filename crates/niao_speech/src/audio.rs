//! Audio loading, resampling, and format conversion for Whisper (16 kHz mono f32).

use crate::error::{SpeechError, SpeechResult};
use std::path::Path;

pub const WHISPER_SAMPLE_RATE: u32 = 16_000;
pub const MIN_SAMPLES: usize = 201;

/// Load a WAV file and return mono f32 samples at 16 kHz.
pub fn load_wav(path: impl AsRef<Path>) -> SpeechResult<(Vec<f32>, u32)> {
    let path = path.as_ref();
    let reader =
        hound::WavReader::open(path).map_err(|e| SpeechError::Io(format!("{path:?}: {e}")))?;
    load_wav_reader(reader)
}

/// Decode WAV bytes (RIFF) to mono f32 @ 16 kHz.
pub fn load_wav_bytes(data: &[u8]) -> SpeechResult<(Vec<f32>, u32)> {
    let cursor = std::io::Cursor::new(data);
    let reader = hound::WavReader::new(cursor).map_err(|e| SpeechError::Audio(e.to_string()))?;
    load_wav_reader(reader)
}

fn load_wav_reader<R: std::io::Read>(
    mut reader: hound::WavReader<R>,
) -> SpeechResult<(Vec<f32>, u32)> {
    let spec = reader.spec();
    if spec.sample_rate == 0 {
        return Err(SpeechError::Audio("invalid sample rate 0".into()));
    }
    let channels = spec.channels.max(1) as usize;
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| SpeechError::Audio(e.to_string()))?,
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample.saturating_sub(1))) - 1;
            if max <= 0 {
                return Err(SpeechError::Audio("unsupported integer bit depth".into()));
            }
            let scale = max as f64;
            reader
                .samples::<i32>()
                .map(|s| {
                    s.map(|v| (v as f64 / scale) as f32)
                        .map_err(|e| SpeechError::Audio(e.to_string()))
                })
                .collect::<SpeechResult<Vec<_>>>()?
        }
    };
    if samples.is_empty() {
        return Ok((Vec::new(), WHISPER_SAMPLE_RATE));
    }
    let mono = if channels == 1 {
        samples
    } else {
        stereo_to_mono(&samples, channels)
    };
    let out = if spec.sample_rate == WHISPER_SAMPLE_RATE {
        mono
    } else {
        resample_linear(&mono, spec.sample_rate, WHISPER_SAMPLE_RATE)?
    };
    Ok((out, WHISPER_SAMPLE_RATE))
}

/// Convert interleaved multi-channel PCM to mono by averaging channels.
pub fn stereo_to_mono(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    let frames = interleaved.len() / channels;
    let mut out = Vec::with_capacity(frames);
    for i in 0..frames {
        let base = i * channels;
        let mut sum = 0.0f32;
        for c in 0..channels {
            sum += interleaved[base + c];
        }
        out.push(sum / channels as f32);
    }
    out
}

/// Linear resample `samples` from `from_rate` to `to_rate`.
pub fn resample_linear(samples: &[f32], from_rate: u32, to_rate: u32) -> SpeechResult<Vec<f32>> {
    if samples.is_empty() {
        return Ok(Vec::new());
    }
    if from_rate == 0 || to_rate == 0 {
        return Err(SpeechError::Param("sample rate must be positive".into()));
    }
    if from_rate == to_rate {
        return Ok(samples.to_vec());
    }
    let out_len = ((samples.len() as u64) * to_rate as u64 / from_rate as u64) as usize;
    if out_len == 0 {
        return Ok(Vec::new());
    }
    let mut out = Vec::with_capacity(out_len);
    let ratio = from_rate as f64 / to_rate as f64;
    for i in 0..out_len {
        let src = i as f64 * ratio;
        let i0 = src.floor() as usize;
        let i1 = (i0 + 1).min(samples.len() - 1);
        let frac = (src - i0 as f64) as f32;
        let v = samples[i0] * (1.0 - frac) + samples[i1] * frac;
        out.push(v);
    }
    Ok(out)
}

/// Normalize f32 samples to [-1, 1] if peak exceeds 1.0 (in-place).
pub fn normalize_peak(samples: &mut [f32]) {
    let peak = samples.iter().fold(0.0f32, |m, &v| m.max(v.abs()));
    if peak > 1.0 {
        let inv = 1.0 / peak;
        for s in samples.iter_mut() {
            *s *= inv;
        }
    }
}

/// Duration in seconds for mono f32 audio at the given sample rate.
pub fn duration_secs(samples: &[f32], sample_rate: u32) -> f64 {
    if sample_rate == 0 {
        return 0.0;
    }
    samples.len() as f64 / sample_rate as f64
}

/// Write mono f32 samples as 16-bit PCM WAV (useful for test fixtures).
pub fn write_wav_f32(
    path: impl AsRef<Path>,
    samples: &[f32],
    sample_rate: u32,
) -> SpeechResult<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path.as_ref(), spec)
        .map_err(|e| SpeechError::Io(e.to_string()))?;
    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let v = (clamped * i16::MAX as f32) as i16;
        writer
            .write_sample(v)
            .map_err(|e| SpeechError::Io(e.to_string()))?;
    }
    writer
        .finalize()
        .map_err(|e| SpeechError::Io(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_same_rate() {
        let s = vec![0.0, 0.5, 1.0];
        let out = resample_linear(&s, 16000, 16000).unwrap();
        assert_eq!(out, s);
    }

    #[test]
    fn stereo_to_mono_avg() {
        let inter = vec![1.0, 3.0, 2.0, 4.0];
        let mono = stereo_to_mono(&inter, 2);
        assert_eq!(mono.len(), 2);
        assert!((mono[0] - 2.0).abs() < 1e-6);
    }

    #[test]
    fn empty_resample() {
        assert!(resample_linear(&[], 44100, 16000).unwrap().is_empty());
    }
}
