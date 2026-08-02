//! Native ntts standard library — text-to-speech via Piper ONNX + eSpeak NG
//! (~pyttsx3 subset).
//!
//! Import with `import "ntts"` (or `import "std/ntts"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_tts::{list_engines, list_voices, version, SynthOptions, TtsEngine, TtsError};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

const E4138: u32 = codes::E4138_NTTS_ARITY;
const E4139: u32 = codes::E4139_NTTS_ERROR;
const E4140: u32 = codes::E4140_NTTS_TYPE;
const E4141: u32 = codes::E4141_NTTS_PARAM;
const E4142: u32 = codes::E4142_NTTS_INVALID_HANDLE;
const E4143: u32 = codes::E4143_NTTS_MODEL;
const E4144: u32 = codes::E4144_NTTS_SYNTH;
const E4145: u32 = codes::E4145_NTTS_AUDIO;

thread_local! {
    static ENGINES: RefCell<HashMap<i64, TtsEngine>> = RefCell::new(HashMap::new());
    static NEXT_ID: RefCell<i64> = const { RefCell::new(1) };
}

fn alloc(engine: TtsEngine) -> i64 {
    let id = NEXT_ID.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    ENGINES.with(|m| m.borrow_mut().insert(id, engine));
    id
}

fn with_engine<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&TtsEngine) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    ENGINES.with(|m| match m.borrow().get(&id) {
        Some(engine) => Ok(Ok(f(engine))),
        None => Ok(Err(error_value(
            E4142,
            "ntts_error",
            format!("invalid or closed ntts engine handle {id}"),
            span,
        ))),
    })
}

fn with_engine_mut<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut TtsEngine) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    ENGINES.with(|m| match m.borrow_mut().get_mut(&id) {
        Some(engine) => Ok(Ok(f(engine))),
        None => Ok(Err(error_value(
            E4142,
            "ntts_error",
            format!("invalid or closed ntts engine handle {id}"),
            span,
        ))),
    })
}

fn remove_engine(id: i64) -> bool {
    ENGINES.with(|m| m.borrow_mut().remove(&id).is_some())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E4140, msg.into())
}

