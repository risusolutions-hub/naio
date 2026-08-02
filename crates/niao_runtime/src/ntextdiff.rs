//! Native ntextdiff standard library — line/word text diff, unified patches,
//! 3-way merge (~difflib + diff-match-patch subset; beside ndiff structural).
//!
//! Import with `import "ntextdiff"` (or `import "std/ntextdiff"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_parallel::available_threads;
use niao_textdiff::{
    char_diff, char_diff_raw, compare, compare_joined, context, context_joined, levenshtein,
    line_changes, matching_blocks, merge, opcodes, parallel_diff, parallel_ratio, parallel_unified,
    patch_apply, patch_make, patch_make_dmp, quick_ratio, ratio, real_quick_ratio, restore,
    splitlines, word_diff, word_diff_inline, DiffOpts, DiffPair, Granularity, Matcher, MergeOpts,
    Opcode, PatchApplyResult, MAX_INPUT_BYTES,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E3563_NTEXTDIFF_ARITY: u32 = codes::E3563_NTEXTDIFF_ARITY;
const E3564_NTEXTDIFF_ERROR: u32 = codes::E3564_NTEXTDIFF_ERROR;
const E3565_NTEXTDIFF_TYPE: u32 = codes::E3565_NTEXTDIFF_TYPE;
const E3566_NTEXTDIFF_INVALID_HANDLE: u32 = codes::E3566_NTEXTDIFF_INVALID_HANDLE;

thread_local! {
    static MATCHERS: RefCell<HashMap<i64, Matcher>> = RefCell::new(HashMap::new());
    static NEXT_HANDLE: RefCell<i64> = const { RefCell::new(1) };
}

fn new_handle() -> i64 {
    NEXT_HANDLE.with(|h| {
        let mut h = h.borrow_mut();
        let id = *h;
        *h += 1;
        id
    })
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3565_NTEXTDIFF_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3563_NTEXTDIFF_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3563_NTEXTDIFF_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn ntextdiff_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3564_NTEXTDIFF_ERROR, "ntextdiff_error", msg.into(), span)
}

fn invalid_handle(span: Span, id: i64) -> ValueRef {
    error_value(
        E3566_NTEXTDIFF_INVALID_HANDLE,
        "ntextdiff_error",
        format!("invalid or closed ntextdiff handle {id}"),
        span,
    )
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


fn bool_val(b: bool) -> NiaoResult<ValueRef> {
    Ok(Value::Bool(b).ref_cell())
}

fn float_val(f: f64) -> NiaoResult<ValueRef> {
    Ok(Value::Float(f).ref_cell())
}

fn str_val(s: impl Into<String>) -> NiaoResult<ValueRef> {
    Ok(Value::String(s.into()).ref_cell())
}

fn string_array(items: Vec<String>) -> NiaoResult<ValueRef> {
    Ok(Value::Array(items.into_iter().map(|s| Value::String(s).ref_cell()).collect()).ref_cell())
}

fn parse_opts(args: &[ValueRef], idx: usize, span: Span) -> NiaoResult<HashMap<String, ValueRef>> {
    if args.len() <= idx {
        return Ok(HashMap::new());
    }
    match &*args[idx].borrow() {
        Value::Object(map) => Ok(map.clone()),
        Value::Nil => Ok(HashMap::new()),
        other => Err(type_err(span, format!("opts must be an object, got {}", other.type_name()))),
    }
}

fn obj_bool(map: &HashMap<String, ValueRef>, key: &str, default: bool) -> bool {
    map.get(key)
        .and_then(|v| match &*v.borrow() {
            Value::Bool(b) => Some(*b),
            Value::Int(n) => Some(*n != 0),
            _ => None,
        })
        .unwrap_or(default)
}

fn obj_int(map: &HashMap<String, ValueRef>, key: &str, default: i64) -> i64 {
    map.get(key)
        .and_then(|v| match &*v.borrow() {
            Value::Int(n) => Some(*n),
            Value::Float(f) if f.fract() == 0.0 => Some(*f as i64),
            _ => None,
        })
        .unwrap_or(default)
}

fn obj_string_opt(map: &HashMap<String, ValueRef>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.clone()),
        _ => None,
    })
}

fn parse_granularity(map: &HashMap<String, ValueRef>) -> Granularity {
    obj_string_opt(map, "granularity")
        .and_then(|s| Granularity::parse(&s))
        .unwrap_or(Granularity::Line)
}

