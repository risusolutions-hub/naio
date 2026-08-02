//! Native niter standard library — iterator & combinatorics toolkit over general
//! Niao values: product, permutations, combinations, groupby, windows, chunked,
//! flatten, zip_longest, and related helpers (~itertools / more-itertools subset).
//!
//! Import with `import "niter"` (or `import "std/niter"`).

use crate::{error_value, values_equal, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use std::collections::HashMap;
use std::rc::Rc;

const MAX_OUTPUT: usize = 16_777_216;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, codes::E3442_NITER_TYPE, msg.into())
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            codes::E3440_NITER_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            codes::E3440_NITER_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn niter_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(codes::E3441_NITER_ERROR, "niter_error", msg.into(), span)
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

fn optional_int(args: &[ValueRef], idx: usize, default: i64) -> i64 {
    if args.len() <= idx {
        return default;
    }
    match &*args[idx].borrow() {
        Value::Int(n) => *n,
        _ => default,
    }
}

fn array_val(items: Vec<ValueRef>) -> NiaoResult<ValueRef> {
    Ok(Value::Array(items).ref_cell())
}

fn clone_val(v: &ValueRef) -> ValueRef {
    Rc::clone(v)
}

/// Accept `Array` or any packed array; normalize to `Vec<ValueRef>`.
fn list_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<ValueRef>> {
    match &*args[idx].borrow() {
        Value::Array(items) => Ok(items.clone()),
        Value::IntArray(v) => Ok(v.iter().map(|n| Value::Int(*n).ref_cell()).collect()),
        Value::FloatArray(v) => Ok(v.iter().map(|n| Value::Float(*n).ref_cell()).collect()),
        Value::BoolArray(v) => Ok(v.iter().map(|n| Value::Bool(*n != 0).ref_cell()).collect()),
        Value::StringArray(v) => Ok(v
            .dense_vec()
            .into_iter()
            .map(|s| Value::String(s).ref_cell())
            .collect()),
        Value::ByteArray(v) => Ok(v.iter().map(|n| Value::Int(*n as i64).ref_cell()).collect()),
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

fn list_args_variadic(args: &[ValueRef], name: &str, span: Span) -> NiaoResult<Vec<Vec<ValueRef>>> {
    let mut out = Vec::with_capacity(args.len());
    for (i, arg) in args.iter().enumerate() {
        match &*arg.borrow() {
            Value::Array(items) => out.push(items.clone()),
            Value::IntArray(v) => out.push(v.iter().map(|n| Value::Int(*n).ref_cell()).collect()),
            Value::FloatArray(v) => out.push(v.iter().map(|n| Value::Float(*n).ref_cell()).collect()),
            Value::BoolArray(v) => out.push(v.iter().map(|n| Value::Bool(*n != 0).ref_cell()).collect()),
            Value::StringArray(v) => {
                out.push(
                    v.dense_vec()
                        .into_iter()
                        .map(|s| Value::String(s).ref_cell())
                        .collect(),
                );
            }
            Value::ByteArray(v) => {
                out.push(v.iter().map(|n| Value::Int(*n as i64).ref_cell()).collect());
            }
            other => {
                return Err(type_err(
                    span,
                    format!(
                        "{name}() expects arrays; argument {} is {}",
                        i + 1,
                        other.type_name()
                    ),
                ));
            }
        }
    }
    Ok(out)
}

fn check_output_size(n: usize, span: Span) -> Result<(), ValueRef> {
    if n > MAX_OUTPUT {
        Err(niter_err(
            span,
            format!("result size {n} exceeds limit {MAX_OUTPUT}"),
        ))
    } else {
        Ok(())
    }
}

fn checked_mul(a: usize, b: usize) -> Option<usize> {
    a.checked_mul(b)
}


fn n_choose_r(n: usize, r: usize) -> Option<usize> {
    if r > n {
        return Some(0);
    }
    let r = r.min(n - r);
    let mut num = 1usize;
    let mut den = 1usize;
    for i in 0..r {
        num = num.checked_mul(n - r + 1 + i)?;
        den = den.checked_mul(i + 1)?;
    }
    Some(num / den)
}

fn n_multichoose_r(n: usize, r: usize) -> Option<usize> {
    n_choose_r(n.checked_add(r)?.checked_sub(1)?, r)
}

fn n_permutations(n: usize, r: usize) -> Option<usize> {
    if r > n {
        return Some(0);
    }
    (0..r).try_fold(1usize, |acc, i| acc.checked_mul(n - i))
}

fn deep_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Array(xs), Value::Array(ys)) => {
            xs.len() == ys.len()
                && xs
                    .iter()
                    .zip(ys.iter())
                    .all(|(x, y)| deep_equal(&x.borrow(), &y.borrow()))
        }
        (Value::Object(xm), Value::Object(ym)) => {
            xm.len() == ym.len()
                && xm.iter().all(|(k, xv)| {
                    ym.get(k)
                        .map(|yv| deep_equal(&xv.borrow(), &yv.borrow()))
                        .unwrap_or(false)
                })
        }
        (Value::IntArray(xs), Value::IntArray(ys)) => xs == ys,
        (Value::FloatArray(xs), Value::FloatArray(ys)) => {
            xs.len() == ys.len() && xs.iter().zip(ys.iter()).all(|(x, y)| (x - y).abs() < f64::EPSILON)
        }
        (Value::BoolArray(xs), Value::BoolArray(ys)) => xs == ys,
        (Value::ByteArray(xs), Value::ByteArray(ys)) => xs == ys,
        (Value::StringArray(xs), Value::StringArray(ys)) => xs == ys,
        _ => values_equal(a, b),
    }
}

