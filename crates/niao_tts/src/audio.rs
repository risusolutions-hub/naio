//! WAV encoding and sample utilities.

use crate::error::{TtsError, TtsResult};
use std::path::Path;

/// Apply linear volume scale and clamp to [-1, 1].
pub fn apply_volume(samples: &mut [f32], volume: f32) {
    if (volume - 1.0).abs() < f32::EPSILON {
        return;
    }
    let v = volume.clamp(0.0, 2.0);
    for s in samples.iter_mut() {
        *s = (*s * v).clamp(-1.0, 1.0);
    }
}

/// Convert mono f32 PCM to 16-bit WAV bytes in memory.
pub fn encode_wav(samples: &[f32], sample_rate: u32) -> TtsResult<Vec<u8>> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = std::io::Cursor::new(Vec::new());
    {
        let mut writer =
            hound::WavWriter::new(&mut cursor, spec).map_err(|e| TtsError::Audio(e.to_string()))?;
        for &s in samples {
            let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            writer
                .write_sample(v)
                .map_err(|e| TtsError::Audio(e.to_string()))?;
        }
        writer
            .finalize()
            .map_err(|e| TtsError::Audio(e.to_string()))?;
    }
    Ok(cursor.into_inner())
}

/// Write mono f32 samples as 16-bit PCM WAV file.
pub fn write_wav(path: impl AsRef<Path>, samples: &[f32], sample_rate: u32) -> TtsResult<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer =
        hound::WavWriter::create(path.as_ref(), spec).map_err(|e| TtsError::Io(e.to_string()))?;
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        writer
            .write_sample(v)
            .map_err(|e| TtsError::Io(e.to_string()))?;
    }
    writer.finalize().map_err(|e| TtsError::Io(e.to_string()))?;
    Ok(())
}

/// Duration in seconds for mono f32 audio.
pub fn duration_secs(samples: &[f32], sample_rate: u32) -> f64 {
    if sample_rate == 0 {
        return 0.0;
    }
    samples.len() as f64 / sample_rate as f64
}

/// Convert i16 PCM to normalized f32.
pub fn i16_to_f32(samples: &[i16]) -> Vec<f32> {
    samples
        .iter()
        .map(|&v| v as f32 / i16::MAX as f32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_empty_wav() {
        let wav = encode_wav(&[], 22050).unwrap();
        assert!(!wav.is_empty());
    }

    #[test]
    fn volume_scales() {
        let mut s = vec![1.0, -1.0];
        apply_volume(&mut s, 0.5);
        assert!((s[0] - 0.5).abs() < 1e-6);
    }
}
