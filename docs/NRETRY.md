# nretry standard library

Retry flaky callables with exponential backoff, jitter, deadlines, and custom retry/stop predicates. A scoped port of Python **tenacity** / **backoff** patterns; pairs with `nfallback` circuit breakers (stop calling when open — `nretry` handles per-call retries when closed).

Wrappers run synchronously on the calling thread; sleeps use `thread::sleep` (no background scheduler).

## Import

```niao
import "nretry"
```

Paths `import "std/nretry"` and `import "nretry"` are equivalent. Flat builtins (`nretry_call`, `nretry_wrap`, …) are also available globally after import.

## Quick start

```niao
import "nretry"

let result = nretry.call(fn() {
    return call_upstream()   // may return err(...)
}, {
    attempts: 5,
    min_wait_ms: 200,
    max_wait_ms: 5000,
    multiplier: 2,
    jitter: "full",
    deadline_ms: 30000,
    retry_on_error: true
})

let flaky = nretry.wrap(fn(url) {
    return http_get(url)
}, nretry.merge_opts(
    nretry.stop_after_attempt(4),
    {jitter: "equal", strategy: "exponential"}
))

let stats = nretry.call_ex(flaky, ["https://api.example/health"])
print(stats.ok)        // true on success
print(stats.attempts)  // invocation count
print(stats.sleep_ms)  // total backoff sleep
```

## Core API

| Method | Description |
|--------|-------------|
| `nretry.call(fn, opts?)` | Invoke `fn` with retry until success or limits hit. Returns the last result. |
| `nretry.call_ex(fn, opts?)` | Same loop; returns `{ok, result, attempts, sleep_ms, elapsed_ms, stopped_by_deadline, stopped_by_attempts}`. |
| `nretry.wrap(fn, opts?)` | Returns a native wrapper that retries on each invocation. |
| `nretry.policy(opts?)` | Build a reusable policy object (config snapshot + `id`). |
| `nretry.policy_call(policy, fn)` | `call` using a policy object or inline opts. |
| `nretry.policy_wrap(policy, fn)` | `wrap` using a policy object or inline opts. |

## Backoff helpers

| Method | Description |
|--------|-------------|
| `nretry.backoff(attempt, opts?)` | Wait milliseconds for 1-based `attempt` (strategy + jitter from opts). |
| `nretry.exponential(attempt, opts?)` | Raw exponential delay before jitter. |
| `nretry.jitter(wait_ms, kind?)` | Apply jitter (`"full"` default). |
| `nretry.sleep(ms)` | Blocking sleep (for manual retry loops). |

## Opts builders

| Method | Description |
|--------|-------------|
| `nretry.default_opts()` | Default policy object (`attempts: 3`, `min_wait_ms: 500`, …). |
| `nretry.validate(opts)` | Parse/validate opts; returns `true` or catchable `nretry_error`. |
| `nretry.merge_opts(opts1, …)` | Shallow-merge opts objects (later keys win). |
| `nretry.stop_after_attempt(n)` | `{attempts: n}`. |
| `nretry.stop_after_delay(ms)` | `{deadline_ms: ms}`. |
| `nretry.stop_never()` | `{attempts: 0}` — retry until deadline/stop predicate. |
| `nretry.attempts_left(used, opts?)` | Remaining attempts under policy. |

## Predicates & hooks

Pass callables in opts:

| Key | Signature | Role |
|-----|-----------|------|
| `retry_on` / `retry_if` | `(result, attempt) -> bool` | Custom retry predicate (overrides `retry_on_error` / `retry_on_nil`). |
| `stop_on` / `stop_if` | `(result, attempt) -> bool` | Stop retrying when `true` (even if retry predicate matches). |
| `before` | `(attempt) -> _` | Called before each attempt. |
| `after` | `(attempt, result) -> _` | Called after each attempt. |
| `before_sleep` | `(attempt, wait_ms, result) -> _` | Called before sleeping between attempts. |

Default retry rule: retry when the result is a catchable `error` (`retry_on_error: true`). Set `retry_on_nil: true` to also retry `nil`.

## Policy options

| Key | Default | Meaning |
|-----|---------|---------|
| `attempts` / `max_attempts` | `3` | Maximum invocation attempts (>= 1). |
| `min_wait_ms` / `wait_ms` | `500` | Base / minimum backoff. |
| `max_wait_ms` | `30000` | Cap per wait. |
| `multiplier` | `2.0` | Exponential factor (>= 1). |
| `strategy` | `"exponential"` | `fixed`, `exponential`, `random_exponential`, `decorrelated`. |
| `jitter` | `"full"` | `none`, `full`, `equal`, `decorrelated`. |
| `deadline_ms` / `timeout_ms` | `nil` | Total wall-clock budget; `0` = no deadline. |
| `retry_on_error` | `true` | Retry catchable errors. |
| `retry_on_nil` | `false` | Retry `nil` results. |
| `sleep` | `true` | Sleep between retries (set `false` to spin for tests). |

## Utilities

| Method | Description |
|--------|-------------|
| `nretry.is_error(value)` | `true` for catchable `error` values. |
| `nretry.is_nil(value)` | `true` for `nil`. |

## Errors

| Code | Meaning |
|------|---------|
| 3513 | Wrong argument count. |
| 3514 | Semantic / validation failure (catchable `nretry_error`). |
| 3515 | Wrong argument type (hard error). |
| 3516 | Retries exhausted (reserved; callers receive the last result by default). |

## See also

- [`nfallback`](NFALLBACK.md) — circuit breakers and value fallback chains.
- [`nasync`](NASYNC.md) — async tasks, timeouts, and cancellation.
