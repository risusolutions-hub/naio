//! Native nsanitize standard library — allowlist HTML sanitizer for user content
//! (XSS-safe), URL scheme policy (~bleach, nh3 subset).
//!
//! Import with `import "nsanitize"` (or `import "std/nsanitize"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_parallel::available_threads;
use niao_sanitize::{
    allowed_url, clean, clean_text, default_protocols, default_tag_attributes, default_tags,
    escape_attr, escape_html, is_html, linkify, parallel_clean, strip_tags, CleanOpts,
    LinkifyOpts, RelativeUrlMode, Sanitizer, MAX_INPUT_BYTES,
};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

const E3538_NSANITIZE_ARITY: u32 = codes::E3538_NSANITIZE_ARITY;
const E3539_NSANITIZE_ERROR: u32 = codes::E3539_NSANITIZE_ERROR;
const E3540_NSANITIZE_TYPE: u32 = codes::E3540_NSANITIZE_TYPE;
const E3541_NSANITIZE_INVALID_HANDLE: u32 = codes::E3541_NSANITIZE_INVALID_HANDLE;

thread_local! {
    static SANITIZERS: RefCell<HashMap<i64, Sanitizer>> = RefCell::new(HashMap::new());
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
    RuntimeError::at(span, E3540_NSANITIZE_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3538_NSANITIZE_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3538_NSANITIZE_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn nsanitize_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3539_NSANITIZE_ERROR, "nsanitize_error", msg.into(), span)
}

fn invalid_handle(span: Span, id: i64) -> ValueRef {
    error_value(
        E3541_NSANITIZE_INVALID_HANDLE,
        "nsanitize_error",
        format!("invalid or closed nsanitize handle {id}"),
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

fn string_list_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<String>> {
    match &*args[idx].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::String(s) => out.push(s.clone()),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() expects string array; item {} is {}",
                                i + 1,
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(out)
        }
        Value::Nil => Ok(Vec::new()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn parse_opts(args: &[ValueRef], idx: usize, span: Span) -> NiaoResult<HashMap<String, ValueRef>> {
    if args.len() <= idx {
        return Ok(HashMap::new());
    }
    match &*args[idx].borrow() {
        Value::Object(map) => Ok(map.clone()),
        Value::Nil => Ok(HashMap::new()),
        other => Err(type_err(
            span,
            format!("opts must be an object, got {}", other.type_name()),
        )),
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

fn obj_string_opt(map: &HashMap<String, ValueRef>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.clone()),
        Value::Nil => None,
        _ => None,
    })
}

fn obj_int(map: &HashMap<String, ValueRef>, key: &str, default: i64) -> i64 {
    map.get(key)
        .and_then(|v| match &*v.borrow() {
            Value::Int(n) => Some(*n),
            _ => None,
        })
        .unwrap_or(default)
}

fn string_set_from_value(v: &ValueRef, span: Span, field: &str) -> NiaoResult<HashSet<String>> {
    match &*v.borrow() {
        Value::Array(items) => {
            let mut out = HashSet::new();
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::String(s) => {
                        out.insert(s.clone());
                    }
                    other => {
                        return Err(type_err(
                            span,
                            format!("{field} item {} must be string, got {}", i + 1, other.type_name()),
                        ));
                    }
                }
            }
            Ok(out)
        }
        Value::Nil => Ok(HashSet::new()),
        other => Err(type_err(
            span,
            format!("{field} must be a string array, got {}", other.type_name()),
        )),
    }
}

fn tag_attrs_from_value(v: &ValueRef, span: Span) -> NiaoResult<HashMap<String, HashSet<String>>> {
    match &*v.borrow() {
        Value::Object(map) => {
            let mut out = HashMap::new();
            for (tag, attrs_v) in map {
                out.insert(tag.clone(), string_set_from_value(attrs_v, span, "attributes")?);
            }
            Ok(out)
        }
        Value::Nil => Ok(HashMap::new()),
        other => Err(type_err(
            span,
            format!("attributes must be an object, got {}", other.type_name()),
        )),
    }
}

fn allowed_classes_from_value(
    v: &ValueRef,
    span: Span,
) -> NiaoResult<HashMap<String, HashSet<String>>> {
    tag_attrs_from_value(v, span)
}

