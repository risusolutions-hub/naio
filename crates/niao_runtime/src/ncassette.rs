//! Native ncassette standard library — VCR-style request/response cassette
//! for record, replay, and passthrough. In-memory string-keyed map with
//! durable save/load of string responses as a minimal JSON object.
//!
//! Import with `import "ncassette"` (or `import "std/ncassette"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::rc::Rc;

// codes.rs integration pending — use local constants until wired.
const E2960_NCASSETTE_ARITY: u32 = 2960;
const E2961_NCASSETTE_ERROR: u32 = 2961;
const E2962_NCASSETTE_TYPE: u32 = 2962;
const E2963_NCASSETTE_INVALID_HANDLE: u32 = 2963;

// ---------------------------------------------------------------------------
// Cassette model
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Record,
    Replay,
    Passthrough,
}

impl Mode {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "record" => Some(Mode::Record),
            "replay" => Some(Mode::Replay),
            "passthrough" => Some(Mode::Passthrough),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Mode::Record => "record",
            Mode::Replay => "replay",
            Mode::Passthrough => "passthrough",
        }
    }
}

struct Cassette {
    mode: Mode,
    map: HashMap<String, ValueRef>,
}

impl Cassette {
    fn new(mode: Mode) -> Self {
        Cassette {
            mode,
            map: HashMap::new(),
        }
    }

    fn with_map(mode: Mode, map: HashMap<String, ValueRef>) -> Self {
        Cassette { mode, map }
    }
}

thread_local! {
    static CASSETTES: RefCell<HashMap<i64, Cassette>> = RefCell::new(HashMap::new());
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

fn with_cassette<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut Cassette) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    CASSETTES.with(|cassettes| {
        let mut cassettes = cassettes.borrow_mut();
        match cassettes.get_mut(&id) {
            Some(c) => Ok(Ok(f(c))),
            None => Ok(Err(error_value(
                E2963_NCASSETTE_INVALID_HANDLE,
                "ncassette_error",
                format!("invalid or closed cassette handle {id}"),
                span,
            ))),
        }
    })
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E2962_NCASSETTE_TYPE, msg)
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E2960_NCASSETTE_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E2960_NCASSETTE_ARITY,
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

fn ncassette_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E2961_NCASSETTE_ERROR, "ncassette_error", msg.into(), span)
}

// ---------------------------------------------------------------------------
// Key + JSON (string→string only)
// ---------------------------------------------------------------------------

fn make_key(method: &str, url: &str, body: &str) -> String {
    let method = method.trim().to_ascii_uppercase();
    format!("{method}|{url}|{body}")
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn stringify_string_map(map: &HashMap<String, ValueRef>) -> Result<String, String> {
    let mut pairs: Vec<(&String, String)> = Vec::new();
    for (k, v) in map {
        match &*v.borrow() {
            Value::String(s) => pairs.push((k, s.clone())),
            other => {
                return Err(format!(
                    "save() only supports string responses; key '{k}' has {}",
                    other.type_name()
                ));
            }
        }
    }
    pairs.sort_by(|a, b| a.0.cmp(b.0));
    let mut out = String::from('{');
    for (i, (k, v)) in pairs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('"');
        out.push_str(&json_escape(k));
        out.push_str("\":\"");
        out.push_str(&json_escape(v));
        out.push('"');
    }
    out.push('}');
    Ok(out)
}

fn skip_ws(chars: &[char], i: &mut usize) {
    while *i < chars.len() && chars[*i].is_whitespace() {
        *i += 1;
    }
}

