//! Native nhotreload standard library — file watch + per-function body diff via
//! `niao_parser`. Live VM swap is a roadmap item (see docs/NHOTRELOAD.md).
//!
//! Import with `import "nhotreload"` (or `import "std/nhotreload"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::{ClassMember, FnDef, Program, Span, TopLevel};
use niao_parser::parse;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::rc::Rc;
use std::time::SystemTime;

const E3200_NHOTRELOAD_ARITY: u32 = 3200;
const E3201_NHOTRELOAD_ERROR: u32 = 3201;
const E3202_NHOTRELOAD_TYPE: u32 = 3202;
const E3203_NHOTRELOAD_INVALID_HANDLE: u32 = 3203;

// ---------------------------------------------------------------------------
// Function extraction
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
struct FnEntry {
    name: String,
    body: String,
    line: usize,
}

fn slice_span(source: &str, span: Span) -> String {
    source
        .get(span.start..span.end)
        .unwrap_or("")
        .trim()
        .to_string()
}

fn push_fn(out: &mut Vec<FnEntry>, name: String, def: &FnDef, source: &str) {
    out.push(FnEntry {
        name,
        body: slice_span(source, def.span),
        line: def.span.line,
    });
}

fn collect_functions(source: &str, program: &Program) -> Vec<FnEntry> {
    let mut out = Vec::new();
    for item in &program.items {
        match item {
            TopLevel::Fn(def) => push_fn(&mut out, def.name.clone(), def, source),
            TopLevel::Class(class) => {
                for member in &class.members {
                    match member {
                        ClassMember::Method { def, .. } | ClassMember::StaticMethod { def, .. } => {
                            push_fn(
                                &mut out,
                                format!("{}.{}", class.name, def.name),
                                def,
                                source,
                            );
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn entries_to_map(entries: &[FnEntry]) -> HashMap<String, String> {
    entries
        .iter()
        .map(|e| (e.name.clone(), e.body.clone()))
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FnChange {
    name: String,
    old_body: String,
    new_body: String,
}

fn diff_maps(old: &HashMap<String, String>, new: &HashMap<String, String>) -> Vec<FnChange> {
    let mut names: Vec<&String> = old.keys().chain(new.keys()).collect();
    names.sort();
    names.dedup();
    let mut out = Vec::new();
    for name in names {
        match (old.get(name), new.get(name)) {
            (Some(a), Some(b)) if a == b => {}
            (Some(a), Some(b)) => out.push(FnChange {
                name: name.clone(),
                old_body: a.clone(),
                new_body: b.clone(),
            }),
            (Some(a), None) => out.push(FnChange {
                name: name.clone(),
                old_body: a.clone(),
                new_body: String::new(),
            }),
            (None, Some(b)) => out.push(FnChange {
                name: name.clone(),
                old_body: String::new(),
                new_body: b.clone(),
            }),
            (None, None) => {}
        }
    }
    out
}

fn parse_source(source: &str, label: &str, span: Span) -> Result<Vec<FnEntry>, ValueRef> {
    match parse(source) {
        Ok(program) => Ok(collect_functions(source, &program)),
        Err(e) => Err(error_value(
            E3201_NHOTRELOAD_ERROR,
            "nhotreload_error",
            format!("{label}: {e}"),
            span,
        )),
    }
}

fn parse_file(path: &str, span: Span) -> Result<(String, Vec<FnEntry>), ValueRef> {
    let text = fs::read_to_string(path).map_err(|e| {
        error_value(
            E3201_NHOTRELOAD_ERROR,
            "nhotreload_error",
            format!("failed to read '{path}': {e}"),
            span,
        )
    })?;
    let entries = parse_source(&text, "parse", span)?;
    Ok((text, entries))
}

fn fn_entries_value(entries: &[FnEntry]) -> Value {
    let items: Vec<ValueRef> = entries
        .iter()
        .map(|e| {
            let mut map = HashMap::new();
            map.insert("name".to_string(), Value::String(e.name.clone()).ref_cell());
            map.insert("body".to_string(), Value::String(e.body.clone()).ref_cell());
            map.insert("line".to_string(), Value::Int(e.line as i64).ref_cell());
            Value::Object(map).ref_cell()
        })
        .collect();
    Value::Array(items)
}

fn changes_value(changes: &[FnChange]) -> Value {
    let items: Vec<ValueRef> = changes
        .iter()
        .map(|c| {
            let mut map = HashMap::new();
            map.insert("name".to_string(), Value::String(c.name.clone()).ref_cell());
            map.insert("old".to_string(), Value::String(c.old_body.clone()).ref_cell());
            map.insert("new".to_string(), Value::String(c.new_body.clone()).ref_cell());
            Value::Object(map).ref_cell()
        })
        .collect();
    Value::Array(items)
}

// ---------------------------------------------------------------------------
// Watch session
// ---------------------------------------------------------------------------

struct WatchState {
    path: String,
    last_mtime: Option<SystemTime>,
    functions: HashMap<String, String>,
    last_changes: Vec<FnChange>,
}

thread_local! {
    static WATCHES: RefCell<HashMap<i64, WatchState>> = RefCell::new(HashMap::new());
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

fn read_mtime(path: &str) -> Option<SystemTime> {
    fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

fn mtimes_differ(a: Option<SystemTime>, b: Option<SystemTime>) -> bool {
    match (a, b) {
        (None, None) => false,
        (Some(x), Some(y)) => x != y,
        _ => true,
    }
}

fn with_watch<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut WatchState) -> Result<T, ValueRef>,
) -> NiaoResult<Result<T, ValueRef>> {
    WATCHES.with(|watches| {
        let mut watches = watches.borrow_mut();
        match watches.get_mut(&id) {
            Some(w) => Ok(f(w)),
            None => Ok(Err(invalid_handle(span, id))),
        }
    })
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3202_NHOTRELOAD_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3200_NHOTRELOAD_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
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
                "{name}() expects an int handle as argument {}, got {}",
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
                "{name}() expects a string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn invalid_handle(span: Span, id: i64) -> ValueRef {
    error_value(
        E3203_NHOTRELOAD_INVALID_HANDLE,
        "nhotreload_error",
        format!("invalid or closed nhotreload handle {id}"),
        span,
    )
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn nhotreload_watch(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhotreload_watch", span)?;
    let path = string_arg(args, 0, "nhotreload_watch", span)?;
    let (_, entries) = match parse_file(&path, span) {
        Ok(v) => v,
        Err(e) => return Ok(e),
    };
    let id = new_handle();
    WATCHES.with(|w| {
        w.borrow_mut().insert(
            id,
            WatchState {
                path: path.clone(),
                last_mtime: read_mtime(&path),
                functions: entries_to_map(&entries),
                last_changes: Vec::new(),
            },
        );
    });
    Ok(Value::Int(id).ref_cell())
}

fn nhotreload_changed(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhotreload_changed", span)?;
    let id = int_arg(args, 0, "nhotreload_changed", span)?;
    match with_watch(id, span, |w| {
        let current = read_mtime(&w.path);
        Ok(mtimes_differ(w.last_mtime, current))
    })? {
        Ok(b) => Ok(Value::Bool(b).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nhotreload_poll(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhotreload_poll", span)?;
    let id = int_arg(args, 0, "nhotreload_poll", span)?;
    match with_watch(id, span, |w| {
        let current_mtime = read_mtime(&w.path);
        let changed = mtimes_differ(w.last_mtime, current_mtime);
        if !changed {
            w.last_changes.clear();
            return Ok(false);
        }
        let (_, entries) = match parse_file(&w.path, span) {
            Ok(v) => v,
            Err(e) => return Err(e),
        };
        let new_map = entries_to_map(&entries);
        w.last_changes = diff_maps(&w.functions, &new_map);
        w.functions = new_map;
        w.last_mtime = current_mtime;
        Ok(true)
    })? {
        Ok(b) => Ok(Value::Bool(b).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nhotreload_diff(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhotreload_diff", span)?;
    let id = int_arg(args, 0, "nhotreload_diff", span)?;
    match with_watch(id, span, |w| Ok(changes_value(&w.last_changes)))? {
        Ok(v) => Ok(v.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nhotreload_functions(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhotreload_functions", span)?;
    let path = string_arg(args, 0, "nhotreload_functions", span)?;
    match parse_file(&path, span) {
        Ok((_, entries)) => Ok(fn_entries_value(&entries).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nhotreload_parse(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhotreload_parse", span)?;
    let source = string_arg(args, 0, "nhotreload_parse", span)?;
    match parse_source(&source, "parse", span) {
        Ok(entries) => Ok(fn_entries_value(&entries).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nhotreload_diff_sources(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nhotreload_diff_sources", span)?;
    let old_src = string_arg(args, 0, "nhotreload_diff_sources", span)?;
    let new_src = string_arg(args, 1, "nhotreload_diff_sources", span)?;
    let old_entries = match parse_source(&old_src, "diff_sources (old)", span) {
        Ok(v) => v,
        Err(e) => return Ok(e),
    };
    let new_entries = match parse_source(&new_src, "diff_sources (new)", span) {
        Ok(v) => v,
        Err(e) => return Ok(e),
    };
    let changes = diff_maps(&entries_to_map(&old_entries), &entries_to_map(&new_entries));
    Ok(changes_value(&changes).ref_cell())
}

fn nhotreload_path(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhotreload_path", span)?;
    let id = int_arg(args, 0, "nhotreload_path", span)?;
    match with_watch(id, span, |w| Ok(w.path.clone()))? {
        Ok(p) => Ok(Value::String(p).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nhotreload_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nhotreload_close", span)?;
    let id = int_arg(args, 0, "nhotreload_close", span)?;
    let removed = WATCHES.with(|w| w.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nhotreload_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nhotreload_fns![
    ("nhotreload_watch", "watch", nhotreload_watch),
    ("nhotreload_changed", "changed", nhotreload_changed),
    ("nhotreload_poll", "poll", nhotreload_poll),
    ("nhotreload_diff", "diff", nhotreload_diff),
    ("nhotreload_functions", "functions", nhotreload_functions),
    ("nhotreload_parse", "parse", nhotreload_parse),
    ("nhotreload_diff_sources", "diff_sources", nhotreload_diff_sources),
    ("nhotreload_path", "path", nhotreload_path),
    ("nhotreload_close", "close", nhotreload_close),
];

fn all_builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs()
        .into_iter()
        .map(|(flat, _, f)| (flat, f))
        .collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nhotreload";
pub const MODULE_PATHS: &[&str] = &["nhotreload", "std/nhotreload"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::PathBuf;

    fn span() -> Span {
        Span::dummy()
    }

    fn s(v: &str) -> ValueRef {
        Value::String(v.to_string()).ref_cell()
    }

    fn i(v: i64) -> ValueRef {
        Value::Int(v).ref_cell()
    }

    fn handle(r: NiaoResult<ValueRef>) -> ValueRef {
        let v = r.unwrap();
        assert!(matches!(&*v.borrow(), Value::Int(_)));
        v
    }

    fn temp_path(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("nhotreload_test_{}_{}", std::process::id(), name));
        p
    }

    fn write_niao(path: &PathBuf, body: &str) {
        let mut f = fs::File::create(path).unwrap();
        f.write_all(body.as_bytes()).unwrap();
    }

    #[test]
    fn parse_and_diff_sources() {
        let old = r#"
fn add(a: int, b: int) -> int {
    return a + b
}
fn main() {
    print(add(1, 2))
}
"#;
        let new = r#"
fn add(a: int, b: int) -> int {
    return a + b + 1
}
fn main() {
    print(add(1, 2))
}
"#;
        let fns = nhotreload_parse(&[s(old)], span()).unwrap();
        match &*fns.borrow() {
            Value::Array(items) => assert_eq!(items.len(), 2),
            other => panic!("expected array, got {other:?}"),
        }
        let diff = nhotreload_diff_sources(&[s(old), s(new)], span()).unwrap();
        {
            let borrowed = diff.borrow();
            match &*borrowed {
                Value::Array(items) => {
                    assert_eq!(items.len(), 1);
                    let item = items[0].borrow();
                    match &*item {
                        Value::Object(map) => {
                            assert!(matches!(
                                &*map["name"].borrow(),
                                Value::String(n) if n == "add"
                            ));
                        }
                        other => panic!("expected object, got {other:?}"),
                    }
                }
                other => panic!("expected array, got {other:?}"),
            }
        }
    }

    #[test]
    fn watch_poll_detects_function_change() {
        let path = temp_path("watch.niao");
        write_niao(
            &path,
            r#"fn foo() { print(1) }
fn main() { foo() }
"#,
        );
        let h = handle(nhotreload_watch(&[s(path.to_str().unwrap())], span()));
        assert!(matches!(
            &*nhotreload_changed(&[h.clone()], span()).unwrap().borrow(),
            Value::Bool(false)
        ));

        write_niao(
            &path,
            r#"fn foo() { print(2) }
fn main() { foo() }
"#,
        );
        let polled = nhotreload_poll(&[h.clone()], span()).unwrap();
        assert!(matches!(&*polled.borrow(), Value::Bool(true)));
        let diff = nhotreload_diff(&[h.clone()], span()).unwrap();
        match &*diff.borrow() {
            Value::Array(items) => assert_eq!(items.len(), 1),
            other => panic!("expected array, got {other:?}"),
        }
        nhotreload_close(&[h], span()).unwrap();
        let _ = fs::remove_file(path);
    }

    #[test]
    fn invalid_handle_returns_error_value() {
        let v = nhotreload_diff(&[i(999_999)], span()).unwrap();
        assert!(matches!(
            &*v.borrow(),
            Value::Error(e) if e.code == E3203_NHOTRELOAD_INVALID_HANDLE
        ));
    }

    #[test]
    fn parse_error_is_catchable() {
        let v = nhotreload_parse(&[s("fn { broken")], span()).unwrap();
        assert!(matches!(
            &*v.borrow(),
            Value::Error(e) if e.code == E3201_NHOTRELOAD_ERROR
        ));
    }

    #[test]
    fn namespace_and_builtins() {
        match namespace() {
            Value::Object(map) => {
                for key in [
                    "watch", "changed", "poll", "diff", "functions", "parse", "diff_sources",
                    "path", "close",
                ] {
                    assert!(map.contains_key(key), "missing {key}");
                }
            }
            other => panic!("expected object, got {other:?}"),
        }
        assert_eq!(builtins().len(), 9);
    }
}
