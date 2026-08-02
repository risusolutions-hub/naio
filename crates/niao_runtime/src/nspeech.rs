//! Native nspeech standard library — speech-to-text via whisper.cpp
//! (~openai-whisper / speechrecognition subset).
//!
//! Import with `import "nspeech"` (or `import "std/nspeech"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_speech::{
    detect_voice, engine_version, language_codes, load_model, load_wav, mic_devices, mic_record,
    resample_linear, LoadOptions, MicDevice, SpeechError, SpeechModel, TranscribeOptions,
    VadOptions, WHISPER_SAMPLE_RATE,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E4130: u32 = codes::E4130_NSPEECH_ARITY;
const E4131: u32 = codes::E4131_NSPEECH_ERROR;
const E4132: u32 = codes::E4132_NSPEECH_TYPE;
const E4133: u32 = codes::E4133_NSPEECH_PARAM;
const E4134: u32 = codes::E4134_NSPEECH_INVALID_HANDLE;
const E4135: u32 = codes::E4135_NSPEECH_AUDIO;
const E4136: u32 = codes::E4136_NSPEECH_MODEL;
const E4137: u32 = codes::E4137_NSPEECH_MIC;

thread_local! {
    static MODELS: RefCell<HashMap<i64, SpeechModel>> = RefCell::new(HashMap::new());
    static NEXT_ID: RefCell<i64> = const { RefCell::new(1) };
}

fn alloc(model: SpeechModel) -> i64 {
    let id = NEXT_ID.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    MODELS.with(|m| m.borrow_mut().insert(id, model));
    id
}

fn with_model<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&SpeechModel) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    MODELS.with(|m| match m.borrow().get(&id) {
        Some(model) => Ok(Ok(f(model))),
        None => Ok(Err(error_value(
            E4134,
            "nspeech_error",
            format!("invalid or closed nspeech model handle {id}"),
            span,
        ))),
    })
}

fn remove_model(id: i64) -> bool {
    MODELS.with(|m| m.borrow_mut().remove(&id).is_some())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E4132, msg.into())
}

fn soft_err(span: Span, err: SpeechError) -> ValueRef {
    let code = match &err {
        SpeechError::Empty | SpeechError::InvalidHandle => E4131,
        SpeechError::Param(_) => E4133,
        SpeechError::Audio(_) => E4135,
        SpeechError::Io(_) => E4135,
        SpeechError::Model(_) | SpeechError::Whisper(_) => E4136,
        SpeechError::Mic(_) => E4137,
    };
    error_value(code, "nspeech_error", err.message(), span)
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E4130,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(
    args: &[ValueRef],
    min: usize,
    max: usize,
    name: &str,
    span: Span,
) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E4130,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn string_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn float_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<f64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n as f64),
        Value::Float(f) => Ok(*f),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a number as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn handle_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(id) if *id > 0 => Ok(*id),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a model handle as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn optional_object(args: &[ValueRef], idx: usize) -> Option<HashMap<String, ValueRef>> {
    args.get(idx).and_then(|v| match &*v.borrow() {
        Value::Object(m) => Some(m.clone()),
        Value::Nil => None,
        _ => None,
    })
}

fn field_bool(map: Option<&HashMap<String, ValueRef>>, key: &str, default: bool) -> bool {
    let Some(map) = map else {
        return default;
    };
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Bool(b)) => b,
        Some(Value::Int(n)) => n != 0,
        _ => default,
    }
}

fn field_f64(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Option<f64> {
    let map = map?;
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Float(f)) => Some(f),
        Some(Value::Int(n)) => Some(n as f64),
        _ => None,
    }
}

fn field_i64(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Option<i64> {
    let map = map?;
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::Int(n)) => Some(n),
        Some(Value::Float(f)) => Some(f as i64),
        _ => None,
    }
}

fn field_string(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Option<String> {
    let map = map?;
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::String(s)) => Some(s),
        _ => None,
    }
}

