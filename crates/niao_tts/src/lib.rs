//! Text-to-speech via Piper ONNX and eSpeak NG for Niao (`ntts`).

mod audio;
mod engine;
mod error;
mod espeak_backend;
mod piper_backend;
mod play;

pub use audio::{duration_secs, encode_wav, write_wav};
pub use engine::{SynthOptions, SynthResult, TtsBackend, TtsEngine};
pub use error::{TtsError, TtsResult};
pub use espeak_backend::{
    engine_version as espeak_version, list_engines, list_voices, EspeakEngine, VoiceInfo,
};
pub use piper_backend::{piper_version, resolve_model_paths, PiperEngine};
pub use play::play_samples;

/// Combined library version string.
pub fn version() -> String {
    format!("{} + {}", piper_version(), espeak_version())
}
