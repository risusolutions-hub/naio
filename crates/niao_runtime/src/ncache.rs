//! Native ncache standard library — in-memory LRU and TTL caches with
//! hit/miss statistics. O(log n) touch/evict via a BTreeMap recency index,
//! lazy TTL expiry (no background threads), string keys, any Niao value.
//!
//! Import with `import "ncache"` (or `import "std/ncache"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Cache model
// ---------------------------------------------------------------------------

struct Entry {
    value: ValueRef,
    tick: u64,
    expires_at_ms: Option<i64>,
}

struct Cache {
    /// 0 = unbounded (TTL caches may cap independently).
    capacity: usize,
    default_ttl_ms: Option<i64>,
    map: HashMap<String, Entry>,
    /// recency tick → key, oldest first.
    recency: BTreeMap<u64, String>,
    tick: u64,
    hits: u64,
    misses: u64,
}

impl Cache {
    fn new(capacity: usize, default_ttl_ms: Option<i64>) -> Self {
        Cache {
            capacity,
            default_ttl_ms,
            map: HashMap::new(),
            recency: BTreeMap::new(),
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

    fn remove_key(&mut self, key: &str) -> Option<ValueRef> {
        if let Some(entry) = self.map.remove(key) {
            self.recency.remove(&entry.tick);
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
                self.map.remove(&key);
            }
        }
    }

    fn set(&mut self, key: String, value: ValueRef, ttl_ms: Option<i64>, now_ms: i64) {
        if let Some(old) = self.map.get(&key) {
            self.recency.remove(&old.tick);
        }
        let tick = self.next_tick();
        let expires_at_ms = ttl_ms.or(self.default_ttl_ms).map(|t| now_ms + t);
        self.recency.insert(tick, key.clone());
        self.map.insert(
            key,
            Entry {
                value,
                tick,
                expires_at_ms,
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

    fn purge(&mut self, now_ms: i64) -> usize {
        let expired: Vec<String> = self
            .map
            .iter()
            .filter(|(_, e)| Self::is_expired(e, now_ms))
            .map(|(k, _)| k.clone())
            .collect();
        let n = expired.len();
        for key in expired {
            self.remove_key(&key);
        }
        n
    }
}

thread_local! {
    static CACHES: RefCell<HashMap<i64, Cache>> = RefCell::new(HashMap::new());
    static NEXT_HANDLE: RefCell<i64> = const { RefCell::new(1) };
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn with_cache<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut Cache) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    CACHES.with(|caches| {
        let mut caches = caches.borrow_mut();
        match caches.get_mut(&id) {
            Some(c) => Ok(Ok(f(c))),
            None => Ok(Err(error_value(
                codes::E2672_NCACHE_INVALID_HANDLE,
                "ncache_error",
                format!("invalid or closed cache handle {id}"),
                span,
            ))),
        }
    })
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::TypeError {
        message: msg.into(),
        line: span.line,
        col: span.col,
    }
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E2670_NCACHE_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E2670_NCACHE_ARITY,
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
                "{name}() expects a string key as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn ncache_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E2671_NCACHE_ERROR, "ncache_error", msg.into(), span)
}

fn new_handle() -> i64 {
    NEXT_HANDLE.with(|h| {
        let mut h = h.borrow_mut();
        let id = *h;
        *h += 1;
        id
    })
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn ncache_new_lru(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncache_new_lru", span)?;
    let capacity = int_arg(args, 0, "ncache_new_lru", span)?;
    if capacity <= 0 {
        return Ok(ncache_err(span, "ncache_new_lru() capacity must be >= 1"));
    }
    let id = new_handle();
    CACHES.with(|caches| {
        caches
            .borrow_mut()
            .insert(id, Cache::new(capacity as usize, None));
    });
    Ok(Value::Int(id).ref_cell())
}

/// ncache_new_ttl(default_ttl_ms, max_size = unbounded)
fn ncache_new_ttl(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ncache_new_ttl", span)?;
    let ttl = int_arg(args, 0, "ncache_new_ttl", span)?;
    if ttl <= 0 {
        return Ok(ncache_err(span, "ncache_new_ttl() ttl_ms must be >= 1"));
    }
    let capacity = if args.len() > 1 {
        let c = int_arg(args, 1, "ncache_new_ttl", span)?;
        if c < 0 {
            return Ok(ncache_err(span, "ncache_new_ttl() max_size must be >= 0"));
        }
        c as usize
    } else {
        0
    };
    let id = new_handle();
    CACHES.with(|caches| {
        caches
            .borrow_mut()
            .insert(id, Cache::new(capacity, Some(ttl)));
    });
    Ok(Value::Int(id).ref_cell())
}

fn ncache_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncache_close", span)?;
    let id = int_arg(args, 0, "ncache_close", span)?;
    let removed = CACHES.with(|caches| caches.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

/// ncache_set(handle, key, value, ttl_ms?)
fn ncache_set(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 3, 4, "ncache_set", span)?;
    let id = int_arg(args, 0, "ncache_set", span)?;
    let key = string_arg(args, 1, "ncache_set", span)?;
    let value = Rc::clone(&args[2]);
    let ttl = if args.len() > 3 {
        let t = int_arg(args, 3, "ncache_set", span)?;
        if t <= 0 {
            return Ok(ncache_err(span, "ncache_set() ttl_ms must be >= 1"));
        }
        Some(t)
    } else {
        None
    };
    let now = now_ms();
    match with_cache(id, span, |c| c.set(key, value, ttl, now))? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn ncache_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncache_get", span)?;
    let id = int_arg(args, 0, "ncache_get", span)?;
    let key = string_arg(args, 1, "ncache_get", span)?;
    let now = now_ms();
    match with_cache(id, span, |c| c.get(&key, now))? {
        Ok(Some(v)) => Ok(v),
        Ok(None) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

/// ncache_get_or(handle, key, fallback) — fallback returned (not stored) on miss.
fn ncache_get_or(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "ncache_get_or", span)?;
    let id = int_arg(args, 0, "ncache_get_or", span)?;
    let key = string_arg(args, 1, "ncache_get_or", span)?;
    let now = now_ms();
    match with_cache(id, span, |c| c.get(&key, now))? {
        Ok(Some(v)) => Ok(v),
        Ok(None) => Ok(Rc::clone(&args[2])),
        Err(e) => Ok(e),
    }
}

fn ncache_has(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncache_has", span)?;
    let id = int_arg(args, 0, "ncache_has", span)?;
    let key = string_arg(args, 1, "ncache_has", span)?;
    let now = now_ms();
    // Non-counting peek: does not affect hit/miss stats or recency.
    let result = with_cache(id, span, |c| {
        match c.map.get(&key) {
            Some(entry) => !Cache::is_expired(entry, now),
            None => false,
        }
    })?;
    match result {
        Ok(b) => Ok(Value::Bool(b).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn ncache_remove(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ncache_remove", span)?;
    let id = int_arg(args, 0, "ncache_remove", span)?;
    let key = string_arg(args, 1, "ncache_remove", span)?;
    match with_cache(id, span, |c| c.remove_key(&key).is_some())? {
        Ok(b) => Ok(Value::Bool(b).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn ncache_clear(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncache_clear", span)?;
    let id = int_arg(args, 0, "ncache_clear", span)?;
    match with_cache(id, span, |c| {
        c.map.clear();
        c.recency.clear();
    })? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

fn ncache_len(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncache_len", span)?;
    let id = int_arg(args, 0, "ncache_len", span)?;
    match with_cache(id, span, |c| c.map.len() as i64)? {
        Ok(n) => Ok(Value::Int(n).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn ncache_keys(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncache_keys", span)?;
    let id = int_arg(args, 0, "ncache_keys", span)?;
    match with_cache(id, span, |c| {
        c.map
            .keys()
            .map(|k| Value::String(k.clone()).ref_cell())
            .collect::<Vec<ValueRef>>()
    })? {
        Ok(keys) => Ok(Value::Array(keys).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn ncache_purge(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncache_purge", span)?;
    let id = int_arg(args, 0, "ncache_purge", span)?;
    let now = now_ms();
    match with_cache(id, span, |c| c.purge(now) as i64)? {
        Ok(n) => Ok(Value::Int(n).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn ncache_stats(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ncache_stats", span)?;
    let id = int_arg(args, 0, "ncache_stats", span)?;
    match with_cache(id, span, |c| {
        (c.hits, c.misses, c.map.len(), c.capacity)
    })? {
        Ok((hits, misses, len, capacity)) => {
            let mut map = HashMap::new();
            map.insert("hits".to_string(), Value::Int(hits as i64).ref_cell());
            map.insert("misses".to_string(), Value::Int(misses as i64).ref_cell());
            map.insert("len".to_string(), Value::Int(len as i64).ref_cell());
            map.insert("capacity".to_string(), Value::Int(capacity as i64).ref_cell());
            let total = hits + misses;
            let rate = if total == 0 {
                0.0
            } else {
                hits as f64 / total as f64
            };
            map.insert("hit_rate".to_string(), Value::Float(rate).ref_cell());
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(e),
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ncache_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ncache_fns![
    ("ncache_new_lru", "new_lru", ncache_new_lru),
    ("ncache_new_ttl", "new_ttl", ncache_new_ttl),
    ("ncache_close", "close", ncache_close),
    ("ncache_set", "set", ncache_set),
    ("ncache_get", "get", ncache_get),
    ("ncache_get_or", "get_or", ncache_get_or),
    ("ncache_has", "has", ncache_has),
    ("ncache_remove", "remove", ncache_remove),
    ("ncache_clear", "clear", ncache_clear),
    ("ncache_len", "len", ncache_len),
    ("ncache_keys", "keys", ncache_keys),
    ("ncache_purge", "purge", ncache_purge),
    ("ncache_stats", "stats", ncache_stats),
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

pub const MODULE_NAME: &str = "ncache";
pub const MODULE_PATHS: &[&str] = &["ncache", "std/ncache"];

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
    fn lru_evicts_least_recent() {
        let h = handle(ncache_new_lru(&[i(2)], span()));
        ncache_set(&[h.clone(), s("a"), i(1)], span()).unwrap();
        ncache_set(&[h.clone(), s("b"), i(2)], span()).unwrap();
        // touch "a" so "b" becomes least recently used
        ncache_get(&[h.clone(), s("a")], span()).unwrap();
        ncache_set(&[h.clone(), s("c"), i(3)], span()).unwrap();
        let b = ncache_get(&[h.clone(), s("b")], span()).unwrap();
        assert!(matches!(&*b.borrow(), Value::Nil));
        let a = ncache_get(&[h.clone(), s("a")], span()).unwrap();
        assert!(matches!(&*a.borrow(), Value::Int(1)));
        ncache_close(&[h], span()).unwrap();
    }

    #[test]
    fn ttl_expires() {
        let h = handle(ncache_new_ttl(&[i(10_000)], span()));
        ncache_set(&[h.clone(), s("k"), i(42), i(1)], span()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let v = ncache_get(&[h.clone(), s("k")], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Nil));
        ncache_close(&[h], span()).unwrap();
    }

    #[test]
    fn stats_track_hits_misses() {
        let h = handle(ncache_new_lru(&[i(4)], span()));
        ncache_set(&[h.clone(), s("x"), i(1)], span()).unwrap();
        ncache_get(&[h.clone(), s("x")], span()).unwrap();
        ncache_get(&[h.clone(), s("missing")], span()).unwrap();
        let stats = ncache_stats(&[h.clone()], span()).unwrap();
        match &*stats.borrow() {
            Value::Object(map) => {
                assert!(matches!(&*map.get("hits").unwrap().borrow(), Value::Int(1)));
                assert!(matches!(&*map.get("misses").unwrap().borrow(), Value::Int(1)));
            }
            other => panic!("expected object, got {other:?}"),
        }
        ncache_close(&[h], span()).unwrap();
    }

    #[test]
    fn invalid_handle_error_value() {
        let v = ncache_get(&[i(424_242), s("k")], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }
}
