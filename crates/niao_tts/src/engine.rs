//! Unified TTS engine handle.

use crate::audio::{apply_volume, duration_secs, encode_wav, write_wav};
use crate::error::TtsResult;
use crate::espeak_backend::EspeakEngine;
use crate::piper_backend::PiperEngine;
use crate::play::play_samples;
use std::path::Path;

/// Synthesis options shared across backends.
#[derive(Debug, Clone, Default)]
pub struct SynthOptions {
    pub voice: Option<String>,
    pub speaker: Option<i64>,
    pub rate: Option<f32>,
    pub volume: Option<f32>,
    pub length_scale: Option<f32>,
    pub noise_scale: Option<f32>,
    pub noise_w: Option<f32>,
}

impl SynthOptions {
    pub fn apply_to_piper(&self, engine: &mut PiperEngine) -> TtsResult<()> {
        if let Some(ref v) = self.voice {
            engine.set_property("voice", v)?;
        }
        if let Some(s) = self.speaker {
            engine.set_property("speaker", &s.to_string())?;
        }
        if let Some(r) = self.rate.or(self.length_scale) {
            engine.set_property("length_scale", &r.to_string())?;
        }
        if let Some(v) = self.volume {
            engine.set_property("volume", &v.to_string())?;
        }
        if let Some(n) = self.noise_scale {
            engine.set_property("noise_scale", &n.to_string())?;
        }
        if let Some(n) = self.noise_w {
            engine.set_property("noise_w", &n.to_string())?;
        }
        Ok(())
    }

    pub fn apply_to_espeak(&self, engine: &mut EspeakEngine) -> TtsResult<()> {
        if let Some(ref v) = self.voice {
            engine.set_property("voice", v)?;
        }
        if let Some(r) = self.rate {
            let wpm = (175.0 / r).round() as i32;
            engine.set_property("rate", &wpm.clamp(80, 450).to_string())?;
        }
        if let Some(v) = self.volume {
            let ev = (v * 100.0).round() as i32;
            engine.set_property("volume", &ev.clamp(0, 200).to_string())?;
        }
        Ok(())
    }
}

/// Active TTS backend.
pub enum TtsBackend {
    Piper(PiperEngine),
    Espeak(EspeakEngine),
}

/// Unified engine exposed to the VM layer.
pub struct TtsEngine {
    backend: TtsBackend,
}

impl TtsEngine {
    pub fn load_piper(path: &Path) -> TtsResult<Self> {
        Ok(Self {
            backend: TtsBackend::Piper(PiperEngine::load(path)?),
        })
    }

    pub fn init_espeak(voice: Option<&str>) -> TtsResult<Self> {
        let mut eng = EspeakEngine::default();
        if let Some(v) = voice {
            eng.voice = v.to_string();
        }
        Ok(Self {
            backend: TtsBackend::Espeak(eng),
        })
    }

    pub fn engine_id(&self) -> &'static str {
        match &self.backend {
            TtsBackend::Piper(_) => "piper",
            TtsBackend::Espeak(_) => "espeak",
        }
    }

    pub fn voices(&self) -> Vec<(String, i64)> {
        match &self.backend {
            TtsBackend::Piper(p) => p.voice_list(),
            TtsBackend::Espeak(_) => crate::espeak_backend::list_voices()
                .unwrap_or_default()
                .into_iter()
                .enumerate()
                .map(|(i, v)| (v.identifier, i as i64))
                .collect(),
        }
    }

    pub fn synth(&mut self, text: &str, opts: &SynthOptions) -> TtsResult<SynthResult> {
        match &mut self.backend {
            TtsBackend::Piper(p) => {
                opts.apply_to_piper(p)?;
                let (samples, sample_rate) = p.synth(text)?;
                let duration = duration_secs(&samples, sample_rate);
                Ok(SynthResult {
                    samples,
                    sample_rate,
                    duration,
                })
            }
            TtsBackend::Espeak(e) => {
                opts.apply_to_espeak(e)?;
                let (mut samples, sample_rate) = e.synth(text)?;
                if let Some(v) = opts.volume {
                    apply_volume(&mut samples, v);
                }
                let duration = duration_secs(&samples, sample_rate);
                Ok(SynthResult {
                    samples,
                    sample_rate,
                    duration,
                })
            }
        }
    }

    pub fn synth_wav(&mut self, text: &str, opts: &SynthOptions) -> TtsResult<Vec<u8>> {
        let result = self.synth(text, opts)?;
        encode_wav(&result.samples, result.sample_rate)
    }

    pub fn save(&mut self, text: &str, path: &Path, opts: &SynthOptions) -> TtsResult<()> {
        let result = self.synth(text, opts)?;
        write_wav(path, &result.samples, result.sample_rate)
    }

    pub fn speak(&mut self, text: &str, opts: &SynthOptions) -> TtsResult<()> {
        let result = self.synth(text, opts)?;
        play_samples(&result.samples, result.sample_rate)
    }

    pub fn get(&self, property: &str) -> TtsResult<String> {
        match &self.backend {
            TtsBackend::Piper(p) => p.get_property(property),
            TtsBackend::Espeak(e) => e.get_property(property),
        }
    }

    pub fn set(&mut self, property: &str, value: &str) -> TtsResult<()> {
        match &mut self.backend {
            TtsBackend::Piper(p) => p.set_property(property, value),
            TtsBackend::Espeak(e) => e.set_property(property, value),
        }
    }
}

/// Synthesized audio payload.
#[derive(Debug, Clone)]
pub struct SynthResult {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub duration: f64,
}