fn clean_opts_from_map(map: &HashMap<String, ValueRef>, span: Span) -> NiaoResult<CleanOpts> {
    let mut opts = CleanOpts::default();
    if let Some(v) = map.get("tags") {
        opts.tags = Some(string_set_from_value(v, span, "tags")?);
    }
    if let Some(v) = map.get("attributes") {
        opts.tag_attributes = Some(tag_attrs_from_value(v, span)?);
    }
    if let Some(v) = map.get("generic_attributes") {
        opts.generic_attributes = string_set_from_value(v, span, "generic_attributes")?;
    }
    if let Some(v) = map.get("protocols") {
        opts.url_schemes = Some(string_set_from_value(v, span, "protocols")?);
    }
    if map.contains_key("strip_comments") {
        opts.strip_comments = obj_bool(map, "strip_comments", true);
    }
    if let Some(rel) = obj_string_opt(map, "link_rel") {
        opts.link_rel = Some(rel);
    }
    if map.contains_key("nofollow_links") && obj_bool(map, "nofollow_links", false) {
        if let Some(ref mut rel) = opts.link_rel {
            if !rel.contains("nofollow") {
                rel.push_str(" nofollow");
            }
        } else {
            opts.link_rel = Some("nofollow".into());
        }
    }
    if let Some(mode) = obj_string_opt(map, "relative_urls") {
        opts.relative_urls = RelativeUrlMode::parse(&mode).ok_or_else(|| {
            type_err(span, format!("unknown relative_urls mode '{mode}'"))
        })?;
    }
    if let Some(v) = map.get("allowed_classes") {
        opts.allowed_classes = allowed_classes_from_value(v, span)?;
    }
    if let Some(v) = map.get("clean_content_tags") {
        opts.clean_content_tags = string_set_from_value(v, span, "clean_content_tags")?;
    }
    Ok(opts)
}

fn linkify_opts_from_map(map: &HashMap<String, ValueRef>, span: Span) -> NiaoResult<LinkifyOpts> {
    let mut opts = LinkifyOpts::default();
    if map.contains_key("parse_email") {
        opts.parse_email = obj_bool(map, "parse_email", true);
    }
    if map.contains_key("new_tab") {
        opts.new_tab = obj_bool(map, "new_tab", true);
    }
    if map.contains_key("nofollow") {
        opts.nofollow = obj_bool(map, "nofollow", false);
    }
    if let Some(v) = map.get("skip_tags") {
        opts.skip_tags = string_set_from_value(v, span, "skip_tags")?;
    }
    if map.contains_key("sanitize_after") {
        opts.sanitize_after = obj_bool(map, "sanitize_after", true);
    }
    opts.clean_opts = clean_opts_from_map(map, span)?;
    Ok(opts)
}

fn check_len(s: &str, span: Span) -> Result<(), ValueRef> {
    if s.len() > MAX_INPUT_BYTES {
        return Err(nsanitize_err(
            span,
            format!("input size {} exceeds limit {MAX_INPUT_BYTES}", s.len()),
        ));
    }
    Ok(())
}

fn str_val(s: impl Into<String>) -> NiaoResult<ValueRef> {
    Ok(Value::String(s.into()).ref_cell())
}

fn bool_val(b: bool) -> NiaoResult<ValueRef> {
    Ok(Value::Bool(b).ref_cell())
}

fn string_array(items: Vec<String>) -> NiaoResult<ValueRef> {
    let out = items
        .into_iter()
        .map(|s| Value::String(s).ref_cell())
        .collect();
    Ok(Value::Array(out).ref_cell())
}

fn with_sanitizer<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&Sanitizer) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    SANITIZERS.with(|stores| {
        let stores = stores.borrow();
        match stores.get(&id) {
            Some(s) => Ok(Ok(f(s))),
            None => Ok(Err(invalid_handle(span, id))),
        }
    })
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

// >>> nsanitize.clean("<b>x</b><script>y</script>")
// => "<b>x</b>"
fn nsanitize_clean(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nsanitize_clean", span)?;
    let html = string_arg(args, 0, "nsanitize_clean", span)?;
    if let Err(e) = check_len(&html, span) {
        return Ok(e);
    }
    let opts_map = parse_opts(args, 1, span)?;
    let opts = clean_opts_from_map(&opts_map, span)?;
    match clean(&html, &opts) {
        Ok(out) => str_val(out),
        Err(e) => Ok(nsanitize_err(span, e.message())),
    }
}

