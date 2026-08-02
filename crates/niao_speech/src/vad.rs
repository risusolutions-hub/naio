//! Voice-activity detection — energy-threshold segmentation (~webrtcvad subset).

use crate::audio::duration_secs;
use crate::error::{SpeechError, SpeechResult};

/// One contiguous speech region in seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct VadSegment {
    pub start: f64,
    pub end: f64,
}

/// Options for energy-based VAD.
#[derive(Debug, Clone)]
pub struct VadOptions {
    /// Frame size in milliseconds (10–30 typical).
    pub frame_ms: u32,
    /// RMS energy threshold (0..1 scale after normalization).
    pub threshold: f32,
    /// Minimum speech segment duration in seconds.
    pub min_speech_secs: f64,
    /// Padding added around each segment in seconds.
    pub pad_secs: f64,
    /// Merge segments separated by less than this gap (seconds).
    pub min_silence_secs: f64,
}

impl Default for VadOptions {
    fn default() -> Self {
        Self {
            frame_ms: 30,
            threshold: 0.008,
            min_speech_secs: 0.25,
            pad_secs: 0.1,
            min_silence_secs: 0.3,
        }
    }
}

/// Detect speech segments via frame RMS energy.
pub fn detect_voice(
    samples: &[f32],
    sample_rate: u32,
    opts: &VadOptions,
) -> SpeechResult<Vec<VadSegment>> {
    if samples.is_empty() {
        return Ok(Vec::new());
    }
    if sample_rate == 0 {
        return Err(SpeechError::Param("sample_rate must be positive".into()));
    }
    let frame_ms = opts.frame_ms.clamp(10, 100);
    let frame_len = ((sample_rate as u64) * frame_ms as u64 / 1000).max(1) as usize;
    let frame_secs = frame_len as f64 / sample_rate as f64;

    let mut raw: Vec<(f64, f64)> = Vec::new();
    let mut in_speech = false;
    let mut seg_start = 0.0f64;
    let mut i = 0usize;
    while i < samples.len() {
        let end = (i + frame_len).min(samples.len());
        let frame = &samples[i..end];
        let rms = frame_rms(frame);
        let t = i as f64 / sample_rate as f64;
        if rms >= opts.threshold {
            if !in_speech {
                seg_start = t;
                in_speech = true;
            }
        } else if in_speech {
            raw.push((seg_start, t + frame_secs));
            in_speech = false;
        }
        i += frame_len;
    }
    if in_speech {
        raw.push((seg_start, duration_secs(samples, sample_rate)));
    }

    merge_and_filter(raw, opts, duration_secs(samples, sample_rate))
}

fn frame_rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    let sum: f32 = frame.iter().map(|&x| x * x).sum();
    (sum / frame.len() as f32).sqrt()
}

fn merge_and_filter(
    raw: Vec<(f64, f64)>,
    opts: &VadOptions,
    total_secs: f64,
) -> SpeechResult<Vec<VadSegment>> {
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (s, e) in raw {
        if let Some(last) = merged.last_mut() {
            if s - last.1 <= opts.min_silence_secs {
                last.1 = e;
                continue;
            }
        }
        merged.push((s, e));
    }
    let mut out = Vec::new();
    for (s, e) in merged {
        let dur = e - s;
        if dur < opts.min_speech_secs {
            continue;
        }
        let start = (s - opts.pad_secs).max(0.0);
        let end = (e + opts.pad_secs).min(total_secs);
        if end > start {
            out.push(VadSegment { start, end });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WHISPER_SAMPLE_RATE;

    fn tone(len: usize, amp: f32) -> Vec<f32> {
        (0..len)
            .map(|i| amp * (2.0 * std::f64::consts::PI * 440.0 * i as f64 / 16000.0).sin() as f32)
            .collect()
    }

    #[test]
    fn detects_speech_region() {
        let mut samples = vec![0.0f32; 16000];
        let speech = tone(8000, 0.3);
        samples[4000..12000].copy_from_slice(&speech);
        let segs = detect_voice(&samples, WHISPER_SAMPLE_RATE, &VadOptions::default()).unwrap();
        assert!(!segs.is_empty());
        assert!(segs[0].start < 0.5);
        assert!(segs[0].end > 0.5);
    }

    #[test]
    fn silence_returns_empty() {
        let samples = vec![0.0f32; 8000];
        let segs = detect_voice(&samples, WHISPER_SAMPLE_RATE, &VadOptions::default()).unwrap();
        assert!(segs.is_empty());
    }
}