fn soft_err(span: Span, err: TtsError) -> ValueRef {
    let code = match &err {
        TtsError::Empty | TtsError::InvalidHandle => E4139,
        TtsError::Param(_) | TtsError::Property(_) => E4141,
        TtsError::Io(_) => E4143,
        TtsError::Model(_) => E4143,
        TtsError::Synth(_) => E4144,
        TtsError::Audio(_) => E4145,
    };
    error_value(code, "ntts_error", err.message(), span)
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E4138,
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
            E4138,
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

fn handle_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(id) if *id > 0 => Ok(*id),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an engine handle as argument {}, got {}",
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

fn field_string(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Option<String> {
    let map = map?;
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::String(s)) => Some(s),
        _ => None,
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

fn synth_opts(map: Option<&HashMap<String, ValueRef>>) -> SynthOptions {
    let mut opts = SynthOptions::default();
    let Some(map) = map else {
        return opts;
    };
    opts.voice = field_string(Some(map), "voice");
    opts.speaker = field_i64(Some(map), "speaker");
    if let Some(v) = field_f64(Some(map), "rate") {
        opts.rate = Some(v as f32);
    }
    if let Some(v) = field_f64(Some(map), "length_scale") {
        opts.length_scale = Some(v as f32);
    }
    if let Some(v) = field_f64(Some(map), "volume") {
        opts.volume = Some(v as f32);
    }
    if let Some(v) = field_f64(Some(map), "noise_scale") {
        opts.noise_scale = Some(v as f32);
    }
    if let Some(v) = field_f64(Some(map), "noise_w") {
        opts.noise_w = Some(v as f32);
    }
    opts
}

fn synth_result_value(result: &niao_tts::SynthResult) -> ValueRef {
    let mut root = HashMap::new();
    root.insert(
        "samples".into(),
        Value::FloatArray(result.samples.iter().map(|&s| s as f64).collect()).ref_cell(),
    );
    root.insert(
        "sample_rate".into(),
        Value::Int(result.sample_rate as i64).ref_cell(),
    );
    root.insert("duration".into(), Value::Float(result.duration).ref_cell());
    Value::Object(root).ref_cell()
}

// >>> import "ntts"; len(ntts.version()) > 0
// => true
fn ntts_version(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ntts_version", span)?;
    Ok(Value::String(version()).ref_cell())
}

// >>> import "ntts"; len(ntts.engines()) >= 2
// => true
fn ntts_engines(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ntts_engines", span)?;
    let mut arr = Vec::new();
    for eng in list_engines() {
        let mut m = HashMap::new();
        for (k, v) in eng {
            m.insert(k.to_string(), Value::String(v.to_string()).ref_cell());
        }
        arr.push(Value::Object(m).ref_cell());
    }
    Ok(Value::Array(arr).ref_cell())
}

// >>> import "ntts"; let r = ntts.load("/missing/model.onnx"); is_error(r)
// => true
fn ntts_load(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ntts_load", span)?;
    let path = string_arg(args, 0, "ntts_load", span)?;
    match TtsEngine::load_piper(Path::new(&path)) {
        Ok(engine) => Ok(Value::Int(alloc(engine)).ref_cell()),
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "ntts"; type(ntts.init_espeak()) == "int"
// => true
fn ntts_init_espeak(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "ntts_init_espeak", span)?;
    let opts = optional_object(args, 0);
    let voice = field_string(opts.as_ref(), "voice");
    match TtsEngine::init_espeak(voice.as_deref()) {
        Ok(engine) => Ok(Value::Int(alloc(engine)).ref_cell()),
        Err(e) => Ok(soft_err(span, e)),
    }
}

// >>> import "ntts"; is_error(ntts.close(999999))
// => true
fn ntts_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntts_close", span)?;
    let id = handle_arg(args, 0, "ntts_close", span)?;
    if remove_engine(id) {
        Ok(Value::Bool(true).ref_cell())
    } else {
        Ok(soft_err(span, TtsError::InvalidHandle))
    }
}

// >>> import "ntts"; let r = ntts.voices(999999); is_error(r)
// => true
fn ntts_voices(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntts_voices", span)?;
    let id = handle_arg(args, 0, "ntts_voices", span)?;
    match with_engine(id, span, |e| e.voices())? {
        Ok(voices) => {
            let mut arr = Vec::new();
            for (name, id) in voices {
                let mut m = HashMap::new();
                m.insert("name".into(), Value::String(name).ref_cell());
                m.insert("id".into(), Value::Int(id).ref_cell());
                arr.push(Value::Object(m).ref_cell());
            }
            Ok(Value::Array(arr).ref_cell())
        }
        Err(e) => Ok(e),
    }
}

// >>> import "ntts"; let r = ntts.synth(999999, "hi"); is_error(r)
// => true
fn ntts_synth(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntts_synth", span)?;
    let id = handle_arg(args, 0, "ntts_synth", span)?;
    let text = string_arg(args, 1, "ntts_synth", span)?;
    let opts = synth_opts(optional_object(args, 2).as_ref());
    match with_engine_mut(id, span, |e| e.synth(&text, &opts))? {
        Ok(Ok(result)) => Ok(synth_result_value(&result)),
        Ok(Err(e)) => Ok(soft_err(span, e)),
        Err(e) => Ok(e),
    }
}

// >>> import "ntts"; let r = ntts.synth_wav(999999, "hi"); is_error(r)
// => true
fn ntts_synth_wav(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntts_synth_wav", span)?;
    let id = handle_arg(args, 0, "ntts_synth_wav", span)?;
    let text = string_arg(args, 1, "ntts_synth_wav", span)?;
    let opts = synth_opts(optional_object(args, 2).as_ref());
    match with_engine_mut(id, span, |e| e.synth_wav(&text, &opts))? {
        Ok(Ok(bytes)) => Ok(Value::ByteArray(bytes).ref_cell()),
        Ok(Err(e)) => Ok(soft_err(span, e)),
        Err(e) => Ok(e),
    }
}

// >>> import "ntts"; let r = ntts.save(999999, "hi", "/tmp/x.wav"); is_error(r)
// => true
fn ntts_save(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "ntts_save", span)?;
    let id = handle_arg(args, 0, "ntts_save", span)?;
    let text = string_arg(args, 1, "ntts_save", span)?;
    let path = string_arg(args, 2, "ntts_save", span)?;
    let opts = synth_opts(optional_object(args, 3).as_ref());
    match with_engine_mut(id, span, |e| e.save(&text, Path::new(&path), &opts))? {
        Ok(Ok(())) => Ok(Value::Bool(true).ref_cell()),
        Ok(Err(e)) => Ok(soft_err(span, e)),
        Err(e) => Ok(e),
    }
}

// >>> import "ntts"; let r = ntts.speak(999999, "hi"); is_error(r)
// => true
fn ntts_speak(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntts_speak", span)?;
    let id = handle_arg(args, 0, "ntts_speak", span)?;
    let text = string_arg(args, 1, "ntts_speak", span)?;
    let opts = synth_opts(optional_object(args, 2).as_ref());
    match with_engine_mut(id, span, |e| e.speak(&text, &opts))? {
        Ok(Ok(())) => Ok(Value::Bool(true).ref_cell()),
        Ok(Err(e)) => Ok(soft_err(span, e)),
        Err(e) => Ok(e),
    }
}

// >>> import "ntts"; let r = ntts.get(999999, "voice"); is_error(r)
// => true
fn ntts_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ntts_get", span)?;
    let id = handle_arg(args, 0, "ntts_get", span)?;
    let prop = string_arg(args, 1, "ntts_get", span)?;
    match with_engine(id, span, |e| e.get(&prop))? {
        Ok(Ok(v)) => Ok(Value::String(v).ref_cell()),
        Ok(Err(e)) => Ok(soft_err(span, e)),
        Err(e) => Ok(e),
    }
}

