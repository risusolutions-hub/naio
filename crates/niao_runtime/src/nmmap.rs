//! Native nmmap standard library — memory-mapped files via memmap2, lazy line
//! index, and byte search over mapped regions.
//!
//! Import with `import "nmmap"` (or `import "std/nmmap"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use memmap2::Mmap;
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::File;
use std::rc::Rc;

// Wired in codes.rs by central integration.
const E3360_NMMAP_ARITY: u32 = 3360;
const E3361_NMMAP_ERROR: u32 = 3361;
const E3362_NMMAP_TYPE: u32 = 3362;
const E3363_NMMAP_INVALID_HANDLE: u32 = 3363;

// ---------------------------------------------------------------------------
// Mapped file model
// ---------------------------------------------------------------------------

struct MappedFile {
    path: String,
    mmap: Mmap,
    /// Lazy line-start offsets (byte indices); `None` until first line op.
    line_starts: Option<Vec<usize>>,
}

impl MappedFile {
    fn ensure_line_index(&mut self) {
        if self.line_starts.is_some() {
            return;
        }
        let data = self.mmap.as_ref();
        let mut starts = Vec::new();
        if !data.is_empty() {
            starts.push(0);
        }
        let mut i = 0usize;
        while i < data.len() {
            match data[i] {
                b'\n' => {
                    if i + 1 < data.len() {
                        starts.push(i + 1);
                    }
                    i += 1;
                }
                b'\r' => {
                    if i + 1 < data.len() && data[i + 1] == b'\n' {
                        if i + 2 < data.len() {
                            starts.push(i + 2);
                        }
                        i += 2;
                    } else {
                        if i + 1 < data.len() {
                            starts.push(i + 1);
                        }
                        i += 1;
                    }
                }
                _ => i += 1,
            }
        }
        self.line_starts = Some(starts);
    }

    fn line_count(&mut self) -> usize {
        self.ensure_line_index();
        self.line_starts.as_ref().map(|s| s.len()).unwrap_or(0)
    }

    fn line_bytes(&mut self, index: usize) -> Option<Vec<u8>> {
        self.ensure_line_index();
        let data = self.mmap.as_ref();
        if data.is_empty() {
            return if index == 0 { Some(Vec::new()) } else { None };
        }
        let starts = self.line_starts.as_ref().unwrap();
        if index >= starts.len() {
            return None;
        }
        let start = starts[index];
        let end = if index + 1 < starts.len() {
            starts[index + 1]
        } else {
            data.len()
        };
        let mut slice = data[start..end.min(data.len())].to_vec();
        if slice.ends_with(b"\r\n") {
            slice.truncate(slice.len() - 2);
        } else if slice.ends_with(b"\n") || slice.ends_with(b"\r") {
            slice.pop();
        }
        Some(slice)
    }

    fn find_bytes(&self, needle: &[u8], start: usize) -> Option<usize> {
        let data = self.mmap.as_ref();
        if needle.is_empty() {
            return Some(start.min(data.len()));
        }
        if start >= data.len() {
            return None;
        }
        data[start..]
            .windows(needle.len())
            .position(|w| w == needle)
            .map(|pos| start + pos)
    }
}

thread_local! {
    static MAPS: RefCell<HashMap<i64, MappedFile>> = RefCell::new(HashMap::new());
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

fn with_map<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut MappedFile) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    MAPS.with(|maps| {
        let mut maps = maps.borrow_mut();
        match maps.get_mut(&id) {
            Some(m) => Ok(Ok(f(m))),
            None => Ok(Err(error_value(
                E3363_NMMAP_INVALID_HANDLE,
                "nmmap_error",
                format!("invalid or closed mmap handle {id}"),
                span,
            ))),
        }
    })
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3360_NMMAP_ARITY,
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
            E3360_NMMAP_ARITY,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3362_NMMAP_TYPE, msg.into())
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

fn nmmap_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3361_NMMAP_ERROR, "nmmap_error", msg.into(), span)
}

fn bytes_from_value(v: &Value, span: Span, name: &str) -> Result<Vec<u8>, RuntimeError> {
    match v {
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        Value::ByteArray(b) => Ok(b.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a string or byte_array needle, got {}",
                other.type_name()
            ),
        )),
    }
}

