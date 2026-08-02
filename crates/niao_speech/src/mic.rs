//! Microphone capture via cpal.

use crate::audio::{stereo_to_mono, WHISPER_SAMPLE_RATE};
use crate::error::{SpeechError, SpeechResult};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat, StreamConfig};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Input device descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicDevice {
    pub index: usize,
    pub name: String,
    pub is_default: bool,
}

/// List available input devices.
pub fn list_devices() -> SpeechResult<Vec<MicDevice>> {
    let host = cpal::default_host();
    let default_name = host.default_input_device().and_then(|d| d.name().ok());
    let devices: Vec<_> = host
        .input_devices()
        .map_err(|e| SpeechError::Mic(e.to_string()))?
        .collect();
    let mut out = Vec::new();
    for (index, device) in devices.into_iter().enumerate() {
        let name = device.name().unwrap_or_else(|_| format!("device-{index}"));
        let is_default = default_name.as_ref().map(|d| d == &name).unwrap_or(false);
        out.push(MicDevice {
            index,
            name,
            is_default,
        });
    }
    Ok(out)
}

/// Record mono f32 audio at 16 kHz for `duration_secs` seconds.
pub fn record_secs(duration_secs: f64, device_index: Option<usize>) -> SpeechResult<Vec<f32>> {
    if duration_secs <= 0.0 {
        return Err(SpeechError::Param("duration must be positive".into()));
    }
    if duration_secs > 3600.0 {
        return Err(SpeechError::Param("duration exceeds 3600s limit".into()));
    }
    let host = cpal::default_host();
    let device = pick_device(&host, device_index)?;
    let (config, sample_format) = pick_config(&device)?;
    let sample_rate = config.sample_rate.0;
    let channels = config.channels as usize;
    let target_samples = (duration_secs * WHISPER_SAMPLE_RATE as f64).ceil() as usize;

    let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let buf_cap = Arc::clone(&buffer);
    let err_flag: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let err_cap = Arc::clone(&err_flag);

    let stream = match sample_format {
        SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _| append_samples(data, channels, &buf_cap),
            move |e| {
                *err_cap.lock().unwrap() = Some(e.to_string());
            },
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            &config,
            move |data: &[i16], _| {
                let f: Vec<f32> = data.iter().map(|&s| s.to_sample()).collect();
                append_samples(&f, channels, &buf_cap);
            },
            move |e| {
                *err_cap.lock().unwrap() = Some(e.to_string());
            },
            None,
        ),
        SampleFormat::U16 => device.build_input_stream(
            &config,
            move |data: &[u16], _| {
                let f: Vec<f32> = data.iter().map(|&s| s.to_sample()).collect();
                append_samples(&f, channels, &buf_cap);
            },
            move |e| {
                *err_cap.lock().unwrap() = Some(e.to_string());
            },
            None,
        ),
        other => {
            return Err(SpeechError::Mic(format!(
                "unsupported sample format: {other:?}"
            )));
        }
    }
    .map_err(|e| SpeechError::Mic(e.to_string()))?;

    stream.play().map_err(|e| SpeechError::Mic(e.to_string()))?;
    std::thread::sleep(Duration::from_secs_f64(duration_secs));
    drop(stream);

    if let Some(msg) = err_flag.lock().unwrap().take() {
        return Err(SpeechError::Mic(msg));
    }

    let raw = buffer.lock().unwrap().clone();
    let mono = if channels == 1 {
        raw
    } else {
        stereo_to_mono(&raw, channels)
    };
    let mut out = if sample_rate == WHISPER_SAMPLE_RATE {
        mono
    } else {
        crate::audio::resample_linear(&mono, sample_rate, WHISPER_SAMPLE_RATE)?
    };
    out.truncate(target_samples.min(out.len()));
    Ok(out)
}

fn append_samples(data: &[f32], channels: usize, buffer: &Arc<Mutex<Vec<f32>>>) {
    let mut buf = buffer.lock().unwrap();
    if channels <= 1 {
        buf.extend_from_slice(data);
    } else {
        buf.extend(stereo_to_mono(data, channels));
    }
}

fn pick_device(host: &cpal::Host, index: Option<usize>) -> SpeechResult<cpal::Device> {
    if let Some(idx) = index {
        let devices: Vec<_> = host
            .input_devices()
            .map_err(|e| SpeechError::Mic(e.to_string()))?
            .collect();
        devices
            .into_iter()
            .nth(idx)
            .ok_or_else(|| SpeechError::Mic(format!("no input device at index {idx}")))
    } else {
        host.default_input_device()
            .ok_or_else(|| SpeechError::Mic("no default input device".into()))
    }
}

fn pick_config(device: &cpal::Device) -> SpeechResult<(StreamConfig, SampleFormat)> {
    let supported = device
        .default_input_config()
        .map_err(|e| SpeechError::Mic(e.to_string()))?;
    let sample_format = supported.sample_format();
    let mut config = supported.config();
    if let Ok(ranges) = device.supported_input_configs() {
        for range in ranges {
            if range.min_sample_rate().0 <= WHISPER_SAMPLE_RATE
                && range.max_sample_rate().0 >= WHISPER_SAMPLE_RATE
                && range.sample_format() == sample_format
            {
                config.sample_rate = cpal::SampleRate(WHISPER_SAMPLE_RATE);
                config.channels = 1;
                break;
            }
        }
    }
    Ok((config, sample_format))
}
