//! eSpeak-ng synthesis via espeak-rs-sys (lightweight, no ONNX model).

use crate::audio::i16_to_f32;
use crate::error::{TtsError, TtsResult};
use espeak_rs_sys::{
    espeakCHARS_UTF8, espeakINITIALIZE_DONT_EXIT, espeak_AUDIO_OUTPUT_AUDIO_OUTPUT_RETRIEVAL,
    espeak_ERROR_EE_OK, espeak_PARAMETER_espeakPITCH, espeak_PARAMETER_espeakRATE,
    espeak_PARAMETER_espeakVOLUME, espeak_POSITION_TYPE_POS_CHARACTER,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{c_int, c_short, c_void, CStr, CString};
use std::path::PathBuf;
use std::ptr;
use std::sync::OnceLock;

const ESPEAK_DATA_DIR: &str = "espeak-ng-data";

static ESPEAK_INIT: OnceLock<TtsResult<()>> = OnceLock::new();

thread_local! {
    static PCM_BUF: RefCell<Vec<i16>> = const { RefCell::new(Vec::new()) };
}

extern "C" fn synth_callback(
    wav: *mut c_short,
    numsamples: c_int,
    _events: *mut espeak_rs_sys::espeak_EVENT,
) -> c_int {
    if wav.is_null() || numsamples <= 0 {
        return 0;
    }
    PCM_BUF.with(|buf| {
        // SAFETY: espeak passes a valid slice for the duration of the callback.
        let slice = unsafe { std::slice::from_raw_parts(wav, numsamples as usize) };
        buf.borrow_mut().extend_from_slice(slice);
    });
    0
}

fn init_espeak() -> TtsResult<()> {
    let data_dir = locate_espeak_data();
    let path_cstr = data_dir
        .as_ref()
        .and_then(|p| CString::new(p.to_string_lossy().as_ref()).ok());
    let path_ptr = path_cstr.as_ref().map_or(ptr::null(), |c| c.as_ptr());

    let sample_rate = unsafe {
        espeak_rs_sys::espeak_Initialize(
            espeak_AUDIO_OUTPUT_AUDIO_OUTPUT_RETRIEVAL,
            0,
            path_ptr,
            espeakINITIALIZE_DONT_EXIT as i32,
        )
    };
    if sample_rate <= 0 {
        return Err(TtsError::Synth(format!(
            "failed to initialize espeak-ng (code {sample_rate}); set PIPER_ESPEAKNG_DATA_DIRECTORY"
        )));
    }
    unsafe {
        espeak_rs_sys::espeak_SetSynthCallback(Some(synth_callback));
    }
    Ok(())
}

fn ensure_init() -> TtsResult<()> {
    ESPEAK_INIT
        .get_or_init(init_espeak)
        .as_ref()
        .map_err(|e| TtsError::Synth(e.message()))
        .map(|_| ())
}

fn locate_espeak_data() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("PIPER_ESPEAKNG_DATA_DIRECTORY") {
        let p = PathBuf::from(dir);
        if p.join(ESPEAK_DATA_DIR).exists() {
            return Some(p);
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        if cwd.join(ESPEAK_DATA_DIR).exists() {
            return Some(cwd);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            if dir.join(ESPEAK_DATA_DIR).exists() {
                return Some(dir.to_path_buf());
            }
        }
    }
    None
}

/// Descriptor for an espeak voice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceInfo {
    pub name: String,
    pub language: String,
    pub identifier: String,
}

/// Runtime espeak engine state.
#[derive(Debug, Clone)]
pub struct EspeakEngine {
    pub voice: String,
    pub rate: i32,
    pub volume: i32,
    pub pitch: i32,
}

impl Default for EspeakEngine {
    fn default() -> Self {
        Self {
            voice: "en".into(),
            rate: 175,
            volume: 100,
            pitch: 50,
        }
    }
}

impl EspeakEngine {
    pub fn apply(&self) -> TtsResult<()> {
        ensure_init()?;
        let voice_c = CString::new(self.voice.as_str())
            .map_err(|_| TtsError::Param("voice contains null byte".into()))?;
        let rc = unsafe { espeak_rs_sys::espeak_SetVoiceByName(voice_c.as_ptr()) };
        if rc != espeak_ERROR_EE_OK {
            return Err(TtsError::Param(format!("unknown voice: {}", self.voice)));
        }
        unsafe {
            espeak_rs_sys::espeak_SetParameter(espeak_PARAMETER_espeakRATE, self.rate, 0);
            espeak_rs_sys::espeak_SetParameter(espeak_PARAMETER_espeakVOLUME, self.volume, 0);
            espeak_rs_sys::espeak_SetParameter(espeak_PARAMETER_espeakPITCH, self.pitch, 0);
        }
        Ok(())
    }

