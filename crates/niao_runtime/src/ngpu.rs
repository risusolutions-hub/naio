//! Native ngpu standard library — GPU detection and live readings (VRAM,
//! utilization, temperature) with cooperative budgets: `set_limit(40)` caps
//! Niao's GPU appetite at 40%, `set_max_temp(80)` arms overheat protection,
//! `ok()` gates new work, and `wait_cool()` pauses batch loops until the
//! GPU cools down.
//!
//! Backends: `nvidia-smi` → `rocm-smi` → detection-only fallback. Readings
//! the system cannot provide are `nil` — never invented.
//!
//! Import with `import "ngpu"` (or `import "std/ngpu"`).

use crate::hw;
use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

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
            codes::E2710_NGPU_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
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

fn optional_index(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<usize> {
    if args.len() <= idx {
        return Ok(0);
    }
    let i = int_arg(args, idx, name, span)?;
    if i < 0 {
        return Err(type_err(span, format!("{name}() GPU index must be >= 0")));
    }
    Ok(i as usize)
}

fn unavailable(span: Span, what: &str) -> ValueRef {
    error_value(
        codes::E2713_NGPU_UNAVAILABLE,
        "ngpu_error",
        format!("{what} not available on this system"),
        span,
    )
}

fn opt_num(n: i64) -> ValueRef {
    if n < 0 {
        Value::Nil.ref_cell()
    } else {
        Value::Int(n).ref_cell()
    }
}

fn gpu_at(index: usize, span: Span) -> Result<hw::GpuInfo, ValueRef> {
    let snap = hw::gpu_snapshot();
    match snap.gpus.get(index) {
        Some(g) => Ok(g.clone()),
        None => Err(error_value(
            codes::E2711_NGPU_ERROR,
            "ngpu_error",
            format!("GPU index {index} out of range ({} detected)", snap.gpus.len()),
            span,
        )),
    }
}

fn gpu_object(g: &hw::GpuInfo) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("index".to_string(), Value::Int(g.index).ref_cell());
    map.insert("name".to_string(), Value::String(g.name.clone()).ref_cell());
    map.insert("vendor".to_string(), Value::String(g.vendor.clone()).ref_cell());
    map.insert("vram_total_mb".to_string(), opt_num(g.vram_total_mb));
    map.insert("vram_used_mb".to_string(), opt_num(g.vram_used_mb));
    map.insert("usage".to_string(), opt_num(g.util_pct));
    map.insert("temp_c".to_string(), opt_num(g.temp_c));
    Value::Object(map).ref_cell()
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn ngpu_available(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 0, "ngpu_available", span)?;
    Ok(Value::Bool(!hw::gpu_snapshot().gpus.is_empty()).ref_cell())
}

fn ngpu_count(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 0, "ngpu_count", span)?;
    Ok(Value::Int(hw::gpu_snapshot().gpus.len() as i64).ref_cell())
}

/// "nvidia-smi" | "rocm-smi" | "detect-only".
fn ngpu_backend(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 0, "ngpu_backend", span)?;
    Ok(Value::String(hw::gpu_snapshot().backend.to_string()).ref_cell())
}

/// All detected GPUs as an array of objects.
fn ngpu_list(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 0, "ngpu_list", span)?;
    let snap = hw::gpu_snapshot();
    let items: Vec<ValueRef> = snap.gpus.iter().map(gpu_object).collect();
    Ok(Value::Array(items).ref_cell())
}

fn ngpu_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "ngpu_info", span)?;
    let index = optional_index(args, 0, "ngpu_info", span)?;
    match gpu_at(index, span) {
        Ok(g) => Ok(gpu_object(&g)),
        Err(e) => Ok(e),
    }
}

fn reading(
    args: &[ValueRef],
    span: Span,
    name: &str,
    what: &str,
    pick: impl Fn(&hw::GpuInfo) -> i64,
) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, name, span)?;
    let index = optional_index(args, 0, name, span)?;
    match gpu_at(index, span) {
        Ok(g) => {
            let v = pick(&g);
            if v < 0 {
                Ok(unavailable(span, what))
            } else {
                Ok(Value::Int(v).ref_cell())
            }
        }
        Err(e) => Ok(e),
    }
}

fn ngpu_usage(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    reading(args, span, "ngpu_usage", "GPU utilization", |g| g.util_pct)
}

