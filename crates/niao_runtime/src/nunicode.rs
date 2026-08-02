//! Native nunicode standard library — Unicode correctness: NFC/NFD normalization,
//! grapheme clusters, categories, display width, casefold (~unicodedata, grapheme).
//!
//! Import with `import "nunicode"` (or `import "std/nunicode"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_unicode::{
    bidi, casefold, categories, category, char_len, chars, combining, decimal, decomposition,
    digit, display_width, east_asian_width, grapheme_at, grapheme_byte_offsets, grapheme_len,
    grapheme_slice, graphemes, is_alphabetic, is_control, is_normalized, is_numeric,
    is_whitespace, lookup, mirrored, name, nfc, nfd, nfkc, nfkd, normalize, numeric,
    parallel_casefold, parallel_display_width, parallel_normalize, script, truncate_width,
    NormalizationForm,
};
use niao_parallel::available_threads;
use std::collections::HashMap;
use std::rc::Rc;

const MAX_GRAPHEMES: usize = 16_777_216;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E3492_NUNICODE_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E3490_NUNICODE_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E3490_NUNICODE_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn nunicode_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E3491_NUNICODE_ERROR, "nunicode_error", msg.into(), span)
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

fn optional_string(args: &[ValueRef], idx: usize, default: &str) -> String {
    if args.len() <= idx {
        return default.to_string();
    }
    match &*args[idx].borrow() {
        Value::String(s) => s.clone(),
        _ => default.to_string(),
    }
}

fn optional_int(args: &[ValueRef], idx: usize, default: i64) -> i64 {
    if args.len() <= idx {
        return default;
    }
    match &*args[idx].borrow() {
        Value::Int(n) => *n,
        _ => default,
    }
}

fn str_val(s: impl Into<String>) -> NiaoResult<ValueRef> {
    Ok(Value::String(s.into()).ref_cell())
}

fn bool_val(b: bool) -> NiaoResult<ValueRef> {
    Ok(Value::Bool(b).ref_cell())
}

fn int_val(n: i64) -> NiaoResult<ValueRef> {
    Ok(Value::Int(n).ref_cell())
}

fn float_val(n: f64) -> NiaoResult<ValueRef> {
    Ok(Value::Float(n).ref_cell())
}

fn nil_val() -> NiaoResult<ValueRef> {
    Ok(Value::Nil.ref_cell())
}

fn string_array(items: Vec<String>) -> NiaoResult<ValueRef> {
    let out = items
        .into_iter()
        .map(|s| Value::String(s).ref_cell())
        .collect();
    Ok(Value::Array(out).ref_cell())
}

fn parse_form(s: &str, span: Span) -> Result<NormalizationForm, ValueRef> {
    NormalizationForm::parse(s).ok_or_else(|| {
        nunicode_err(
            span,
            format!("unknown normalization form '{s}' (use NFC, NFD, NFKC, or NFKD)"),
        )
    })
}

fn single_char(s: &str, span: Span, fn_name: &str) -> Result<char, ValueRef> {
    let mut it = s.chars();
    let first = match it.next() {
        Some(c) => c,
        None => {
            return Err(nunicode_err(
                span,
                format!("{fn_name}() expects a non-empty string"),
            ))
        }
    };
    if it.next().is_some() {
        return Err(nunicode_err(
            span,
            format!("{fn_name}() expects exactly one Unicode scalar"),
        ));
    }
    Ok(first)
}