fn parse_diff_opts(map: &HashMap<String, ValueRef>) -> DiffOpts {
    let mut opts = DiffOpts::default();
    opts.ignore_whitespace = obj_bool(map, "ignore_whitespace", false);
    opts.ignore_case = obj_bool(map, "ignore_case", false);
    opts.context = obj_int(map, "context", 3) as usize;
    opts.join = obj_bool(map, "join", false);
    opts.autojunk = obj_bool(map, "autojunk", false);
    opts.fuzz = obj_int(map, "fuzz", 0) as i32;
    if let Some(s) = obj_string_opt(map, "fromfile") {
        opts.fromfile = s;
    }
    if let Some(s) = obj_string_opt(map, "tofile") {
        opts.tofile = s;
    }
    if let Some(s) = obj_string_opt(map, "fromfiledate") {
        opts.fromfiledate = s;
    }
    if let Some(s) = obj_string_opt(map, "tofiledate") {
        opts.tofiledate = s;
    }
    if let Some(s) = obj_string_opt(map, "lineterm") {
        opts.lineterm = s;
    }
    if let Some(s) = obj_string_opt(map, "algorithm") {
        if let Some(a) = niao_textdiff::parse_algorithm(&s) {
            opts.algorithm = a;
        }
    }
    opts
}

fn parse_merge_opts(map: &HashMap<String, ValueRef>) -> MergeOpts {
    let mut opts = MergeOpts::default();
    if let Some(s) = obj_string_opt(map, "marker_ours") {
        opts.marker_ours = s;
    }
    if let Some(s) = obj_string_opt(map, "marker_base") {
        opts.marker_base = s;
    }
    if let Some(s) = obj_string_opt(map, "marker_theirs") {
        opts.marker_theirs = s;
    }
    if let Some(s) = obj_string_opt(map, "marker_end") {
        opts.marker_end = s;
    }
    opts
}

fn opcodes_to_array(ops: Vec<Opcode>) -> NiaoResult<ValueRef> {
    let items: Vec<ValueRef> = ops
        .into_iter()
        .map(|op| {
            let mut m = HashMap::new();
            m.insert("tag".into(), Value::String(op.tag).ref_cell());
            m.insert("i1".into(), Value::Int(op.i1 as i64).ref_cell());
            m.insert("i2".into(), Value::Int(op.i2 as i64).ref_cell());
            m.insert("j1".into(), Value::Int(op.j1 as i64).ref_cell());
            m.insert("j2".into(), Value::Int(op.j2 as i64).ref_cell());
            Value::Object(m).ref_cell()
        })
        .collect();
    Ok(Value::Array(items).ref_cell())
}

fn changes_to_array(items: Vec<(String, String)>) -> NiaoResult<ValueRef> {
    Ok(Value::Array(
        items
            .into_iter()
            .map(|(tag, value)| {
                let mut m = HashMap::new();
                m.insert("tag".into(), Value::String(tag).ref_cell());
                m.insert("value".into(), Value::String(value).ref_cell());
                Value::Object(m).ref_cell()
            })
            .collect(),
    )
    .ref_cell())
}

fn char_changes_to_array(items: Vec<niao_textdiff::CharChange>) -> NiaoResult<ValueRef> {
    Ok(Value::Array(
        items
            .into_iter()
            .map(|c| {
                let mut m = HashMap::new();
                m.insert("op".into(), Value::Int(c.op as i64).ref_cell());
                m.insert("text".into(), Value::String(c.text).ref_cell());
                Value::Object(m).ref_cell()
            })
            .collect(),
    )
    .ref_cell())
}

fn patch_result_obj(res: PatchApplyResult) -> NiaoResult<ValueRef> {
    let mut m = HashMap::new();
    m.insert("text".into(), Value::String(res.text).ref_cell());
    m.insert(
        "applied".into(),
        Value::Array(res.applied.into_iter().map(|b| Value::Bool(b).ref_cell()).collect()).ref_cell(),
    );
    Ok(Value::Object(m).ref_cell())
}