    pub fn synth(&self, text: &str) -> TtsResult<(Vec<f32>, u32)> {
        if text.trim().is_empty() {
            return Err(TtsError::Empty);
        }
        self.apply()?;
        let text_c =
            CString::new(text).map_err(|_| TtsError::Param("text contains null byte".into()))?;

        PCM_BUF.with(|buf| buf.borrow_mut().clear());

        let rc = unsafe {
            espeak_rs_sys::espeak_Synth(
                text_c.as_ptr() as *const c_void,
                0,
                0,
                espeak_POSITION_TYPE_POS_CHARACTER,
                0,
                espeakCHARS_UTF8,
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        if rc != espeak_ERROR_EE_OK {
            return Err(TtsError::Synth(format!("espeak_Synth failed (code {rc})")));
        }
        unsafe {
            espeak_rs_sys::espeak_Synchronize();
        }
        let pcm = PCM_BUF.with(|buf| buf.borrow().clone());
        let rate = unsafe { espeak_rs_sys::espeak_ng_GetSampleRate() } as u32;
        Ok((i16_to_f32(&pcm), rate))
    }

    pub fn set_property(&mut self, key: &str, value: &str) -> TtsResult<()> {
        match key {
            "voice" => {
                self.voice = value.to_string();
                Ok(())
            }
            "rate" => {
                self.rate = value
                    .parse()
                    .map_err(|_| TtsError::Param("rate must be an integer".into()))?;
                Ok(())
            }
            "volume" => {
                self.volume = value
                    .parse()
                    .map_err(|_| TtsError::Param("volume must be an integer 0-200".into()))?;
                Ok(())
            }
            "pitch" => {
                self.pitch = value
                    .parse()
                    .map_err(|_| TtsError::Param("pitch must be an integer".into()))?;
                Ok(())
            }
            other => Err(TtsError::Property(other.to_string())),
        }
    }

    pub fn get_property(&self, key: &str) -> TtsResult<String> {
        match key {
            "voice" => Ok(self.voice.clone()),
            "rate" => Ok(self.rate.to_string()),
            "volume" => Ok(self.volume.to_string()),
            "pitch" => Ok(self.pitch.to_string()),
            "engine" => Ok("espeak".into()),
            other => Err(TtsError::Property(other.to_string())),
        }
    }
}

/// List installed espeak voices.
pub fn list_voices() -> TtsResult<Vec<VoiceInfo>> {
    ensure_init()?;
    let mut out = Vec::new();
    let head = unsafe { espeak_rs_sys::espeak_ListVoices(ptr::null_mut()) };
    if head.is_null() {
        return Ok(out);
    }
    let mut idx = 0usize;
    loop {
        let voice_ptr = unsafe { *head.add(idx) };
        if voice_ptr.is_null() {
            break;
        }
        let voice = unsafe { &*voice_ptr };
        let name = unsafe { CStr::from_ptr(voice.name) }
            .to_string_lossy()
            .into_owned();
        let lang = unsafe { CStr::from_ptr(voice.languages) }
            .to_string_lossy()
            .into_owned();
        let id = unsafe { CStr::from_ptr(voice.identifier) }
            .to_string_lossy()
            .into_owned();
        out.push(VoiceInfo {
            name,
            language: lang,
            identifier: id,
        });
        idx += 1;
    }
    Ok(out)
}

pub fn engine_version() -> &'static str {
    "espeak-ng"
}

pub fn list_engines() -> Vec<HashMap<&'static str, &'static str>> {
    vec![
        [
            ("id", "piper"),
            ("name", "Piper ONNX"),
            (
                "description",
                "Neural TTS via Piper models + espeak phonemization",
            ),
        ]
        .into_iter()
        .collect(),
        [
            ("id", "espeak"),
            ("name", "eSpeak NG"),
            (
                "description",
                "Lightweight formant synthesis (no model file)",
            ),
        ]
        .into_iter()
        .collect(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_voices_or_skip() {
        if ensure_init().is_err() {
            return;
        }
        let voices = list_voices().unwrap();
        assert!(!voices.is_empty());
    }
}
