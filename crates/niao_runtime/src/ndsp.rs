//! Native ndsp standard library — digital signal processing
//! (~scipy.signal subset; pairs with nnum FFT + naudio).
//!
//! Import with `import "ndsp"` (or `import "std/ndsp"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_dsp::{
    bartlett, blackman, boxcar, butter, cheby1, chirp, convolve, correlate, decimate, detrend,
    fftconvolve, filtfilt, find_peaks, firwin, freqz, gausspulse, get_window, hamming, hann,
    hilbert, iirfilter, istft, kaiser, lfilter, medfilt, periodogram, resample, resample_poly,
    sawtooth, sos2tf, sosfilt, sosfiltfilt, sosfreqz, spectrogram, square, stft, tf2sos, tukey,
    upfirdn, welch, Btype, ConvMode, DspError, Ftype, IirOut, Sos, SpectralOpts,
};
use niao_errors::codes;
use std::collections::HashMap;
use std::rc::Rc;

const E4100: u32 = codes::E4100_NDSP_ARITY;
const E4101: u32 = codes::E4101_NDSP_ERROR;
const E4102: u32 = codes::E4102_NDSP_TYPE;
const E4103: u32 = codes::E4103_NDSP_PARAM;
const E4104: u32 = codes::E4104_NDSP_LENGTH;
const E4105: u32 = codes::E4105_NDSP_FILTER;

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E4100,
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
            E4100,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E4102, msg.into())
}

fn soft_err(span: Span, err: DspError) -> ValueRef {
    let code = match &err {
        DspError::Empty | DspError::Length(_) => E4104,
        DspError::Param(_) => E4103,
        DspError::Filter(_) => E4105,
        DspError::Domain(_) => E4101,
    };
    error_value(code, "ndsp_error", err.message(), span)
}

fn ok_floats(v: Vec<f64>) -> ValueRef {
    Value::FloatArray(v).ref_cell()
}

fn num_from(v: &Value, name: &str, span: Span) -> NiaoResult<f64> {
    match v {
        Value::Int(n) => Ok(*n as f64),
        Value::Float(f) => Ok(*f),
        other => Err(type_err(
            span,
            format!("{name}() expects a number, got {}", other.type_name()),
        )),
    }
}

fn floats(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<f64>> {
    match &*args[idx].borrow() {
        Value::FloatArray(v) => Ok(v.clone()),
        Value::IntArray(v) => Ok(v.iter().map(|n| *n as f64).collect()),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(num_from(&*item.borrow(), name, span)?);
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() expects array/float_array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn float_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<f64> {
    num_from(&*args[idx].borrow(), name, span)
}

fn int_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        Value::Float(f) => Ok(*f as i64),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects int as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn string_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn optional_string(args: &[ValueRef], idx: usize) -> Option<String> {
    args.get(idx).and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.clone()),
        Value::Nil => None,
        _ => None,
    })
}

fn optional_object(args: &[ValueRef], idx: usize) -> Option<HashMap<String, ValueRef>> {
    args.get(idx).and_then(|v| match &*v.borrow() {
        Value::Object(m) => Some(m.clone()),
        Value::Nil => None,
        _ => None,
    })
}