fn pairs_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<DiffPair>> {
    match &*args[idx].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::Object(map) => {
                        let from = map
                            .get("from")
                            .or_else(|| map.get("a"))
                            .ok_or_else(|| {
                                type_err(span, format!("{name}() pair {} missing from/a", i + 1))
                            })?;
                        let to = map
                            .get("to")
                            .or_else(|| map.get("b"))
                            .ok_or_else(|| {
                                type_err(span, format!("{name}() pair {} missing to/b", i + 1))
                            })?;
                        let from_s = match &*from.borrow() {
                            Value::String(s) => s.clone(),
                            other => {
                                return Err(type_err(
                                    span,
                                    format!("pair {} from is {}", i + 1, other.type_name()),
                                ));
                            }
                        };
                        let to_s = match &*to.borrow() {
                            Value::String(s) => s.clone(),
                            other => {
                                return Err(type_err(
                                    span,
                                    format!("pair {} to is {}", i + 1, other.type_name()),
                                ));
                            }
                        };
                        out.push(DiffPair { from: from_s, to: to_s });
                    }
                    other => {
                        return Err(type_err(
                            span,
                            format!("{name}() pair {} must be object, got {}", i + 1, other.type_name()),
                        ));
                    }
                }
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!("{name}() expects array of pairs, got {}", other.type_name()),
        )),
    }
}

fn with_matcher<F>(id: i64, span: Span, f: F) -> NiaoResult<ValueRef>
where
    F: FnOnce(&Matcher) -> NiaoResult<ValueRef>,
{
    MATCHERS.with(|m| {
        let m = m.borrow();
        match m.get(&id) {
            Some(mat) => f(mat),
            None => Ok(invalid_handle(span, id)),
        }
    })
}

// >>> import "ntextdiff"
// >>> ntextdiff.compare("a\nb\n", "a\nc\n")[1]
// "- b"
fn ntextdiff_compare(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntextdiff_compare", span)?;
    let a = string_arg(args, 0, "ntextdiff_compare", span)?;
    let b = string_arg(args, 1, "ntextdiff_compare", span)?;
    let opts = parse_diff_opts(&parse_opts(args, 2, span)?);
    if opts.join {
        match compare_joined(&a, &b, &opts) {
            Ok(s) => str_val(s),
            Err(e) => Ok(ntextdiff_err(span, e.to_string())),
        }
    } else {
        match compare(&a, &b, &opts) {
            Ok(lines) => string_array(lines),
            Err(e) => Ok(ntextdiff_err(span, e.to_string())),
        }
    }
}

// >>> ntextdiff.unified("a\n", "b\n", {join: true}) != ""
fn ntextdiff_unified(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntextdiff_unified", span)?;
    let a = string_arg(args, 0, "ntextdiff_unified", span)?;
    let b = string_arg(args, 1, "ntextdiff_unified", span)?;
    let opts = parse_diff_opts(&parse_opts(args, 2, span)?);
    if opts.join {
        match niao_textdiff::unified_joined(&a, &b, &opts) {
            Ok(s) => str_val(s),
            Err(e) => Ok(ntextdiff_err(span, e.to_string())),
        }
    } else {
        match niao_textdiff::unified(&a, &b, &opts) {
            Ok(lines) => string_array(lines),
            Err(e) => Ok(ntextdiff_err(span, e.to_string())),
        }
    }
}

// >>> ntextdiff.context("a\n", "b\n", {join: true}) != ""
fn ntextdiff_context(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntextdiff_context", span)?;
    let a = string_arg(args, 0, "ntextdiff_context", span)?;
    let b = string_arg(args, 1, "ntextdiff_context", span)?;
    let opts = parse_diff_opts(&parse_opts(args, 2, span)?);
    if opts.join {
        match context_joined(&a, &b, &opts) {
            Ok(s) => str_val(s),
            Err(e) => Ok(ntextdiff_err(span, e.to_string())),
        }
    } else {
        match context(&a, &b, &opts) {
            Ok(lines) => string_array(lines),
            Err(e) => Ok(ntextdiff_err(span, e.to_string())),
        }
    }
}

// >>> ntextdiff.ratio("abc", "abc") >= 0.99
fn ntextdiff_ratio(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntextdiff_ratio", span)?;
    let a = string_arg(args, 0, "ntextdiff_ratio", span)?;
    let b = string_arg(args, 1, "ntextdiff_ratio", span)?;
    let map = parse_opts(args, 2, span)?;
    let opts = parse_diff_opts(&map);
    let g = parse_granularity(&map);
    match ratio(&a, &b, &opts, g) {
        Ok(r) => float_val(r),
        Err(e) => Ok(ntextdiff_err(span, e.to_string())),
    }
}

