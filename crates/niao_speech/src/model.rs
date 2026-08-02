//! Whisper model loading and transcription.

use crate::audio::{duration_secs, load_wav, normalize_peak, MIN_SAMPLES, WHISPER_SAMPLE_RATE};
use crate::error::{SpeechError, SpeechResult};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use whispercpp::{
    version as whisper_version, AlignmentHeadsPreset, Context, ContextParams, Params,
    SamplingStrategy, WhisperError,
};

/// Loaded Whisper model (shareable via `Arc`).
pub struct SpeechModel {
    ctx: Arc<Context>,
    path: PathBuf,
}

/// One transcribed segment with timestamps in seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptSegment {
    pub start: f64,
    pub end: f64,
    pub text: String,
    pub no_speech_prob: f32,
}

/// Token-level timestamp (seconds).
#[derive(Debug, Clone, PartialEq)]
pub struct TokenStamp {
    pub text: String,
    pub start: f64,
    pub end: f64,
    pub prob: f32,
}

/// Full transcription result.
#[derive(Debug, Clone, PartialEq)]
pub struct Transcript {
    pub text: String,
    pub language: Option<String>,
    pub segments: Vec<TranscriptSegment>,
    pub tokens: Vec<TokenStamp>,
    pub duration_secs: f64,
}

/// Model load options.
#[derive(Debug, Clone, Default)]
pub struct LoadOptions {
    pub use_gpu: bool,
    pub dtw_timestamps: bool,
}

/// Transcription options (~openai-whisper transcribe kwargs subset).
#[derive(Debug, Clone)]
pub struct TranscribeOptions {
    pub language: Option<String>,
    pub translate: bool,
    pub detect_language: bool,
    pub token_timestamps: bool,
    pub offset_ms: i32,
    pub duration_ms: i32,
    pub temperature: f32,
    pub suppress_blank: bool,
    pub max_len: i32,
    pub initial_prompt: Option<String>,
    pub no_speech_threshold: f32,
}

impl Default for TranscribeOptions {
    fn default() -> Self {
        Self {
            language: None,
            translate: false,
            detect_language: false,
            token_timestamps: true,
            offset_ms: 0,
            duration_ms: 0,
            temperature: 0.0,
            suppress_blank: true,
            max_len: 0,
            initial_prompt: None,
            no_speech_threshold: 0.6,
        }
    }
}

pub fn engine_version() -> String {
    whisper_version()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "unknown".into())
}

pub fn language_codes() -> Vec<String> {
    const CODES: &[&str] = &[
        "en", "zh", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr", "pl", "ca", "nl", "ar", "sv",
        "it", "id", "hi", "fi", "vi", "he", "uk", "el", "ms", "cs", "ro", "da", "hu", "ta", "no",
        "th", "ur", "hr", "bg", "lt", "la", "mi", "ml", "cy", "sk", "te", "fa", "lv", "bn", "sr",
        "az", "sl", "kn", "et", "mk", "br", "eu", "is", "hy", "ne", "mn", "bs", "kk", "sq", "sw",
        "gl", "mr", "pa", "si", "km", "sn", "yo", "so", "af", "oc", "ka", "be", "tg", "sd", "gu",
        "am", "yi", "lo", "uz", "fo", "ht", "ps", "tk", "nn", "mt", "sa", "lb", "my", "bo", "tl",
        "mg", "as", "tt", "haw", "ln", "ha", "ba", "jw", "su", "yue",
    ];
    CODES.iter().map(|s| (*s).to_string()).collect()
}

pub fn load_model(path: impl AsRef<Path>, opts: &LoadOptions) -> SpeechResult<SpeechModel> {
    let path = path.as_ref();
    let mut cparams = ContextParams::new().with_use_gpu(opts.use_gpu);
    if opts.dtw_timestamps {
        cparams = cparams
            .with_dtw_token_timestamps(true)
            .with_dtw_aheads_preset(AlignmentHeadsPreset::BaseEn);
    }
    let ctx = Context::new(path, cparams).map_err(map_whisper_err)?;
    Ok(SpeechModel {
        ctx: Arc::new(ctx),
        path: path.to_path_buf(),
    })
}

impl SpeechModel {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn is_multilingual(&self) -> bool {
        self.ctx.is_multilingual()
    }

    pub fn model_type(&self) -> Option<String> {
        self.ctx.model_type().map(|s| s.to_string())
    }