fn field_f64(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Option<f64> {
    let map = map?;
    let v = map.get(key)?;
    match &*v.borrow() {
        Value::Float(f) => Some(*f),
        Value::Int(n) => Some(*n as f64),
        _ => None,
    }
}

fn field_i64(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Option<i64> {
    let map = map?;
    let v = map.get(key)?;
    match &*v.borrow() {
        Value::Int(n) => Some(*n),
        Value::Float(f) => Some(*f as i64),
        _ => None,
    }
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

fn field_string(map: Option<&HashMap<String, ValueRef>>, key: &str) -> Option<String> {
    let map = map?;
    match map.get(key).map(|v| v.borrow().clone()) {
        Some(Value::String(s)) => Some(s),
        _ => None,
    }
}

fn cutoffs_from(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<f64>> {
    match &*args[idx].borrow() {
        Value::Float(f) => Ok(vec![*f]),
        Value::Int(n) => Ok(vec![*n as f64]),
        Value::FloatArray(v) => Ok(v.clone()),
        Value::Array(items) => {
            let mut out = Vec::new();
            for item in items {
                out.push(num_from(&*item.borrow(), name, span)?);
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() expects cutoff number or array, got {}",
                other.type_name()
            ),
        )),
    }
}

fn sos_from(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Sos> {
    match &*args[idx].borrow() {
        Value::Array(rows) => {
            let mut sos = Sos::new();
            for row in rows {
                let coeffs = match &*row.borrow() {
                    Value::FloatArray(v) => v.clone(),
                    Value::Array(items) => {
                        let mut c = Vec::new();
                        for it in items {
                            c.push(num_from(&*it.borrow(), name, span)?);
                        }
                        c
                    }
                    other => {
                        return Err(type_err(
                            span,
                            format!("{name}() sos row must be array, got {}", other.type_name()),
                        ))
                    }
                };
                if coeffs.len() != 6 {
                    return Err(type_err(
                        span,
                        format!("{name}() each sos row must have 6 coefficients"),
                    ));
                }
                sos.push([
                    coeffs[0], coeffs[1], coeffs[2], coeffs[3], coeffs[4], coeffs[5],
                ]);
            }
            Ok(sos)
        }
        other => Err(type_err(
            span,
            format!("{name}() expects sos array, got {}", other.type_name()),
        )),
    }
}

fn sos_to_value(sos: &Sos) -> ValueRef {
    let rows: Vec<ValueRef> = sos
        .iter()
        .map(|s| Value::FloatArray(s.to_vec()).ref_cell())
        .collect();
    Value::Array(rows).ref_cell()
}

fn tf_to_value(b: Vec<f64>, a: Vec<f64>) -> ValueRef {
    let mut m = HashMap::new();
    m.insert("b".into(), ok_floats(b));
    m.insert("a".into(), ok_floats(a));
    Value::Object(m).ref_cell()
}

fn iir_to_value(out: IirOut) -> ValueRef {
    match out {
        IirOut::Ba(tf) => tf_to_value(tf.b, tf.a),
        IirOut::Sos(sos) => sos_to_value(&sos),
    }
}

fn parse_mode(s: &str) -> Result<ConvMode, DspError> {
    ConvMode::parse(s)
}

fn spectral_opts(map: Option<&HashMap<String, ValueRef>>) -> SpectralOpts {
    let mut o = SpectralOpts::default();
    if let Some(fs) = field_f64(map, "fs") {
        o.fs = fs;
    }
    if let Some(w) = field_string(map, "window") {
        o.window = w;
    }
    if let Some(n) = field_i64(map, "nperseg") {
        o.nperseg = n.max(1) as usize;
    }
    if let Some(n) = field_i64(map, "noverlap") {
        o.noverlap = Some(n.max(0) as usize);
    }
    if let Some(n) = field_i64(map, "nfft") {
        o.nfft = Some(n.max(1) as usize);
    }
    o
}

fn ok_or_soft<T, F>(span: Span, r: Result<T, DspError>, f: F) -> ValueRef
where
    F: FnOnce(T) -> ValueRef,
{
    match r {
        Ok(v) => f(v),
        Err(e) => soft_err(span, e),
    }
}

// >>> import "ndsp"; len(ndsp.convolve([1.0, 2.0, 3.0], [0.0, 1.0], "full"))
// => 4
fn ndsp_convolve(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ndsp_convolve", span)?;
    let a = floats(args, 0, "ndsp_convolve", span)?;
    let b = floats(args, 1, "ndsp_convolve", span)?;
    let mode = optional_string(args, 2).unwrap_or_else(|| "full".into());
    Ok(ok_or_soft(
        span,
        parse_mode(&mode).and_then(|mode| convolve(&a, &b, mode)),
        ok_floats,
    ))
}

// >>> import "ndsp"; len(ndsp.correlate([1.0, 2.0, 3.0], [0.0, 1.0], "full"))
// => 4
fn ndsp_correlate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ndsp_correlate", span)?;
    let a = floats(args, 0, "ndsp_correlate", span)?;
    let b = floats(args, 1, "ndsp_correlate", span)?;
    let mode = optional_string(args, 2).unwrap_or_else(|| "full".into());
    Ok(ok_or_soft(
        span,
        parse_mode(&mode).and_then(|mode| correlate(&a, &b, mode)),
        ok_floats,
    ))
}

// >>> import "ndsp"; len(ndsp.fftconvolve([1.0, 0.0, 0.0, 0.0], [1.0, 2.0], "full"))
// => 5
fn ndsp_fftconvolve(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ndsp_fftconvolve", span)?;
    let a = floats(args, 0, "ndsp_fftconvolve", span)?;
    let b = floats(args, 1, "ndsp_fftconvolve", span)?;
    let mode = optional_string(args, 2).unwrap_or_else(|| "full".into());
    Ok(ok_or_soft(
        span,
        parse_mode(&mode).and_then(|mode| fftconvolve(&a, &b, mode)),
        ok_floats,
    ))
}

// >>> import "ndsp"; len(ndsp.hann(8))
// => 8
fn ndsp_hann(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndsp_hann", span)?;
    let m = int_arg(args, 0, "ndsp_hann", span)?;
    if m < 0 {
        return Ok(soft_err(span, DspError::Param("M must be >= 0".into())));
    }
    Ok(ok_floats(hann(m as usize)))
}

// >>> import "ndsp"; len(ndsp.hamming(8))
// => 8
fn ndsp_hamming(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndsp_hamming", span)?;
    let m = int_arg(args, 0, "ndsp_hamming", span)?;
    if m < 0 {
        return Ok(soft_err(span, DspError::Param("M must be >= 0".into())));
    }
    Ok(ok_floats(hamming(m as usize)))
}

// >>> import "ndsp"; len(ndsp.blackman(8))
// => 8
fn ndsp_blackman(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndsp_blackman", span)?;
    let m = int_arg(args, 0, "ndsp_blackman", span)?;
    if m < 0 {
        return Ok(soft_err(span, DspError::Param("M must be >= 0".into())));
    }
    Ok(ok_floats(blackman(m as usize)))
}

// >>> import "ndsp"; len(ndsp.bartlett(5))
// => 5
fn ndsp_bartlett(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndsp_bartlett", span)?;
    let m = int_arg(args, 0, "ndsp_bartlett", span)?;
    if m < 0 {
        return Ok(soft_err(span, DspError::Param("M must be >= 0".into())));
    }
    Ok(ok_floats(bartlett(m as usize)))
}

// >>> import "ndsp"; len(ndsp.boxcar(4))
// => 4
fn ndsp_boxcar(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndsp_boxcar", span)?;
    let m = int_arg(args, 0, "ndsp_boxcar", span)?;
    if m < 0 {
        return Ok(soft_err(span, DspError::Param("M must be >= 0".into())));
    }
    Ok(ok_floats(boxcar(m as usize)))
}

// >>> import "ndsp"; len(ndsp.kaiser(8, 8.6))
// => 8
fn ndsp_kaiser(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndsp_kaiser", span)?;
    let m = int_arg(args, 0, "ndsp_kaiser", span)?;
    let beta = float_arg(args, 1, "ndsp_kaiser", span)?;
    if m < 0 {
        return Ok(soft_err(span, DspError::Param("M must be >= 0".into())));
    }
    Ok(ok_or_soft(span, kaiser(m as usize, beta), ok_floats))
}

// >>> import "ndsp"; len(ndsp.tukey(8, 0.5))
// => 8
fn ndsp_tukey(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ndsp_tukey", span)?;
    let m = int_arg(args, 0, "ndsp_tukey", span)?;
    let alpha = if args.len() > 1 {
        float_arg(args, 1, "ndsp_tukey", span)?
    } else {
        0.5
    };
    if m < 0 {
        return Ok(soft_err(span, DspError::Param("M must be >= 0".into())));
    }
    Ok(ok_or_soft(span, tukey(m as usize, alpha), ok_floats))
}

// >>> import "ndsp"; len(ndsp.get_window("hann", 16))
// => 16
fn ndsp_get_window(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ndsp_get_window", span)?;
    let name = string_arg(args, 0, "ndsp_get_window", span)?;
    let nx = int_arg(args, 1, "ndsp_get_window", span)?;
    let fftbins = if args.len() > 2 {
        match &*args[2].borrow() {
            Value::Bool(b) => *b,
            Value::Int(n) => *n != 0,
            _ => true,
        }
    } else {
        true
    };
    if nx < 0 {
        return Ok(soft_err(span, DspError::Param("Nx must be >= 0".into())));
    }
    Ok(ok_or_soft(
        span,
        get_window(&name, nx as usize, fftbins),
        ok_floats,
    ))
}

// >>> import "ndsp"; len(ndsp.firwin(11, 0.2))
// => 11
fn ndsp_firwin(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ndsp_firwin", span)?;
    let numtaps = int_arg(args, 0, "ndsp_firwin", span)?;
    let cutoffs = cutoffs_from(args, 1, "ndsp_firwin", span)?;
    let opts = optional_object(args, 2);
    let window = field_string(opts.as_ref(), "window").unwrap_or_else(|| "hamming".into());
    let pass_zero = field_bool(opts.as_ref(), "pass_zero", true);
    let fs = field_f64(opts.as_ref(), "fs").unwrap_or(2.0);
    if numtaps <= 0 {
        return Ok(soft_err(
            span,
            DspError::Param("numtaps must be > 0".into()),
        ));
    }
    Ok(ok_or_soft(
        span,
        firwin(numtaps as usize, &cutoffs, &window, pass_zero, fs),
        ok_floats,
    ))
}

fn parse_btype(s: &str) -> Result<Btype, DspError> {
    Btype::parse(s)
}

fn parse_ftype(s: &str) -> Result<Ftype, DspError> {
    Ftype::parse(s)
}

// >>> import "ndsp"; let f = ndsp.butter(2, 0.2); len(f.b) > 0
// => true
fn ndsp_butter(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ndsp_butter", span)?;
    let order = int_arg(args, 0, "ndsp_butter", span)?;
    let wn = cutoffs_from(args, 1, "ndsp_butter", span)?;
    let opts = optional_object(args, 2);
    let btype = field_string(opts.as_ref(), "btype").unwrap_or_else(|| "lowpass".into());
    let fs = field_f64(opts.as_ref(), "fs").unwrap_or(2.0);
    let output_sos = field_string(opts.as_ref(), "output")
        .map(|s| s.eq_ignore_ascii_case("sos"))
        .unwrap_or(false);
    if order <= 0 {
        return Ok(soft_err(span, DspError::Param("order must be > 0".into())));
    }
    Ok(ok_or_soft(
        span,
        parse_btype(&btype).and_then(|bt| butter(order as usize, &wn, bt, fs, output_sos)),
        iir_to_value,
    ))
}

// >>> import "ndsp"; let f = ndsp.cheby1(2, 1.0, 0.2); len(f.a) > 0
// => true
fn ndsp_cheby1(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "ndsp_cheby1", span)?;
    let order = int_arg(args, 0, "ndsp_cheby1", span)?;
    let rp = float_arg(args, 1, "ndsp_cheby1", span)?;
    let wn = cutoffs_from(args, 2, "ndsp_cheby1", span)?;
    let opts = optional_object(args, 3);
    let btype = field_string(opts.as_ref(), "btype").unwrap_or_else(|| "lowpass".into());
    let fs = field_f64(opts.as_ref(), "fs").unwrap_or(2.0);
    let output_sos = field_string(opts.as_ref(), "output")
        .map(|s| s.eq_ignore_ascii_case("sos"))
        .unwrap_or(false);
    Ok(ok_or_soft(
        span,
        parse_btype(&btype).and_then(|bt| cheby1(order as usize, rp, &wn, bt, fs, output_sos)),
        iir_to_value,
    ))
}

// >>> import "ndsp"; let f = ndsp.iirfilter(2, 0.2); len(f.b) > 0
// => true
fn ndsp_iirfilter(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ndsp_iirfilter", span)?;
    let order = int_arg(args, 0, "ndsp_iirfilter", span)?;
    let wn = cutoffs_from(args, 1, "ndsp_iirfilter", span)?;
    let opts = optional_object(args, 2);
    let btype = field_string(opts.as_ref(), "btype").unwrap_or_else(|| "lowpass".into());
    let ftype = field_string(opts.as_ref(), "ftype").unwrap_or_else(|| "butter".into());
    let rp = field_f64(opts.as_ref(), "rp").unwrap_or(1.0);
    let fs = field_f64(opts.as_ref(), "fs").unwrap_or(2.0);
    let output_sos = field_string(opts.as_ref(), "output")
        .map(|s| s.eq_ignore_ascii_case("sos"))
        .unwrap_or(false);
    Ok(ok_or_soft(
        span,
        parse_btype(&btype).and_then(|bt| {
            parse_ftype(&ftype)
                .and_then(|ft| iirfilter(order as usize, &wn, bt, ft, rp, fs, output_sos))
        }),
        iir_to_value,
    ))
}

// >>> import "ndsp"; len(ndsp.lfilter([0.5, 0.5], [1.0], [1.0, 1.0, 1.0]))
// => 3
fn ndsp_lfilter(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "ndsp_lfilter", span)?;
    let b = floats(args, 0, "ndsp_lfilter", span)?;
    let a = floats(args, 1, "ndsp_lfilter", span)?;
    let x = floats(args, 2, "ndsp_lfilter", span)?;
    Ok(ok_or_soft(span, lfilter(&b, &a, &x), ok_floats))
}

// >>> import "ndsp"; len(ndsp.filtfilt([0.25, 0.5, 0.25], [1.0], [1.0, 2.0, 3.0, 2.0, 1.0, 0.5, 0.25, 0.0]))
// => 8
fn ndsp_filtfilt(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "ndsp_filtfilt", span)?;
    let b = floats(args, 0, "ndsp_filtfilt", span)?;
    let a = floats(args, 1, "ndsp_filtfilt", span)?;
    let x = floats(args, 2, "ndsp_filtfilt", span)?;
    Ok(ok_or_soft(span, filtfilt(&b, &a, &x), ok_floats))
}