// >>> ntextdiff.quick_ratio("abc", "abd") > 0.0
fn ntextdiff_quick_ratio(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntextdiff_quick_ratio", span)?;
    let a = string_arg(args, 0, "ntextdiff_quick_ratio", span)?;
    let b = string_arg(args, 1, "ntextdiff_quick_ratio", span)?;
    let map = parse_opts(args, 2, span)?;
    let opts = parse_diff_opts(&map);
    let g = parse_granularity(&map);
    match quick_ratio(&a, &b, &opts, g) {
        Ok(r) => float_val(r),
        Err(e) => Ok(ntextdiff_err(span, e.to_string())),
    }
}

// >>> ntextdiff.real_quick_ratio("abc", "abd") > 0.0
fn ntextdiff_real_quick_ratio(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntextdiff_real_quick_ratio", span)?;
    let a = string_arg(args, 0, "ntextdiff_real_quick_ratio", span)?;
    let b = string_arg(args, 1, "ntextdiff_real_quick_ratio", span)?;
    let map = parse_opts(args, 2, span)?;
    let opts = parse_diff_opts(&map);
    let g = parse_granularity(&map);
    match real_quick_ratio(&a, &b, &opts, g) {
        Ok(r) => float_val(r),
        Err(e) => Ok(ntextdiff_err(span, e.to_string())),
    }
}

// >>> len(ntextdiff.opcodes("a\n", "b\n")) >= 1
fn ntextdiff_opcodes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntextdiff_opcodes", span)?;
    let a = string_arg(args, 0, "ntextdiff_opcodes", span)?;
    let b = string_arg(args, 1, "ntextdiff_opcodes", span)?;
    let map = parse_opts(args, 2, span)?;
    let opts = parse_diff_opts(&map);
    let g = parse_granularity(&map);
    match opcodes(&a, &b, &opts, g) {
        Ok(ops) => opcodes_to_array(ops),
        Err(e) => Ok(ntextdiff_err(span, e.to_string())),
    }
}

// >>> len(ntextdiff.matching_blocks("a\n", "a\n")) >= 1
fn ntextdiff_matching_blocks(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntextdiff_matching_blocks", span)?;
    let a = string_arg(args, 0, "ntextdiff_matching_blocks", span)?;
    let b = string_arg(args, 1, "ntextdiff_matching_blocks", span)?;
    let map = parse_opts(args, 2, span)?;
    let opts = parse_diff_opts(&map);
    let g = parse_granularity(&map);
    match matching_blocks(&a, &b, &opts, g) {
        Ok(blocks) => {
            let items: Vec<ValueRef> = blocks
                .into_iter()
                .map(|b| {
                    let mut m = HashMap::new();
                    m.insert("a".into(), Value::Int(b.a as i64).ref_cell());
                    m.insert("b".into(), Value::Int(b.b as i64).ref_cell());
                    m.insert("size".into(), Value::Int(b.size as i64).ref_cell());
                    Value::Object(m).ref_cell()
                })
                .collect();
            Ok(Value::Array(items).ref_cell())
        }
        Err(e) => Ok(ntextdiff_err(span, e.to_string())),
    }
}

// >>> len(ntextdiff.line_changes("a\n", "b\n")) >= 1
fn ntextdiff_line_changes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntextdiff_line_changes", span)?;
    let a = string_arg(args, 0, "ntextdiff_line_changes", span)?;
    let b = string_arg(args, 1, "ntextdiff_line_changes", span)?;
    let opts = parse_diff_opts(&parse_opts(args, 2, span)?);
    match line_changes(&a, &b, &opts) {
        Ok(ch) => changes_to_array(ch.into_iter().map(|c| (c.tag, c.value)).collect()),
        Err(e) => Ok(ntextdiff_err(span, e.to_string())),
    }
}