fn floats_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<f32>> {
    match &*args[idx].borrow() {
        Value::FloatArray(v) => Ok(v.iter().map(|&x| x as f32).collect()),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match &*item.borrow() {
                    Value::Float(f) => out.push(*f as f32),
                    Value::Int(n) => out.push(*n as f32),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() expects numeric array as argument {}, got {}",
                                idx + 1,
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() expects float_array/array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn load_opts(map: Option<&HashMap<String, ValueRef>>) -> LoadOptions {
    let mut opts = LoadOptions::default();
    if let Some(map) = map {
        opts.use_gpu = field_bool(Some(map), "gpu", opts.use_gpu);
        opts.dtw_timestamps = field_bool(Some(map), "dtw", opts.dtw_timestamps);
    }
    opts
}

fn transcribe_opts(map: Option<&HashMap<String, ValueRef>>) -> TranscribeOptions {
    let mut opts = TranscribeOptions::default();
    let Some(map) = map else {
        return opts;
    };
    opts.language = field_string(Some(map), "language");
    opts.translate = field_bool(Some(map), "translate", opts.translate);
    opts.detect_language = field_bool(Some(map), "detect_language", opts.detect_language);
    opts.token_timestamps = field_bool(Some(map), "token_timestamps", opts.token_timestamps);
    if let Some(v) = field_i64(Some(map), "offset_ms") {
        opts.offset_ms = v as i32;
    }
    if let Some(v) = field_i64(Some(map), "duration_ms") {
        opts.duration_ms = v as i32;
    }
    if let Some(v) = field_f64(Some(map), "temperature") {
        opts.temperature = v as f32;
    }
    opts.suppress_blank = field_bool(Some(map), "suppress_blank", opts.suppress_blank);
    if let Some(v) = field_i64(Some(map), "max_len") {
        opts.max_len = v as i32;
    }
    opts.initial_prompt = field_string(Some(map), "initial_prompt");
    if let Some(v) = field_f64(Some(map), "no_speech_threshold") {
        opts.no_speech_threshold = v as f32;
    }
    opts
}

fn vad_opts(map: Option<&HashMap<String, ValueRef>>) -> VadOptions {
    let mut opts = VadOptions::default();
    let Some(map) = map else {
        return opts;
    };
    if let Some(v) = field_i64(Some(map), "frame_ms") {
        opts.frame_ms = v as u32;
    }
    if let Some(v) = field_f64(Some(map), "threshold") {
        opts.threshold = v as f32;
    }
    if let Some(v) = field_f64(Some(map), "min_speech") {
        opts.min_speech_secs = v;
    }
    if let Some(v) = field_f64(Some(map), "pad") {
        opts.pad_secs = v;
    }
    if let Some(v) = field_f64(Some(map), "min_silence") {
        opts.min_silence_secs = v;
    }
    opts
}

fn transcript_value(tr: &niao_speech::Transcript) -> ValueRef {
    let mut root = HashMap::new();
    root.insert("text".into(), Value::String(tr.text.clone()).ref_cell());
    if let Some(ref lang) = tr.language {
        root.insert("language".into(), Value::String(lang.clone()).ref_cell());
    }
    root.insert("duration".into(), Value::Float(tr.duration_secs).ref_cell());
    let mut segs = Vec::new();
    for s in &tr.segments {
        let mut m = HashMap::new();
        m.insert("start".into(), Value::Float(s.start).ref_cell());
        m.insert("end".into(), Value::Float(s.end).ref_cell());
        m.insert("text".into(), Value::String(s.text.clone()).ref_cell());
        m.insert(
            "no_speech_prob".into(),
            Value::Float(s.no_speech_prob as f64).ref_cell(),
        );
        segs.push(Value::Object(m).ref_cell());
    }
    root.insert("segments".into(), Value::Array(segs).ref_cell());
    if !tr.tokens.is_empty() {
        let mut toks = Vec::new();
        for t in &tr.tokens {
            let mut m = HashMap::new();
            m.insert("text".into(), Value::String(t.text.clone()).ref_cell());
            m.insert("start".into(), Value::Float(t.start).ref_cell());
            m.insert("end".into(), Value::Float(t.end).ref_cell());
            m.insert("prob".into(), Value::Float(t.prob as f64).ref_cell());
            toks.push(Value::Object(m).ref_cell());
        }
        root.insert("tokens".into(), Value::Array(toks).ref_cell());
    }
    Value::Object(root).ref_cell()
}

fn mic_device_value(d: &MicDevice) -> ValueRef {
    let mut m = HashMap::new();
    m.insert("index".into(), Value::Int(d.index as i64).ref_cell());
    m.insert("name".into(), Value::String(d.name.clone()).ref_cell());
    m.insert("default".into(), Value::Bool(d.is_default).ref_cell());
    Value::Object(m).ref_cell()
}

fn ok_or_soft<T>(span: Span, r: Result<T, SpeechError>, f: impl FnOnce(T) -> ValueRef) -> ValueRef {
    match r {
        Ok(v) => f(v),
        Err(e) => soft_err(span, e),
    }
}

// >>> import "nspeech"; len(nspeech.version()) > 0
// => true
fn nspeech_version(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nspeech_version", span)?;
    Ok(Value::String(engine_version()).ref_cell())
}

// >>> import "nspeech"; len(nspeech.languages()) > 0
// => true
fn nspeech_languages(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nspeech_languages", span)?;
    let langs = language_codes();
    Ok(Value::Array(
        langs
            .into_iter()
            .map(|s| Value::String(s).ref_cell())
            .collect(),
    )
    .ref_cell())
}

// >>> import "nspeech"; let r = nspeech.load("/missing/model.bin"); is_error(r)
// => true
fn nspeech_load(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nspeech_load", span)?;
    let path = string_arg(args, 0, "nspeech_load", span)?;
    let opts = load_opts(optional_object(args, 1).as_ref());
    Ok(ok_or_soft(span, load_model(&path, &opts), |m| {
        Value::Int(alloc(m)).ref_cell()
    }))
}

// >>> import "nspeech"; is_error(nspeech.close(999999))
// => true
fn nspeech_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nspeech_close", span)?;
    let id = handle_arg(args, 0, "nspeech_close", span)?;
    if remove_model(id) {
        Ok(Value::Bool(true).ref_cell())
    } else {
        Ok(soft_err(span, SpeechError::InvalidHandle))
    }
}

// >>> import "nspeech"; let r = nspeech.transcribe(999999, [0.0]); is_error(r)
// => true
fn nspeech_transcribe(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nspeech_transcribe", span)?;
    let id = handle_arg(args, 0, "nspeech_transcribe", span)?;
    let samples = floats_arg(args, 1, "nspeech_transcribe", span)?;
    let opts = transcribe_opts(optional_object(args, 2).as_ref());
    match with_model(id, span, |m| m.transcribe(&samples, &opts))? {
        Ok(Ok(tr)) => Ok(transcript_value(&tr)),
        Ok(Err(e)) => Ok(soft_err(span, e)),
        Err(e) => Ok(e),
    }
}

// >>> import "nspeech"; let r = nspeech.transcribe_file(999999, "/missing.wav"); is_error(r)
// => true
fn nspeech_transcribe_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nspeech_transcribe_file", span)?;
    let id = handle_arg(args, 0, "nspeech_transcribe_file", span)?;
    let path = string_arg(args, 1, "nspeech_transcribe_file", span)?;
    let opts = transcribe_opts(optional_object(args, 2).as_ref());
    match with_model(id, span, |m| m.transcribe_file(&path, &opts))? {
        Ok(Ok(tr)) => Ok(transcript_value(&tr)),
        Ok(Err(e)) => Ok(soft_err(span, e)),
        Err(e) => Ok(e),
    }
}

// >>> import "nspeech"; let r = nspeech.detect_language(999999, [0.0]); is_error(r)
// => true
fn nspeech_detect_language(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nspeech_detect_language", span)?;
    let id = handle_arg(args, 0, "nspeech_detect_language", span)?;
    let samples = floats_arg(args, 1, "nspeech_detect_language", span)?;
    match with_model(id, span, |m| m.detect_language(&samples))? {
        Ok(Ok(lang)) => Ok(Value::String(lang).ref_cell()),
        Ok(Err(e)) => Ok(soft_err(span, e)),
        Err(e) => Ok(e),
    }
}

// >>> import "nspeech"; let r = nspeech.load_audio("/missing.wav"); is_error(r)
// => true
fn nspeech_load_audio(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nspeech_load_audio", span)?;
    let path = string_arg(args, 0, "nspeech_load_audio", span)?;
    Ok(ok_or_soft(span, load_wav(&path), |(samples, rate)| {
        let mut m = HashMap::new();
        m.insert(
            "samples".into(),
            Value::FloatArray(samples.iter().map(|&s| s as f64).collect()).ref_cell(),
        );
        m.insert("sample_rate".into(), Value::Int(rate as i64).ref_cell());
        m.insert("channels".into(), Value::Int(1).ref_cell());
        Value::Object(m).ref_cell()
    }))
}

// >>> import "nspeech"; len(nspeech.resample([0.0, 1.0], 16000, 8000))
// => 1
fn nspeech_resample(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nspeech_resample", span)?;
    let samples = floats_arg(args, 0, "nspeech_resample", span)?;
    let from_rate = float_arg(args, 1, "nspeech_resample", span)? as u32;
    let to_rate = if args.len() > 2 {
        float_arg(args, 2, "nspeech_resample", span)? as u32
    } else {
        WHISPER_SAMPLE_RATE
    };
    Ok(ok_or_soft(
        span,
        resample_linear(&samples, from_rate, to_rate),
        |out| Value::FloatArray(out.iter().map(|&s| s as f64).collect()).ref_cell(),
    ))
}

// >>> import "nspeech"; len(nspeech.vad([]))
// => 0
fn nspeech_vad(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nspeech_vad", span)?;
    let samples = floats_arg(args, 0, "nspeech_vad", span)?;
    let opts = vad_opts(optional_object(args, 1).as_ref());
    Ok(ok_or_soft(
        span,
        detect_voice(&samples, WHISPER_SAMPLE_RATE, &opts),
        |segs| {
            let arr = segs
                .into_iter()
                .map(|s| {
                    let mut m = HashMap::new();
                    m.insert("start".into(), Value::Float(s.start).ref_cell());
                    m.insert("end".into(), Value::Float(s.end).ref_cell());
                    Value::Object(m).ref_cell()
                })
                .collect();
            Value::Array(arr).ref_cell()
        },
    ))
}

// >>> import "nspeech"; let r = nspeech.vad_file("/missing.wav"); is_error(r)
// => true
fn nspeech_vad_file(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nspeech_vad_file", span)?;
    let path = string_arg(args, 0, "nspeech_vad_file", span)?;
    let opts = vad_opts(optional_object(args, 1).as_ref());
    let loaded = match load_wav(&path) {
        Ok(v) => v,
        Err(e) => return Ok(soft_err(span, e)),
    };
    Ok(ok_or_soft(
        span,
        detect_voice(&loaded.0, WHISPER_SAMPLE_RATE, &opts),
        |segs| {
            let arr = segs
                .into_iter()
                .map(|s| {
                    let mut m = HashMap::new();
                    m.insert("start".into(), Value::Float(s.start).ref_cell());
                    m.insert("end".into(), Value::Float(s.end).ref_cell());
                    Value::Object(m).ref_cell()
                })
                .collect();
            Value::Array(arr).ref_cell()
        },
    ))
}

// >>> import "nspeech"; type(nspeech.mic_devices()) == "array"
// => true
fn nspeech_mic_devices(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nspeech_mic_devices", span)?;
    Ok(ok_or_soft(span, mic_devices(), |devs| {
        Value::Array(devs.iter().map(mic_device_value).collect()).ref_cell()
    }))
}

// >>> import "nspeech"; let r = nspeech.mic_record(0.0); is_error(r)
// => true
fn nspeech_mic_record(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nspeech_mic_record", span)?;
    let secs = float_arg(args, 0, "nspeech_mic_record", span)?;
    let device = optional_object(args, 1).and_then(|m| field_i64(Some(&m), "device"));
    Ok(ok_or_soft(
        span,
        mic_record(secs, device.map(|d| d as usize)),
        |samples| Value::FloatArray(samples.iter().map(|&s| s as f64).collect()).ref_cell(),
    ))
}

// >>> import "nspeech"; let r = nspeech.mic_transcribe(999999, 0.0); is_error(r)
// => true
fn nspeech_mic_transcribe(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nspeech_mic_transcribe", span)?;
    let id = handle_arg(args, 0, "nspeech_mic_transcribe", span)?;
    let secs = float_arg(args, 1, "nspeech_mic_transcribe", span)?;
    let opts_map = optional_object(args, 2);
    let device = opts_map.as_ref().and_then(|m| field_i64(Some(m), "device"));
    let tr_opts = transcribe_opts(opts_map.as_ref());
    let samples = match mic_record(secs, device.map(|d| d as usize)) {
        Ok(s) => s,
        Err(e) => return Ok(soft_err(span, e)),
    };
    match with_model(id, span, |m| m.transcribe(&samples, &tr_opts))? {
        Ok(Ok(tr)) => Ok(transcript_value(&tr)),
        Ok(Err(e)) => Ok(soft_err(span, e)),
        Err(e) => Ok(e),
    }
}

macro_rules! nspeech_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nspeech_fns![
    ("nspeech_version", "version", nspeech_version),
    ("nspeech_languages", "languages", nspeech_languages),
    ("nspeech_load", "load", nspeech_load),
    ("nspeech_close", "close", nspeech_close),
    ("nspeech_transcribe", "transcribe", nspeech_transcribe),
    (
        "nspeech_transcribe_file",
        "transcribe_file",
        nspeech_transcribe_file
    ),
    (
        "nspeech_detect_language",
        "detect_language",
        nspeech_detect_language
    ),
    ("nspeech_load_audio", "load_audio", nspeech_load_audio),
    ("nspeech_resample", "resample", nspeech_resample),
    ("nspeech_vad", "vad", nspeech_vad),
    ("nspeech_vad_file", "vad_file", nspeech_vad_file),
    ("nspeech_mic_devices", "mic_devices", nspeech_mic_devices),
    ("nspeech_mic_record", "mic_record", nspeech_mic_record),
    (
        "nspeech_mic_transcribe",
        "mic_transcribe",
        nspeech_mic_transcribe
    ),
];

pub const MODULE_NAME: &str = "nspeech";
pub const MODULE_PATHS: &[&str] = &["nspeech", "std/nspeech"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs()
        .into_iter()
        .map(|(f, _, fn_)| (f, fn_))
        .collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}