fn object_field(obj: &ValueRef, field: &str) -> Option<ValueRef> {
    match &*obj.borrow() {
        Value::Object(map) => map.get(field).map(Rc::clone),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Combinatorics kernels
// ---------------------------------------------------------------------------

fn cartesian_product(pools: &[Vec<ValueRef>], span: Span) -> Result<Vec<Vec<ValueRef>>, ValueRef> {
    if pools.is_empty() {
        return Ok(vec![Vec::new()]);
    }
    if pools.iter().any(|p| p.is_empty()) {
        return Ok(Vec::new());
    }
    let mut total = 1usize;
    for p in pools {
        total = checked_mul(total, p.len()).ok_or_else(|| {
            niter_err(span, "product size overflow")
        })?;
        check_output_size(total, span)?;
    }
    let mut out = Vec::with_capacity(total);
    let mut indices = vec![0usize; pools.len()];
    loop {
        let mut row = Vec::with_capacity(pools.len());
        for (pool, &idx) in pools.iter().zip(indices.iter()) {
            row.push(clone_val(&pool[idx]));
        }
        out.push(row);
        let mut carry = true;
        for i in (0..pools.len()).rev() {
            if carry {
                indices[i] += 1;
                if indices[i] < pools[i].len() {
                    carry = false;
                } else {
                    indices[i] = 0;
                }
            }
        }
        if carry {
            break;
        }
    }
    Ok(out)
}

fn product_repeat(pool: &[ValueRef], repeat: usize, span: Span) -> Result<Vec<Vec<ValueRef>>, ValueRef> {
    if repeat == 0 {
        return Ok(vec![Vec::new()]);
    }
    if pool.is_empty() {
        return Ok(Vec::new());
    }
    let pools = vec![pool.to_vec(); repeat];
    cartesian_product(&pools, span)
}

fn combination_rows(items: &[ValueRef], r: usize, span: Span) -> Result<Vec<Vec<ValueRef>>, ValueRef> {
    let n = items.len();
    if r > n {
        return Ok(Vec::new());
    }
    let count = n_choose_r(n, r).ok_or_else(|| niter_err(span, "combinations size overflow"))?;
    check_output_size(count, span)?;
    if r == 0 {
        return Ok(vec![Vec::new()]);
    }
    let mut out = Vec::with_capacity(count);
    let mut idx: Vec<usize> = (0..r).collect();
    loop {
        out.push(idx.iter().map(|&i| clone_val(&items[i])).collect());
        let mut i = r as i32 - 1;
        while i >= 0 && idx[i as usize] == i as usize + n - r {
            i -= 1;
        }
        if i < 0 {
            break;
        }
        idx[i as usize] += 1;
        for j in (i as usize + 1)..r {
            idx[j] = idx[j - 1] + 1;
        }
    }
    Ok(out)
}

fn combination_replacement_rows(items: &[ValueRef], r: usize, span: Span) -> Result<Vec<Vec<ValueRef>>, ValueRef> {
    let n = items.len();
    if r == 0 {
        return Ok(vec![Vec::new()]);
    }
    if n == 0 {
        return Ok(Vec::new());
    }
    let count = n_multichoose_r(n, r).ok_or_else(|| niter_err(span, "combinations size overflow"))?;
    check_output_size(count, span)?;
    let mut out = Vec::with_capacity(count);
    let mut idx = vec![0usize; r];
    loop {
        out.push(idx.iter().map(|&i| clone_val(&items[i])).collect());
        let mut i = r as i32 - 1;
        while i >= 0 && idx[i as usize] == n - 1 {
            i -= 1;
        }
        if i < 0 {
            break;
        }
        idx[i as usize] += 1;
        for j in (i as usize + 1)..r {
            idx[j] = idx[i as usize];
        }
    }
    Ok(out)
}

fn permutation_rows(items: &[ValueRef], r: Option<usize>, span: Span) -> Result<Vec<Vec<ValueRef>>, ValueRef> {
    let n = items.len();
    let r = r.unwrap_or(n);
    if r > n {
        return Ok(Vec::new());
    }
    let count = n_permutations(n, r).ok_or_else(|| niter_err(span, "permutations size overflow"))?;
    check_output_size(count, span)?;
    if r == 0 {
        return Ok(vec![Vec::new()]);
    }
    let mut out = Vec::with_capacity(count);
    let mut used = vec![false; n];
    let mut stack: Vec<usize> = Vec::with_capacity(r);
    fn visit(
        items: &[ValueRef],
        n: usize,
        r: usize,
        used: &mut [bool],
        stack: &mut Vec<usize>,
        out: &mut Vec<Vec<ValueRef>>,
    ) {
        if stack.len() == r {
            out.push(stack.iter().map(|&i| clone_val(&items[i])).collect());
            return;
        }
        for i in 0..n {
            if !used[i] {
                used[i] = true;
                stack.push(i);
                visit(items, n, r, used, stack, out);
                stack.pop();
                used[i] = false;
            }
        }
    }
    visit(items, n, r, &mut used, &mut stack, &mut out);
    Ok(out)
}

// ---------------------------------------------------------------------------
// Public builtins
// ---------------------------------------------------------------------------

// >>> import "niter"
// >>> niter.product([1, 2], ["a", "b"])
// => [[1, "a"], [1, "b"], [2, "a"], [2, "b"]]
fn niter_product(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 16, "niter_product", span)?;
    let pools = list_args_variadic(args, "niter_product", span)?;
    match cartesian_product(&pools, span) {
        Ok(rows) => {
            let out = rows.into_iter().map(|row| array_val(row).unwrap()).collect();
            array_val(out)
        }
        Err(e) => Ok(e),
    }
}

// >>> niter.product_repeat([0, 1], 2)
// => [[0, 0], [0, 1], [1, 0], [1, 1]]
fn niter_product_repeat(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "niter_product_repeat", span)?;
    let pool = list_arg(args, 0, "niter_product_repeat", span)?;
    let repeat = int_arg(args, 1, "niter_product_repeat", span)?;
    if repeat < 0 {
        return Ok(niter_err(
            span,
            "niter.product_repeat() repeat must be >= 0",
        ));
    }
    match product_repeat(&pool, repeat as usize, span) {
        Ok(rows) => {
            let out = rows.into_iter().map(|row| array_val(row).unwrap()).collect();
            array_val(out)
        }
        Err(e) => Ok(e),
    }
}

