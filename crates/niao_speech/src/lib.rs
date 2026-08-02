//! Speech-to-text via whisper.cpp for Niao (`nspeech`).

mod audio;
mod error;
mod mic;
mod model;
mod vad;

pub use audio::{
    duration_secs, load_wav, load_wav_bytes, normalize_peak, resample_linear, stereo_to_mono,
    write_wav_f32, MIN_SAMPLES, WHISPER_SAMPLE_RATE,
};
pub use error::{SpeechError, SpeechResult};
pub use mic::{list_devices as mic_devices, record_secs as mic_record, MicDevice};
pub use model::{
    align_naive, engine_version, language_codes, load_model, LoadOptions, SpeechModel, TokenStamp,
    TranscribeOptions, Transcript, TranscriptSegment,
};
pub use vad::{detect_voice, VadOptions, VadSegment};