// >>> import "ntts"; let r = ntts.set(999999, "rate", "1.0"); is_error(r)
// => true
fn ntts_set(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "ntts_set", span)?;
    let id = handle_arg(args, 0, "ntts_set", span)?;
    let prop = string_arg(args, 1, "ntts_set", span)?;
    let value = string_arg(args, 2, "ntts_set", span)?;
    match with_engine_mut(id, span, |e| e.set(&prop, &value))? {
        Ok(Ok(())) => Ok(Value::Bool(true).ref_cell()),
        Ok(Err(e)) => Ok(soft_err(span, e)),
        Err(e) => Ok(e),
    }
}

// >>> import "ntts"; type(ntts.list_voices()) == "array"
// => true
fn ntts_list_voices(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ntts_list_voices", span)?;
    match list_voices() {
        Ok(voices) => {
            let mut arr = Vec::new();
            for v in voices {
                let mut m = HashMap::new();
                m.insert("name".into(), Value::String(v.name).ref_cell());
                m.insert("language".into(), Value::String(v.language).ref_cell());
                m.insert("id".into(), Value::String(v.identifier).ref_cell());
                arr.push(Value::Object(m).ref_cell());
            }
            Ok(Value::Array(arr).ref_cell())
        }
        Err(e) => Ok(soft_err(span, e)),
    }
}

macro_rules! ntts_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ntts_fns![
    ("ntts_version", "version", ntts_version),
    ("ntts_engines", "engines", ntts_engines),
    ("ntts_load", "load", ntts_load),
    ("ntts_init_espeak", "init_espeak", ntts_init_espeak),
    ("ntts_close", "close", ntts_close),
    ("ntts_voices", "voices", ntts_voices),
    ("ntts_list_voices", "list_voices", ntts_list_voices),
    ("ntts_synth", "synth", ntts_synth),
    ("ntts_synth_wav", "synth_wav", ntts_synth_wav),
    ("ntts_save", "save", ntts_save),
    ("ntts_speak", "speak", ntts_speak),
    ("ntts_get", "get", ntts_get),
    ("ntts_set", "set", ntts_set),
];

pub const MODULE_NAME: &str = "ntts";
pub const MODULE_PATHS: &[&str] = &["ntts", "std/ntts"];

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
