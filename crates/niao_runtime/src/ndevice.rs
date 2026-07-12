//! Native ndevice standard library — the device brain that ties ncpu / ngpu /
//! nram / nnpu together:
//!
//! - `detect()` — one call, full hardware report.
//! - `profile("eco" | "balanced" | "performance")` — preset limits + max temps.
//! - `guard_start()` — background safety monitor: samples temperatures and
//!   memory every tick and raises a global throttle level (0 ok · 1 warm ·
//!   2 hot · 3 critical) when the device is in trouble.
//! - `pace()` — drop into hot loops; sleeps 0/2/8/25 ms by throttle level so
//!   an overheating device automatically gets less work and cools down.
//! - `threads()` — worker count under CPU limit + throttle.
//! - `best_device(task?)` — "gpu" | "npu" | "cpu" for the current machine,
//!   budgets, and temperature situation.
//!
//! Import with `import "ndevice"` (or `import "std/ndevice"`).

use crate::hw;
use crate::{NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::{Mutex, OnceLock};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::TypeError {
        message: msg.into(),
        line: span.line,
        col: span.col,
    }
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E2740_NDEVICE_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
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

fn int_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an int as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn opt_num(n: i64) -> ValueRef {
    if n < 0 {
        Value::Nil.ref_cell()
    } else {
        Value::Int(n).ref_cell()
    }
}

fn obj_int(map: &HashMap<String, ValueRef>, key: &str) -> Option<i64> {
    map.get(key).and_then(|v| match &*v.borrow() {
        Value::Int(n) => Some(*n),
        _ => None,
    })
}

fn profile_slot() -> &'static Mutex<String> {
    static SLOT: OnceLock<Mutex<String>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(String::from("custom")))
}

fn set_profile_name(name: &str) {
    if let Ok(mut p) = profile_slot().lock() {
        *p = name.to_string();
    }
}

// ---------------------------------------------------------------------------
// Detection & reports
// ---------------------------------------------------------------------------

fn cpu_report() -> ValueRef {
    let mut map = HashMap::new();
    map.insert("cores".to_string(), Value::Int(hw::logical_cores() as i64).ref_cell());
    map.insert("brand".to_string(), Value::String(hw::cpu_brand()).ref_cell());
    map.insert("arch".to_string(), Value::String(std::env::consts::ARCH.to_string()).ref_cell());
    map.insert("temp_c".to_string(), opt_num(hw::cpu_temp_c()));
    map.insert(
        "limit_pct".to_string(),
        Value::Int(hw::CPU_LIMIT_PCT.load(Ordering::Relaxed) as i64).ref_cell(),
    );
    Value::Object(map).ref_cell()
}

fn gpu_report() -> ValueRef {
    let snap = hw::gpu_snapshot();
    let mut map = HashMap::new();
    map.insert("available".to_string(), Value::Bool(!snap.gpus.is_empty()).ref_cell());
    map.insert("count".to_string(), Value::Int(snap.gpus.len() as i64).ref_cell());
    map.insert("backend".to_string(), Value::String(snap.backend.to_string()).ref_cell());
    if let Some(g) = snap.gpus.first() {
        map.insert("name".to_string(), Value::String(g.name.clone()).ref_cell());
        map.insert("vram_total_mb".to_string(), opt_num(g.vram_total_mb));
        map.insert("temp_c".to_string(), opt_num(g.temp_c));
    } else {
        map.insert("name".to_string(), Value::Nil.ref_cell());
    }
    map.insert(
        "limit_pct".to_string(),
        Value::Int(hw::GPU_LIMIT_PCT.load(Ordering::Relaxed) as i64).ref_cell(),
    );
    Value::Object(map).ref_cell()
}

fn ram_report() -> ValueRef {
    let (total, avail) = hw::ram_stats_mb();
    let mut map = HashMap::new();
    map.insert("total_mb".to_string(), opt_num(total));
    map.insert("available_mb".to_string(), opt_num(avail));
    map.insert("process_mb".to_string(), Value::Int(hw::process_mb()).ref_cell());
    map.insert("limit_mb".to_string(), Value::Int(hw::ram_budget_mb()).ref_cell());
    map.insert(
        "pressure".to_string(),
        Value::String(hw::ram_pressure().to_string()).ref_cell(),
    );
    Value::Object(map).ref_cell()
}