// >>> len(ntextdiff.word_diff("a b", "a c")) >= 1
fn ntextdiff_word_diff(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntextdiff_word_diff", span)?;
    let a = string_arg(args, 0, "ntextdiff_word_diff", span)?;
    let b = string_arg(args, 1, "ntextdiff_word_diff", span)?;
    let opts = parse_diff_opts(&parse_opts(args, 2, span)?);
    match word_diff(&a, &b, &opts) {
        Ok(ch) => changes_to_array(ch.into_iter().map(|c| (c.tag, c.value)).collect()),
        Err(e) => Ok(ntextdiff_err(span, e.to_string())),
    }
}

// >>> len(ntextdiff.word_diff_inline("a b", "a c")) > 0
fn ntextdiff_word_diff_inline(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntextdiff_word_diff_inline", span)?;
    let a = string_arg(args, 0, "ntextdiff_word_diff_inline", span)?;
    let b = string_arg(args, 1, "ntextdiff_word_diff_inline", span)?;
    let opts = parse_diff_opts(&parse_opts(args, 2, span)?);
    match word_diff_inline(&a, &b, &opts) {
        Ok(s) => str_val(s),
        Err(e) => Ok(ntextdiff_err(span, e.to_string())),
    }
}

// >>> len(ntextdiff.char_diff("abc", "axc")) >= 1
fn ntextdiff_char_diff(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntextdiff_char_diff", span)?;
    let a = string_arg(args, 0, "ntextdiff_char_diff", span)?;
    let b = string_arg(args, 1, "ntextdiff_char_diff", span)?;
    let opts = parse_diff_opts(&parse_opts(args, 2, span)?);
    match char_diff(&a, &b, &opts) {
        Ok(ch) => char_changes_to_array(ch),
        Err(e) => Ok(ntextdiff_err(span, e.to_string())),
    }
}

// >>> len(ntextdiff.char_diff_raw("abc", "axc")) >= 1
fn ntextdiff_char_diff_raw(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntextdiff_char_diff_raw", span)?;
    let a = string_arg(args, 0, "ntextdiff_char_diff_raw", span)?;
    let b = string_arg(args, 1, "ntextdiff_char_diff_raw", span)?;
    let opts = parse_diff_opts(&parse_opts(args, 2, span)?);
    match char_diff_raw(&a, &b, &opts) {
        Ok(ch) => char_changes_to_array(ch),
        Err(e) => Ok(ntextdiff_err(span, e.to_string())),
    }
}

// >>> ntextdiff.levenshtein("abc", "axc") >= 1
fn ntextdiff_levenshtein(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ntextdiff_levenshtein", span)?;
    let a = string_arg(args, 0, "ntextdiff_levenshtein", span)?;
    let b = string_arg(args, 1, "ntextdiff_levenshtein", span)?;
    match levenshtein(&a, &b) {
        Ok(n) => Ok(Value::Int(n as i64).ref_cell()),
        Err(e) => Ok(ntextdiff_err(span, e.to_string())),
    }
}

// >>> ntextdiff.restore(1, ntextdiff.compare("a\n", "b\n")) == "a\n"
fn ntextdiff_restore(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ntextdiff_restore", span)?;
    let which = int_arg(args, 0, "ntextdiff_restore", span)?;
    let lines = match &*args[1].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!("restore() line {} is {}", i + 1, other.type_name()),
                        ));
                    }
                }
            }
            out
        }
        other => {
            return Err(type_err(
                span,
                format!("restore() expects string array, got {}", other.type_name()),
            ));
        }
    };
    match restore(which as i32, &lines) {
        Ok(s) => str_val(s),
        Err(e) => Ok(ntextdiff_err(span, e.to_string())),
    }
}

// >>> len(ntextdiff.patch_make("a\n", "b\n")) > 0
fn ntextdiff_patch_make(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntextdiff_patch_make", span)?;
    let a = string_arg(args, 0, "ntextdiff_patch_make", span)?;
    let b = string_arg(args, 1, "ntextdiff_patch_make", span)?;
    let map = parse_opts(args, 2, span)?;
    let dmp = obj_bool(&map, "dmp", false);
    let opts = parse_diff_opts(&map);
    let res = if dmp {
        patch_make_dmp(&a, &b, &opts)
    } else {
        patch_make(&a, &b, &opts)
    };
    match res {
        Ok(s) => str_val(s),
        Err(e) => Ok(ntextdiff_err(span, e.to_string())),
    }
}