// >>> import "ndsp"; let f = ndsp.butter(2, 0.2, {output: "sos"}); len(ndsp.sosfilt(f, [1.0, 0.0, 0.0, 0.0]))
// => 4
fn ndsp_sosfilt(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndsp_sosfilt", span)?;
    let sos = sos_from(args, 0, "ndsp_sosfilt", span)?;
    let x = floats(args, 1, "ndsp_sosfilt", span)?;
    Ok(ok_or_soft(span, sosfilt(&sos, &x), ok_floats))
}

// >>> import "ndsp"; let f = ndsp.butter(2, 0.2, {output: "sos"}); len(ndsp.sosfiltfilt(f, [1.0, 2.0, 3.0, 2.0, 1.0]))
// => 5
fn ndsp_sosfiltfilt(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndsp_sosfiltfilt", span)?;
    let sos = sos_from(args, 0, "ndsp_sosfiltfilt", span)?;
    let x = floats(args, 1, "ndsp_sosfiltfilt", span)?;
    Ok(ok_or_soft(span, sosfiltfilt(&sos, &x), ok_floats))
}

// >>> import "ndsp"; let f = ndsp.butter(2, 0.2); len(ndsp.tf2sos(f.b, f.a)) > 0
// => true
fn ndsp_tf2sos(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndsp_tf2sos", span)?;
    let b = floats(args, 0, "ndsp_tf2sos", span)?;
    let a = floats(args, 1, "ndsp_tf2sos", span)?;
    Ok(sos_to_value(&tf2sos(&b, &a)))
}