// >>> niter.combinations([1, 2, 3], 2)
// => [[1, 2], [1, 3], [2, 3]]
fn niter_combinations(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "niter_combinations", span)?;
    let items = list_arg(args, 0, "niter_combinations", span)?;
    let r = int_arg(args, 1, "niter_combinations", span)?;
    if r < 0 {
        return Ok(niter_err(span, "niter.combinations() r must be >= 0"));
    }
    match combination_rows(&items, r as usize, span) {
        Ok(rows) => {
            let out = rows.into_iter().map(|row| array_val(row).unwrap()).collect();
            array_val(out)
        }
        Err(e) => Ok(e),
    }
}

// >>> niter.combinations_with_replacement("AB", 2)
// => [["A", "A"], ["A", "B"], ["B", "B"]]
fn niter_combinations_with_replacement(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "niter_combinations_with_replacement", span)?;
    let items = list_arg(args, 0, "niter_combinations_with_replacement", span)?;
    let r = int_arg(args, 1, "niter_combinations_with_replacement", span)?;
    if r < 0 {
        return Ok(niter_err(
            span,
            "niter.combinations_with_replacement() r must be >= 0",
        ));
    }
    match combination_replacement_rows(&items, r as usize, span) {
        Ok(rows) => {
            let out = rows.into_iter().map(|row| array_val(row).unwrap()).collect();
            array_val(out)
        }
        Err(e) => Ok(e),
    }
}