fn string_list(args: &[ValueRef], name: &str, span: Span) -> NiaoResult<Vec<String>> {
    let mut out = Vec::with_capacity(args.len());
    for (i, arg) in args.iter().enumerate() {
        match &*arg.borrow() {
            Value::String(s) => out.push(s.clone()),
            other => {
                return Err(type_err(
                    span,
                    format!(
                        "{name}() expects strings; argument {} is {}",
                        i + 1,
                        other.type_name()
                    ),
                ));
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

// >>> import "nunicode"
// >>> nunicode.normalize("e\u{0301}")
// => "é"
fn nunicode_normalize(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nunicode_normalize", span)?;
    let s = string_arg(args, 0, "nunicode_normalize", span)?;
    let form = if args.len() == 2 {
        let f = string_arg(args, 1, "nunicode_normalize", span)?;
        match parse_form(&f, span) {
            Ok(v) => v,
            Err(e) => return Ok(e),
        }
    } else {
        NormalizationForm::Nfc
    };
    str_val(normalize(&s, form))
}

// >>> nunicode.is_normalized("é")
// => true
fn nunicode_is_normalized(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nunicode_is_normalized", span)?;
    let s = string_arg(args, 0, "nunicode_is_normalized", span)?;
    let form = if args.len() == 2 {
        let f = string_arg(args, 1, "nunicode_is_normalized", span)?;
        match parse_form(&f, span) {
            Ok(v) => v,
            Err(e) => return Ok(e),
        }
    } else {
        NormalizationForm::Nfc
    };
    bool_val(is_normalized(&s, form))
}

// >>> nunicode.nfc("e\u{0301}")
// => "é"
fn nunicode_nfc(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_nfc", span)?;
    str_val(nfc(&string_arg(args, 0, "nunicode_nfc", span)?))
}

// >>> nunicode.nfd("é")
// => "e\u{0301}"
fn nunicode_nfd(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_nfd", span)?;
    str_val(nfd(&string_arg(args, 0, "nunicode_nfd", span)?))
}

// >>> nunicode.nfkc("ﬁ")
// => "fi"
fn nunicode_nfkc(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_nfkc", span)?;
    str_val(nfkc(&string_arg(args, 0, "nunicode_nfkc", span)?))
}

// >>> nunicode.nfkd("ﬁ")
// => "fi"
fn nunicode_nfkd(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_nfkd", span)?;
    str_val(nfkd(&string_arg(args, 0, "nunicode_nfkd", span)?))
}

// >>> nunicode.graphemes("🇺🇸")
// => ["🇺🇸"]
fn nunicode_graphemes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_graphemes", span)?;
    let s = string_arg(args, 0, "nunicode_graphemes", span)?;
    let gs = graphemes(&s);
    if gs.len() > MAX_GRAPHEMES {
        return Ok(nunicode_err(
            span,
            format!("grapheme count {} exceeds limit {MAX_GRAPHEMES}", gs.len()),
        ));
    }
    string_array(gs)
}

// >>> nunicode.grapheme_len("🇺🇸")
// => 1
fn nunicode_grapheme_len(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_grapheme_len", span)?;
    int_val(grapheme_len(&string_arg(args, 0, "nunicode_grapheme_len", span)?) as i64)
}

// >>> nunicode.grapheme_at("abc", 1)
// => "b"
fn nunicode_grapheme_at(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nunicode_grapheme_at", span)?;
    let s = string_arg(args, 0, "nunicode_grapheme_at", span)?;
    let idx = int_arg(args, 1, "nunicode_grapheme_at", span)?;
    if idx < 0 {
        return Ok(nunicode_err(span, "grapheme index must be >= 0"));
    }
    match grapheme_at(&s, idx as usize) {
        Some(g) => str_val(g),
        None => nil_val(),
    }
}

// >>> nunicode.grapheme_slice("abc", 0, 2)
// => "ab"
fn nunicode_grapheme_slice(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nunicode_grapheme_slice", span)?;
    let s = string_arg(args, 0, "nunicode_grapheme_slice", span)?;
    let start = int_arg(args, 1, "nunicode_grapheme_slice", span)?;
    if start < 0 {
        return Ok(nunicode_err(span, "start index must be >= 0"));
    }
    let end = if args.len() == 3 {
        let e = int_arg(args, 2, "nunicode_grapheme_slice", span)?;
        if e < 0 {
            return Ok(nunicode_err(span, "end index must be >= 0"));
        }
        Some(e as usize)
    } else {
        None
    };
    match grapheme_slice(&s, start as usize, end) {
        Some(g) => str_val(g),
        None => Ok(nunicode_err(span, "invalid grapheme slice range")),
    }
}

// >>> nunicode.chars("ab")
// => ["a", "b"]
fn nunicode_chars(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_chars", span)?;
    string_array(chars(&string_arg(args, 0, "nunicode_chars", span)?))
}

// >>> nunicode.char_len("é")
// => 1
fn nunicode_char_len(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_char_len", span)?;
    int_val(char_len(&string_arg(args, 0, "nunicode_char_len", span)?) as i64)
}

// >>> nunicode.grapheme_offsets("ab")
// => [0, 1]
fn nunicode_grapheme_offsets(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_grapheme_offsets", span)?;
    let offs = grapheme_byte_offsets(&string_arg(args, 0, "nunicode_grapheme_offsets", span)?);
    let out = offs.into_iter().map(|n| Value::Int(n as i64).ref_cell()).collect();
    Ok(Value::Array(out).ref_cell())
}

// >>> nunicode.display_width("你好")
// => 4
fn nunicode_display_width(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_display_width", span)?;
    int_val(display_width(&string_arg(args, 0, "nunicode_display_width", span)?) as i64)
}

// >>> nunicode.truncate_width("你好世界", 4, "..")
// => "你好.."
fn nunicode_truncate_width(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nunicode_truncate_width", span)?;
    let s = string_arg(args, 0, "nunicode_truncate_width", span)?;
    let max_w = int_arg(args, 1, "nunicode_truncate_width", span)?;
    if max_w < 0 {
        return Ok(nunicode_err(span, "max width must be >= 0"));
    }
    let suffix = optional_string(args, 2, "...");
    str_val(truncate_width(&s, max_w as usize, &suffix))
}

// >>> nunicode.casefold("Straße")
// => "strasse"
fn nunicode_casefold(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_casefold", span)?;
    str_val(casefold(&string_arg(args, 0, "nunicode_casefold", span)?))
}

// >>> nunicode.category("A")
// => "Lu"
fn nunicode_category(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_category", span)?;
    let s = string_arg(args, 0, "nunicode_category", span)?;
    let ch = match single_char(&s, span, "nunicode.category") {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    match category(ch) {
        Some(c) => str_val(c),
        None => nil_val(),
    }
}

// >>> nunicode.categories("A1")
// => ["Lu", "Nd"]
fn nunicode_categories(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_categories", span)?;
    string_array(categories(&string_arg(args, 0, "nunicode_categories", span)?))
}

// >>> nunicode.name("A")
// => "LATIN CAPITAL LETTER A"
fn nunicode_name(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_name", span)?;
    let s = string_arg(args, 0, "nunicode_name", span)?;
    let ch = match single_char(&s, span, "nunicode.name") {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    match name(ch) {
        Some(n) => str_val(n),
        None => nil_val(),
    }
}

// >>> nunicode.lookup("LATIN CAPITAL LETTER A")
// => "A"
fn nunicode_lookup(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_lookup", span)?;
    let q = string_arg(args, 0, "nunicode_lookup", span)?;
    match lookup(&q) {
        Some(ch) => str_val(ch.to_string()),
        None => nil_val(),
    }
}

// >>> nunicode.script("A")
// => "Latn"
fn nunicode_script(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_script", span)?;
    let s = string_arg(args, 0, "nunicode_script", span)?;
    let ch = match single_char(&s, span, "nunicode.script") {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    match script(ch) {
        Some(sc) => str_val(sc),
        None => nil_val(),
    }
}

// >>> nunicode.bidi("A")
// => "L"
fn nunicode_bidi(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_bidi", span)?;
    let s = string_arg(args, 0, "nunicode_bidi", span)?;
    let ch = match single_char(&s, span, "nunicode.bidi") {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    str_val(bidi(ch))
}

// >>> nunicode.combining("\u{0301}")
// => 230
fn nunicode_combining(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_combining", span)?;
    let s = string_arg(args, 0, "nunicode_combining", span)?;
    let ch = match single_char(&s, span, "nunicode.combining") {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    int_val(combining(ch) as i64)
}

// >>> nunicode.east_asian_width("你")
// => "W"
fn nunicode_east_asian_width(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_east_asian_width", span)?;
    let s = string_arg(args, 0, "nunicode_east_asian_width", span)?;
    let ch = match single_char(&s, span, "nunicode.east_asian_width") {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    str_val(east_asian_width(ch))
}

// >>> nunicode.decimal("9")
// => 9
fn nunicode_decimal(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_decimal", span)?;
    let s = string_arg(args, 0, "nunicode_decimal", span)?;
    let ch = match single_char(&s, span, "nunicode.decimal") {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    match decimal(ch) {
        Some(n) => int_val(n),
        None => int_val(-1),
    }
}

// >>> nunicode.digit("9", 10)
// => 9
fn nunicode_digit(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nunicode_digit", span)?;
    let s = string_arg(args, 0, "nunicode_digit", span)?;
    let ch = match single_char(&s, span, "nunicode.digit") {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    let base = optional_int(args, 1, 10) as u32;
    match digit(ch, base) {
        Some(n) => int_val(n),
        None => int_val(-1),
    }
}

// >>> nunicode.numeric("½")
// => 0.5
fn nunicode_numeric(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_numeric", span)?;
    let s = string_arg(args, 0, "nunicode_numeric", span)?;
    let ch = match single_char(&s, span, "nunicode.numeric") {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    match numeric(ch) {
        Some(n) => float_val(n),
        None => nil_val(),
    }
}

// >>> nunicode.mirrored("(")
// => true
fn nunicode_mirrored(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_mirrored", span)?;
    let s = string_arg(args, 0, "nunicode_mirrored", span)?;
    let ch = match single_char(&s, span, "nunicode.mirrored") {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    bool_val(mirrored(ch))
}

// >>> nunicode.decomposition("Å")
// => "0041 030A"
fn nunicode_decomposition(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_decomposition", span)?;
    let s = string_arg(args, 0, "nunicode_decomposition", span)?;
    let ch = match single_char(&s, span, "nunicode.decomposition") {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    str_val(decomposition(ch))
}

// >>> nunicode.is_alphabetic("A")
// => true
fn nunicode_is_alphabetic(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_is_alphabetic", span)?;
    let s = string_arg(args, 0, "nunicode_is_alphabetic", span)?;
    let ch = match single_char(&s, span, "nunicode.is_alphabetic") {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    bool_val(is_alphabetic(ch))
}

// >>> nunicode.is_numeric("9")
// => true
fn nunicode_is_numeric(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_is_numeric", span)?;
    let s = string_arg(args, 0, "nunicode_is_numeric", span)?;
    let ch = match single_char(&s, span, "nunicode.is_numeric") {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    bool_val(is_numeric(ch))
}

// >>> nunicode.is_whitespace(" ")
// => true
fn nunicode_is_whitespace(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_is_whitespace", span)?;
    let s = string_arg(args, 0, "nunicode_is_whitespace", span)?;
    let ch = match single_char(&s, span, "nunicode.is_whitespace") {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    bool_val(is_whitespace(ch))
}

// >>> nunicode.is_control("\n")
// => true
fn nunicode_is_control(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_is_control", span)?;
    let s = string_arg(args, 0, "nunicode_is_control", span)?;
    let ch = match single_char(&s, span, "nunicode.is_control") {
        Ok(c) => c,
        Err(e) => return Ok(e),
    };
    bool_val(is_control(ch))
}

// >>> nunicode.parallel_normalize(["e\u{0301}"], "NFC")
// => ["é"]
fn nunicode_parallel_normalize(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nunicode_parallel_normalize", span)?;
    let items = match &*args[0].borrow() {
        Value::Array(vals) => {
            let mut out = Vec::with_capacity(vals.len());
            for (i, v) in vals.iter().enumerate() {
                match &*v.borrow() {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "nunicode.parallel_normalize() expects string array; index {i} is {}",
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            out
        }
        other => {
            return Err(type_err(
                span,
                format!(
                    "nunicode.parallel_normalize() expects an array, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    let form = if args.len() == 2 {
        let f = string_arg(args, 1, "nunicode_parallel_normalize", span)?;
        match parse_form(&f, span) {
            Ok(v) => v,
            Err(e) => return Ok(e),
        }
    } else {
        NormalizationForm::Nfc
    };
    let threads = available_threads();
    string_array(parallel_normalize(&items, form, threads))
}

// >>> nunicode.parallel_display_width(["ab", "你"])
// => [2, 2]
fn nunicode_parallel_display_width(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_parallel_display_width", span)?;
    let items = match &*args[0].borrow() {
        Value::Array(vals) => {
            let mut out = Vec::with_capacity(vals.len());
            for (i, v) in vals.iter().enumerate() {
                match &*v.borrow() {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "nunicode.parallel_display_width() expects string array; index {i} is {}",
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            out
        }
        other => {
            return Err(type_err(
                span,
                format!(
                    "nunicode.parallel_display_width() expects an array, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    let widths = parallel_display_width(&items, available_threads());
    let out = widths
        .into_iter()
        .map(|w| Value::Int(w as i64).ref_cell())
        .collect();
    Ok(Value::Array(out).ref_cell())
}

// >>> nunicode.parallel_casefold(["Straße"])
// => ["strasse"]
fn nunicode_parallel_casefold(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nunicode_parallel_casefold", span)?;
    let items = match &*args[0].borrow() {
        Value::Array(vals) => {
            let mut out = Vec::with_capacity(vals.len());
            for (i, v) in vals.iter().enumerate() {
                match &*v.borrow() {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "nunicode.parallel_casefold() expects string array; index {i} is {}",
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            out
        }
        other => {
            return Err(type_err(
                span,
                format!(
                    "nunicode.parallel_casefold() expects an array, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    string_array(parallel_casefold(&items, available_threads()))
}

// ---------------------------------------------------------------------------
// Module exports
// ---------------------------------------------------------------------------

macro_rules! nunicode_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nunicode_fns![
    ("nunicode_normalize", "normalize", nunicode_normalize),
    ("nunicode_is_normalized", "is_normalized", nunicode_is_normalized),
    ("nunicode_nfc", "nfc", nunicode_nfc),
    ("nunicode_nfd", "nfd", nunicode_nfd),
    ("nunicode_nfkc", "nfkc", nunicode_nfkc),
    ("nunicode_nfkd", "nfkd", nunicode_nfkd),
    ("nunicode_graphemes", "graphemes", nunicode_graphemes),
    ("nunicode_grapheme_len", "grapheme_len", nunicode_grapheme_len),
    ("nunicode_grapheme_at", "grapheme_at", nunicode_grapheme_at),
    ("nunicode_grapheme_slice", "grapheme_slice", nunicode_grapheme_slice),
    ("nunicode_chars", "chars", nunicode_chars),
    ("nunicode_char_len", "char_len", nunicode_char_len),
    ("nunicode_grapheme_offsets", "grapheme_offsets", nunicode_grapheme_offsets),
    ("nunicode_display_width", "display_width", nunicode_display_width),
    ("nunicode_truncate_width", "truncate_width", nunicode_truncate_width),
    ("nunicode_casefold", "casefold", nunicode_casefold),
    ("nunicode_category", "category", nunicode_category),
    ("nunicode_categories", "categories", nunicode_categories),
    ("nunicode_name", "name", nunicode_name),
    ("nunicode_lookup", "lookup", nunicode_lookup),
    ("nunicode_script", "script", nunicode_script),
    ("nunicode_bidi", "bidi", nunicode_bidi),
    ("nunicode_combining", "combining", nunicode_combining),
    ("nunicode_east_asian_width", "east_asian_width", nunicode_east_asian_width),
    ("nunicode_decimal", "decimal", nunicode_decimal),
    ("nunicode_digit", "digit", nunicode_digit),
    ("nunicode_numeric", "numeric", nunicode_numeric),
    ("nunicode_mirrored", "mirrored", nunicode_mirrored),
    ("nunicode_decomposition", "decomposition", nunicode_decomposition),
    ("nunicode_is_alphabetic", "is_alphabetic", nunicode_is_alphabetic),
    ("nunicode_is_numeric", "is_numeric", nunicode_is_numeric),
    ("nunicode_is_whitespace", "is_whitespace", nunicode_is_whitespace),
    ("nunicode_is_control", "is_control", nunicode_is_control),
    ("nunicode_parallel_normalize", "parallel_normalize", nunicode_parallel_normalize),
    (
        "nunicode_parallel_display_width",
        "parallel_display_width",
        nunicode_parallel_display_width
    ),
    ("nunicode_parallel_casefold", "parallel_casefold", nunicode_parallel_casefold),
];

pub const MODULE_NAME: &str = "nunicode";
pub const MODULE_PATHS: &[&str] = &["nunicode", "std/nunicode"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    #[test]
    fn nfc_builtin() {
        let out = nunicode_nfc(
            &[Value::String("e\u{0301}".into()).ref_cell()],
            span(),
        )
        .unwrap();
        assert!(matches!(&*out.borrow(), Value::String(s) if s == "é"));
    }

    #[test]
    fn grapheme_len_flag() {
        let out = nunicode_grapheme_len(&[Value::String("🇺🇸".into()).ref_cell()], span()).unwrap();
        assert!(matches!(&*out.borrow(), Value::Int(1)));
    }
}