fn npu_report() -> ValueRef {
    let npu = hw::npu_detect();
    let mut map = HashMap::new();
    map.insert("available".to_string(), Value::Bool(npu.present).ref_cell());
    map.insert("vendor".to_string(), Value::String(npu.vendor).ref_cell());
    map.insert("name".to_string(), Value::String(npu.name).ref_cell());
    Value::Object(map).ref_cell()
}

fn ndevice_detect(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 0, "ndevice_detect", span)?;
    let mut map = HashMap::new();
    map.insert("cpu".to_string(), cpu_report());
    map.insert("gpu".to_string(), gpu_report());
    map.insert("ram".to_string(), ram_report());
    map.insert("npu".to_string(), npu_report());
    map.insert("os".to_string(), Value::String(std::env::consts::OS.to_string()).ref_cell());
    Ok(Value::Object(map).ref_cell())
}

fn ndevice_summary(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 0, "ndevice_summary", span)?;
    let cores = hw::logical_cores();
    let (total, _) = hw::ram_stats_mb();
    let snap = hw::gpu_snapshot();
    let gpu = snap
        .gpus
        .first()
        .map(|g| g.name.clone())
        .unwrap_or_else(|| "no GPU".to_string());
    let npu = hw::npu_detect();
    let npu_part = if npu.present { npu.name } else { "no NPU".to_string() };
    let ram_part = if total > 0 {
        format!("{:.1} GB RAM", total as f64 / 1024.0)
    } else {
        "RAM unknown".to_string()
    };
    let line = format!(
        "{} · {cores} cores · {ram_part} · {gpu} · {npu_part}",
        hw::cpu_brand()
    );
    Ok(Value::String(line).ref_cell())
}

// ---------------------------------------------------------------------------
// Profiles & limits
// ---------------------------------------------------------------------------

fn apply_profile(name: &str) -> bool {
    let (cpu, gpu, gpu_max, cpu_max) = match name {
        "eco" => (50u8, 50u8, 75u8, 85u8),
        "balanced" => (75, 80, 80, 90),
        "performance" => (100, 100, 85, 95),
        _ => return false,
    };
    hw::CPU_LIMIT_PCT.store(cpu, Ordering::Relaxed);
    hw::GPU_LIMIT_PCT.store(gpu, Ordering::Relaxed);
    hw::NPU_LIMIT_PCT.store(gpu, Ordering::Relaxed);
    hw::GPU_MAX_TEMP_C.store(gpu_max, Ordering::Relaxed);
    hw::CPU_MAX_TEMP_C.store(cpu_max, Ordering::Relaxed);
    set_profile_name(name);
    true
}

fn ndevice_profile(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ndevice_profile", span)?;
    let name = string_arg(args, 0, "ndevice_profile", span)?;
    if !apply_profile(&name) {
        return Err(type_err(
            span,
            format!("ndevice_profile() unknown profile '{name}' (eco|balanced|performance)"),
        ));
    }
    Ok(Value::Nil.ref_cell())
}

fn ndevice_get_profile(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 0, "ndevice_get_profile", span)?;
    let name = profile_slot()
        .lock()
        .map(|p| p.clone())
        .unwrap_or_else(|_| "custom".to_string());
    Ok(Value::String(name).ref_cell())
}