// >>> niter.permutations([1, 2, 3], 2)
// => [[1, 2], [1, 3], [2, 1], [2, 3], [3, 1], [3, 2]]
fn niter_permutations(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "niter_permutations", span)?;
    let items = list_arg(args, 0, "niter_permutations", span)?;
    let r = if args.len() == 2 {
        let r = int_arg(args, 1, "niter_permutations", span)?;
        if r < 0 {
            return Ok(niter_err(span, "niter.permutations() r must be >= 0"));
        }
        Some(r as usize)
    } else {
        None
    };
    match permutation_rows(&items, r, span) {
        Ok(rows) => {
            let out = rows.into_iter().map(|row| array_val(row).unwrap()).collect();
            array_val(out)
        }
        Err(e) => Ok(e),
    }
}

// >>> niter.groupby([1, 1, 2, 2, 1])
// => [{key: 1, items: [1, 1]}, {key: 2, items: [2, 2]}, {key: 1, items: [1]}]
fn niter_groupby(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "niter_groupby", span)?;
    let items = list_arg(args, 0, "niter_groupby", span)?;
    if items.is_empty() {
        return array_val(Vec::new());
    }
    let mut groups: Vec<(ValueRef, Vec<ValueRef>)> = Vec::new();
    let mut current_key = clone_val(&items[0]);
    let mut bucket = vec![clone_val(&items[0])];
    for item in items.iter().skip(1) {
        if deep_equal(&item.borrow(), &current_key.borrow()) {
            bucket.push(clone_val(item));
        } else {
            groups.push((current_key, bucket));
            current_key = clone_val(item);
            bucket = vec![clone_val(item)];
        }
    }
    groups.push((current_key, bucket));
    let out = groups
        .into_iter()
        .map(|(key, items)| {
            let mut map = HashMap::new();
            map.insert("key".to_string(), key);
            map.insert("items".to_string(), Value::Array(items).ref_cell());
            Value::Object(map).ref_cell()
        })
        .collect();
    array_val(out)
}

// >>> niter.groupby_key([{x: 1}, {x: 1}, {x: 2}], "x")
// => [{key: 1, items: [{x: 1}, {x: 1}]}, {key: 2, items: [{x: 2}]}]
fn niter_groupby_key(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "niter_groupby_key", span)?;
    let items = list_arg(args, 0, "niter_groupby_key", span)?;
    let field = match &*args[1].borrow() {
        Value::String(s) => s.clone(),
        other => {
            return Err(type_err(
                span,
                format!(
                    "niter.groupby_key() expects a string field name, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    if items.is_empty() {
        return array_val(Vec::new());
    }
    let first_key = object_field(&items[0], &field).unwrap_or_else(|| Value::Nil.ref_cell());
    let mut groups: Vec<(ValueRef, Vec<ValueRef>)> = Vec::new();
    let mut current_key = first_key;
    let mut bucket = vec![clone_val(&items[0])];
    for item in items.iter().skip(1) {
        let key = object_field(item, &field).unwrap_or_else(|| Value::Nil.ref_cell());
        if deep_equal(&key.borrow(), &current_key.borrow()) {
            bucket.push(clone_val(item));
        } else {
            groups.push((current_key, bucket));
            current_key = key;
            bucket = vec![clone_val(item)];
        }
    }
    groups.push((current_key, bucket));
    let out = groups
        .into_iter()
        .map(|(key, items)| {
            let mut map = HashMap::new();
            map.insert("key".to_string(), key);
            map.insert("items".to_string(), Value::Array(items).ref_cell());
            Value::Object(map).ref_cell()
        })
        .collect();
    array_val(out)
}

// >>> niter.windows([1, 2, 3, 4], 2)
// => [[1, 2], [2, 3], [3, 4]]
fn niter_windows(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "niter_windows", span)?;
    let items = list_arg(args, 0, "niter_windows", span)?;
    let size = int_arg(args, 1, "niter_windows", span)?;
    let step = optional_int(args, 2, 1);
    if size <= 0 {
        return Ok(niter_err(span, "niter.windows() size must be > 0"));
    }
    if step <= 0 {
        return Ok(niter_err(span, "niter.windows() step must be > 0"));
    }
    let size = size as usize;
    let step = step as usize;
    if items.len() < size {
        return array_val(Vec::new());
    }
    let count = 1 + (items.len() - size) / step;
    if let Err(e) = check_output_size(count, span) {
        return Ok(e);
    }
    let mut out = Vec::with_capacity(count);
    let mut start = 0usize;
    while start + size <= items.len() {
        let window: Vec<ValueRef> = items[start..start + size].iter().map(clone_val).collect();
        out.push(array_val(window).unwrap());
        start += step;
    }
    array_val(out)
}

// >>> niter.chunked([1, 2, 3, 4, 5], 2)
// => [[1, 2], [3, 4], [5]]
fn niter_chunked(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "niter_chunked", span)?;
    let items = list_arg(args, 0, "niter_chunked", span)?;
    let size = int_arg(args, 1, "niter_chunked", span)?;
    if size <= 0 {
        return Ok(niter_err(span, "niter.chunked() size must be > 0"));
    }
    let size = size as usize;
    let count = items.len().div_ceil(size);
    if let Err(e) = check_output_size(count, span) {
        return Ok(e);
    }
    let mut out = Vec::with_capacity(count);
    for chunk in items.chunks(size) {
        let row: Vec<ValueRef> = chunk.iter().map(clone_val).collect();
        out.push(array_val(row).unwrap());
    }
    array_val(out)
}

// >>> niter.flatten([[1, 2], [3]])
// => [1, 2, 3]
fn niter_flatten(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "niter_flatten", span)?;
    let outer = list_arg(args, 0, "niter_flatten", span)?;
    let mut total = 0usize;
    let mut nested: Vec<Vec<ValueRef>> = Vec::with_capacity(outer.len());
    for item in outer {
        match &*item.borrow() {
            Value::Array(inner) => {
                total = total.saturating_add(inner.len());
                if total > MAX_OUTPUT {
                    return Ok(niter_err(
                        span,
                        format!("flatten result exceeds limit {MAX_OUTPUT}"),
                    ));
                }
                nested.push(inner.clone());
            }
            other => {
                return Err(type_err(
                    span,
                    format!(
                        "niter.flatten() expects array of arrays; got {}",
                        other.type_name()
                    ),
                ));
            }
        }
    }
    let mut out = Vec::with_capacity(total);
    for inner in nested {
        for v in inner {
            out.push(v);
        }
    }
    array_val(out)
}