// >>> import "ndsp"; let f = ndsp.butter(2, 0.2, {output: "sos"}); let tf = ndsp.sos2tf(f); len(tf.b) > 0
// => true
fn ndsp_sos2tf(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndsp_sos2tf", span)?;
    let sos = sos_from(args, 0, "ndsp_sos2tf", span)?;
    let tf = sos2tf(&sos);
    Ok(tf_to_value(tf.b, tf.a))
}

// >>> import "ndsp"; len(ndsp.resample([1.0, 2.0, 3.0, 4.0], 2))
// => 2
fn ndsp_resample(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ndsp_resample", span)?;
    let x = floats(args, 0, "ndsp_resample", span)?;
    let num = int_arg(args, 1, "ndsp_resample", span)?;
    if num < 0 {
        return Ok(soft_err(span, DspError::Param("num must be >= 0".into())));
    }
    Ok(ok_or_soft(span, resample(&x, num as usize), ok_floats))
}

// >>> import "ndsp"; len(ndsp.resample_poly([1.0, 2.0, 3.0, 4.0], 2, 1)) > 0
// => true
fn ndsp_resample_poly(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "ndsp_resample_poly", span)?;
    let x = floats(args, 0, "ndsp_resample_poly", span)?;
    let up = int_arg(args, 1, "ndsp_resample_poly", span)?;
    let down = int_arg(args, 2, "ndsp_resample_poly", span)?;
    Ok(ok_or_soft(
        span,
        resample_poly(&x, up.max(0) as usize, down.max(0) as usize),
        ok_floats,
    ))
}

