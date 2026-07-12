//! Native nsnap standard library — fast binary value snapshots with content
//! fingerprints and staleness checks. Wire format magic `NSNP1`.
//!
//! Import with `import "nsnap"` (or `import "std/nsnap"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_crypto::{hex, sha256};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

const E3420_NSNAP_ARITY: u32 = 3420;
const E3421_NSNAP_ERROR: u32 = 3421;
const E3422_NSNAP_TYPE: u32 = 3422;
const E3423_NSNAP_FORMAT: u32 = 3423;

const MAGIC: &[u8; 5] = b"NSNP1";
const VERSION: u8 = 1;
const HEADER_LEN: usize = 5 + 1 + 8 + 32 + 4; // magic + ver + created_ms + fp + payload_len

// Value tags in payload
const TAG_NIL: u8 = 0;
const TAG_BOOL: u8 = 1;
const TAG_INT: u8 = 2;
const TAG_FLOAT: u8 = 3;
const TAG_STRING: u8 = 4;
const TAG_ARRAY: u8 = 5;
const TAG_OBJECT: u8 = 6;
const TAG_INT_ARRAY: u8 = 7;
const TAG_FLOAT_ARRAY: u8 = 8;
const TAG_BOOL_ARRAY: u8 = 9;
const TAG_BYTE_ARRAY: u8 = 10;
const TAG_STRING_ARRAY: u8 = 11;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Binary codec
// ---------------------------------------------------------------------------

struct Encoder {
    buf: Vec<u8>,
}

impl Encoder {
    fn new() -> Self {
        Encoder { buf: Vec::new() }
    }

    fn push_u32(&mut self, n: u32) {
        self.buf.extend_from_slice(&n.to_le_bytes());
    }

    fn push_i64(&mut self, n: i64) {
        self.buf.extend_from_slice(&n.to_le_bytes());
    }

    fn push_f64(&mut self, n: f64) {
        self.buf.extend_from_slice(&n.to_le_bytes());
    }

    fn push_bytes(&mut self, b: &[u8]) {
        self.push_u32(b.len() as u32);
        self.buf.extend_from_slice(b);
    }

    fn encode_value(&mut self, v: &Value) -> Result<(), String> {
        match v {
            Value::Nil => self.buf.push(TAG_NIL),
            Value::Bool(b) => {
                self.buf.push(TAG_BOOL);
                self.buf.push(if *b { 1 } else { 0 });
            }
            Value::Int(n) => {
                self.buf.push(TAG_INT);
                self.push_i64(*n);
            }
            Value::Float(f) => {
                self.buf.push(TAG_FLOAT);
                self.push_f64(*f);
            }
            Value::String(s) => {
                self.buf.push(TAG_STRING);
                self.push_bytes(s.as_bytes());
            }
            Value::Array(items) => {
                self.buf.push(TAG_ARRAY);
                self.push_u32(items.len() as u32);
                for item in items {
                    self.encode_value(&item.borrow())?;
                }
            }
            Value::Object(map) => {
                self.buf.push(TAG_OBJECT);
                self.push_u32(map.len() as u32);
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                for k in keys {
                    self.push_bytes(k.as_bytes());
                    self.encode_value(&map[k].borrow())?;
                }
            }
            Value::IntArray(items) => {
                self.buf.push(TAG_INT_ARRAY);
                self.push_u32(items.len() as u32);
                for n in items {
                    self.push_i64(*n);
                }
            }
            Value::FloatArray(items) => {
                self.buf.push(TAG_FLOAT_ARRAY);
                self.push_u32(items.len() as u32);
                for f in items {
                    self.push_f64(*f);
                }
            }
            Value::BoolArray(items) => {
                self.buf.push(TAG_BOOL_ARRAY);
                self.push_u32(items.len() as u32);
                self.buf.extend_from_slice(items);
            }
            Value::ByteArray(items) => {
                self.buf.push(TAG_BYTE_ARRAY);
                self.push_bytes(items);
            }
            Value::StringArray(sa) => {
                self.buf.push(TAG_STRING_ARRAY);
                let items = sa.dense_vec();
                self.push_u32(items.len() as u32);
                for s in &items {
                    self.push_bytes(s.as_bytes());
                }
            }
            other => {
                return Err(format!(
                    "nsnap cannot snapshot values of type {}",
                    other.type_name()
                ));
            }
        }
        Ok(())
    }

    fn finish(self) -> Vec<u8> {
        self.buf
    }
}

