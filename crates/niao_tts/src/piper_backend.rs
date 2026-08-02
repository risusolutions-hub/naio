//! Piper ONNX TTS backend.

use crate::audio::apply_volume;
use crate::error::{TtsError, TtsResult};
use piper_rs::{ModelConfig, Piper};
use std::fs::File;
use std::path::{Path, PathBuf};

/// Resolve `.onnx` + `.onnx.json` paths from a user path.
pub fn resolve_model_paths(path: &Path) -> TtsResult<(PathBuf, PathBuf)> {
    if path.is_dir() {
        let mut onnx = None;
        for entry in std::fs::read_dir(path).map_err(|e| TtsError::Io(e.to_string()))? {
            let entry = entry.map_err(|e| TtsError::Io(e.to_string()))?;
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("onnx") {
                onnx = Some(p);
                break;
            }
        }
        let model =
            onnx.ok_or_else(|| TtsError::Model(format!("no .onnx in {}", path.display())))?;
        let config = config_path_for(&model)?;
        return Ok((model, config));
    }

    if path.extension().and_then(|e| e.to_str()) == Some("onnx") {
        let config = config_path_for(path)?;
        return Ok((path.to_path_buf(), config));
    }

    Err(TtsError::Model(format!(
        "expected .onnx file or directory, got {}",
        path.display()
    )))
}

fn config_path_for(model: &Path) -> TtsResult<PathBuf> {
    let json = format!("{}.json", model.display());
    let cfg = PathBuf::from(json);
    if cfg.is_file() {
        return Ok(cfg);
    }
    Err(TtsError::Model(format!(
        "missing config file: {}",
        cfg.display()
    )))
}

/// Loaded Piper model with mutable inference session.
pub struct PiperEngine {
    inner: Piper,
    config: ModelConfig,
    pub length_scale: f32,
    pub noise_scale: f32,
    pub noise_w: f32,
    pub speaker_id: i64,
    pub volume: f32,
    pub voice: String,
}

impl PiperEngine {
    pub fn load(path: &Path) -> TtsResult<Self> {
        let (model, config_path) = resolve_model_paths(path)?;
        let file = File::open(&config_path).map_err(|e| TtsError::Model(e.to_string()))?;
        let config: ModelConfig =
            serde_json::from_reader(file).map_err(|e| TtsError::Model(e.to_string()))?;
        let inner = Piper::new(&model, &config_path).map_err(|e| TtsError::Model(e.to_string()))?;
        let inf = config.inference.clone();
        let voice = config.espeak.voice.clone();
        Ok(Self {
            inner,
            config,
            length_scale: inf.length_scale,
            noise_scale: inf.noise_scale,
            noise_w: inf.noise_w,
            speaker_id: 0,
            volume: 1.0,
            voice,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.config.audio.sample_rate
    }

    pub fn voice_list(&self) -> Vec<(String, i64)> {
        match self.inner.voices() {
            Some(map) => map.iter().map(|(k, v)| (k.clone(), *v)).collect(),
            None => vec![(self.voice.clone(), 0)],
        }
    }

    pub fn synth(&mut self, text: &str) -> TtsResult<(Vec<f32>, u32)> {
        if text.trim().is_empty() {
            return Err(TtsError::Empty);
        }
        let (mut samples, rate) = self
            .inner
            .create(
                text,
                false,
                Some(self.speaker_id),
                Some(self.length_scale),
                Some(self.noise_scale),
                Some(self.noise_w),
            )
            .map_err(|e| TtsError::Synth(e.to_string()))?;
        apply_volume(&mut samples, self.volume);
        Ok((samples, rate))
    }

    pub fn set_property(&mut self, key: &str, value: &str) -> TtsResult<()> {
        match key {
            "voice" | "speaker" => {
                if let Ok(id) = value.parse::<i64>() {
                    self.speaker_id = id;
                    return Ok(());
                }
                if let Some(map) = self.inner.voices() {
                    if let Some(&id) = map.get(value) {
                        self.speaker_id = id;
                        self.voice = value.to_string();
                        return Ok(());
                    }
                }
                self.voice = value.to_string();
                Ok(())
            }
            "rate" | "length_scale" => {
                let v: f32 = value
                    .parse()
                    .map_err(|_| TtsError::Param("rate/length_scale must be a number".into()))?;
                if v <= 0.0 {
                    return Err(TtsError::Param("rate must be positive".into()));
                }
                self.length_scale = v;
                Ok(())
            }
            "volume" => {
                let v: f32 = value
                    .parse()
                    .map_err(|_| TtsError::Param("volume must be a number".into()))?;
                self.volume = v.clamp(0.0, 2.0);
                Ok(())
            }
            "noise_scale" => {
                self.noise_scale = value
                    .parse()
                    .map_err(|_| TtsError::Param("noise_scale must be a number".into()))?;
                Ok(())
            }
            "noise_w" => {
                self.noise_w = value
                    .parse()
                    .map_err(|_| TtsError::Param("noise_w must be a number".into()))?;
                Ok(())
            }
            other => Err(TtsError::Property(other.to_string())),
        }
    }

    pub fn get_property(&self, key: &str) -> TtsResult<String> {
        match key {
            "voice" => Ok(self.voice.clone()),
            "speaker" => Ok(self.speaker_id.to_string()),
            "rate" | "length_scale" => Ok(self.length_scale.to_string()),
            "volume" => Ok(self.volume.to_string()),
            "noise_scale" => Ok(self.noise_scale.to_string()),
            "noise_w" => Ok(self.noise_w.to_string()),
            "engine" => Ok("piper".into()),
            "sample_rate" => Ok(self.sample_rate().to_string()),
            other => Err(TtsError::Property(other.to_string())),
        }
    }
}

pub fn piper_version() -> &'static str {
    "piper-rs/0.2.0"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_missing_config() {
        let err = resolve_model_paths(Path::new("/nonexistent/model.onnx"));
        assert!(err.is_err());
    }
}
