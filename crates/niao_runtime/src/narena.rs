//! Native narena standard library — pooled packed-buffer reuse arena.
//! Allocates `ByteArray` buffers from a thread-local pool to reduce allocation
//! churn; `recycle` returns buffers and `reset` clears outstanding borrows.
//!
//! Import with `import "narena"` (or `import "std/narena"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// Wired in codes.rs by central integration.
const E3370_NARENA_ARITY: u32 = 3370;
const E3371_NARENA_ERROR: u32 = 3371;
const E3372_NARENA_TYPE: u32 = 3372;
const E3373_NARENA_INVALID_HANDLE: u32 = 3373;

// ---------------------------------------------------------------------------
// Arena model
// ---------------------------------------------------------------------------

struct Arena {
    /// Preferred capacity for new/recycled buffers.
    block_size: usize,
    /// Maximum buffers kept in the free pool.
    pool_cap: usize,
    free: Vec<Vec<u8>>,
    outstanding: usize,
    total_allocated: u64,
    total_recycled: u64,
    reset_count: u64,
}

impl Arena {
    fn new(block_size: usize, pool_cap: usize) -> Self {
        Arena {
            block_size: block_size.max(1),
            pool_cap: pool_cap.max(1),
            free: Vec::new(),
            outstanding: 0,
            total_allocated: 0,
            total_recycled: 0,
            reset_count: 0,
        }
    }

    fn alloc(&mut self, size: usize) -> Vec<u8> {
        let need = size.max(self.block_size);
        let mut buf = self.free.pop().unwrap_or_else(|| Vec::with_capacity(need));
        if buf.capacity() < need {
            buf.reserve(need - buf.capacity());
        }
        buf.clear();
        buf.resize(need, 0);
        self.outstanding += 1;
        self.total_allocated += 1;
        buf
    }

    fn recycle(&mut self, mut buf: Vec<u8>) {
        buf.clear();
        if self.free.len() < self.pool_cap {
            self.free.push(buf);
        }
        if self.outstanding > 0 {
            self.outstanding -= 1;
        }
        self.total_recycled += 1;
    }

    fn reset(&mut self) {
        self.outstanding = 0;
        self.reset_count += 1;
    }
}