// >>> niter.chain([1], [2, 3])
// => [1, 2, 3]
fn niter_chain(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 16, "niter_chain", span)?;
    let pools = list_args_variadic(args, "niter_chain", span)?;
    let total: usize = pools.iter().map(|p| p.len()).sum();
    if let Err(e) = check_output_size(total, span) {
        return Ok(e);
    }
    let mut out = Vec::with_capacity(total);
    for pool in pools {
        for v in pool {
            out.push(v);
        }
    }
    array_val(out)
}

// >>> niter.zip_longest([1, 2], [3])
// => [[1, 3], [2, nil]]
fn niter_zip_longest(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "niter_zip_longest", span)?;
    let a = list_arg(args, 0, "niter_zip_longest", span)?;
    let b = list_arg(args, 1, "niter_zip_longest", span)?;
    let fill = if args.len() == 3 {
        clone_val(&args[2])
    } else {
        Value::Nil.ref_cell()
    };
    let len = a.len().max(b.len());
    if let Err(e) = check_output_size(len, span) {
        return Ok(e);
    }
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let left = a.get(i).map(clone_val).unwrap_or_else(|| clone_val(&fill));
        let right = b.get(i).map(clone_val).unwrap_or_else(|| clone_val(&fill));
        out.push(array_val(vec![left, right]).unwrap());
    }
    array_val(out)
}

// >>> niter.pairwise([1, 2, 3])
// => [[1, 2], [2, 3]]
fn niter_pairwise(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "niter_pairwise", span)?;
    let items = list_arg(args, 0, "niter_pairwise", span)?;
    if items.len() < 2 {
        return array_val(Vec::new());
    }
    let count = items.len() - 1;
    if let Err(e) = check_output_size(count, span) {
        return Ok(e);
    }
    let mut out = Vec::with_capacity(count);
    for w in items.windows(2) {
        out.push(array_val(vec![clone_val(&w[0]), clone_val(&w[1])]).unwrap());
    }
    array_val(out)
}

// >>> niter.enumerate(["a", "b"], 1)
// => [[1, "a"], [2, "b"]]
fn niter_enumerate(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "niter_enumerate", span)?;
    let items = list_arg(args, 0, "niter_enumerate", span)?;
    let start = optional_int(args, 1, 0);
    if let Err(e) = check_output_size(items.len(), span) {
        return Ok(e);
    }
    let mut out = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let idx = start.saturating_add(i as i64);
        out.push(
            array_val(vec![Value::Int(idx).ref_cell(), clone_val(item)]).unwrap(),
        );
    }
    array_val(out)
}