/// Bulk limits: {cpu_pct, gpu_pct, npu_pct, ram_mb, ram_pct, gpu_max_temp, cpu_max_temp}.
fn ndevice_set_limits(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ndevice_set_limits", span)?;
    let map = match &*args[0].borrow() {
        Value::Object(map) => map.clone(),
        other => {
            return Err(type_err(
                span,
                format!("ndevice_set_limits() expects an object, got {}", other.type_name()),
            ))
        }
    };
    let check_pct = |v: i64, key: &str| -> NiaoResult<u8> {
        if (1..=100).contains(&v) {
            Ok(v as u8)
        } else {
            Err(type_err(span, format!("ndevice_set_limits() {key} must be 1..=100")))
        }
    };
    if let Some(v) = obj_int(&map, "cpu_pct") {
        hw::CPU_LIMIT_PCT.store(check_pct(v, "cpu_pct")?, Ordering::Relaxed);
    }
    if let Some(v) = obj_int(&map, "gpu_pct") {
        hw::GPU_LIMIT_PCT.store(check_pct(v, "gpu_pct")?, Ordering::Relaxed);
    }
    if let Some(v) = obj_int(&map, "npu_pct") {
        hw::NPU_LIMIT_PCT.store(check_pct(v, "npu_pct")?, Ordering::Relaxed);
    }
    if let Some(v) = obj_int(&map, "ram_mb") {
        if v < 0 {
            return Err(type_err(span, "ndevice_set_limits() ram_mb must be >= 0"));
        }
        hw::RAM_LIMIT_MB.store(v, Ordering::Relaxed);
    }
    if let Some(v) = obj_int(&map, "ram_pct") {
        if !(0..=100).contains(&v) {
            return Err(type_err(span, "ndevice_set_limits() ram_pct must be 0..=100"));
        }
        hw::RAM_LIMIT_PCT.store(v as u8, Ordering::Relaxed);
    }
    if let Some(v) = obj_int(&map, "gpu_max_temp") {
        if !(0..=110).contains(&v) {
            return Err(type_err(span, "ndevice_set_limits() gpu_max_temp must be 0..=110"));
        }
        hw::GPU_MAX_TEMP_C.store(v as u8, Ordering::Relaxed);
    }
    if let Some(v) = obj_int(&map, "cpu_max_temp") {
        if !(0..=110).contains(&v) {
            return Err(type_err(span, "ndevice_set_limits() cpu_max_temp must be 0..=110"));
        }
        hw::CPU_MAX_TEMP_C.store(v as u8, Ordering::Relaxed);
    }
    set_profile_name("custom");
    Ok(Value::Nil.ref_cell())
}

fn ndevice_limits(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 0, "ndevice_limits", span)?;
    let mut map = HashMap::new();
    map.insert(
        "cpu_pct".to_string(),
        Value::Int(hw::CPU_LIMIT_PCT.load(Ordering::Relaxed) as i64).ref_cell(),
    );
    map.insert(
        "gpu_pct".to_string(),
        Value::Int(hw::GPU_LIMIT_PCT.load(Ordering::Relaxed) as i64).ref_cell(),
    );
    map.insert(
        "npu_pct".to_string(),
        Value::Int(hw::NPU_LIMIT_PCT.load(Ordering::Relaxed) as i64).ref_cell(),
    );
    map.insert("ram_mb".to_string(), Value::Int(hw::ram_budget_mb()).ref_cell());
    map.insert(
        "gpu_max_temp".to_string(),
        Value::Int(hw::GPU_MAX_TEMP_C.load(Ordering::Relaxed) as i64).ref_cell(),
    );
    map.insert(
        "cpu_max_temp".to_string(),
        Value::Int(hw::CPU_MAX_TEMP_C.load(Ordering::Relaxed) as i64).ref_cell(),
    );
    Ok(Value::Object(map).ref_cell())
}

// ---------------------------------------------------------------------------
// Safety guard
// ---------------------------------------------------------------------------

/// guard_start(opts?) — opts: {interval_ms, gpu_max_temp, cpu_max_temp}.
fn ndevice_guard_start(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "ndevice_guard_start", span)?;
    let mut interval_ms: i64 = 1000;
    if let Some(v) = args.first() {
        match &*v.borrow() {
            Value::Object(map) => {
                if let Some(iv) = obj_int(map, "interval_ms") {
                    interval_ms = iv;
                }
                if let Some(t) = obj_int(map, "gpu_max_temp") {
                    if !(0..=110).contains(&t) {
                        return Err(type_err(span, "gpu_max_temp must be 0..=110"));
                    }
                    hw::GPU_MAX_TEMP_C.store(t as u8, Ordering::Relaxed);
                }
                if let Some(t) = obj_int(map, "cpu_max_temp") {
                    if !(0..=110).contains(&t) {
                        return Err(type_err(span, "cpu_max_temp must be 0..=110"));
                    }
                    hw::CPU_MAX_TEMP_C.store(t as u8, Ordering::Relaxed);
                }
            }
            Value::Int(iv) => interval_ms = *iv,
            Value::Nil => {}
            other => {
                return Err(type_err(
                    span,
                    format!(
                        "ndevice_guard_start() expects an options object, got {}",
                        other.type_name()
                    ),
                ))
            }
        }
    }
    if !(100..=60_000).contains(&interval_ms) {
        return Err(type_err(span, "ndevice_guard_start() interval_ms must be 100..=60000"));
    }
    let started = hw::guard_start(interval_ms as u64);
    Ok(Value::Bool(started).ref_cell())
}