fn parse_json_string(chars: &[char], i: &mut usize) -> Result<String, String> {
    if *i >= chars.len() || chars[*i] != '"' {
        return Err("expected '\"' starting a JSON string".into());
    }
    *i += 1;
    let mut out = String::new();
    while *i < chars.len() {
        let c = chars[*i];
        *i += 1;
        match c {
            '"' => return Ok(out),
            '\\' => {
                if *i >= chars.len() {
                    return Err("unterminated escape in JSON string".into());
                }
                let e = chars[*i];
                *i += 1;
                match e {
                    '"' | '\\' | '/' => out.push(e),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'u' => {
                        if *i + 4 > chars.len() {
                            return Err("incomplete \\uXXXX escape".into());
                        }
                        let hex: String = chars[*i..*i + 4].iter().collect();
                        *i += 4;
                        let code = u32::from_str_radix(&hex, 16)
                            .map_err(|_| format!("invalid \\u escape '{hex}'"))?;
                        out.push(
                            char::from_u32(code)
                                .ok_or_else(|| format!("invalid unicode codepoint U+{hex}"))?,
                        );
                    }
                    other => return Err(format!("unsupported escape '\\{other}'")),
                }
            }
            c if (c as u32) < 0x20 => {
                return Err("unescaped control character in JSON string".into());
            }
            c => out.push(c),
        }
    }
    Err("unterminated JSON string".into())
}

fn parse_string_map(text: &str) -> Result<HashMap<String, String>, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    skip_ws(&chars, &mut i);
    if i >= chars.len() || chars[i] != '{' {
        return Err("expected '{' starting cassette JSON object".into());
    }
    i += 1;
    let mut map = HashMap::new();
    skip_ws(&chars, &mut i);
    if i < chars.len() && chars[i] == '}' {
        return Ok(map);
    }
    loop {
        skip_ws(&chars, &mut i);
        let key = parse_json_string(&chars, &mut i)?;
        skip_ws(&chars, &mut i);
        if i >= chars.len() || chars[i] != ':' {
            return Err("expected ':' after JSON key".into());
        }
        i += 1;
        skip_ws(&chars, &mut i);
        let value = parse_json_string(&chars, &mut i)?;
        map.insert(key, value);
        skip_ws(&chars, &mut i);
        if i >= chars.len() {
            return Err("unterminated JSON object".into());
        }
        match chars[i] {
            ',' => {
                i += 1;
                continue;
            }
            '}' => {
                i += 1;
                skip_ws(&chars, &mut i);
                if i != chars.len() {
                    return Err("trailing data after JSON object".into());
                }
                return Ok(map);
            }
            other => return Err(format!("unexpected '{other}' in JSON object")),
        }
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn ncassette_new(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncassette_new", span)?;
    let mode_s = string_arg(args, 0, "ncassette_new", span)?;
    let Some(mode) = Mode::parse(&mode_s) else {
        return Ok(ncassette_err(
            span,
            format!("ncassette_new() mode must be \"record\", \"replay\", or \"passthrough\", got '{mode_s}'"),
        ));
    };
    let id = new_handle();
    CASSETTES.with(|c| {
        c.borrow_mut().insert(id, Cassette::new(mode));
    });
    Ok(Value::Int(id).ref_cell())
}

/// ncassette_key(method, url, body?)
fn ncassette_key(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "ncassette_key", span)?;
    let method = string_arg(args, 0, "ncassette_key", span)?;
    let url = string_arg(args, 1, "ncassette_key", span)?;
    let body = if args.len() > 2 {
        string_arg(args, 2, "ncassette_key", span)?
    } else {
        String::new()
    };
    Ok(Value::String(make_key(&method, &url, &body)).ref_cell())
}