// >>> ntextdiff.patch_apply("a\n", ntextdiff.patch_make("a\n", "b\n")).text != ""
fn ntextdiff_patch_apply(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntextdiff_patch_apply", span)?;
    let text = string_arg(args, 0, "ntextdiff_patch_apply", span)?;
    let patch = string_arg(args, 1, "ntextdiff_patch_apply", span)?;
    let opts = parse_diff_opts(&parse_opts(args, 2, span)?);
    match patch_apply(&text, &patch, &opts) {
        Ok(res) => patch_result_obj(res),
        Err(e) => Ok(ntextdiff_err(span, e.to_string())),
    }
}

// >>> ntextdiff.merge("a\n", "b\n", "c\n").merged != ""
fn ntextdiff_merge(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "ntextdiff_merge", span)?;
    let base = string_arg(args, 0, "ntextdiff_merge", span)?;
    let ours = string_arg(args, 1, "ntextdiff_merge", span)?;
    let theirs = string_arg(args, 2, "ntextdiff_merge", span)?;
    let map = parse_opts(args, 3, span)?;
    let mut mopts = parse_merge_opts(&map);
    mopts.diff = parse_diff_opts(&map);
    match merge(&base, &ours, &theirs, &mopts) {
        Ok(res) => {
            let mut out = HashMap::new();
            out.insert("merged".into(), Value::String(res.merged).ref_cell());
            let conflicts: Vec<ValueRef> = res
                .conflicts
                .into_iter()
                .map(|c| {
                    let mut m = HashMap::new();
                    m.insert("start".into(), Value::Int(c.start as i64).ref_cell());
                    m.insert("end".into(), Value::Int(c.end as i64).ref_cell());
                    m.insert("base".into(), Value::String(c.base).ref_cell());
                    m.insert("ours".into(), Value::String(c.ours).ref_cell());
                    m.insert("theirs".into(), Value::String(c.theirs).ref_cell());
                    Value::Object(m).ref_cell()
                })
                .collect();
            out.insert("conflicts".into(), Value::Array(conflicts).ref_cell());
            Ok(Value::Object(out).ref_cell())
        }
        Err(e) => Ok(ntextdiff_err(span, e.to_string())),
    }
}

// >>> len(ntextdiff.splitlines("a\nb")) == 2
fn ntextdiff_splitlines(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ntextdiff_splitlines", span)?;
    let text = string_arg(args, 0, "ntextdiff_splitlines", span)?;
    let keepends = if args.len() > 1 {
        match &*args[1].borrow() {
            Value::Bool(b) => *b,
            Value::Object(map) => obj_bool(map, "keepends", false),
            other => {
                return Err(type_err(
                    span,
                    format!("splitlines() opts must be bool/object, got {}", other.type_name()),
                ));
            }
        }
    } else {
        false
    };
    string_array(splitlines(&text, keepends))
}

// >>> let m = ntextdiff.matcher("a\n", "b\n"); ntextdiff.close(m)
fn ntextdiff_matcher(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ntextdiff_matcher", span)?;
    let a = string_arg(args, 0, "ntextdiff_matcher", span)?;
    let b = string_arg(args, 1, "ntextdiff_matcher", span)?;
    let map = parse_opts(args, 2, span)?;
    let opts = parse_diff_opts(&map);
    let g = parse_granularity(&map);
    match Matcher::new(&a, &b, opts, g) {
        Ok(m) => {
            let id = new_handle();
            MATCHERS.with(|h| h.borrow_mut().insert(id, m));
            Ok(Value::Int(id).ref_cell())
        }
        Err(e) => Ok(ntextdiff_err(span, e.to_string())),
    }
}

// >>> ntextdiff.close(0) == false
fn ntextdiff_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntextdiff_close", span)?;
    let id = int_arg(args, 0, "ntextdiff_close", span)?;
    let removed = MATCHERS.with(|h| h.borrow_mut().remove(&id).is_some());
    bool_val(removed)
}

// >>> let m = ntextdiff.matcher("a", "a"); ntextdiff.matcher_ratio(m) >= 0.99
fn ntextdiff_matcher_ratio(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntextdiff_matcher_ratio", span)?;
    let id = int_arg(args, 0, "ntextdiff_matcher_ratio", span)?;
    with_matcher(id, span, |m| float_val(m.ratio()))
}