// >>> import "ndsp"; len(ndsp.decimate([1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], 2))
// => 4
fn ndsp_decimate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ndsp_decimate", span)?;
    let x = floats(args, 0, "ndsp_decimate", span)?;
    let q = int_arg(args, 1, "ndsp_decimate", span)?;
    let n = if args.len() > 2 {
        Some(int_arg(args, 2, "ndsp_decimate", span)? as usize)
    } else {
        None
    };
    Ok(ok_or_soft(
        span,
        decimate(&x, q.max(0) as usize, n),
        ok_floats,
    ))
}

// >>> import "ndsp"; len(ndsp.upfirdn([0.5, 0.5], [1.0, 2.0, 3.0], 1, 1)) > 0
// => true
fn ndsp_upfirdn(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 4, "ndsp_upfirdn", span)?;
    let h = floats(args, 0, "ndsp_upfirdn", span)?;
    let x = floats(args, 1, "ndsp_upfirdn", span)?;
    let up = if args.len() > 2 {
        int_arg(args, 2, "ndsp_upfirdn", span)? as usize
    } else {
        1
    };
    let down = if args.len() > 3 {
        int_arg(args, 3, "ndsp_upfirdn", span)? as usize
    } else {
        1
    };
    Ok(ok_or_soft(span, upfirdn(&h, &x, up, down), ok_floats))
}

fn stft_to_value(st: niao_dsp::StftResult) -> ValueRef {
    let mut z = HashMap::new();
    z.insert("re".into(), ok_floats(st.re));
    z.insert("im".into(), ok_floats(st.im));
    z.insert(
        "shape".into(),
        Value::IntArray(vec![st.shape[0] as i64, st.shape[1] as i64]).ref_cell(),
    );
    let mut m = HashMap::new();
    m.insert("f".into(), ok_floats(st.f));
    m.insert("t".into(), ok_floats(st.t));
    m.insert("Zxx".into(), Value::Object(z).ref_cell());
    Value::Object(m).ref_cell()
}