// >>> nsanitize.strip("<p>hi</p>")
// => "hi"
fn nsanitize_strip(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nsanitize_strip", span)?;
    let html = string_arg(args, 0, "nsanitize_strip", span)?;
    if let Err(e) = check_len(&html, span) {
        return Ok(e);
    }
    let opts = parse_opts(args, 1, span)?;
    let strip_comments = obj_bool(&opts, "strip_comments", true);
    match strip_tags(&html, strip_comments) {
        Ok(out) => str_val(out),
        Err(e) => Ok(nsanitize_err(span, e.message())),
    }
}

// >>> nsanitize.linkify("see https://example.com")
// => "see <a href=...>..."
fn nsanitize_linkify(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nsanitize_linkify", span)?;
    let text = string_arg(args, 0, "nsanitize_linkify", span)?;
    if let Err(e) = check_len(&text, span) {
        return Ok(e);
    }
    let opts_map = parse_opts(args, 1, span)?;
    let opts = linkify_opts_from_map(&opts_map, span)?;
    match linkify(&text, &opts) {
        Ok(out) => str_val(out),
        Err(e) => Ok(nsanitize_err(span, e.message())),
    }
}

// >>> nsanitize.escape("<b>")
// => "&lt;b&gt;"
fn nsanitize_escape(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nsanitize_escape", span)?;
    let text = string_arg(args, 0, "nsanitize_escape", span)?;
    if let Err(e) = check_len(&text, span) {
        return Ok(e);
    }
    let opts = parse_opts(args, 1, span)?;
    let attr = obj_bool(&opts, "attribute", false);
    let out = if attr {
        escape_attr(&text)
    } else {
        escape_html(&text)
    };
    str_val(out)
}

// >>> nsanitize.allowed_url("https://x.com")
// => true
fn nsanitize_allowed_url(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nsanitize_allowed_url", span)?;
    let url = string_arg(args, 0, "nsanitize_allowed_url", span)?;
    let opts = parse_opts(args, 1, span)?;
    let protocols = if let Some(v) = opts.get("protocols") {
        string_set_from_value(v, span, "protocols")?
    } else {
        default_protocols()
    };
    bool_val(allowed_url(&url, &protocols))
}

// >>> nsanitize.is_html("<b>x</b>")
// => true
fn nsanitize_is_html(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsanitize_is_html", span)?;
    let text = string_arg(args, 0, "nsanitize_is_html", span)?;
    bool_val(is_html(&text))
}

// >>> nsanitize.clean_text("a & b")
// => "a &amp; b"
fn nsanitize_clean_text(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsanitize_clean_text", span)?;
    let text = string_arg(args, 0, "nsanitize_clean_text", span)?;
    if let Err(e) = check_len(&text, span) {
        return Ok(e);
    }
    str_val(clean_text(&text))
}

// >>> let h = nsanitize.compile({tags: ["b", "i"]})
fn nsanitize_compile(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nsanitize_compile", span)?;
    let opts_map = parse_opts(args, 0, span)?;
    let opts = clean_opts_from_map(&opts_map, span)?;
    match Sanitizer::new(opts) {
        Ok(s) => {
            let id = new_handle();
            SANITIZERS.with(|stores| stores.borrow_mut().insert(id, s));
            Ok(Value::Int(id).ref_cell())
        }
        Err(e) => Ok(nsanitize_err(span, e.message())),
    }
}

// >>> nsanitize.close(h)
fn nsanitize_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsanitize_close", span)?;
    let id = match &*args[0].borrow() {
        Value::Int(n) => *n,
        other => {
            return Err(type_err(
                span,
                format!("nsanitize_close() expects int handle, got {}", other.type_name()),
            ));
        }
    };
    SANITIZERS.with(|stores| {
        stores.borrow_mut().remove(&id);
    });
    Ok(Value::Nil.ref_cell())
}