fn ndevice_guard_stop(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 0, "ndevice_guard_stop", span)?;
    hw::guard_stop();
    Ok(Value::Nil.ref_cell())
}

fn ndevice_guard_running(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 0, "ndevice_guard_running", span)?;
    Ok(Value::Bool(hw::guard_running()).ref_cell())
}

fn ndevice_status(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 0, "ndevice_status", span)?;
    let st = hw::guard_status();
    let mut map = HashMap::new();
    map.insert("guard_running".to_string(), Value::Bool(st.running).ref_cell());
    map.insert("ticks".to_string(), Value::Int(st.ticks as i64).ref_cell());
    map.insert("throttle_level".to_string(), Value::Int(st.level as i64).ref_cell());
    map.insert("reason".to_string(), Value::String(st.reason).ref_cell());
    map.insert("gpu_temp_c".to_string(), opt_num(st.gpu_temp_c));
    map.insert("cpu_temp_c".to_string(), opt_num(st.cpu_temp_c));
    map.insert("ram_used_pct".to_string(), opt_num(st.ram_used_pct));
    map.insert("threads".to_string(), Value::Int(hw::allowed_threads() as i64).ref_cell());
    Ok(Value::Object(map).ref_cell())
}

fn ndevice_throttle_level(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 0, "ndevice_throttle_level", span)?;
    Ok(Value::Int(hw::throttle_level() as i64).ref_cell())
}

/// Manual throttle override (also handy in tests): 0..=3.
fn ndevice_set_throttle(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ndevice_set_throttle", span)?;
    let level = int_arg(args, 0, "ndevice_set_throttle", span)?;
    if !(0..=3).contains(&level) {
        return Err(type_err(span, "ndevice_set_throttle() expects 0..=3"));
    }
    hw::set_throttle(level as u8, "manual");
    Ok(Value::Nil.ref_cell())
}

/// Cooperative pacing — call inside hot loops (training steps, LLM decode).
fn ndevice_pace(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 0, "ndevice_pace", span)?;
    hw::pace();
    Ok(Value::Nil.ref_cell())
}

fn ndevice_threads(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 0, "ndevice_threads", span)?;
    Ok(Value::Int(hw::allowed_threads() as i64).ref_cell())
}

fn ndevice_ok(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 0, "ndevice_ok", span)?;
    Ok(Value::Bool(hw::throttle_level() < 2).ref_cell())
}