// >>> niter.take([1, 2, 3, 4], 2)
// => [1, 2]
fn niter_take(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "niter_take", span)?;
    let items = list_arg(args, 0, "niter_take", span)?;
    let n = int_arg(args, 1, "niter_take", span)?;
    if n < 0 {
        return Ok(niter_err(span, "niter.take() n must be >= 0"));
    }
    let n = n as usize;
    let out: Vec<ValueRef> = items.into_iter().take(n).collect();
    array_val(out)
}

// >>> niter.drop([1, 2, 3], 1)
// => [2, 3]
fn niter_drop(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "niter_drop", span)?;
    let items = list_arg(args, 0, "niter_drop", span)?;
    let n = int_arg(args, 1, "niter_drop", span)?;
    if n < 0 {
        return Ok(niter_err(span, "niter.drop() n must be >= 0"));
    }
    let n = n as usize;
    let out: Vec<ValueRef> = items.into_iter().skip(n).collect();
    array_val(out)
}

// >>> niter.islice([0, 1, 2, 3, 4], 1, 4, 2)
// => [1, 3]
fn niter_islice(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 4, "niter_islice", span)?;
    let items = list_arg(args, 0, "niter_islice", span)?;
    let start = int_arg(args, 1, "niter_islice", span)?;
    if start < 0 {
        return Ok(niter_err(span, "niter.islice() start must be >= 0"));
    }
    let stop = if args.len() >= 3 {
        let s = int_arg(args, 2, "niter_islice", span)?;
        if s < 0 {
            return Ok(niter_err(span, "niter.islice() stop must be >= 0"));
        }
        s as usize
    } else {
        items.len()
    };
    let step = if args.len() == 4 {
        let s = int_arg(args, 3, "niter_islice", span)?;
        if s <= 0 {
            return Ok(niter_err(span, "niter.islice() step must be > 0"));
        }
        s as usize
    } else {
        1
    };
    let start = start as usize;
    let mut out = Vec::new();
    let mut i = start;
    while i < stop.min(items.len()) {
        out.push(clone_val(&items[i]));
        i += step;
        if out.len() > MAX_OUTPUT {
            return Ok(niter_err(
                span,
                format!("islice result exceeds limit {MAX_OUTPUT}"),
            ));
        }
    }
    array_val(out)
}

// >>> niter.repeat("x", 3)
// => ["x", "x", "x"]
fn niter_repeat(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "niter_repeat", span)?;
    let value = clone_val(&args[0]);
    let n = int_arg(args, 1, "niter_repeat", span)?;
    if n < 0 {
        return Ok(niter_err(span, "niter.repeat() times must be >= 0"));
    }
    let n = n as usize;
    if let Err(e) = check_output_size(n, span) {
        return Ok(e);
    }
    let out = vec![value; n];
    array_val(out)
}

// >>> niter.count(3)
// => [0, 1, 2]
fn niter_count(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "niter_count", span)?;
    let (start, stop, step) = match args.len() {
        1 => (0i64, int_arg(args, 0, "niter_count", span)?, 1i64),
        2 => (
            int_arg(args, 0, "niter_count", span)?,
            int_arg(args, 1, "niter_count", span)?,
            1,
        ),
        _ => (
            int_arg(args, 0, "niter_count", span)?,
            int_arg(args, 1, "niter_count", span)?,
            int_arg(args, 2, "niter_count", span)?,
        ),
    };
    if step == 0 {
        return Ok(niter_err(span, "niter.count() step must not be 0"));
    }
    let mut out = Vec::new();
    if step > 0 {
        let mut i = start;
        while i < stop {
            out.push(Value::Int(i).ref_cell());
            if out.len() > MAX_OUTPUT {
                return Ok(niter_err(
                    span,
                    format!("count result exceeds limit {MAX_OUTPUT}"),
                ));
            }
            i = i.saturating_add(step);
        }
    } else {
        let mut i = start;
        while i > stop {
            out.push(Value::Int(i).ref_cell());
            if out.len() > MAX_OUTPUT {
                return Ok(niter_err(
                    span,
                    format!("count result exceeds limit {MAX_OUTPUT}"),
                ));
            }
            i = i.saturating_add(step);
        }
    }
    array_val(out)
}

// >>> niter.unique_justseen([1, 1, 2, 1, 2, 2])
// => [1, 2, 1, 2]
fn niter_unique_justseen(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "niter_unique_justseen", span)?;
    let items = list_arg(args, 0, "niter_unique_justseen", span)?;
    if items.is_empty() {
        return array_val(Vec::new());
    }
    let mut out = vec![clone_val(&items[0])];
    for item in items.iter().skip(1) {
        if !deep_equal(&item.borrow(), &out.last().unwrap().borrow()) {
            out.push(clone_val(item));
        }
    }
    array_val(out)
}