struct Decoder<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Decoder<'a> {
    fn new(data: &'a [u8]) -> Self {
        Decoder { data, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn read_exact(&mut self, n: usize) -> Result<&'a [u8], String> {
        if self.pos + n > self.data.len() {
            return Err("unexpected end of snapshot payload".into());
        }
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn read_u32(&mut self) -> Result<u32, String> {
        let b = self.read_exact(4)?;
        Ok(u32::from_le_bytes(b.try_into().unwrap()))
    }

    fn read_i64(&mut self) -> Result<i64, String> {
        let b = self.read_exact(8)?;
        Ok(i64::from_le_bytes(b.try_into().unwrap()))
    }

    fn read_f64(&mut self) -> Result<f64, String> {
        let b = self.read_exact(8)?;
        Ok(f64::from_le_bytes(b.try_into().unwrap()))
    }

    fn read_bytes(&mut self) -> Result<Vec<u8>, String> {
        let len = self.read_u32()? as usize;
        Ok(self.read_exact(len)?.to_vec())
    }

    fn read_string(&mut self) -> Result<String, String> {
        let bytes = self.read_bytes()?;
        String::from_utf8(bytes).map_err(|e| format!("invalid UTF-8 in snapshot: {e}"))
    }

    fn decode_value(&mut self) -> Result<Value, String> {
        let tag = self.read_exact(1)?[0];
        match tag {
            TAG_NIL => Ok(Value::Nil),
            TAG_BOOL => {
                let b = self.read_exact(1)?[0];
                Ok(Value::Bool(b != 0))
            }
            TAG_INT => Ok(Value::Int(self.read_i64()?)),
            TAG_FLOAT => Ok(Value::Float(self.read_f64()?)),
            TAG_STRING => Ok(Value::String(self.read_string()?)),
            TAG_ARRAY => {
                let len = self.read_u32()? as usize;
                let mut items = Vec::with_capacity(len);
                for _ in 0..len {
                    items.push(self.decode_value()?.ref_cell());
                }
                Ok(Value::Array(items))
            }
            TAG_OBJECT => {
                let len = self.read_u32()? as usize;
                let mut map = HashMap::with_capacity(len);
                for _ in 0..len {
                    let key = self.read_string()?;
                    let val = self.decode_value()?.ref_cell();
                    map.insert(key, val);
                }
                Ok(Value::Object(map))
            }
            TAG_INT_ARRAY => {
                let len = self.read_u32()? as usize;
                let mut items = Vec::with_capacity(len);
                for _ in 0..len {
                    items.push(self.read_i64()?);
                }
                Ok(Value::IntArray(items))
            }
            TAG_FLOAT_ARRAY => {
                let len = self.read_u32()? as usize;
                let mut items = Vec::with_capacity(len);
                for _ in 0..len {
                    items.push(self.read_f64()?);
                }
                Ok(Value::FloatArray(items))
            }
            TAG_BOOL_ARRAY => {
                let len = self.read_u32()? as usize;
                Ok(Value::BoolArray(self.read_exact(len)?.to_vec()))
            }
            TAG_BYTE_ARRAY => Ok(Value::ByteArray(self.read_bytes()?)),
            TAG_STRING_ARRAY => {
                let len = self.read_u32()? as usize;
                let mut items = Vec::with_capacity(len);
                for _ in 0..len {
                    items.push(self.read_string()?);
                }
                Ok(Value::StringArray(crate::StringArray::dense(items)))
            }
            _ => Err(format!("unknown snapshot value tag 0x{tag:02x}")),
        }
    }
}

fn encode_payload(v: &Value) -> Result<Vec<u8>, String> {
    let mut enc = Encoder::new();
    enc.encode_value(v)?;
    Ok(enc.finish())
}

fn fingerprint_payload(payload: &[u8]) -> [u8; 32] {
    sha256(payload)
}

fn pack_snapshot(v: &Value, created_ms: i64) -> Result<Vec<u8>, String> {
    let payload = encode_payload(v)?;
    let fp = fingerprint_payload(&payload);
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    out.extend_from_slice(&created_ms.to_le_bytes());
    out.extend_from_slice(&fp);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

struct SnapshotHeader {
    created_ms: i64,
    fingerprint: [u8; 32],
    payload: Vec<u8>,
}

fn parse_snapshot(bytes: &[u8]) -> Result<SnapshotHeader, String> {
    if bytes.len() < HEADER_LEN {
        return Err("snapshot too short".into());
    }
    if &bytes[..5] != MAGIC {
        return Err("invalid snapshot magic (expected NSNP1)".into());
    }
    if bytes[5] != VERSION {
        return Err(format!("unsupported snapshot version {}", bytes[5]));
    }
    let created_ms = i64::from_le_bytes(bytes[6..14].try_into().unwrap());
    let mut fingerprint = [0u8; 32];
    fingerprint.copy_from_slice(&bytes[14..46]);
    let payload_len = u32::from_le_bytes(bytes[46..50].try_into().unwrap()) as usize;
    if bytes.len() != HEADER_LEN + payload_len {
        return Err("snapshot payload length mismatch".into());
    }
    let payload = bytes[HEADER_LEN..].to_vec();
    let expected = fingerprint_payload(&payload);
    if expected != fingerprint {
        return Err("snapshot fingerprint mismatch (corrupt data)".into());
    }
    Ok(SnapshotHeader {
        created_ms,
        fingerprint,
        payload,
    })
}

fn value_fingerprint(v: &Value) -> Result<[u8; 32], String> {
    Ok(fingerprint_payload(&encode_payload(v)?))
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3422_NSNAP_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E3420_NSNAP_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E3420_NSNAP_ARITY,
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

fn bytes_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<u8>> {
    match &*args[idx].borrow() {
        Value::ByteArray(b) => Ok(b.clone()),
        Value::String(s) => Ok(s.as_bytes().to_vec()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects byte[] or string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn nsnap_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3421_NSNAP_ERROR, "nsnap_error", msg.into(), span)
}

fn nsnap_format_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3423_NSNAP_FORMAT, "nsnap_error", msg.into(), span)
}

fn header_to_info(header: &SnapshotHeader) -> HashMap<String, ValueRef> {
    let mut map = HashMap::new();
    map.insert("magic".to_string(), Value::String("NSNP1".into()).ref_cell());
    map.insert("version".to_string(), Value::Int(VERSION as i64).ref_cell());
    map.insert(
        "created_ms".to_string(),
        Value::Int(header.created_ms).ref_cell(),
    );
    map.insert(
        "fingerprint".to_string(),
        Value::String(hex::encode(&header.fingerprint)).ref_cell(),
    );
    map.insert(
        "payload_len".to_string(),
        Value::Int(header.payload.len() as i64).ref_cell(),
    );
    map
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn nsnap_capture(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsnap_capture", span)?;
    let v = args[0].borrow().clone();
    match pack_snapshot(&v, now_ms()) {
        Ok(bytes) => Ok(Value::ByteArray(bytes).ref_cell()),
        Err(msg) => Ok(nsnap_err(span, msg)),
    }
}

fn nsnap_restore(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsnap_restore", span)?;
    let bytes = bytes_arg(args, 0, "nsnap_restore", span)?;
    let header = match parse_snapshot(&bytes) {
        Ok(h) => h,
        Err(msg) => return Ok(nsnap_format_err(span, msg)),
    };
    let mut dec = Decoder::new(&header.payload);
    match dec.decode_value() {
        Ok(v) => {
            if dec.remaining() != 0 {
                return Ok(nsnap_format_err(span, "trailing bytes in snapshot payload"));
            }
            Ok(v.ref_cell())
        }
        Err(msg) => Ok(nsnap_format_err(span, msg)),
    }
}

fn nsnap_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsnap_info", span)?;
    let bytes = bytes_arg(args, 0, "nsnap_info", span)?;
    match parse_snapshot(&bytes) {
        Ok(header) => Ok(Value::Object(header_to_info(&header)).ref_cell()),
        Err(msg) => Ok(nsnap_format_err(span, msg)),
    }
}

fn nsnap_validate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsnap_validate", span)?;
    let bytes = bytes_arg(args, 0, "nsnap_validate", span)?;
    Ok(Value::Bool(parse_snapshot(&bytes).is_ok()).ref_cell())
}

fn nsnap_fingerprint(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsnap_fingerprint", span)?;
    let v = args[0].borrow().clone();
    match value_fingerprint(&v) {
        Ok(fp) => Ok(Value::String(hex::encode(&fp)).ref_cell()),
        Err(msg) => Ok(nsnap_err(span, msg)),
    }
}

fn nsnap_fingerprint_bytes(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nsnap_fingerprint_bytes", span)?;
    let bytes = bytes_arg(args, 0, "nsnap_fingerprint_bytes", span)?;
    match parse_snapshot(&bytes) {
        Ok(header) => Ok(Value::String(hex::encode(&header.fingerprint)).ref_cell()),
        Err(msg) => Ok(nsnap_format_err(span, msg)),
    }
}

/// nsnap_stale(snapshot_bytes, current_value) — true when fingerprints differ.
fn nsnap_stale(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsnap_stale", span)?;
    let bytes = bytes_arg(args, 0, "nsnap_stale", span)?;
    let current = args[1].borrow().clone();
    let header = match parse_snapshot(&bytes) {
        Ok(h) => h,
        Err(msg) => return Ok(nsnap_format_err(span, msg)),
    };
    let current_fp = match value_fingerprint(&current) {
        Ok(fp) => fp,
        Err(msg) => return Ok(nsnap_err(span, msg)),
    };
    Ok(Value::Bool(header.fingerprint != current_fp).ref_cell())
}

/// nsnap_stale_since(snapshot_bytes, since_ms) — true when created_ms < since_ms.
fn nsnap_stale_since(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsnap_stale_since", span)?;
    let bytes = bytes_arg(args, 0, "nsnap_stale_since", span)?;
    let since = int_arg(args, 1, "nsnap_stale_since", span)?;
    let header = match parse_snapshot(&bytes) {
        Ok(h) => h,
        Err(msg) => return Ok(nsnap_format_err(span, msg)),
    };
    Ok(Value::Bool(header.created_ms < since).ref_cell())
}

fn nsnap_stale_hash(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nsnap_stale_hash", span)?;
    let bytes = bytes_arg(args, 0, "nsnap_stale_hash", span)?;
    let expected = string_arg(args, 1, "nsnap_stale_hash", span)?;
    let header = match parse_snapshot(&bytes) {
        Ok(h) => h,
        Err(msg) => return Ok(nsnap_format_err(span, msg)),
    };
    let actual = hex::encode(&header.fingerprint);
    Ok(Value::Bool(actual != expected).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nsnap_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nsnap_fns![
    ("nsnap_capture", "capture", nsnap_capture),
    ("nsnap_restore", "restore", nsnap_restore),
    ("nsnap_info", "info", nsnap_info),
    ("nsnap_validate", "validate", nsnap_validate),
    ("nsnap_fingerprint", "fingerprint", nsnap_fingerprint),
    ("nsnap_fingerprint_bytes", "fingerprint_bytes", nsnap_fingerprint_bytes),
    ("nsnap_stale", "stale", nsnap_stale),
    ("nsnap_stale_since", "stale_since", nsnap_stale_since),
    ("nsnap_stale_hash", "stale_hash", nsnap_stale_hash),
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

pub const MODULE_NAME: &str = "nsnap";
pub const MODULE_PATHS: &[&str] = &["nsnap", "std/nsnap"];

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

    fn sample() -> ValueRef {
        let mut obj = HashMap::new();
        obj.insert("n".to_string(), Value::Int(42).ref_cell());
        obj.insert("tags".to_string(), Value::StringArray(crate::StringArray::dense(vec!["a".into()])).ref_cell());
        Value::Object(obj).ref_cell()
    }

    #[test]
    fn roundtrip_capture_restore() {
        let v = sample();
        let snap = nsnap_capture(&[v.clone()], span()).unwrap();
        let restored = nsnap_restore(&[snap], span()).unwrap();
        assert!(crate::values_equal(&v.borrow(), &restored.borrow()));
    }

    #[test]
    fn stale_detects_change() {
        let v = sample();
        let snap = nsnap_capture(&[v], span()).unwrap();
        let changed = Value::Int(99).ref_cell();
        let stale = nsnap_stale(&[snap.clone(), changed], span()).unwrap();
        assert!(matches!(&*stale.borrow(), Value::Bool(true)));
        let fresh = nsnap_stale(&[snap, sample()], span()).unwrap();
        assert!(matches!(&*fresh.borrow(), Value::Bool(false)));
    }

    #[test]
    fn validate_rejects_garbage() {
        let bad = Value::ByteArray(b"not a snap".to_vec()).ref_cell();
        let ok = nsnap_validate(&[bad], span()).unwrap();
        assert!(matches!(&*ok.borrow(), Value::Bool(false)));
    }

    #[test]
    fn fingerprint_matches_bytes() {
        let v = sample();
        let snap = nsnap_capture(&[v.clone()], span()).unwrap();
        let fp1 = nsnap_fingerprint(&[v], span()).unwrap();
        let fp2 = nsnap_fingerprint_bytes(&[snap], span()).unwrap();
        let r1 = fp1.borrow();
        let r2 = fp2.borrow();
        match (&*r1, &*r2) {
            (Value::String(a), Value::String(b)) => assert_eq!(a, b),
            _ => panic!("expected strings"),
        }
    }
}