// >>> import "ndsp"; let s = ndsp.stft([1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], {nperseg: 4}); len(s.f) > 0
// => true
fn ndsp_stft(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ndsp_stft", span)?;
    let x = floats(args, 0, "ndsp_stft", span)?;
    let opts = spectral_opts(optional_object(args, 1).as_ref());
    Ok(ok_or_soft(span, stft(&x, &opts), stft_to_value))
}

// >>> import "ndsp"; let x = [1.0, 0.5, 0.0, (0.0 - 0.5), (0.0 - 1.0), (0.0 - 0.5), 0.0, 0.5]; let s = ndsp.stft(x, {nperseg: 4}); len(ndsp.istft(s.Zxx, {nperseg: 4})) > 0
// => true
fn ndsp_istft(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ndsp_istft", span)?;
    let zxx = match &*args[0].borrow() {
        Value::Object(m) => m.clone(),
        other => {
            return Err(type_err(
                span,
                format!("ndsp_istft() expects Zxx object, got {}", other.type_name()),
            ))
        }
    };
    let re = match zxx.get("re").map(|v| v.borrow().clone()) {
        Some(Value::FloatArray(v)) => v,
        _ => {
            return Ok(soft_err(
                span,
                DspError::Param("Zxx.re float_array required".into()),
            ))
        }
    };
    let im = match zxx.get("im").map(|v| v.borrow().clone()) {
        Some(Value::FloatArray(v)) => v,
        _ => {
            return Ok(soft_err(
                span,
                DspError::Param("Zxx.im float_array required".into()),
            ))
        }
    };
    let shape = match zxx.get("shape").map(|v| v.borrow().clone()) {
        Some(Value::IntArray(v)) if v.len() >= 2 => [v[0].max(0) as usize, v[1].max(0) as usize],
        _ => {
            return Ok(soft_err(
                span,
                DspError::Param("Zxx.shape [nfreq, nframes] required".into()),
            ))
        }
    };
    let opts = spectral_opts(optional_object(args, 1).as_ref());
    Ok(ok_or_soft(span, istft(&re, &im, shape, &opts), ok_floats))
}

// >>> import "ndsp"; let s = ndsp.spectrogram([1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], {nperseg: 4}); len(s.Sxx) > 0
// => true
fn ndsp_spectrogram(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ndsp_spectrogram", span)?;
    let x = floats(args, 0, "ndsp_spectrogram", span)?;
    let opts = spectral_opts(optional_object(args, 1).as_ref());
    Ok(ok_or_soft(span, spectrogram(&x, &opts), |s| {
        let mut m = HashMap::new();
        m.insert("f".into(), ok_floats(s.f));
        m.insert("t".into(), ok_floats(s.t));
        m.insert("Sxx".into(), ok_floats(s.sxx));
        m.insert(
            "shape".into(),
            Value::IntArray(vec![s.shape[0] as i64, s.shape[1] as i64]).ref_cell(),
        );
        Value::Object(m).ref_cell()
    }))
}

// >>> import "ndsp"; let p = ndsp.welch([1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], {nperseg: 4}); len(p.Pxx) > 0
// => true
fn ndsp_welch(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ndsp_welch", span)?;
    let x = floats(args, 0, "ndsp_welch", span)?;
    let opts = spectral_opts(optional_object(args, 1).as_ref());
    Ok(ok_or_soft(span, welch(&x, &opts), |p| {
        let mut m = HashMap::new();
        m.insert("f".into(), ok_floats(p.f));
        m.insert("Pxx".into(), ok_floats(p.pxx));
        Value::Object(m).ref_cell()
    }))
}

// >>> import "ndsp"; let p = ndsp.periodogram([1.0, 0.0, 0.0, 0.0]); len(p.f) > 0
// => true
fn ndsp_periodogram(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ndsp_periodogram", span)?;
    let x = floats(args, 0, "ndsp_periodogram", span)?;
    let opts = spectral_opts(optional_object(args, 1).as_ref());
    Ok(ok_or_soft(span, periodogram(&x, &opts), |p| {
        let mut m = HashMap::new();
        m.insert("f".into(), ok_floats(p.f));
        m.insert("Pxx".into(), ok_floats(p.pxx));
        Value::Object(m).ref_cell()
    }))
}

// >>> import "ndsp"; len(ndsp.detrend([1.0, 2.0, 3.0], "linear"))
// => 3
fn ndsp_detrend(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ndsp_detrend", span)?;
    let x = floats(args, 0, "ndsp_detrend", span)?;
    let kind = optional_string(args, 1).unwrap_or_else(|| "linear".into());
    Ok(ok_or_soft(span, detrend(&x, &kind), ok_floats))
}

