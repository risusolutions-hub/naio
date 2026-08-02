//! Native nmem standard library — script long-term memory with KV storage,
//! optional capacity, lazy TTL expiry, and tags. Handle-based like ncache.
//!
//! Import with `import "nmem"` (or `import "std/nmem"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

// Wired in codes.rs by central integration.
const E3050_NMEM_ARITY: u32 = 3050;
const E3051_NMEM_ERROR: u32 = 3051;
const E3052_NMEM_TYPE: u32 = 3052;
const E3053_NMEM_INVALID_HANDLE: u32 = 3053;

// ---------------------------------------------------------------------------
// Memory model
// ---------------------------------------------------------------------------

struct Entry {
    value: ValueRef,
    tick: u64,
    expires_at_ms: Option<i64>,
    tags: HashSet<String>,
}

struct Memory {
    /// 0 = unbounded.
    capacity: usize,
    map: HashMap<String, Entry>,
    /// recency tick → key, oldest first (LRU eviction).
    recency: BTreeMap<u64, String>,
    /// tag → keys that carry it.
    tag_index: HashMap<String, HashSet<String>>,
    tick: u64,
    hits: u64,
    misses: u64,
}

impl Memory {
    fn new(capacity: usize) -> Self {
        Memory {
            capacity,
            map: HashMap::new(),
            recency: BTreeMap::new(),
            tag_index: HashMap::new(),
            tick: 0,
            hits: 0,
            misses: 0,
        }
    }

    fn next_tick(&mut self) -> u64 {
        self.tick += 1;
        self.tick
    }

    fn touch(&mut self, key: &str) {
        let tick = self.next_tick();
        if let Some(entry) = self.map.get_mut(key) {
            self.recency.remove(&entry.tick);
            entry.tick = tick;
            self.recency.insert(tick, key.to_string());
        }
    }

    fn is_expired(entry: &Entry, now_ms: i64) -> bool {
        matches!(entry.expires_at_ms, Some(exp) if now_ms >= exp)
    }

    fn unindex_tags(&mut self, key: &str, tags: &HashSet<String>) {
        for tag in tags {
            if let Some(set) = self.tag_index.get_mut(tag) {
                set.remove(key);
                if set.is_empty() {
                    self.tag_index.remove(tag);
                }
            }
        }
    }

    fn remove_key(&mut self, key: &str) -> Option<ValueRef> {
        if let Some(entry) = self.map.remove(key) {
            self.recency.remove(&entry.tick);
            self.unindex_tags(key, &entry.tags);
            Some(entry.value)
        } else {
            None
        }
    }

    fn evict_to_capacity(&mut self) {
        if self.capacity == 0 {
            return;
        }
        while self.map.len() > self.capacity {
            let Some((&oldest_tick, _)) = self.recency.iter().next() else {
                break;
            };
            if let Some(key) = self.recency.remove(&oldest_tick) {
                if let Some(entry) = self.map.remove(&key) {
                    self.unindex_tags(&key, &entry.tags);
                }
            }
        }
    }

    fn set(&mut self, key: String, value: ValueRef, ttl_ms: Option<i64>, now_ms: i64) {
        let tags = if let Some(old) = self.map.get(&key) {
            self.recency.remove(&old.tick);
            old.tags.clone()
        } else {
            HashSet::new()
        };
        let tick = self.next_tick();
        let expires_at_ms = ttl_ms.map(|t| now_ms + t);
        self.recency.insert(tick, key.clone());
        self.map.insert(
            key,
            Entry {
                value,
                tick,
                expires_at_ms,
                tags,
            },
        );
        self.evict_to_capacity();
    }

    fn get(&mut self, key: &str, now_ms: i64) -> Option<ValueRef> {
        let expired = match self.map.get(key) {
            Some(entry) => Self::is_expired(entry, now_ms),
            None => {
                self.misses += 1;
                return None;
            }
        };
        if expired {
            self.remove_key(key);
            self.misses += 1;
            return None;
        }
        self.hits += 1;
        self.touch(key);
        self.map.get(key).map(|e| Rc::clone(&e.value))
    }

    fn has(&mut self, key: &str, now_ms: i64) -> bool {
        match self.map.get(key) {
            Some(entry) if Self::is_expired(entry, now_ms) => {
                self.remove_key(key);
                false
            }
            Some(_) => true,
            None => false,
        }
    }