fn ngpu_temp_c(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    reading(args, span, "ngpu_temp_c", "GPU temperature", |g| g.temp_c)
}

fn ngpu_vram_total_mb(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    reading(args, span, "ngpu_vram_total_mb", "GPU memory info", |g| g.vram_total_mb)
}

fn ngpu_vram_used_mb(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    reading(args, span, "ngpu_vram_used_mb", "GPU memory info", |g| g.vram_used_mb)
}

/// Limit Niao's GPU appetite (advisory budget consulted by ok()/ndevice).
fn ngpu_set_limit(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ngpu_set_limit", span)?;
    let pct = int_arg(args, 0, "ngpu_set_limit", span)?;
    if !(1..=100).contains(&pct) {
        return Err(type_err(span, "ngpu_set_limit() expects 1..=100"));
    }
    hw::GPU_LIMIT_PCT.store(pct as u8, Ordering::Relaxed);
    Ok(Value::Nil.ref_cell())
}

fn ngpu_get_limit(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 0, "ngpu_get_limit", span)?;
    Ok(Value::Int(hw::GPU_LIMIT_PCT.load(Ordering::Relaxed) as i64).ref_cell())
}

/// Arm overheat protection: guard throttles when temp reaches this (0 = off).
fn ngpu_set_max_temp(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 1, "ngpu_set_max_temp", span)?;
    let c = int_arg(args, 0, "ngpu_set_max_temp", span)?;
    if !(0..=110).contains(&c) {
        return Err(type_err(span, "ngpu_set_max_temp() expects 0..=110"));
    }
    hw::GPU_MAX_TEMP_C.store(c as u8, Ordering::Relaxed);
    Ok(Value::Nil.ref_cell())
}

fn ngpu_get_max_temp(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 0, "ngpu_get_max_temp", span)?;
    Ok(Value::Int(hw::GPU_MAX_TEMP_C.load(Ordering::Relaxed) as i64).ref_cell())
}

/// Safe to start new GPU work right now?
/// false when: utilization above limit, temp at/above max, or throttle >= 2.
fn ngpu_ok(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "ngpu_ok", span)?;
    let index = optional_index(args, 0, "ngpu_ok", span)?;
    if hw::throttle_level() >= 2 {
        return Ok(Value::Bool(false).ref_cell());
    }
    let g = match gpu_at(index, span) {
        Ok(g) => g,
        Err(e) => return Ok(e),
    };
    let limit = hw::GPU_LIMIT_PCT.load(Ordering::Relaxed) as i64;
    if g.util_pct >= 0 && limit < 100 && g.util_pct >= limit {
        return Ok(Value::Bool(false).ref_cell());
    }
    let max_temp = hw::GPU_MAX_TEMP_C.load(Ordering::Relaxed) as i64;
    if max_temp > 0 && g.temp_c >= 0 && g.temp_c >= max_temp {
        return Ok(Value::Bool(false).ref_cell());
    }
    Ok(Value::Bool(true).ref_cell())
}