// >>> import "ndsp"; let h = ndsp.hilbert([1.0, 0.0, (0.0 - 1.0), 0.0]); len(h.re)
// => 4
fn ndsp_hilbert(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ndsp_hilbert", span)?;
    let x = floats(args, 0, "ndsp_hilbert", span)?;
    Ok(ok_or_soft(span, hilbert(&x), |(re, im)| {
        let mut m = HashMap::new();
        m.insert("re".into(), ok_floats(re));
        m.insert("im".into(), ok_floats(im));
        Value::Object(m).ref_cell()
    }))
}

// >>> import "ndsp"; len(ndsp.medfilt([1.0, 100.0, 1.0], 3))
// => 3
fn ndsp_medfilt(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ndsp_medfilt", span)?;
    let x = floats(args, 0, "ndsp_medfilt", span)?;
    let k = if args.len() > 1 {
        int_arg(args, 1, "ndsp_medfilt", span)? as usize
    } else {
        3
    };
    Ok(ok_or_soft(span, medfilt(&x, k), ok_floats))
}

// >>> import "ndsp"; let p = ndsp.find_peaks([0.0, 1.0, 0.0, 2.0, 0.0]); len(p.peaks)
// => 2
fn ndsp_find_peaks(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ndsp_find_peaks", span)?;
    let x = floats(args, 0, "ndsp_find_peaks", span)?;
    let opts = optional_object(args, 1);
    let height = field_f64(opts.as_ref(), "height");
    let distance = field_i64(opts.as_ref(), "distance").map(|d| d.max(1) as usize);
    let p = find_peaks(&x, height, distance);
    let mut m = HashMap::new();
    m.insert(
        "peaks".into(),
        Value::IntArray(p.peaks.iter().map(|&i| i as i64).collect()).ref_cell(),
    );
    m.insert("heights".into(), ok_floats(p.heights));
    Ok(Value::Object(m).ref_cell())
}

// >>> import "ndsp"; let r = ndsp.freqz([1.0, 1.0], [1.0], {worN: 8}); len(r.w)
// => 8
fn ndsp_freqz(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "ndsp_freqz", span)?;
    let b = floats(args, 0, "ndsp_freqz", span)?;
    let (a, opts_idx) = if args.len() >= 2 {
        match &*args[1].borrow() {
            Value::Object(_) | Value::Nil => (vec![1.0], 1),
            _ => (floats(args, 1, "ndsp_freqz", span)?, 2),
        }
    } else {
        (vec![1.0], 1)
    };
    let opts = optional_object(args, opts_idx);
    let wor_n = field_i64(opts.as_ref(), "worN").unwrap_or(512).max(1) as usize;
    let fs = field_f64(opts.as_ref(), "fs");
    Ok(ok_or_soft(span, freqz(&b, &a, wor_n, fs), |r| {
        let mut h = HashMap::new();
        h.insert("re".into(), ok_floats(r.re));
        h.insert("im".into(), ok_floats(r.im));
        let mut m = HashMap::new();
        m.insert("w".into(), ok_floats(r.w));
        m.insert("h".into(), Value::Object(h).ref_cell());
        Value::Object(m).ref_cell()
    }))
}

// >>> import "ndsp"; let f = ndsp.butter(2, 0.2, {output: "sos"}); let r = ndsp.sosfreqz(f, {worN: 8}); len(r.w)
// => 8
fn ndsp_sosfreqz(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ndsp_sosfreqz", span)?;
    let sos = sos_from(args, 0, "ndsp_sosfreqz", span)?;
    let opts = optional_object(args, 1);
    let wor_n = field_i64(opts.as_ref(), "worN").unwrap_or(512).max(1) as usize;
    let fs = field_f64(opts.as_ref(), "fs");
    Ok(ok_or_soft(span, sosfreqz(&sos, wor_n, fs), |r| {
        let mut h = HashMap::new();
        h.insert("re".into(), ok_floats(r.re));
        h.insert("im".into(), ok_floats(r.im));
        let mut m = HashMap::new();
        m.insert("w".into(), ok_floats(r.w));
        m.insert("h".into(), Value::Object(h).ref_cell());
        Value::Object(m).ref_cell()
    }))
}

// >>> import "ndsp"; len(ndsp.chirp([0.0, 0.1, 0.2], 1.0, 1.0, 10.0))
// => 3
fn ndsp_chirp(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 4, 5, "ndsp_chirp", span)?;
    let t = floats(args, 0, "ndsp_chirp", span)?;
    let f0 = float_arg(args, 1, "ndsp_chirp", span)?;
    let t1 = float_arg(args, 2, "ndsp_chirp", span)?;
    let f1 = float_arg(args, 3, "ndsp_chirp", span)?;
    let method = optional_string(args, 4).unwrap_or_else(|| "linear".into());
    Ok(ok_or_soft(span, chirp(&t, f0, t1, f1, &method), ok_floats))
}