    fn tag(&mut self, key: &str, tag: String, now_ms: i64) -> Result<(), ()> {
        match self.map.get(key) {
            Some(entry) if Self::is_expired(entry, now_ms) => {
                self.remove_key(key);
                Err(())
            }
            Some(_) => {
                if let Some(entry) = self.map.get_mut(key) {
                    entry.tags.insert(tag.clone());
                }
                self.tag_index
                    .entry(tag)
                    .or_default()
                    .insert(key.to_string());
                Ok(())
            }
            None => Err(()),
        }
    }

    fn by_tag(&mut self, tag: &str, now_ms: i64) -> Vec<String> {
        let candidates: Vec<String> = self
            .tag_index
            .get(tag)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default();
        let mut out = Vec::new();
        for key in candidates {
            match self.map.get(&key) {
                Some(entry) if Self::is_expired(entry, now_ms) => {
                    self.remove_key(&key);
                }
                Some(_) => out.push(key),
                None => {
                    if let Some(set) = self.tag_index.get_mut(tag) {
                        set.remove(&key);
                    }
                }
            }
        }
        out.sort();
        out
    }

    fn search(&mut self, substr: &str, now_ms: i64) -> Vec<String> {
        let keys: Vec<String> = self.map.keys().cloned().collect();
        let mut out = Vec::new();
        for key in keys {
            match self.map.get(&key) {
                Some(entry) if Self::is_expired(entry, now_ms) => {
                    self.remove_key(&key);
                }
                Some(_) if key.contains(substr) => out.push(key),
                _ => {}
            }
        }
        out.sort();
        out
    }

    fn export(&mut self, now_ms: i64) -> HashMap<String, ValueRef> {
        let keys: Vec<String> = self.map.keys().cloned().collect();
        let mut out = HashMap::new();
        for key in keys {
            match self.map.get(&key) {
                Some(entry) if Self::is_expired(entry, now_ms) => {
                    self.remove_key(&key);
                }
                Some(entry) => {
                    out.insert(key, Rc::clone(&entry.value));
                }
                None => {}
            }
        }
        out
    }

    fn import(&mut self, obj: &HashMap<String, ValueRef>, now_ms: i64) {
        for (k, v) in obj {
            self.set(k.clone(), Rc::clone(v), None, now_ms);
        }
    }

    fn clear(&mut self) {
        self.map.clear();
        self.recency.clear();
        self.tag_index.clear();
    }
}