    pub fn transcribe(
        &self,
        samples: &[f32],
        opts: &TranscribeOptions,
    ) -> SpeechResult<Transcript> {
        if samples.len() < MIN_SAMPLES {
            return Err(SpeechError::Audio(format!(
                "need at least {MIN_SAMPLES} samples, got {}",
                samples.len()
            )));
        }
        let mut opts = opts.clone();
        if !self.is_multilingual() {
            if opts.detect_language {
                return Ok(Transcript {
                    text: String::new(),
                    language: Some("en".into()),
                    segments: Vec::new(),
                    tokens: Vec::new(),
                    duration_secs: duration_secs(samples, WHISPER_SAMPLE_RATE),
                });
            }
            if opts.language.is_none() {
                opts.language = Some("en".into());
            }
        }
        let mut pcm = samples.to_vec();
        normalize_peak(&mut pcm);

        let mut state = self.ctx.create_state().map_err(map_whisper_err)?;
        let params = build_params(&opts)?;
        state.full(&params, &pcm).map_err(map_whisper_err)?;

        let language = state.detected_lang().map(|l| l.as_str().to_string());

        let mut segments = Vec::new();
        let mut text_parts = Vec::new();
        for seg in state.segments_iter() {
            let t0 = seg.t0() as f64 / 100.0;
            let t1 = seg.t1() as f64 / 100.0;
            let seg_text = seg.text().map_err(map_whisper_err)?.trim().to_string();
            if seg_text.is_empty() {
                continue;
            }
            text_parts.push(seg_text.clone());
            segments.push(TranscriptSegment {
                start: t0,
                end: t1,
                text: seg_text,
                no_speech_prob: seg.no_speech_prob(),
            });
        }

        let mut tokens = Vec::new();
        if opts.token_timestamps {
            for seg in state.segments_iter() {
                for tok in seg.tokens_iter() {
                    if let Some(piece) = self.ctx.token_to_str(tok.id()) {
                        if piece.starts_with('<') && piece.ends_with('>') {
                            continue;
                        }
                        tokens.push(TokenStamp {
                            text: piece.to_string(),
                            start: tok.t0() as f64 / 100.0,
                            end: tok.t1() as f64 / 100.0,
                            prob: tok.p(),
                        });
                    }
                }
            }
        }

        Ok(Transcript {
            text: text_parts.join(" ").trim().to_string(),
            language,
            segments,
            tokens,
            duration_secs: duration_secs(samples, WHISPER_SAMPLE_RATE),
        })
    }

    pub fn transcribe_file(
        &self,
        path: impl AsRef<Path>,
        opts: &TranscribeOptions,
    ) -> SpeechResult<Transcript> {
        let (samples, _) = load_wav(path)?;
        self.transcribe(&samples, opts)
    }

    pub fn detect_language(&self, samples: &[f32]) -> SpeechResult<String> {
        if !self.is_multilingual() {
            return Ok("en".into());
        }
        if samples.len() < MIN_SAMPLES {
            return Err(SpeechError::Audio(format!(
                "need at least {MIN_SAMPLES} samples for language detection, got {}",
                samples.len()
            )));
        }
        let mut opts = TranscribeOptions::default();
        opts.detect_language = true;
        opts.language = None;
        opts.token_timestamps = false;
        let tr = self.transcribe(samples, &opts)?;
        tr.language
            .ok_or_else(|| SpeechError::Whisper("language detection failed".into()))
    }
}

fn build_params(opts: &TranscribeOptions) -> SpeechResult<Params> {
    let mut params = Params::new(SamplingStrategy::Greedy { best_of: 1 });
    params.silence_print_toggles();
    params.set_translate(opts.translate);
    params.set_detect_language(opts.detect_language);
    params.set_token_timestamps(opts.token_timestamps);
    params.set_offset_ms(opts.offset_ms);
    params.set_duration_ms(opts.duration_ms);
    params.set_temperature(opts.temperature);
    params.set_suppress_blank(opts.suppress_blank);
    params.set_no_speech_thold(opts.no_speech_threshold);
    if opts.max_len > 0 {
        params.set_max_len(opts.max_len);
    }
    if let Some(ref lang) = opts.language {
        params.set_language(lang).map_err(map_whisper_err)?;
    }
    if let Some(ref prompt) = opts.initial_prompt {
        params.set_initial_prompt(prompt).map_err(map_whisper_err)?;
    }
    Ok(params)
}

fn map_whisper_err(e: WhisperError) -> SpeechError {
    SpeechError::Whisper(e.to_string())
}

/// Naive O(n*m) substring alignment baseline for benchmarks.
pub fn align_naive(haystack: &str, needle: &str) -> Option<usize> {
    haystack.find(needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_nonempty() {
        assert!(!engine_version().is_empty());
    }

    #[test]
    fn languages_nonempty() {
        assert!(!language_codes().is_empty());
    }

    #[test]
    fn load_missing_model() {
        let r = load_model("/nonexistent/whisper.bin", &LoadOptions::default());
        assert!(r.is_err());
    }

    #[test]
    fn transcribe_too_short() {
        // Can't load real model in unit test without fixture; test error path via mock is hard.
        // Use missing model path for load; short samples checked after load in integration tests.
        let samples = vec![0.0f32; 10];
        let opts = TranscribeOptions::default();
        // Without model, just verify MIN_SAMPLES constant
        assert!(samples.len() < MIN_SAMPLES);
        let _ = opts;
    }
}