// >>> niter.compress([1, 2, 3, 4], [true, false, true, false])
// => [1, 3]
fn niter_compress(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "niter_compress", span)?;
    let data = list_arg(args, 0, "niter_compress", span)?;
    let selectors = list_arg(args, 1, "niter_compress", span)?;
    let len = data.len().min(selectors.len());
    let mut out = Vec::new();
    for i in 0..len {
        let keep = matches!(&*selectors[i].borrow(), Value::Bool(true));
        if keep {
            out.push(clone_val(&data[i]));
        }
    }
    array_val(out)
}

// ---------------------------------------------------------------------------
// Module exports
// ---------------------------------------------------------------------------

macro_rules! niter_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

niter_fns![
    ("niter_product", "product", niter_product),
    ("niter_product_repeat", "product_repeat", niter_product_repeat),
    ("niter_combinations", "combinations", niter_combinations),
    (
        "niter_combinations_with_replacement",
        "combinations_with_replacement",
        niter_combinations_with_replacement
    ),
    ("niter_permutations", "permutations", niter_permutations),
    ("niter_groupby", "groupby", niter_groupby),
    ("niter_groupby_key", "groupby_key", niter_groupby_key),
    ("niter_windows", "windows", niter_windows),
    ("niter_chunked", "chunked", niter_chunked),
    ("niter_flatten", "flatten", niter_flatten),
    ("niter_chain", "chain", niter_chain),
    ("niter_zip_longest", "zip_longest", niter_zip_longest),
    ("niter_pairwise", "pairwise", niter_pairwise),
    ("niter_enumerate", "enumerate", niter_enumerate),
    ("niter_take", "take", niter_take),
    ("niter_drop", "drop", niter_drop),
    ("niter_islice", "islice", niter_islice),
    ("niter_repeat", "repeat", niter_repeat),
    ("niter_count", "count", niter_count),
    ("niter_unique_justseen", "unique_justseen", niter_unique_justseen),
    ("niter_compress", "compress", niter_compress),
];