/// Block (sleeping in 250 ms steps) until the GPU cools to `target_c`
/// (default max_temp − 10) or `timeout_ms` (default 30000) passes.
/// Returns true when cool, false on timeout or when temps are unavailable.
fn ngpu_wait_cool(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 2, "ngpu_wait_cool", span)?;
    let max_temp = hw::GPU_MAX_TEMP_C.load(Ordering::Relaxed) as i64;
    let target = if !args.is_empty() {
        int_arg(args, 0, "ngpu_wait_cool", span)?
    } else if max_temp > 0 {
        (max_temp - 10).max(30)
    } else {
        return Ok(unavailable(span, "wait_cool without target (set_max_temp first)"));
    };
    let timeout_ms = if args.len() > 1 {
        int_arg(args, 1, "ngpu_wait_cool", span)?.max(0) as u64
    } else {
        30_000
    };
    let start = Instant::now();
    loop {
        hw::gpu_refresh();
        let snap = hw::gpu_snapshot();
        let temp = snap.gpus.iter().map(|g| g.temp_c).max().unwrap_or(-1);
        if temp < 0 {
            return Ok(Value::Bool(false).ref_cell());
        }
        if temp <= target {
            return Ok(Value::Bool(true).ref_cell());
        }
        if start.elapsed() >= Duration::from_millis(timeout_ms) {
            return Ok(Value::Bool(false).ref_cell());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Force a fresh probe on the next reading.
fn ngpu_refresh(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 0, "ngpu_refresh", span)?;
    hw::gpu_refresh();
    Ok(Value::Nil.ref_cell())
}

fn ngpu_status(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 0, "ngpu_status", span)?;
    let snap = hw::gpu_snapshot();
    let mut map = HashMap::new();
    map.insert("backend".to_string(), Value::String(snap.backend.to_string()).ref_cell());
    map.insert("count".to_string(), Value::Int(snap.gpus.len() as i64).ref_cell());
    map.insert(
        "limit_pct".to_string(),
        Value::Int(hw::GPU_LIMIT_PCT.load(Ordering::Relaxed) as i64).ref_cell(),
    );
    map.insert(
        "max_temp_c".to_string(),
        Value::Int(hw::GPU_MAX_TEMP_C.load(Ordering::Relaxed) as i64).ref_cell(),
    );
    map.insert(
        "throttle_level".to_string(),
        Value::Int(hw::throttle_level() as i64).ref_cell(),
    );
    let gpus: Vec<ValueRef> = snap.gpus.iter().map(gpu_object).collect();
    map.insert("gpus".to_string(), Value::Array(gpus).ref_cell());
    Ok(Value::Object(map).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ngpu_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ngpu_fns![
    ("ngpu_available", "available", ngpu_available),
    ("ngpu_count", "count", ngpu_count),
    ("ngpu_backend", "backend", ngpu_backend),
    ("ngpu_list", "list", ngpu_list),
    ("ngpu_info", "info", ngpu_info),
    ("ngpu_usage", "usage", ngpu_usage),
    ("ngpu_temp_c", "temp_c", ngpu_temp_c),
    ("ngpu_vram_total_mb", "vram_total_mb", ngpu_vram_total_mb),
    ("ngpu_vram_used_mb", "vram_used_mb", ngpu_vram_used_mb),
    ("ngpu_set_limit", "set_limit", ngpu_set_limit),
    ("ngpu_get_limit", "get_limit", ngpu_get_limit),
    ("ngpu_set_max_temp", "set_max_temp", ngpu_set_max_temp),
    ("ngpu_get_max_temp", "get_max_temp", ngpu_get_max_temp),
    ("ngpu_ok", "ok", ngpu_ok),
    ("ngpu_wait_cool", "wait_cool", ngpu_wait_cool),
    ("ngpu_refresh", "refresh", ngpu_refresh),
    ("ngpu_status", "status", ngpu_status),
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

pub const MODULE_NAME: &str = "ngpu";
pub const MODULE_PATHS: &[&str] = &["ngpu", "std/ngpu"];

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
    fn count_and_available_consistent() {
        let count = match &*ngpu_count(&[], span()).unwrap().borrow() {
            Value::Int(n) => *n,
            other => panic!("expected int, got {other:?}"),
        };
        match &*ngpu_available(&[], span()).unwrap().borrow() {
            Value::Bool(b) => assert_eq!(*b, count > 0),
            other => panic!("expected bool, got {other:?}"),
        }
    }

    #[test]
    fn limits_roundtrip() {
        ngpu_set_limit(&[Value::Int(40).ref_cell()], span()).unwrap();
        match &*ngpu_get_limit(&[], span()).unwrap().borrow() {
            Value::Int(n) => assert_eq!(*n, 40),
            other => panic!("expected int, got {other:?}"),
        }
        ngpu_set_max_temp(&[Value::Int(80).ref_cell()], span()).unwrap();
        match &*ngpu_get_max_temp(&[], span()).unwrap().borrow() {
            Value::Int(n) => assert_eq!(*n, 80),
            other => panic!("expected int, got {other:?}"),
        }
        ngpu_set_limit(&[Value::Int(100).ref_cell()], span()).unwrap();
        ngpu_set_max_temp(&[Value::Int(0).ref_cell()], span()).unwrap();
    }

    #[test]
    fn bad_index_is_error_value() {
        let r = ngpu_info(&[Value::Int(99).ref_cell()], span()).unwrap();
        assert!(matches!(&*r.borrow(), Value::Error(_)));
    }

    #[test]
    fn status_shape() {
        match &*ngpu_status(&[], span()).unwrap().borrow() {
            Value::Object(map) => {
                assert!(map.contains_key("backend"));
                assert!(map.contains_key("gpus"));
                assert!(map.contains_key("throttle_level"));
            }
            other => panic!("expected object, got {other:?}"),
        }
    }
}