/// Pick the best device for a task: "train" | "infer" | "embed" | "auto".
fn ndevice_best_device(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "ndevice_best_device", span)?;
    let task = if args.is_empty() {
        "auto".to_string()
    } else {
        string_arg(args, 0, "ndevice_best_device", span)?
    };
    let snap = hw::gpu_snapshot();
    let gpu_usable = !snap.gpus.is_empty() && hw::throttle_level() < 2 && {
        let max_temp = hw::GPU_MAX_TEMP_C.load(Ordering::Relaxed) as i64;
        let temp = snap.gpus.iter().map(|g| g.temp_c).max().unwrap_or(-1);
        max_temp == 0 || temp < 0 || temp < max_temp
    };
    let npu_usable = hw::npu_detect().present && hw::throttle_level() < 2;
    let choice = match task.as_str() {
        // Training wants raw parallel compute: GPU first, CPU fallback.
        "train" => {
            if gpu_usable {
                "gpu"
            } else {
                "cpu"
            }
        }
        // Inference & embeddings run great on NPUs when present.
        "infer" | "embed" => {
            if npu_usable {
                "npu"
            } else if gpu_usable {
                "gpu"
            } else {
                "cpu"
            }
        }
        _ => {
            if gpu_usable {
                "gpu"
            } else if npu_usable {
                "npu"
            } else {
                "cpu"
            }
        }
    };
    Ok(Value::String(choice.to_string()).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ndevice_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ndevice_fns![
    ("ndevice_detect", "detect", ndevice_detect),
    ("ndevice_summary", "summary", ndevice_summary),
    ("ndevice_profile", "profile", ndevice_profile),
    ("ndevice_get_profile", "get_profile", ndevice_get_profile),
    ("ndevice_set_limits", "set_limits", ndevice_set_limits),
    ("ndevice_limits", "limits", ndevice_limits),
    ("ndevice_guard_start", "guard_start", ndevice_guard_start),
    ("ndevice_guard_stop", "guard_stop", ndevice_guard_stop),
    ("ndevice_guard_running", "guard_running", ndevice_guard_running),
    ("ndevice_status", "status", ndevice_status),
    ("ndevice_throttle_level", "throttle_level", ndevice_throttle_level),
    ("ndevice_set_throttle", "set_throttle", ndevice_set_throttle),
    ("ndevice_pace", "pace", ndevice_pace),
    ("ndevice_threads", "threads", ndevice_threads),
    ("ndevice_ok", "ok", ndevice_ok),
    ("ndevice_best_device", "best_device", ndevice_best_device),
];

fn all_builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "ndevice";
pub const MODULE_PATHS: &[&str] = &["ndevice", "std/ndevice"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    #[test]
    fn profiles_apply() {
        ndevice_profile(&[Value::String("eco".into()).ref_cell()], span()).unwrap();
        assert_eq!(hw::CPU_LIMIT_PCT.load(Ordering::Relaxed), 50);
        assert_eq!(hw::GPU_MAX_TEMP_C.load(Ordering::Relaxed), 75);
        ndevice_profile(&[Value::String("performance".into()).ref_cell()], span()).unwrap();
        assert_eq!(hw::CPU_LIMIT_PCT.load(Ordering::Relaxed), 100);
        assert!(
            ndevice_profile(&[Value::String("bogus".into()).ref_cell()], span()).is_err()
        );
    }

    #[test]
    fn set_limits_bulk() {
        let mut opts = HashMap::new();
        opts.insert("cpu_pct".to_string(), Value::Int(40).ref_cell());
        opts.insert("gpu_max_temp".to_string(), Value::Int(70).ref_cell());
        ndevice_set_limits(&[Value::Object(opts).ref_cell()], span()).unwrap();
        assert_eq!(hw::CPU_LIMIT_PCT.load(Ordering::Relaxed), 40);
        assert_eq!(hw::GPU_MAX_TEMP_C.load(Ordering::Relaxed), 70);
        // restore
        ndevice_profile(&[Value::String("performance".into()).ref_cell()], span()).unwrap();
        hw::GPU_MAX_TEMP_C.store(0, Ordering::Relaxed);
        hw::CPU_MAX_TEMP_C.store(0, Ordering::Relaxed);
    }

    #[test]
    fn throttle_and_pace() {
        ndevice_set_throttle(&[Value::Int(3).ref_cell()], span()).unwrap();
        match &*ndevice_ok(&[], span()).unwrap().borrow() {
            Value::Bool(b) => assert!(!*b),
            other => panic!("expected bool, got {other:?}"),
        }
        ndevice_pace(&[], span()).unwrap(); // sleeps 25ms, must not error
        ndevice_set_throttle(&[Value::Int(0).ref_cell()], span()).unwrap();
    }

    #[test]
    fn best_device_returns_known_word() {
        match &*ndevice_best_device(&[], span()).unwrap().borrow() {
            Value::String(s) => assert!(["cpu", "gpu", "npu"].contains(&s.as_str())),
            other => panic!("expected string, got {other:?}"),
        }
    }
}