pub const MODULE_NAME: &str = "niter";
pub const MODULE_PATHS: &[&str] = &["niter", "std/niter"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs().into_iter().map(|(flat, _, f)| (flat, f)).collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    fn int_list(vals: &[i64]) -> Vec<ValueRef> {
        vals.iter().map(|n| Value::Int(*n).ref_cell()).collect()
    }

    fn as_nested_ints(v: &ValueRef) -> Vec<Vec<i64>> {
        match &*v.borrow() {
            Value::Array(rows) => rows
                .iter()
                .map(|row| match &*row.borrow() {
                    Value::Array(cols) => cols
                        .iter()
                        .map(|c| match &*c.borrow() {
                            Value::Int(n) => *n,
                            other => panic!("expected int, got {other:?}"),
                        })
                        .collect(),
                    other => panic!("expected array row, got {other:?}"),
                })
                .collect(),
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[test]
    fn product_two() {
        let a = int_list(&[1, 2]);
        let b = int_list(&[10, 20]);
        let out = niter_product(&[array_val(a).unwrap(), array_val(b).unwrap()], span()).unwrap();
        assert_eq!(
            as_nested_ints(&out),
            vec![vec![1, 10], vec![1, 20], vec![2, 10], vec![2, 20]]
        );
    }

    #[test]
    fn combinations_r2() {
        let items = int_list(&[1, 2, 3]);
        let out = niter_combinations(
            &[array_val(items).unwrap(), Value::Int(2).ref_cell()],
            span(),
        )
        .unwrap();
        assert_eq!(as_nested_ints(&out), vec![vec![1, 2], vec![1, 3], vec![2, 3]]);
    }

    #[test]
    fn permutations_r2() {
        let items = int_list(&[1, 2, 3]);
        let out = niter_permutations(
            &[array_val(items).unwrap(), Value::Int(2).ref_cell()],
            span(),
        )
        .unwrap();
        let rows = as_nested_ints(&out);
        assert_eq!(rows.len(), 6);
    }

    #[test]
    fn zip_longest_fill() {
        let a = int_list(&[1, 2]);
        let b = int_list(&[3]);
        let out = niter_zip_longest(
            &[
                array_val(a).unwrap(),
                array_val(b).unwrap(),
                Value::Int(0).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        assert_eq!(as_nested_ints(&out), vec![vec![1, 3], vec![2, 0]]);
    }

    #[test]
    fn windows_step() {
        let items = int_list(&[1, 2, 3, 4, 5]);
        let out = niter_windows(
            &[
                array_val(items).unwrap(),
                Value::Int(2).ref_cell(),
                Value::Int(2).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        assert_eq!(as_nested_ints(&out), vec![vec![1, 2], vec![3, 4]]);
    }

    #[test]
    fn groupby_consecutive() {
        let items = int_list(&[1, 1, 2, 2, 1]);
        let out = niter_groupby(&[array_val(items).unwrap()], span()).unwrap();
        match &*out.borrow() {
            Value::Array(groups) => assert_eq!(groups.len(), 3),
            other => panic!("expected array, got {other:?}"),
        }
    }

    #[test]
    fn oversized_product_errors() {
        let big = int_list(&[0; 5000]);
        let args = vec![
            array_val(big.clone()).unwrap(),
            array_val(big.clone()).unwrap(),
            array_val(big.clone()).unwrap(),
            array_val(big).unwrap(),
        ];
        let out = niter_product(&args, span()).unwrap();
        assert!(matches!(&*out.borrow(), Value::Error(_)));
    }

    // Micro-benchmarks (run: cargo test -p niao_runtime bench_ -- --nocapture)
    #[test]
    fn bench_combinations_c20_3() {
        use std::time::Instant;
        let items: Vec<ValueRef> = (0..20).map(|n| Value::Int(n).ref_cell()).collect();
        let start = Instant::now();
        let out = niter_combinations(
            &[array_val(items).unwrap(), Value::Int(3).ref_cell()],
            span(),
        )
        .unwrap();
        let elapsed = start.elapsed();
        let count = match &*out.borrow() {
            Value::Array(a) => a.len(),
            _ => 0,
        };
        println!("bench combinations C(20,3): count={count} time={}µs", elapsed.as_micros());
        assert_eq!(count, 1140);
    }

    #[test]
    fn bench_permutations_p9_4() {
        use std::time::Instant;
        let items: Vec<ValueRef> = (0..9).map(|n| Value::Int(n).ref_cell()).collect();
        let start = Instant::now();
        let out = niter_permutations(
            &[array_val(items).unwrap(), Value::Int(4).ref_cell()],
            span(),
        )
        .unwrap();
        let elapsed = start.elapsed();
        let count = match &*out.borrow() {
            Value::Array(a) => a.len(),
            _ => 0,
        };
        println!("bench permutations P(9,4): count={count} time={}µs", elapsed.as_micros());
        assert_eq!(count, 3024);
    }

    #[test]
    fn bench_product_50x50() {
        use std::time::Instant;
        let a: Vec<ValueRef> = (0..50).map(|n| Value::Int(n).ref_cell()).collect();
        let b: Vec<ValueRef> = (0..50).map(|n| Value::Int(n).ref_cell()).collect();
        let start = Instant::now();
        let out = niter_product(
            &[array_val(a).unwrap(), array_val(b).unwrap()],
            span(),
        )
        .unwrap();
        let elapsed = start.elapsed();
        let count = match &*out.borrow() {
            Value::Array(a) => a.len(),
            _ => 0,
        };
        println!("bench product 50x50: count={count} time={}µs", elapsed.as_micros());
        assert_eq!(count, 2500);
    }

    #[test]
    fn bench_windows_10k() {
        use std::time::Instant;
        let items: Vec<ValueRef> = (0..10_000).map(|n| Value::Int(n).ref_cell()).collect();
        let start = Instant::now();
        let out = niter_windows(
            &[array_val(items).unwrap(), Value::Int(5).ref_cell()],
            span(),
        )
        .unwrap();
        let elapsed = start.elapsed();
        let count = match &*out.borrow() {
            Value::Array(a) => a.len(),
            _ => 0,
        };
        println!("bench windows 10k w=5: count={count} time={}µs", elapsed.as_micros());
        assert_eq!(count, 9996);
    }

    #[test]
    fn bench_zip_longest_5k_3k() {
        use std::time::Instant;
        let a: Vec<ValueRef> = (0..5000).map(|n| Value::Int(n).ref_cell()).collect();
        let b: Vec<ValueRef> = (0..3000).map(|n| Value::Int(n).ref_cell()).collect();
        let start = Instant::now();
        let out = niter_zip_longest(
            &[
                array_val(a).unwrap(),
                array_val(b).unwrap(),
                Value::Int(0).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        let elapsed = start.elapsed();
        let count = match &*out.borrow() {
            Value::Array(a) => a.len(),
            _ => 0,
        };
        println!("bench zip_longest 5k/3k: count={count} time={}µs", elapsed.as_micros());
        assert_eq!(count, 5000);
    }
}