fn ncassette_put(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "ncassette_put", span)?;
    let id = int_arg(args, 0, "ncassette_put", span)?;
    let key = string_arg(args, 1, "ncassette_put", span)?;
    let value = Rc::clone(&args[2]);
    match with_cassette(id, span, |c| {
        c.map.insert(key, value);
    })? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn ncassette_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncassette_get", span)?;
    let id = int_arg(args, 0, "ncassette_get", span)?;
    let key = string_arg(args, 1, "ncassette_get", span)?;
    match with_cassette(id, span, |c| c.map.get(&key).map(Rc::clone))? {
        Ok(Some(v)) => Ok(v),
        Ok(None) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn ncassette_has(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncassette_has", span)?;
    let id = int_arg(args, 0, "ncassette_has", span)?;
    let key = string_arg(args, 1, "ncassette_has", span)?;
    match with_cassette(id, span, |c| c.map.contains_key(&key))? {
        Ok(b) => Ok(Value::Bool(b).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn ncassette_save(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncassette_save", span)?;
    let id = int_arg(args, 0, "ncassette_save", span)?;
    let path = string_arg(args, 1, "ncassette_save", span)?;
    let json = match with_cassette(id, span, |c| stringify_string_map(&c.map))? {
        Ok(Ok(s)) => s,
        Ok(Err(msg)) => return Ok(ncassette_err(span, msg)),
        Err(e) => return Ok(e),
    };
    if let Err(e) = fs::write(Path::new(&path), json) {
        return Ok(ncassette_err(span, format!("save '{path}': {e}")));
    }
    Ok(Value::Bool(true).ref_cell())
}

fn ncassette_load(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncassette_load", span)?;
    let path = string_arg(args, 0, "ncassette_load", span)?;
    let text = match fs::read_to_string(Path::new(&path)) {
        Ok(t) => t,
        Err(e) => return Ok(ncassette_err(span, format!("load '{path}': {e}"))),
    };
    let raw = match parse_string_map(&text) {
        Ok(m) => m,
        Err(e) => return Ok(ncassette_err(span, format!("load '{path}': {e}"))),
    };
    let map = raw
        .into_iter()
        .map(|(k, v)| (k, Value::String(v).ref_cell()))
        .collect();
    let id = new_handle();
    CASSETTES.with(|c| {
        c.borrow_mut()
            .insert(id, Cassette::with_map(Mode::Replay, map));
    });
    Ok(Value::Int(id).ref_cell())
}

fn ncassette_len(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncassette_len", span)?;
    let id = int_arg(args, 0, "ncassette_len", span)?;
    match with_cassette(id, span, |c| c.map.len() as i64)? {
        Ok(n) => Ok(Value::Int(n).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn ncassette_keys(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncassette_keys", span)?;
    let id = int_arg(args, 0, "ncassette_keys", span)?;
    match with_cassette(id, span, |c| {
        let mut keys: Vec<_> = c.map.keys().cloned().collect();
        keys.sort();
        keys.into_iter()
            .map(|k| Value::String(k).ref_cell())
            .collect::<Vec<ValueRef>>()
    })? {
        Ok(keys) => Ok(Value::Array(keys).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn ncassette_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncassette_close", span)?;
    let id = int_arg(args, 0, "ncassette_close", span)?;
    let removed = CASSETTES.with(|c| c.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

fn ncassette_mode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncassette_mode", span)?;
    let id = int_arg(args, 0, "ncassette_mode", span)?;
    match with_cassette(id, span, |c| c.mode.as_str().to_string())? {
        Ok(m) => Ok(Value::String(m).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn ncassette_clear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncassette_clear", span)?;
    let id = int_arg(args, 0, "ncassette_clear", span)?;
    match with_cassette(id, span, |c| {
        c.map.clear();
    })? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ncassette_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ncassette_fns![
    ("ncassette_new", "new", ncassette_new),
    ("ncassette_key", "key", ncassette_key),
    ("ncassette_put", "put", ncassette_put),
    ("ncassette_get", "get", ncassette_get),
    ("ncassette_has", "has", ncassette_has),
    ("ncassette_save", "save", ncassette_save),
    ("ncassette_load", "load", ncassette_load),
    ("ncassette_len", "len", ncassette_len),
    ("ncassette_keys", "keys", ncassette_keys),
    ("ncassette_close", "close", ncassette_close),
    ("ncassette_mode", "mode", ncassette_mode),
    ("ncassette_clear", "clear", ncassette_clear),
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

pub const MODULE_NAME: &str = "ncassette";
pub const MODULE_PATHS: &[&str] = &["ncassette", "std/ncassette"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;
    use std::time::{SystemTime, UNIX_EPOCH};

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
        assert!(matches!(&*v.borrow(), Value::Int(_)), "expected handle int");
        v
    }

    fn temp_path(name: &str) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!("ncassette_{name}_{nanos}.json"))
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn key_is_stable_and_canonical() {
        let k1 = ncassette_key(&[s("get"), s("/api"), s("{\"a\":1}")], span()).unwrap();
        let k2 = ncassette_key(&[s("GET"), s("/api"), s("{\"a\":1}")], span()).unwrap();
        assert!(matches!(&*k1.borrow(), Value::String(a) if a == "GET|/api|{\"a\":1}"));
        assert_eq!(format!("{:?}", k1.borrow()), format!("{:?}", k2.borrow()));
        let k3 = ncassette_key(&[s("post"), s("/x")], span()).unwrap();
        assert!(matches!(&*k3.borrow(), Value::String(a) if a == "POST|/x|"));
    }

    #[test]
    fn put_get_has_len_keys_clear() {
        let h = handle(ncassette_new(&[s("record")], span()));
        assert!(matches!(
            &*ncassette_mode(&[h.clone()], span()).unwrap().borrow(),
            Value::String(m) if m == "record"
        ));
        let key = "GET|/users|";
        ncassette_put(&[h.clone(), s(key), s("{\"ok\":true}")], span()).unwrap();
        assert!(matches!(
            &*ncassette_has(&[h.clone(), s(key)], span()).unwrap().borrow(),
            Value::Bool(true)
        ));
        let v = ncassette_get(&[h.clone(), s(key)], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::String(r) if r == "{\"ok\":true}"));
        assert!(matches!(
            &*ncassette_len(&[h.clone()], span()).unwrap().borrow(),
            Value::Int(1)
        ));
        let keys = ncassette_keys(&[h.clone()], span()).unwrap();
        assert!(matches!(&*keys.borrow(), Value::Array(a) if a.len() == 1));
        ncassette_clear(&[h.clone()], span()).unwrap();
        assert!(matches!(
            &*ncassette_len(&[h.clone()], span()).unwrap().borrow(),
            Value::Int(0)
        ));
        ncassette_close(&[h], span()).unwrap();
    }

    #[test]
    fn save_load_roundtrip_string_json() {
        let path = temp_path("roundtrip");
        let h = handle(ncassette_new(&[s("record")], span()));
        ncassette_put(&[h.clone(), s("GET|/a|"), s("alpha")], span()).unwrap();
        ncassette_put(&[h.clone(), s("POST|/b|x"), s("bravo\"quote")], span()).unwrap();
        let ok = ncassette_save(&[h.clone(), s(&path)], span()).unwrap();
        assert!(matches!(&*ok.borrow(), Value::Bool(true)));
        ncassette_close(&[h], span()).unwrap();

        let loaded = handle(ncassette_load(&[s(&path)], span()));
        assert!(matches!(
            &*ncassette_mode(&[loaded.clone()], span()).unwrap().borrow(),
            Value::String(m) if m == "replay"
        ));
        let a = ncassette_get(&[loaded.clone(), s("GET|/a|")], span()).unwrap();
        assert!(matches!(&*a.borrow(), Value::String(r) if r == "alpha"));
        let b = ncassette_get(&[loaded.clone(), s("POST|/b|x")], span()).unwrap();
        assert!(matches!(&*b.borrow(), Value::String(r) if r == "bravo\"quote"));
        assert!(matches!(
            &*ncassette_len(&[loaded.clone()], span()).unwrap().borrow(),
            Value::Int(2)
        ));
        ncassette_close(&[loaded], span()).unwrap();
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn save_rejects_non_string_response() {
        let h = handle(ncassette_new(&[s("record")], span()));
        ncassette_put(&[h.clone(), s("k"), i(42)], span()).unwrap();
        let path = temp_path("bad");
        let err = ncassette_save(&[h.clone(), s(&path)], span()).unwrap();
        assert!(matches!(&*err.borrow(), Value::Error(_)));
        ncassette_close(&[h], span()).unwrap();
    }

    #[test]
    fn invalid_handle_and_mode() {
        let bad = ncassette_new(&[s("live")], span()).unwrap();
        assert!(matches!(&*bad.borrow(), Value::Error(_)));
        let v = ncassette_get(&[i(424_242), s("k")], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn json_escape_roundtrip_helpers() {
        assert_eq!(json_escape("a\"b\\c"), r#"a\"b\\c"#);
        assert_eq!(json_escape("line\nnewline"), "line\\nnewline");
        let mut map = HashMap::new();
        map.insert(
            "k".to_string(),
            Value::String("v\t1".to_string()).ref_cell(),
        );
        let json = stringify_string_map(&map).unwrap();
        let parsed = parse_string_map(&json).unwrap();
        assert_eq!(parsed.get("k").map(String::as_str), Some("v\t1"));
    }
}