// >>> import "ndsp"; len(ndsp.sawtooth([0.0, 3.1415926535], 0.5))
// => 2
fn ndsp_sawtooth(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ndsp_sawtooth", span)?;
    let t = floats(args, 0, "ndsp_sawtooth", span)?;
    let width = if args.len() > 1 {
        float_arg(args, 1, "ndsp_sawtooth", span)?
    } else {
        1.0
    };
    Ok(ok_or_soft(span, sawtooth(&t, width), ok_floats))
}

// >>> import "ndsp"; len(ndsp.square([0.0, 3.1415926535], 0.5))
// => 2
fn ndsp_square(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ndsp_square", span)?;
    let t = floats(args, 0, "ndsp_square", span)?;
    let duty = if args.len() > 1 {
        float_arg(args, 1, "ndsp_square", span)?
    } else {
        0.5
    };
    Ok(ok_or_soft(span, square(&t, duty), ok_floats))
}

// >>> import "ndsp"; len(ndsp.gausspulse([0.0, 0.01, (0.0 - 0.01)], 1000.0, 0.5))
// => 3
fn ndsp_gausspulse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "ndsp_gausspulse", span)?;
    let t = floats(args, 0, "ndsp_gausspulse", span)?;
    let fc = if args.len() > 1 {
        float_arg(args, 1, "ndsp_gausspulse", span)?
    } else {
        1000.0
    };
    let bw = if args.len() > 2 {
        float_arg(args, 2, "ndsp_gausspulse", span)?
    } else {
        0.5
    };
    Ok(ok_or_soft(span, gausspulse(&t, fc, bw), ok_floats))
}

macro_rules! ndsp_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ndsp_fns![
    ("ndsp_convolve", "convolve", ndsp_convolve),
    ("ndsp_correlate", "correlate", ndsp_correlate),
    ("ndsp_fftconvolve", "fftconvolve", ndsp_fftconvolve),
    ("ndsp_hann", "hann", ndsp_hann),
    ("ndsp_hamming", "hamming", ndsp_hamming),
    ("ndsp_blackman", "blackman", ndsp_blackman),
    ("ndsp_bartlett", "bartlett", ndsp_bartlett),
    ("ndsp_boxcar", "boxcar", ndsp_boxcar),
    ("ndsp_kaiser", "kaiser", ndsp_kaiser),
    ("ndsp_tukey", "tukey", ndsp_tukey),
    ("ndsp_get_window", "get_window", ndsp_get_window),
    ("ndsp_firwin", "firwin", ndsp_firwin),
    ("ndsp_butter", "butter", ndsp_butter),
    ("ndsp_cheby1", "cheby1", ndsp_cheby1),
    ("ndsp_iirfilter", "iirfilter", ndsp_iirfilter),
    ("ndsp_lfilter", "lfilter", ndsp_lfilter),
    ("ndsp_filtfilt", "filtfilt", ndsp_filtfilt),
    ("ndsp_sosfilt", "sosfilt", ndsp_sosfilt),
    ("ndsp_sosfiltfilt", "sosfiltfilt", ndsp_sosfiltfilt),
    ("ndsp_tf2sos", "tf2sos", ndsp_tf2sos),
    ("ndsp_sos2tf", "sos2tf", ndsp_sos2tf),
    ("ndsp_resample", "resample", ndsp_resample),
    ("ndsp_resample_poly", "resample_poly", ndsp_resample_poly),
    ("ndsp_decimate", "decimate", ndsp_decimate),
    ("ndsp_upfirdn", "upfirdn", ndsp_upfirdn),
    ("ndsp_stft", "stft", ndsp_stft),
    ("ndsp_istft", "istft", ndsp_istft),
    ("ndsp_spectrogram", "spectrogram", ndsp_spectrogram),
    ("ndsp_welch", "welch", ndsp_welch),
    ("ndsp_periodogram", "periodogram", ndsp_periodogram),
    ("ndsp_detrend", "detrend", ndsp_detrend),
    ("ndsp_hilbert", "hilbert", ndsp_hilbert),
    ("ndsp_medfilt", "medfilt", ndsp_medfilt),
    ("ndsp_find_peaks", "find_peaks", ndsp_find_peaks),
    ("ndsp_freqz", "freqz", ndsp_freqz),
    ("ndsp_sosfreqz", "sosfreqz", ndsp_sosfreqz),
    ("ndsp_chirp", "chirp", ndsp_chirp),
    ("ndsp_sawtooth", "sawtooth", ndsp_sawtooth),
    ("ndsp_square", "square", ndsp_square),
    ("ndsp_gausspulse", "gausspulse", ndsp_gausspulse),
];

pub const MODULE_NAME: &str = "ndsp";
pub const MODULE_PATHS: &[&str] = &["ndsp", "std/ndsp"];

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