thread_local! {
    static ARENAS: RefCell<HashMap<i64, Arena>> = RefCell::new(HashMap::new());
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

fn with_arena<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&mut Arena) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    ARENAS.with(|arenas| {
        let mut arenas = arenas.borrow_mut();
        match arenas.get_mut(&id) {
            Some(a) => Ok(Ok(f(a))),
            None => Ok(Err(error_value(
                E3373_NARENA_INVALID_HANDLE,
                "narena_error",
                format!("invalid or closed arena handle {id}"),
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
            E3370_NARENA_ARITY,
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
            E3370_NARENA_ARITY,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E3372_NARENA_TYPE, msg.into())
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

fn narena_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E3371_NARENA_ERROR, "narena_error", msg.into(), span)
}

fn byte_array_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<u8>> {
    match &*args[idx].borrow() {
        Value::ByteArray(b) => Ok(b.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a byte_array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

/// narena_new(block_size?, pool_cap?) → handle
fn narena_new(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 2, "narena_new", span)?;
    let block_size = if args.is_empty() {
        4096
    } else {
        let n = int_arg(args, 0, "narena_new", span)?;
        if n <= 0 {
            return Ok(narena_err(span, "block_size must be > 0"));
        }
        n as usize
    };
    let pool_cap = if args.len() < 2 {
        16
    } else {
        let n = int_arg(args, 1, "narena_new", span)?;
        if n <= 0 {
            return Ok(narena_err(span, "pool_cap must be > 0"));
        }
        n as usize
    };
    let id = new_handle();
    ARENAS.with(|arenas| {
        arenas
            .borrow_mut()
            .insert(id, Arena::new(block_size, pool_cap));
    });
    Ok(Value::Int(id).ref_cell())
}

/// narena_alloc(handle, size) → byte_array
fn narena_alloc(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "narena_alloc", span)?;
    let id = int_arg(args, 0, "narena_alloc", span)?;
    let size = int_arg(args, 1, "narena_alloc", span)?;
    if size < 0 {
        return Ok(narena_err(span, "size must be >= 0"));
    }
    match with_arena(id, span, |a| a.alloc(size as usize))? {
        Ok(buf) => Ok(Value::ByteArray(buf).ref_cell()),
        Err(e) => Ok(e),
    }
}

/// narena_recycle(handle, byte_array) → nil
fn narena_recycle(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "narena_recycle", span)?;
    let id = int_arg(args, 0, "narena_recycle", span)?;
    let buf = byte_array_arg(args, 1, "narena_recycle", span)?;
    match with_arena(id, span, |a| a.recycle(buf))? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

/// narena_reset(handle) → nil
fn narena_reset(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "narena_reset", span)?;
    let id = int_arg(args, 0, "narena_reset", span)?;
    match with_arena(id, span, |a| a.reset())? {
        Ok(()) => Ok(Value::Nil.ref_cell()),
        Err(e) => Ok(e),
    }
}

/// narena_stats(handle) → object
fn narena_stats(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "narena_stats", span)?;
    let id = int_arg(args, 0, "narena_stats", span)?;
    match with_arena(id, span, |a| {
        (
            a.block_size,
            a.pool_cap,
            a.free.len(),
            a.outstanding,
            a.total_allocated,
            a.total_recycled,
            a.reset_count,
        )
    })? {
        Ok((block_size, pool_cap, pooled, outstanding, allocated, recycled, resets)) => {
            let mut map = HashMap::new();
            map.insert(
                "block_size".to_string(),
                Value::Int(block_size as i64).ref_cell(),
            );
            map.insert(
                "pool_cap".to_string(),
                Value::Int(pool_cap as i64).ref_cell(),
            );
            map.insert("pooled".to_string(), Value::Int(pooled as i64).ref_cell());
            map.insert(
                "outstanding".to_string(),
                Value::Int(outstanding as i64).ref_cell(),
            );
            map.insert(
                "total_allocated".to_string(),
                Value::Int(allocated as i64).ref_cell(),
            );
            map.insert(
                "total_recycled".to_string(),
                Value::Int(recycled as i64).ref_cell(),
            );
            map.insert(
                "reset_count".to_string(),
                Value::Int(resets as i64).ref_cell(),
            );
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(e),
    }
}

/// narena_close(handle) → bool
fn narena_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "narena_close", span)?;
    let id = int_arg(args, 0, "narena_close", span)?;
    let removed = ARENAS.with(|arenas| arenas.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! narena_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

narena_fns![
    ("narena_new", "new", narena_new),
    ("narena_alloc", "alloc", narena_alloc),
    ("narena_recycle", "recycle", narena_recycle),
    ("narena_reset", "reset", narena_reset),
    ("narena_stats", "stats", narena_stats),
    ("narena_close", "close", narena_close),
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

pub const MODULE_NAME: &str = "narena";
pub const MODULE_PATHS: &[&str] = &["narena", "std/narena"];

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

    fn handle(r: NiaoResult<ValueRef>) -> ValueRef {
        r.unwrap()
    }

    #[test]
    fn alloc_recycle_and_stats() {
        let h = handle(narena_new(&[i(64), i(4)], span()));
        let b1 = narena_alloc(&[h.clone(), i(32)], span()).unwrap();
        let b2 = narena_alloc(&[h.clone(), i(32)], span()).unwrap();
        assert!(matches!(&*b1.borrow(), Value::ByteArray(v) if v.len() == 32));
        let bytes = match &*b1.borrow() {
            Value::ByteArray(v) => v.clone(),
            _ => panic!(),
        };
        narena_recycle(&[h.clone(), Value::ByteArray(bytes).ref_cell()], span()).unwrap();
        narena_reset(&[h.clone()], span()).unwrap();
        let stats = narena_stats(&[h.clone()], span()).unwrap();
        match &*stats.borrow() {
            Value::Object(m) => {
                assert!(
                    matches!(&*m.get("total_allocated").unwrap().borrow(), Value::Int(n) if *n >= 2)
                );
                assert!(matches!(&*m.get("pooled").unwrap().borrow(), Value::Int(n) if *n >= 1));
            }
            _ => panic!(),
        }
        narena_close(&[h], span()).unwrap();
        let _ = b2;
    }

    #[test]
    fn invalid_handle() {
        let v = narena_alloc(&[i(42), i(8)], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }
}
