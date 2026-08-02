//! Native nglob standard library — glob patterns, `**` recursion,
//! gitignore-style matching, walk with filters (~glob, fnmatch, pathspec subset).
//!
//! Import with `import "nglob"` (or `import "std/nglob"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_glob::{
    compile, escape, filter_strs, glob_paths, has_magic, match_any, match_basename, match_str,
    parallel_classify, parallel_filter, paths_matching_globs, translate, walk, walk_paths,
    CompileOpts, GlobOpts, MatchKind, WalkOpts,
};
use niao_parallel::available_threads;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

const E3510_NGLOB_ARITY: u32 = codes::E3526_NGLOB_ARITY;
const E3511_NGLOB_ERROR: u32 = codes::E3527_NGLOB_ERROR;
const E3512_NGLOB_TYPE: u32 = codes::E3528_NGLOB_TYPE;
const E3513_NGLOB_INVALID_HANDLE: u32 = codes::E3529_NGLOB_INVALID_HANDLE;

thread_local! {
    static MATCHERS: RefCell<HashMap<i64, niao_glob::CompiledMatcher>> = RefCell::new(HashMap::new());
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
    RuntimeError::at(span, E3512_NGLOB_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3510_NGLOB_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3510_NGLOB_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn nglob_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3511_NGLOB_ERROR, "nglob_error", msg.into(), span)
}