fn clamp_range(
    len: usize,
    start: i64,
    end: Option<i64>,
    span: Span,
) -> Result<(usize, usize), ValueRef> {
    if start < 0 {
        return Err(nmmap_err(span, "start offset must be >= 0"));
    }
    let start = start as usize;
    if start > len {
        return Err(nmmap_err(
            span,
            format!("start offset {start} exceeds mapped length {len}"),
        ));
    }
    let end = match end {
        Some(e) if e < 0 => return Err(nmmap_err(span, "end offset must be >= 0")),
        Some(e) => e as usize,
        None => len,
    };
    if end > len {
        return Err(nmmap_err(
            span,
            format!("end offset {end} exceeds mapped length {len}"),
        ));
    }
    if end < start {
        return Err(nmmap_err(
            span,
            format!("end offset {end} is before start {start}"),
        ));
    }
    Ok((start, end))
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// nmmap_open(path) → handle
fn nmmap_open(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmmap_open", span)?;
    let path = string_arg(args, 0, "nmmap_open", span)?;
    let file = match File::open(&path) {
        Ok(f) => f,
        Err(e) => return Ok(nmmap_err(span, format!("failed to open '{}': {e}", path))),
    };
    let mmap = match unsafe { Mmap::map(&file) } {
        Ok(m) => m,
        Err(e) => return Ok(nmmap_err(span, format!("failed to mmap '{}': {e}", path))),
    };
    let id = new_handle();
    MAPS.with(|maps| {
        maps.borrow_mut().insert(
            id,
            MappedFile {
                path: path.clone(),
                mmap,
                line_starts: None,
            },
        );
    });
    Ok(Value::Int(id).ref_cell())
}

/// nmmap_close(handle) → bool
fn nmmap_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmmap_close", span)?;
    let id = int_arg(args, 0, "nmmap_close", span)?;
    let removed = MAPS.with(|maps| maps.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

/// nmmap_len(handle) → byte length
fn nmmap_len(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmmap_len", span)?;
    let id = int_arg(args, 0, "nmmap_len", span)?;
    match with_map(id, span, |m| m.mmap.len())? {
        Ok(n) => Ok(Value::Int(n as i64).ref_cell()),
        Err(e) => Ok(e),
    }
}

/// nmmap_bytes(handle, start, end?) → byte_array slice
fn nmmap_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nmmap_bytes", span)?;
    let id = int_arg(args, 0, "nmmap_bytes", span)?;
    let start = int_arg(args, 1, "nmmap_bytes", span)?;
    let end = if args.len() == 3 {
        Some(int_arg(args, 2, "nmmap_bytes", span)?)
    } else {
        None
    };
    match with_map(id, span, |m| {
        let len = m.mmap.len();
        match clamp_range(len, start, end, span) {
            Ok((s, e)) => Ok(Value::ByteArray(m.mmap.as_ref()[s..e].to_vec()).ref_cell()),
            Err(err) => Err(err),
        }
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// nmmap_text(handle, start, end?) → UTF-8 string slice
fn nmmap_text(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nmmap_text", span)?;
    let id = int_arg(args, 0, "nmmap_text", span)?;
    let start = int_arg(args, 1, "nmmap_text", span)?;
    let end = if args.len() == 3 {
        Some(int_arg(args, 2, "nmmap_text", span)?)
    } else {
        None
    };
    match with_map(id, span, |m| {
        let len = m.mmap.len();
        let (s, e) = match clamp_range(len, start, end, span) {
            Ok(r) => r,
            Err(err) => return Err(err),
        };
        match std::str::from_utf8(&m.mmap.as_ref()[s..e]) {
            Ok(text) => Ok(Value::String(text.to_string()).ref_cell()),
            Err(e) => Err(nmmap_err(
                span,
                format!("slice [{s}..{e}] is not valid UTF-8: {e}"),
            )),
        }
    })? {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Ok(e),
        Err(e) => Ok(e),
    }
}

/// nmmap_line_count(handle) → number of lines (builds lazy index)
fn nmmap_line_count(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmmap_line_count", span)?;
    let id = int_arg(args, 0, "nmmap_line_count", span)?;
    match with_map(id, span, |m| m.line_count())? {
        Ok(n) => Ok(Value::Int(n as i64).ref_cell()),
        Err(e) => Ok(e),
    }
}

/// nmmap_line(handle, index) → line text without trailing newline
fn nmmap_line(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmmap_line", span)?;
    let id = int_arg(args, 0, "nmmap_line", span)?;
    let index = int_arg(args, 1, "nmmap_line", span)?;
    if index < 0 {
        return Ok(nmmap_err(span, "line index must be >= 0"));
    }
    match with_map(id, span, |m| m.line_bytes(index as usize))? {
        Ok(Some(bytes)) => match std::str::from_utf8(&bytes) {
            Ok(s) => Ok(Value::String(s.to_string()).ref_cell()),
            Err(e) => Ok(nmmap_err(
                span,
                format!("line {index} is not valid UTF-8: {e}"),
            )),
        },
        Ok(None) => {
            let count = MAPS.with(|maps| {
                maps.borrow_mut()
                    .get_mut(&id)
                    .map(|m| m.line_count())
                    .unwrap_or(0)
            });
            Ok(nmmap_err(
                span,
                format!("line index {index} out of range (file has {count} lines)"),
            ))
        }
        Err(e) => Ok(e),
    }
}

/// nmmap_find(handle, needle, start?) → byte offset or -1
fn nmmap_find(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nmmap_find", span)?;
    let id = int_arg(args, 0, "nmmap_find", span)?;
    let needle = bytes_from_value(&*args[1].borrow(), span, "nmmap_find")?;
    let start = if args.len() == 3 {
        int_arg(args, 2, "nmmap_find", span)?
    } else {
        0
    };
    if start < 0 {
        return Ok(nmmap_err(span, "start offset must be >= 0"));
    }
    match with_map(id, span, |m| m.find_bytes(&needle, start as usize))? {
        Ok(Some(off)) => Ok(Value::Int(off as i64).ref_cell()),
        Ok(None) => Ok(Value::Int(-1).ref_cell()),
        Err(e) => Ok(e),
    }
}

/// nmmap_stats(handle) → {path, len, lines_indexed, line_count}
fn nmmap_stats(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmmap_stats", span)?;
    let id = int_arg(args, 0, "nmmap_stats", span)?;
    match with_map(id, span, |m| {
        let len = m.mmap.len();
        let indexed = m.line_starts.is_some();
        let lines = if indexed { m.line_count() } else { 0 };
        (m.path.clone(), len, indexed, lines)
    })? {
        Ok((path, len, indexed, lines)) => {
            let mut map = HashMap::new();
            map.insert("path".to_string(), Value::String(path).ref_cell());
            map.insert("len".to_string(), Value::Int(len as i64).ref_cell());
            map.insert("lines_indexed".to_string(), Value::Bool(indexed).ref_cell());
            map.insert(
                "line_count".to_string(),
                Value::Int(lines as i64).ref_cell(),
            );
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nmmap_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nmmap_fns![
    ("nmmap_open", "open", nmmap_open),
    ("nmmap_close", "close", nmmap_close),
    ("nmmap_len", "len", nmmap_len),
    ("nmmap_bytes", "bytes", nmmap_bytes),
    ("nmmap_text", "text", nmmap_text),
    ("nmmap_line_count", "line_count", nmmap_line_count),
    ("nmmap_line", "line", nmmap_line),
    ("nmmap_find", "find", nmmap_find),
    ("nmmap_stats", "stats", nmmap_stats),
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

pub const MODULE_NAME: &str = "nmmap";
pub const MODULE_PATHS: &[&str] = &["nmmap", "std/nmmap"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;
    use std::io::Write;

    fn span() -> Span {
        Span::dummy()
    }

    fn i(v: i64) -> ValueRef {
        Value::Int(v).ref_cell()
    }

    fn s(v: &str) -> ValueRef {
        Value::String(v.to_string()).ref_cell()
    }

    fn handle(r: NiaoResult<ValueRef>) -> ValueRef {
        let v = r.unwrap();
        assert!(matches!(&*v.borrow(), Value::Int(_)));
        v
    }

    fn temp_file(contents: &[u8]) -> String {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("nmmap_test_{}.txt", std::process::id()));
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(contents).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn open_read_and_find() {
        let path = temp_file(b"hello world\nsecond line\n");
        let h = handle(nmmap_open(&[s(&path)], span()));
        assert_eq!(
            match &*nmmap_len(&[h.clone()], span()).unwrap().borrow() {
                Value::Int(n) => *n,
                _ => panic!(),
            },
            25
        );
        let text = nmmap_text(&[h.clone(), i(0), i(5)], span()).unwrap();
        assert!(matches!(&*text.borrow(), Value::String(s) if s == "hello"));
        let off = match &*nmmap_find(&[h.clone(), s("world")], span())
            .unwrap()
            .borrow()
        {
            Value::Int(n) => *n,
            _ => panic!(),
        };
        assert_eq!(off, 6);
        assert!(matches!(
            &*nmmap_find(&[h.clone(), s("missing")], span())
                .unwrap()
                .borrow(),
            Value::Int(-1)
        ));
        nmmap_close(&[h], span()).unwrap();
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn lazy_line_index() {
        let path = temp_file(b"a\nb\r\nc\r\n");
        let h = handle(nmmap_open(&[s(&path)], span()));
        let stats_before = nmmap_stats(&[h.clone()], span()).unwrap();
        match &*stats_before.borrow() {
            Value::Object(m) => {
                assert!(matches!(
                    &*m.get("lines_indexed").unwrap().borrow(),
                    Value::Bool(false)
                ));
            }
            _ => panic!(),
        }
        let count = match &*nmmap_line_count(&[h.clone()], span()).unwrap().borrow() {
            Value::Int(n) => *n,
            _ => panic!(),
        };
        assert_eq!(count, 3);
        let line1 = nmmap_line(&[h.clone(), i(1)], span()).unwrap();
        assert!(matches!(&*line1.borrow(), Value::String(s) if s == "b"));
        nmmap_close(&[h], span()).unwrap();
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn invalid_handle() {
        let v = nmmap_len(&[i(999_999)], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }
}