thread_local! {
    static MEMORIES: RefCell<HashMap<i64, Memory>> = RefCell::new(HashMap::new());
    static NEXT_HANDLE: RefCell<i64> = const { RefCell::new(1) };
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn with_mem<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut Memory) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    MEMORIES.with(|mems| {
        let mut mems = mems.borrow_mut();
        match mems.get_mut(&id) {
            Some(m) => Ok(Ok(f(m))),
            None => Ok(Err(error_value(
                E3053_NMEM_INVALID_HANDLE,
                "nmem_error",
                format!("invalid or closed memory handle {id}"),
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
            E3050_NMEM_ARITY,
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
            E3050_NMEM_ARITY,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn int_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(RuntimeError::at(
            span,
            E3052_NMEM_TYPE,
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
        other => Err(RuntimeError::at(
            span,
            E3052_NMEM_TYPE,
            format!(
                "{name}() expects a string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn object_arg(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> NiaoResult<HashMap<String, ValueRef>> {
    match &*args[idx].borrow() {
        Value::Object(map) => Ok(map.iter().map(|(k, v)| (k.clone(), Rc::clone(v))).collect()),
        other => Err(RuntimeError::at(
            span,
            E3052_NMEM_TYPE,
            format!(
                "{name}() expects an object as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn nmem_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3051_NMEM_ERROR, "nmem_error", msg.into(), span)
}

fn new_handle() -> i64 {
    NEXT_HANDLE.with(|h| {
        let mut h = h.borrow_mut();
        let id = *h;
        *h += 1;
        id
    })
}

fn keys_to_array(keys: Vec<String>) -> ValueRef {
    Value::Array(
        keys.into_iter()
            .map(|k| Value::String(k).ref_cell())
            .collect(),
    )
    .ref_cell()
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// nmem_new(capacity?) — capacity 0 / omitted = unbounded; otherwise max entries (LRU).
fn nmem_new(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nmem_new", span)?;
    let capacity = if args.is_empty() {
        0usize
    } else {
        let c = int_arg(args, 0, "nmem_new", span)?;
        if c < 0 {
            return Ok(nmem_err(span, "nmem_new() capacity must be >= 0"));
        }
        c as usize
    };
    let id = new_handle();
    MEMORIES.with(|mems| {
        mems.borrow_mut().insert(id, Memory::new(capacity));
    });
    Ok(Value::Int(id).ref_cell())
}

fn nmem_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmem_close", span)?;
    let id = int_arg(args, 0, "nmem_close", span)?;
    let removed = MEMORIES.with(|mems| mems.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

/// nmem_set(handle, key, value, ttl_ms?)
fn nmem_set(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "nmem_set", span)?;
    let id = int_arg(args, 0, "nmem_set", span)?;
    let key = string_arg(args, 1, "nmem_set", span)?;
    let value = Rc::clone(&args[2]);
    let ttl = if args.len() > 3 {
        let t = int_arg(args, 3, "nmem_set", span)?;
        if t <= 0 {
            return Ok(nmem_err(span, "nmem_set() ttl_ms must be >= 1"));
        }
        Some(t)
    } else {
        None
    };
    let now = now_ms();
    match with_mem(id, span, |m| m.set(key, value, ttl, now))? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nmem_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmem_get", span)?;
    let id = int_arg(args, 0, "nmem_get", span)?;
    let key = string_arg(args, 1, "nmem_get", span)?;
    let now = now_ms();
    match with_mem(id, span, |m| m.get(&key, now))? {
        Ok(Some(v)) => Ok(v),
        Ok(None) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nmem_has(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmem_has", span)?;
    let id = int_arg(args, 0, "nmem_has", span)?;
    let key = string_arg(args, 1, "nmem_has", span)?;
    let now = now_ms();
    match with_mem(id, span, |m| m.has(&key, now))? {
        Ok(b) => Ok(Value::Bool(b).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nmem_remove(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmem_remove", span)?;
    let id = int_arg(args, 0, "nmem_remove", span)?;
    let key = string_arg(args, 1, "nmem_remove", span)?;
    match with_mem(id, span, |m| m.remove_key(&key).is_some())? {
        Ok(b) => Ok(Value::Bool(b).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nmem_clear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmem_clear", span)?;
    let id = int_arg(args, 0, "nmem_clear", span)?;
    match with_mem(id, span, |m| m.clear())? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nmem_tag(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nmem_tag", span)?;
    let id = int_arg(args, 0, "nmem_tag", span)?;
    let key = string_arg(args, 1, "nmem_tag", span)?;
    let tag = string_arg(args, 2, "nmem_tag", span)?;
    let now = now_ms();
    match with_mem(id, span, |m| m.tag(&key, tag, now))? {
        Ok(Ok(())) => Ok(Value::Bool(true).ref_cell()),
        Ok(Err(())) => Ok(nmem_err(span, format!("nmem_tag() key not found: {key}"))),
        Err(e) => Ok(e),
    }
}

fn nmem_by_tag(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmem_by_tag", span)?;
    let id = int_arg(args, 0, "nmem_by_tag", span)?;
    let tag = string_arg(args, 1, "nmem_by_tag", span)?;
    let now = now_ms();
    match with_mem(id, span, |m| m.by_tag(&tag, now))? {
        Ok(keys) => Ok(keys_to_array(keys)),
        Err(e) => Ok(e),
    }
}

fn nmem_search(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmem_search", span)?;
    let id = int_arg(args, 0, "nmem_search", span)?;
    let substr = string_arg(args, 1, "nmem_search", span)?;
    let now = now_ms();
    match with_mem(id, span, |m| m.search(&substr, now))? {
        Ok(keys) => Ok(keys_to_array(keys)),
        Err(e) => Ok(e),
    }
}

fn nmem_stats(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmem_stats", span)?;
    let id = int_arg(args, 0, "nmem_stats", span)?;
    match with_mem(id, span, |m| (m.map.len(), m.capacity, m.hits, m.misses))? {
        Ok((len, capacity, hits, misses)) => {
            let mut map = HashMap::new();
            map.insert("len".to_string(), Value::Int(len as i64).ref_cell());
            map.insert(
                "capacity".to_string(),
                Value::Int(capacity as i64).ref_cell(),
            );
            map.insert("hits".to_string(), Value::Int(hits as i64).ref_cell());
            map.insert("misses".to_string(), Value::Int(misses as i64).ref_cell());
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(e),
    }
}

fn nmem_export(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmem_export", span)?;
    let id = int_arg(args, 0, "nmem_export", span)?;
    let now = now_ms();
    match with_mem(id, span, |m| m.export(now))? {
        Ok(obj) => Ok(Value::Object(obj).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nmem_import(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nmem_import", span)?;
    let id = int_arg(args, 0, "nmem_import", span)?;
    let imported = object_arg(args, 1, "nmem_import", span)?;
    let now = now_ms();
    match with_mem(id, span, |m| m.import(&imported, now))? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn nmem_len(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmem_len", span)?;
    let id = int_arg(args, 0, "nmem_len", span)?;
    match with_mem(id, span, |m| m.map.len() as i64)? {
        Ok(n) => Ok(Value::Int(n).ref_cell()),
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nmem_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nmem_fns![
    ("nmem_new", "new", nmem_new),
    ("nmem_set", "set", nmem_set),
    ("nmem_get", "get", nmem_get),
    ("nmem_has", "has", nmem_has),
    ("nmem_remove", "remove", nmem_remove),
    ("nmem_clear", "clear", nmem_clear),
    ("nmem_tag", "tag", nmem_tag),
    ("nmem_by_tag", "by_tag", nmem_by_tag),
    ("nmem_search", "search", nmem_search),
    ("nmem_stats", "stats", nmem_stats),
    ("nmem_export", "export", nmem_export),
    ("nmem_import", "import", nmem_import),
    ("nmem_close", "close", nmem_close),
    ("nmem_len", "len", nmem_len),
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

pub const MODULE_NAME: &str = "nmem";
pub const MODULE_PATHS: &[&str] = &["nmem", "std/nmem"];

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

    #[test]
    fn set_get_has_remove() {
        let h = handle(nmem_new(&[], span()));
        nmem_set(&[h.clone(), s("a"), i(1)], span()).unwrap();
        let v = nmem_get(&[h.clone(), s("a")], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Int(1)));
        let has = nmem_has(&[h.clone(), s("a")], span()).unwrap();
        assert!(matches!(&*has.borrow(), Value::Bool(true)));
        let rem = nmem_remove(&[h.clone(), s("a")], span()).unwrap();
        assert!(matches!(&*rem.borrow(), Value::Bool(true)));
        let miss = nmem_get(&[h.clone(), s("a")], span()).unwrap();
        assert!(matches!(&*miss.borrow(), Value::Nil));
        nmem_close(&[h], span()).unwrap();
    }

    #[test]
    fn capacity_evicts_lru() {
        let h = handle(nmem_new(&[i(2)], span()));
        nmem_set(&[h.clone(), s("a"), i(1)], span()).unwrap();
        nmem_set(&[h.clone(), s("b"), i(2)], span()).unwrap();
        nmem_get(&[h.clone(), s("a")], span()).unwrap();
        nmem_set(&[h.clone(), s("c"), i(3)], span()).unwrap();
        let b = nmem_get(&[h.clone(), s("b")], span()).unwrap();
        assert!(matches!(&*b.borrow(), Value::Nil));
        let a = nmem_get(&[h.clone(), s("a")], span()).unwrap();
        assert!(matches!(&*a.borrow(), Value::Int(1)));
        nmem_close(&[h], span()).unwrap();
    }

    #[test]
    fn ttl_expires_lazily() {
        let h = handle(nmem_new(&[], span()));
        nmem_set(&[h.clone(), s("k"), i(42), i(1)], span()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let v = nmem_get(&[h.clone(), s("k")], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Nil));
        nmem_close(&[h], span()).unwrap();
    }

    #[test]
    fn tag_and_by_tag() {
        let h = handle(nmem_new(&[], span()));
        nmem_set(&[h.clone(), s("user:1"), s("alice")], span()).unwrap();
        nmem_set(&[h.clone(), s("user:2"), s("bob")], span()).unwrap();
        nmem_set(&[h.clone(), s("note:1"), s("x")], span()).unwrap();
        nmem_tag(&[h.clone(), s("user:1"), s("people")], span()).unwrap();
        nmem_tag(&[h.clone(), s("user:2"), s("people")], span()).unwrap();
        let keys = nmem_by_tag(&[h.clone(), s("people")], span()).unwrap();
        match &*keys.borrow() {
            Value::Array(arr) => {
                assert_eq!(arr.len(), 2);
            }
            other => panic!("expected array, got {other:?}"),
        }
        nmem_close(&[h], span()).unwrap();
    }

    #[test]
    fn search_substring() {
        let h = handle(nmem_new(&[], span()));
        nmem_set(&[h.clone(), s("user:1"), i(1)], span()).unwrap();
        nmem_set(&[h.clone(), s("user:2"), i(2)], span()).unwrap();
        nmem_set(&[h.clone(), s("sess:1"), i(3)], span()).unwrap();
        let keys = nmem_search(&[h.clone(), s("user:")], span()).unwrap();
        match &*keys.borrow() {
            Value::Array(arr) => assert_eq!(arr.len(), 2),
            other => panic!("expected array, got {other:?}"),
        }
        nmem_close(&[h], span()).unwrap();
    }

    #[test]
    fn stats_hits_misses() {
        let h = handle(nmem_new(&[i(4)], span()));
        nmem_set(&[h.clone(), s("x"), i(1)], span()).unwrap();
        nmem_get(&[h.clone(), s("x")], span()).unwrap();
        nmem_get(&[h.clone(), s("missing")], span()).unwrap();
        let stats = nmem_stats(&[h.clone()], span()).unwrap();
        match &*stats.borrow() {
            Value::Object(map) => {
                assert!(matches!(&*map.get("hits").unwrap().borrow(), Value::Int(1)));
                assert!(matches!(
                    &*map.get("misses").unwrap().borrow(),
                    Value::Int(1)
                ));
                assert!(matches!(&*map.get("len").unwrap().borrow(), Value::Int(1)));
                assert!(matches!(
                    &*map.get("capacity").unwrap().borrow(),
                    Value::Int(4)
                ));
            }
            other => panic!("expected object, got {other:?}"),
        }
        nmem_close(&[h], span()).unwrap();
    }

    #[test]
    fn export_import_roundtrip() {
        let h = handle(nmem_new(&[], span()));
        nmem_set(&[h.clone(), s("a"), i(1)], span()).unwrap();
        nmem_set(&[h.clone(), s("b"), s("hello")], span()).unwrap();
        let exported = nmem_export(&[h.clone()], span()).unwrap();
        let h2 = handle(nmem_new(&[], span()));
        nmem_import(&[h2.clone(), exported], span()).unwrap();
        let a = nmem_get(&[h2.clone(), s("a")], span()).unwrap();
        assert!(matches!(&*a.borrow(), Value::Int(1)));
        let b = nmem_get(&[h2.clone(), s("b")], span()).unwrap();
        assert!(matches!(&*b.borrow(), Value::String(x) if x == "hello"));
        nmem_close(&[h], span()).unwrap();
        nmem_close(&[h2], span()).unwrap();
    }

    #[test]
    fn clear_and_len() {
        let h = handle(nmem_new(&[], span()));
        nmem_set(&[h.clone(), s("a"), i(1)], span()).unwrap();
        nmem_set(&[h.clone(), s("b"), i(2)], span()).unwrap();
        let n = nmem_len(&[h.clone()], span()).unwrap();
        assert!(matches!(&*n.borrow(), Value::Int(2)));
        nmem_clear(&[h.clone()], span()).unwrap();
        let n0 = nmem_len(&[h.clone()], span()).unwrap();
        assert!(matches!(&*n0.borrow(), Value::Int(0)));
        nmem_close(&[h], span()).unwrap();
    }

    #[test]
    fn invalid_handle_error_value() {
        let v = nmem_get(&[i(424_242), s("k")], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn type_error_on_bad_key() {
        let h = handle(nmem_new(&[], span()));
        let err = nmem_get(&[h.clone(), i(1)], span());
        assert!(err.is_err());
        assert_eq!(err.unwrap_err().code(), E3052_NMEM_TYPE);
        nmem_close(&[h], span()).unwrap();
    }

    #[test]
    fn arity_error() {
        let err = nmem_get(&[], span());
        assert!(err.is_err());
        assert_eq!(err.unwrap_err().code(), E3050_NMEM_ARITY);
    }
}