// >>> nsanitize.apply(h, "<b>x</b>")
fn nsanitize_apply(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsanitize_apply", span)?;
    let id = match &*args[0].borrow() {
        Value::Int(n) => *n,
        other => {
            return Err(type_err(
                span,
                format!("nsanitize_apply() expects int handle, got {}", other.type_name()),
            ));
        }
    };
    let html = string_arg(args, 1, "nsanitize_apply", span)?;
    if let Err(e) = check_len(&html, span) {
        return Ok(e);
    }
    match with_sanitizer(id, span, |s| s.clean(&html))? {
        Ok(out) => str_val(out),
        Err(e) => Ok(e),
    }
}

// >>> len(nsanitize.parallel_clean(["<b>a</b>", "<i>b</i>"]))
// => 2
fn nsanitize_parallel_clean(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nsanitize_parallel_clean", span)?;
    let items = string_list_arg(args, 0, "nsanitize_parallel_clean", span)?;
    for (i, s) in items.iter().enumerate() {
        if s.len() > MAX_INPUT_BYTES {
            return Ok(nsanitize_err(
                span,
                format!("item {} size {} exceeds limit {MAX_INPUT_BYTES}", i + 1, s.len()),
            ));
        }
    }
    let opts_map = parse_opts(args, 1, span)?;
    let opts = clean_opts_from_map(&opts_map, span)?;
    let threads = obj_int(&opts_map, "threads", available_threads() as i64) as usize;
    match parallel_clean(&items, &opts, threads) {
        Ok(out) => string_array(out),
        Err(e) => Ok(nsanitize_err(span, e.message())),
    }
}

// >>> len(nsanitize.default_tags()) > 0
fn nsanitize_default_tags(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nsanitize_default_tags", span)?;
    let mut tags: Vec<String> = default_tags().into_iter().collect();
    tags.sort();
    string_array(tags)
}

// >>> nsanitize.default_protocols()[0]
fn nsanitize_default_protocols(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nsanitize_default_protocols", span)?;
    let mut protos: Vec<String> = default_protocols().into_iter().collect();
    protos.sort();
    string_array(protos)
}

// >>> keys(nsanitize.default_attributes())
fn nsanitize_default_attributes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nsanitize_default_attributes", span)?;
    let attrs = default_tag_attributes();
    let mut map = HashMap::new();
    for (tag, set) in attrs {
        let mut list: Vec<String> = set.into_iter().collect();
        list.sort();
        let arr: Vec<ValueRef> = list
            .into_iter()
            .map(|s| Value::String(s).ref_cell())
            .collect();
        map.insert(tag, Value::Array(arr).ref_cell());
    }
    Ok(Value::Object(map).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nsanitize_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nsanitize_fns![
    ("nsanitize_clean", "clean", nsanitize_clean),
    ("nsanitize_strip", "strip", nsanitize_strip),
    ("nsanitize_linkify", "linkify", nsanitize_linkify),
    ("nsanitize_escape", "escape", nsanitize_escape),
    ("nsanitize_allowed_url", "allowed_url", nsanitize_allowed_url),
    ("nsanitize_is_html", "is_html", nsanitize_is_html),
    ("nsanitize_clean_text", "clean_text", nsanitize_clean_text),
    ("nsanitize_compile", "compile", nsanitize_compile),
    ("nsanitize_close", "close", nsanitize_close),
    ("nsanitize_apply", "apply", nsanitize_apply),
    ("nsanitize_parallel_clean", "parallel_clean", nsanitize_parallel_clean),
    ("nsanitize_default_tags", "default_tags", nsanitize_default_tags),
    ("nsanitize_default_protocols", "default_protocols", nsanitize_default_protocols),
    ("nsanitize_default_attributes", "default_attributes", nsanitize_default_attributes),
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

pub const MODULE_NAME: &str = "nsanitize";
pub const MODULE_PATHS: &[&str] = &["nsanitize", "std/nsanitize"];

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
    fn clean_doctest() {
        let v = nsanitize_clean(
            &[Value::String("<b>x</b><script>y</script>".into()).ref_cell()],
            span(),
        )
        .unwrap();
        match &*v.borrow() {
            Value::String(s) => {
                assert!(s.contains("<b>x</b>"));
                assert!(!s.contains("<script"));
            }
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn allowed_url_blocks_js() {
        let v = nsanitize_allowed_url(
            &[Value::String("javascript:alert(1)".into()).ref_cell()],
            span(),
        )
        .unwrap();
        assert_eq!(*v.borrow(), Value::Bool(false));
    }
}
