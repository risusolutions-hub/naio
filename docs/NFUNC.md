# nfunc standard library

Function toolkit: partial application, currying, composition, memoize/LRU caches, once, debounce, and throttle. A scoped port of Python `functools` / `toolz` patterns for Niao callables.

Wrappers are native functions backed by Rust. User-defined `fn` handlers dispatch through the runtime call hook; native callees are invoked directly for zero hook overhead.

## Import

```niao
import "nfunc"
```

Paths `import "std/nfunc"` and `import "nfunc"` are equivalent. Flat builtins (`nfunc_partial`, `nfunc_memoize`, …) are also available globally after import.

## Quick start

```niao
import "nfunc"

let add = fn(a, b) { return a + b }
let inc = nfunc.partial(add, 1)
print(inc(5))                    // 6

let curried = nfunc.curry(add)
print(curried(2)(3))             // 5

let f = nfunc.compose(
    fn(x) { return x + 1 },
    fn(x) { return x * 2 }
)
print(f(3))                      // 7  — (3*2)+1

let cached = nfunc.memoize_lru(fn(n) { return n * n }, 128)
print(cached(10))                // 100
print(nfunc.cache_info(cached))  // {hits, misses, currsize, maxsize, hit_rate}
```

## Combinators

| Method | Description |
|--------|-------------|
| `nfunc.partial(fn, ...args)` | Bind leading positional arguments. |
| `nfunc.partial_right(fn, ...args)` | Bind trailing positional arguments. |
| `nfunc.curry(fn, arity?)` | Unary currying. Arity defaults from `fn` param count; native fns need explicit arity. |
| `nfunc.compose(...fns)` | Right-to-left: `compose(f,g)(x) = f(g(x))`. |
| `nfunc.pipe(value, ...fns)` | Left-to-right application. |
| `nfunc.apply(fn, args)` | Call `fn` with an argument array. |
| `nfunc.identity(x?)` | Passthrough value, or the identity function when called with no args. |
| `nfunc.flip(fn)` | Swap the first two arguments. |
| `nfunc.constant(value)` | Zero-argument function returning a fixed value. |
| `nfunc.arity(fn)` | Parameter count for user functions; `nil` for native. |

## Caching & rate control

| Method | Description |
|--------|-------------|
| `nfunc.memoize(fn)` | Unbounded memo cache (hashable args only). |
| `nfunc.memoize_lru(fn, maxsize)` | LRU-bounded memo cache. |
| `nfunc.once(fn)` | Invoke at most once. |
| `nfunc.throttle(fn, interval_ms, opts?)` | Rate-limit invocations at call time. |
| `nfunc.debounce(fn, wait_ms, opts?)` | Collapse rapid calls at call time. |
| `nfunc.debounce_flush(wrapped)` | Force a pending debounced invocation. |
| `nfunc.cache_info(wrapped)` | `{hits, misses, currsize, maxsize, hit_rate}` for memo/once wrappers. |
| `nfunc.cache_clear(wrapped)` | Drop cached results. |

### Hashable memo keys

Memoization accepts `nil`, `bool`, `int`, `float`, `string`, and arrays of those types. Objects, native handles, and functions return a catchable `nfunc_error` on first use.

### Throttle / debounce options

Both accept an optional opts object:

| Key | Default (throttle) | Default (debounce) | Meaning |
|-----|-------------------|-------------------|---------|
| `leading` | `true` | `false` | Invoke on the leading edge of a quiet window. |
| `trailing` | `false` | `true` | Queue a trailing invocation. |

No background threads — timing is evaluated on each call (same model as `ncache` TTL and `nwatch` polling). For trailing debounce across idle periods, call `debounce_flush` after the quiet window.

## Errors

| Code | Meaning |
|------|---------|
| 2693 | Wrong argument count. |
| 2694 | Operation failed (catchable `nfunc_error`). |
| 2695 | Type mismatch (hard error). |
| 2696 | Unhashable memo argument. |