// >>> let m = ntextdiff.matcher("a", "b"); ntextdiff.matcher_quick_ratio(m) > 0.0
fn ntextdiff_matcher_quick_ratio(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntextdiff_matcher_quick_ratio", span)?;
    let id = int_arg(args, 0, "ntextdiff_matcher_quick_ratio", span)?;
    with_matcher(id, span, |m| float_val(m.quick_ratio()))
}

// >>> let m = ntextdiff.matcher("a", "b"); ntextdiff.matcher_real_quick_ratio(m) > 0.0
fn ntextdiff_matcher_real_quick_ratio(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntextdiff_matcher_real_quick_ratio", span)?;
    let id = int_arg(args, 0, "ntextdiff_matcher_real_quick_ratio", span)?;
    with_matcher(id, span, |m| float_val(m.real_quick_ratio()))
}

// >>> let m = ntextdiff.matcher("a\n", "b\n"); len(ntextdiff.matcher_opcodes(m)) >= 1
fn ntextdiff_matcher_opcodes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntextdiff_matcher_opcodes", span)?;
    let id = int_arg(args, 0, "ntextdiff_matcher_opcodes", span)?;
    with_matcher(id, span, |m| opcodes_to_array(m.opcodes()))
}

// >>> let m = ntextdiff.matcher("a\n", "a\n"); len(ntextdiff.matcher_matching_blocks(m)) >= 1
fn ntextdiff_matcher_matching_blocks(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntextdiff_matcher_matching_blocks", span)?;
    let id = int_arg(args, 0, "ntextdiff_matcher_matching_blocks", span)?;
    with_matcher(id, span, |m| {
        let blocks: Vec<ValueRef> = m
            .matching_blocks()
            .into_iter()
            .map(|b| {
                let mut map = HashMap::new();
                map.insert("a".into(), Value::Int(b.a as i64).ref_cell());
                map.insert("b".into(), Value::Int(b.b as i64).ref_cell());
                map.insert("size".into(), Value::Int(b.size as i64).ref_cell());
                Value::Object(map).ref_cell()
            })
            .collect();
        Ok(Value::Array(blocks).ref_cell())
    })
}

// >>> len(ntextdiff.parallel_diff([{from: "a\n", to: "b\n"}])) == 1
fn ntextdiff_parallel_diff(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ntextdiff_parallel_diff", span)?;
    let pairs = pairs_arg(args, 0, "ntextdiff_parallel_diff", span)?;
    let map = parse_opts(args, 1, span)?;
    let opts = parse_diff_opts(&map);
    let threads = obj_int(&map, "threads", available_threads() as i64) as usize;
    match parallel_diff(&pairs, &opts, threads) {
        Ok(results) => {
            let items: Vec<ValueRef> = results
                .into_iter()
                .map(|r| {
                    let mut m = HashMap::new();
                    m.insert(
                        "unified".into(),
                        Value::Array(r.unified.into_iter().map(|s| Value::String(s).ref_cell()).collect())
                            .ref_cell(),
                    );
                    m.insert("ratio".into(), Value::Float(r.ratio).ref_cell());
                    Value::Object(m).ref_cell()
                })
                .collect();
            Ok(Value::Array(items).ref_cell())
        }
        Err(e) => Ok(ntextdiff_err(span, e.to_string())),
    }
}

// >>> len(ntextdiff.parallel_ratio([{from: "a", to: "a"}])) == 1
fn ntextdiff_parallel_ratio(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ntextdiff_parallel_ratio", span)?;
    let pairs = pairs_arg(args, 0, "ntextdiff_parallel_ratio", span)?;
    let map = parse_opts(args, 1, span)?;
    let opts = parse_diff_opts(&map);
    let threads = obj_int(&map, "threads", available_threads() as i64) as usize;
    match parallel_ratio(&pairs, &opts, threads) {
        Ok(vals) => Ok(Value::Array(vals.into_iter().map(|f| Value::Float(f).ref_cell()).collect()).ref_cell()),
        Err(e) => Ok(ntextdiff_err(span, e.to_string())),
    }
}