fn invalid_handle(span: Span, id: i64) -> ValueRef {
    error_value(
        E3513_NGLOB_INVALID_HANDLE,
        "nglob_error",
        format!("invalid or closed nglob handle {id}"),
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

fn handle_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    int_arg(args, idx, name, span)
}

fn bool_val(b: bool) -> NiaoResult<ValueRef> {
    Ok(Value::Bool(b).ref_cell())
}

fn int_val(n: i64) -> NiaoResult<ValueRef> {
    Ok(Value::Int(n).ref_cell())
}

fn str_val(s: impl Into<String>) -> NiaoResult<ValueRef> {
    Ok(Value::String(s.into()).ref_cell())
}

fn string_array(items: Vec<String>) -> NiaoResult<ValueRef> {
    let out = items
        .into_iter()
        .map(|s| Value::String(s).ref_cell())
        .collect();
    Ok(Value::Array(out).ref_cell())
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

fn obj_bool(map: &HashMap<String, ValueRef>, key: &str, default: bool) -> bool {
    map.get(key)
        .map(|v| matches!(&*v.borrow(), Value::Bool(b) if *b))
        .unwrap_or(default)
}

fn obj_bool_inv(map: &HashMap<String, ValueRef>, key: &str, default: bool) -> bool {
    map.get(key)
        .and_then(|v| match &*v.borrow() {
            Value::Bool(b) => Some(*b),
            _ => None,
        })
        .unwrap_or(default)
}

fn obj_string(map: &HashMap<String, ValueRef>, key: &str, default: &str) -> String {
    map.get(key)
        .and_then(|v| match &*v.borrow() {
            Value::String(s) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_else(|| default.to_string())
}

fn obj_int_opt(map: &HashMap<String, ValueRef>, key: &str) -> Option<usize> {
    map.get(key).and_then(|v| match &*v.borrow() {
        Value::Int(n) if *n >= 0 => Some(*n as usize),
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

fn case_sensitive_from_opts(map: &HashMap<String, ValueRef>) -> bool {
    if obj_bool(map, "case_insensitive", false) {
        return false;
    }
    obj_bool_inv(map, "case_sensitive", true)
}

fn glob_err(span: Span, e: niao_glob::GlobError) -> ValueRef {
    nglob_err(span, e.to_string())
}

fn with_matcher<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&niao_glob::CompiledMatcher) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    MATCHERS.with(|stores| {
        let stores = stores.borrow();
        match stores.get(&id) {
            Some(m) => Ok(Ok(f(m))),
            None => Ok(Err(invalid_handle(span, id))),
        }
    })
}

// ---------------------------------------------------------------------------
// fnmatch-style
// ---------------------------------------------------------------------------

// >>> import "nglob"
// >>> nglob.match("foo.py", "*.py")
// => true
fn nglob_match(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nglob_match", span)?;
    let path = string_arg(args, 0, "nglob_match", span)?;
    let pat = string_arg(args, 1, "nglob_match", span)?;
    let opts = parse_opts(args, 2, span)?;
    let case_sensitive = case_sensitive_from_opts(&opts);
    let basename_only = obj_bool(&opts, "basename_only", false);
    let ok = if basename_only {
        match_basename(&path, &pat, case_sensitive)
    } else {
        match_str(&path, &pat, case_sensitive)
    };
    match ok {
        Ok(v) => bool_val(v),
        Err(e) => Ok(glob_err(span, e)),
    }
}

// >>> nglob.match_case("Foo.py", "*.py")
// => false
fn nglob_match_case(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nglob_match_case", span)?;
    let path = string_arg(args, 0, "nglob_match_case", span)?;
    let pat = string_arg(args, 1, "nglob_match_case", span)?;
    match match_str(&path, &pat, true) {
        Ok(v) => bool_val(v),
        Err(e) => Ok(glob_err(span, e)),
    }
}

// >>> nglob.filter(["a.py", "b.txt"], "*.py")
// => ["a.py"]
fn nglob_filter(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nglob_filter", span)?;
    let paths = string_list_arg(args, 0, "nglob_filter", span)?;
    let pat = string_arg(args, 1, "nglob_filter", span)?;
    let opts = parse_opts(args, 2, span)?;
    let case_sensitive = case_sensitive_from_opts(&opts);
    match filter_strs(&paths, &pat, case_sensitive) {
        Ok(hits) => string_array(hits.into_iter().map(|s| s.to_string()).collect()),
        Err(e) => Ok(glob_err(span, e)),
    }
}

// >>> nglob.has_magic("*.py")
// => true
fn nglob_has_magic(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nglob_has_magic", span)?;
    let pat = string_arg(args, 0, "nglob_has_magic", span)?;
    bool_val(has_magic(&pat))
}

// >>> nglob.escape("a*b")
// => "a\\*b"
fn nglob_escape(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nglob_escape", span)?;
    let s = string_arg(args, 0, "nglob_escape", span)?;
    str_val(escape(&s))
}

// >>> nglob.translate("*.py")
// => "(?s:...)\\z"
fn nglob_translate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nglob_translate", span)?;
    let pat = string_arg(args, 0, "nglob_translate", span)?;
    let opts = parse_opts(args, 1, span)?;
    let case_sensitive = case_sensitive_from_opts(&opts);
    match translate(&pat, case_sensitive) {
        Ok(re) => str_val(re),
        Err(e) => Ok(glob_err(span, e)),
    }
}

// ---------------------------------------------------------------------------
// Filesystem glob
// ---------------------------------------------------------------------------

// >>> len(nglob.glob("*.niao")) >= 0
// => true
fn nglob_glob(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nglob_glob", span)?;
    let pat = string_arg(args, 0, "nglob_glob", span)?;
    let opts_map = parse_opts(args, 1, span)?;
    let mut opts = GlobOpts::default();
    opts.root = PathBuf::from(obj_string(&opts_map, "root", "."));
    opts.recursive = obj_bool(&opts_map, "recursive", false);
    opts.hidden = obj_bool(&opts_map, "hidden", false);
    opts.follow_links = obj_bool(&opts_map, "follow_links", false);
    opts.case_sensitive = case_sensitive_from_opts(&opts_map);
    match glob_paths(&pat, &opts) {
        Ok(paths) => string_array(paths),
        Err(e) => Ok(glob_err(span, e)),
    }
}

// ---------------------------------------------------------------------------
// Compiled matcher handles
// ---------------------------------------------------------------------------

// >>> let m = nglob.compile(["*.py", "*.rs"])
// >>> nglob.matches(m, "lib.rs")
// => true
fn nglob_compile(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nglob_compile", span)?;
    let patterns = string_list_arg(args, 0, "nglob_compile", span)?;
    let opts_map = parse_opts(args, 1, span)?;
    let compile_opts = CompileOpts {
        gitignore: obj_bool(&opts_map, "gitignore", false),
        case_sensitive: case_sensitive_from_opts(&opts_map),
        root: PathBuf::from(obj_string(&opts_map, "root", ".")),
    };
    match compile(&patterns, &compile_opts) {
        Ok(m) => {
            let id = new_handle();
            MATCHERS.with(|stores| {
                stores.borrow_mut().insert(id, m);
            });
            int_val(id)
        }
        Err(e) => Ok(glob_err(span, e)),
    }
}

// >>> nglob.close(m)
// => true
fn nglob_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nglob_close", span)?;
    let id = handle_arg(args, 0, "nglob_close", span)?;
    let removed = MATCHERS.with(|stores| stores.borrow_mut().remove(&id).is_some());
    bool_val(removed)
}

fn nglob_matches(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nglob_matches", span)?;
    let id = handle_arg(args, 0, "nglob_matches", span)?;
    let path = string_arg(args, 1, "nglob_matches", span)?;
    let is_dir = if args.len() == 3 {
        matches!(&*args[2].borrow(), Value::Bool(true))
    } else {
        false
    };
    match with_matcher(id, span, |m| m.matches_with(&path, is_dir))? {
        Ok(v) => bool_val(v),
        Err(e) => Ok(e),
    }
}

// >>> nglob.ignored(m, "vendor/x.py")
fn nglob_ignored(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nglob_ignored", span)?;
    let id = handle_arg(args, 0, "nglob_ignored", span)?;
    let path = string_arg(args, 1, "nglob_ignored", span)?;
    let is_dir = if args.len() == 3 {
        matches!(&*args[2].borrow(), Value::Bool(true))
    } else {
        false
    };
    match with_matcher(id, span, |m| m.ignored(&path, is_dir))? {
        Ok(v) => bool_val(v),
        Err(e) => Ok(e),
    }
}

// >>> nglob.classify(m, "src/main.py")
// => "none"
fn nglob_classify(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nglob_classify", span)?;
    let id = handle_arg(args, 0, "nglob_classify", span)?;
    let path = string_arg(args, 1, "nglob_classify", span)?;
    let is_dir = if args.len() == 3 {
        matches!(&*args[2].borrow(), Value::Bool(true))
    } else {
        false
    };
    match with_matcher(id, span, |m| {
        let kind = m.classify(&path, is_dir);
        match kind {
            MatchKind::Whitelist => "whitelist",
            MatchKind::Ignore => "ignore",
            MatchKind::None => "none",
        }
    })? {
        Ok(s) => str_val(s),
        Err(e) => Ok(e),
    }
}

fn nglob_filter_paths(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nglob_filter_paths", span)?;
    let id = handle_arg(args, 0, "nglob_filter_paths", span)?;
    let paths = string_list_arg(args, 1, "nglob_filter_paths", span)?;
    match with_matcher(id, span, |m| m.filter_owned(&paths))? {
        Ok(hits) => string_array(hits),
        Err(e) => Ok(e),
    }
}

fn nglob_match_any(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nglob_match_any", span)?;
    let path = string_arg(args, 0, "nglob_match_any", span)?;
    let patterns = string_list_arg(args, 1, "nglob_match_any", span)?;
    let opts = parse_opts(args, 2, span)?;
    let case_sensitive = case_sensitive_from_opts(&opts);
    match match_any(&path, &patterns, case_sensitive) {
        Ok(v) => bool_val(v),
        Err(e) => Ok(glob_err(span, e)),
    }
}

fn nglob_pattern_count(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nglob_pattern_count", span)?;
    let id = handle_arg(args, 0, "nglob_pattern_count", span)?;
    match with_matcher(id, span, |m| m.pattern_count() as i64)? {
        Ok(n) => int_val(n),
        Err(e) => Ok(e),
    }
}

fn nglob_is_gitignore(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nglob_is_gitignore", span)?;
    let id = handle_arg(args, 0, "nglob_is_gitignore", span)?;
    match with_matcher(id, span, |m| m.is_gitignore())? {
        Ok(v) => bool_val(v),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Walk & parallel
// ---------------------------------------------------------------------------

fn walk_entry_array(entries: Vec<niao_glob::WalkEntry>) -> NiaoResult<ValueRef> {
    let items: Vec<ValueRef> = entries
        .into_iter()
        .map(|e| {
            let mut obj = HashMap::new();
            obj.insert("path".to_string(), Value::String(e.path).ref_cell());
            obj.insert("is_dir".to_string(), Value::Bool(e.is_dir).ref_cell());
            obj.insert("depth".to_string(), Value::Int(e.depth as i64).ref_cell());
            Value::Object(obj).ref_cell()
        })
        .collect();
    Ok(Value::Array(items).ref_cell())
}

// >>> len(nglob.walk(".")) >= 0
// => true
fn nglob_walk(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nglob_walk", span)?;
    let root = string_arg(args, 0, "nglob_walk", span)?;
    let opts_map = parse_opts(args, 1, span)?;
    let include = opts_map
        .get("include")
        .map(|v| string_list_from_value(v, span, "include"))
        .transpose()?
        .unwrap_or_default();
    let exclude = opts_map
        .get("exclude")
        .map(|v| string_list_from_value(v, span, "exclude"))
        .transpose()?
        .unwrap_or_default();
    let opts = WalkOpts {
        root: PathBuf::from(root),
        include,
        exclude,
        gitignore: obj_bool(&opts_map, "gitignore", true),
        hidden: obj_bool(&opts_map, "hidden", false),
        max_depth: obj_int_opt(&opts_map, "max_depth"),
        follow_links: obj_bool(&opts_map, "follow_links", false),
        files_only: obj_bool_inv(&opts_map, "files_only", true),
        case_sensitive: case_sensitive_from_opts(&opts_map),
        threads: obj_int(&opts_map, "threads", available_threads() as i64) as usize,
    };
    match walk(&opts) {
        Ok(entries) => walk_entry_array(entries),
        Err(e) => Ok(glob_err(span, e)),
    }
}

fn string_list_from_value(v: &ValueRef, span: Span, field: &str) -> NiaoResult<Vec<String>> {
    match &*v.borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::String(s) => out.push(s.clone()),
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
        other => Err(type_err(
            span,
            format!("{field} must be a string array, got {}", other.type_name()),
        )),
    }
}

fn nglob_walk_paths(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nglob_walk_paths", span)?;
    let root = string_arg(args, 0, "nglob_walk_paths", span)?;
    let opts_map = parse_opts(args, 1, span)?;
    let include = opts_map
        .get("include")
        .map(|v| string_list_from_value(v, span, "include"))
        .transpose()?
        .unwrap_or_default();
    let exclude = opts_map
        .get("exclude")
        .map(|v| string_list_from_value(v, span, "exclude"))
        .transpose()?
        .unwrap_or_default();
    let opts = WalkOpts {
        root: PathBuf::from(root),
        include,
        exclude,
        gitignore: obj_bool(&opts_map, "gitignore", true),
        hidden: obj_bool(&opts_map, "hidden", false),
        max_depth: obj_int_opt(&opts_map, "max_depth"),
        follow_links: obj_bool(&opts_map, "follow_links", false),
        files_only: obj_bool_inv(&opts_map, "files_only", true),
        case_sensitive: case_sensitive_from_opts(&opts_map),
        threads: obj_int(&opts_map, "threads", available_threads() as i64) as usize,
    };
    match walk_paths(&opts) {
        Ok(paths) => string_array(paths),
        Err(e) => Ok(glob_err(span, e)),
    }
}

fn nglob_parallel_filter(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nglob_parallel_filter", span)?;
    let paths = string_list_arg(args, 0, "nglob_parallel_filter", span)?;
    let pat = string_arg(args, 1, "nglob_parallel_filter", span)?;
    let opts = parse_opts(args, 2, span)?;
    let case_sensitive = case_sensitive_from_opts(&opts);
    let threads = obj_int(&opts, "threads", available_threads() as i64) as usize;
    match parallel_filter(&paths, &pat, case_sensitive, threads) {
        Ok(hits) => string_array(hits),
        Err(e) => Ok(glob_err(span, e)),
    }
}

fn nglob_paths_matching(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nglob_paths_matching", span)?;
    let paths = string_list_arg(args, 0, "nglob_paths_matching", span)?;
    let patterns = string_list_arg(args, 1, "nglob_paths_matching", span)?;
    let opts = parse_opts(args, 2, span)?;
    let case_sensitive = case_sensitive_from_opts(&opts);
    match paths_matching_globs(&paths, &patterns, case_sensitive) {
        Ok(hits) => string_array(hits),
        Err(e) => Ok(glob_err(span, e)),
    }
}

fn nglob_parallel_classify(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nglob_parallel_classify", span)?;
    let paths = string_list_arg(args, 0, "nglob_parallel_classify", span)?;
    let patterns = string_list_arg(args, 1, "nglob_parallel_classify", span)?;
    let opts = parse_opts(args, 2, span)?;
    let gitignore = obj_bool(&opts, "gitignore", false);
    let case_sensitive = case_sensitive_from_opts(&opts);
    let threads = obj_int(&opts, "threads", available_threads() as i64) as usize;
    match parallel_classify(&paths, &patterns, gitignore, case_sensitive, threads) {
        Ok(hits) => string_array(hits),
        Err(e) => Ok(glob_err(span, e)),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nglob_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nglob_fns![
    ("nglob_match", "match", nglob_match),
    ("nglob_match_case", "match_case", nglob_match_case),
    ("nglob_filter", "filter", nglob_filter),
    ("nglob_has_magic", "has_magic", nglob_has_magic),
    ("nglob_escape", "escape", nglob_escape),
    ("nglob_translate", "translate", nglob_translate),
    ("nglob_glob", "glob", nglob_glob),
    ("nglob_compile", "compile", nglob_compile),
    ("nglob_close", "close", nglob_close),
    ("nglob_matches", "matches", nglob_matches),
    ("nglob_ignored", "ignored", nglob_ignored),
    ("nglob_classify", "classify", nglob_classify),
    ("nglob_filter_paths", "filter_paths", nglob_filter_paths),
    ("nglob_match_any", "match_any", nglob_match_any),
    ("nglob_pattern_count", "pattern_count", nglob_pattern_count),
    ("nglob_is_gitignore", "is_gitignore", nglob_is_gitignore),
    ("nglob_walk", "walk", nglob_walk),
    ("nglob_walk_paths", "walk_paths", nglob_walk_paths),
    ("nglob_parallel_filter", "parallel_filter", nglob_parallel_filter),
    ("nglob_paths_matching", "paths_matching", nglob_paths_matching),
    ("nglob_parallel_classify", "parallel_classify", nglob_parallel_classify),
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

pub const MODULE_NAME: &str = "nglob";
pub const MODULE_PATHS: &[&str] = &["nglob", "std/nglob"];

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
    fn match_doctest() {
        let v = nglob_match(
            &[
                Value::String("foo.py".into()).ref_cell(),
                Value::String("*.py".into()).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        assert_eq!(*v.borrow(), Value::Bool(true));
    }

    #[test]
    fn compile_and_matches() {
        let h = nglob_compile(
            &[Value::Array(vec![Value::String("*.rs".into()).ref_cell()]).ref_cell()],
            span(),
        )
        .unwrap();
        let id = match &*h.borrow() {
            Value::Int(n) => *n,
            other => panic!("expected int handle, got {other:?}"),
        };
        let ok = nglob_matches(
            &[Value::Int(id).ref_cell(), Value::String("lib.rs".into()).ref_cell()],
            span(),
        )
        .unwrap();
        assert_eq!(*ok.borrow(), Value::Bool(true));
    }
}
