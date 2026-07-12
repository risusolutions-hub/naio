# nfallback standard library

Graceful degradation helpers: pick the first usable value in a chain, and a named token-failure circuit breaker for tripping open after repeated failures.

## Import

```niao
import "nfallback"
```

Paths `import "std/nfallback"` and `import "nfallback"` are equivalent. Flat builtins (`nfallback_first`, `nfallback_circuit`, …) are also available globally after import.

## Quick start

```niao
import "nfallback"

let v = nfallback.first([nil, err, "ok"])     // "ok"
let w = nfallback.coalesce(nil, err, 42)      // 42
print(nfallback.or(nil, "fallback"))          // "fallback"

if !nfallback.is_open("payments") {
    let ok = call_payments()                  // your code
    nfallback.circuit("payments", ok, {threshold: 5, reset_ms: 30000})
}
```

## Value selection

A value is **usable** when it is neither `nil` nor an `error`. Selection helpers skip unusable entries and return `nil` when nothing remains.

| Method | Description |
|--------|-------------|
| `nfallback.first(array)` | First usable element of `array`. |
| `nfallback.try_chain(array)` | Same as `first`. |
| `nfallback.coalesce(v1, …, v16)` | First usable among 1–16 arguments. |
| `nfallback.or(a, b)` | `a` if usable, otherwise `b` (even if `b` is nil/error). |

## Circuit breaker

Named circuits live in a thread-local map. Each entry tracks `fails`, `opened_at_ms`, and `reset_ms`.

| Method | Description |
|--------|-------------|
| `nfallback.circuit(name, success, opts?)` | Record a bool outcome. On `false`, increment fails; open when `fails >= threshold` (default **5**). Returns `true` if the circuit is **closed** (allowing), `false` if **open**. |
| `nfallback.is_open(name)` | `true` while open (auto-closes after `reset_ms` from `opened_at_ms`). Unknown name → `false`. |
| `nfallback.reset(name)` | Clear fails and force-close. |
| `nfallback.allow(name)` | Force-close (same openness effect as `reset`). |

### Options

```niao
nfallback.circuit("svc", false, {
    threshold: 3,       // or fail_threshold — failures before open (default 5)
    reset_ms: 10000     // auto-close after this many ms (default 30000; 0 = never)
})
```

While open, further `circuit` calls return `false` without changing state. A successful outcome while closed resets the fail counter to 0.

## Errors

| Code | Meaning |
|------|---------|
| 3040 | Wrong argument count. |
| 3041 | Semantic error (e.g. `threshold < 1`) — catchable `error`. |
| 3042 | Wrong argument type. |