// >>> len(ntextdiff.parallel_unified([{from: "a\n", to: "b\n"}])) == 1
fn ntextdiff_parallel_unified(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ntextdiff_parallel_unified", span)?;
    let pairs = pairs_arg(args, 0, "ntextdiff_parallel_unified", span)?;
    let map = parse_opts(args, 1, span)?;
    let opts = parse_diff_opts(&map);
    let threads = obj_int(&map, "threads", available_threads() as i64) as usize;
    match parallel_unified(&pairs, &opts, threads) {
        Ok(batch) => {
            let items: Vec<ValueRef> = batch
                .into_iter()
                .map(|lines| {
                    Value::Array(lines.into_iter().map(|s| Value::String(s).ref_cell()).collect()).ref_cell()
                })
                .collect();
            Ok(Value::Array(items).ref_cell())
        }
        Err(e) => Ok(ntextdiff_err(span, e.to_string())),
    }
}

// >>> ntextdiff.max_input_bytes() > 0
fn ntextdiff_max_input_bytes(_args: &[ValueRef], _span: Span) -> NiaoResult<ValueRef> {
    Ok(Value::Int(MAX_INPUT_BYTES as i64).ref_cell())
}

macro_rules! ntextdiff_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ntextdiff_fns![
    ("ntextdiff_compare", "compare", ntextdiff_compare),
    ("ntextdiff_unified", "unified", ntextdiff_unified),
    ("ntextdiff_context", "context", ntextdiff_context),
    ("ntextdiff_ratio", "ratio", ntextdiff_ratio),
    ("ntextdiff_quick_ratio", "quick_ratio", ntextdiff_quick_ratio),
    ("ntextdiff_real_quick_ratio", "real_quick_ratio", ntextdiff_real_quick_ratio),
    ("ntextdiff_opcodes", "opcodes", ntextdiff_opcodes),
    ("ntextdiff_matching_blocks", "matching_blocks", ntextdiff_matching_blocks),
    ("ntextdiff_line_changes", "line_changes", ntextdiff_line_changes),
    ("ntextdiff_word_diff", "word_diff", ntextdiff_word_diff),
    ("ntextdiff_word_diff_inline", "word_diff_inline", ntextdiff_word_diff_inline),
    ("ntextdiff_char_diff", "char_diff", ntextdiff_char_diff),
    ("ntextdiff_char_diff_raw", "char_diff_raw", ntextdiff_char_diff_raw),
    ("ntextdiff_levenshtein", "levenshtein", ntextdiff_levenshtein),
    ("ntextdiff_restore", "restore", ntextdiff_restore),
    ("ntextdiff_patch_make", "patch_make", ntextdiff_patch_make),
    ("ntextdiff_patch_apply", "patch_apply", ntextdiff_patch_apply),
    ("ntextdiff_merge", "merge", ntextdiff_merge),
    ("ntextdiff_splitlines", "splitlines", ntextdiff_splitlines),
    ("ntextdiff_matcher", "matcher", ntextdiff_matcher),
    ("ntextdiff_close", "close", ntextdiff_close),
    ("ntextdiff_matcher_ratio", "matcher_ratio", ntextdiff_matcher_ratio),
    ("ntextdiff_matcher_quick_ratio", "matcher_quick_ratio", ntextdiff_matcher_quick_ratio),
    ("ntextdiff_matcher_real_quick_ratio", "matcher_real_quick_ratio", ntextdiff_matcher_real_quick_ratio),
    ("ntextdiff_matcher_opcodes", "matcher_opcodes", ntextdiff_matcher_opcodes),
    ("ntextdiff_matcher_matching_blocks", "matcher_matching_blocks", ntextdiff_matcher_matching_blocks),
    ("ntextdiff_parallel_diff", "parallel_diff", ntextdiff_parallel_diff),
    ("ntextdiff_parallel_ratio", "parallel_ratio", ntextdiff_parallel_ratio),
    ("ntextdiff_parallel_unified", "parallel_unified", ntextdiff_parallel_unified),
    ("ntextdiff_max_input_bytes", "max_input_bytes", ntextdiff_max_input_bytes),
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

pub const MODULE_NAME: &str = "ntextdiff";
pub const MODULE_PATHS: &[&str] = &["ntextdiff", "std/ntextdiff"];

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
    fn compare_doctest() {
        let v = ntextdiff_compare(
            &[
                Value::String("a\nb\n".into()).ref_cell(),
                Value::String("a\nc\n".into()).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        match &*v.borrow() {
            Value::Array(items) => assert!(items.len() >= 2),
            other => panic!("expected array, got {other:?}"),
        }
    }
}
