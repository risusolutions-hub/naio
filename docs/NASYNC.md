# nasync standard library

Structured async ergonomics over the shared background task pool used by `io`, `net`, `nsqlite`, and `npg`. Spawn callables, wait with timeouts, gather/race combinators, cancellation shields, task groups, and async channels. A scoped port of Python `asyncio` / `trio` patterns for Niao's task-id async model.

Tasks are opaque positive integer handles. Callable arguments must be **sendable** across threads (same rules as `parallel`).

## Import

```niao
import "nasync"
```

Paths `import "std/nasync"` and `import "nasync"` are equivalent. Flat builtins (`nasync_spawn`, `nasync_gather`, …) are also available globally after import.

## Quick start

```niao
import "nasync"

let t1 = nasync.spawn(fn() { return io_read_file("a.txt") })
let t2 = nasync.spawn(fn() { return io_read_file("b.txt") })
let [a, b] = nasync.gather([t1, t2])

let fast = nasync.race([
    nasync.sleep_async(5000),
    net_async_http_get("https://example.com"),
])
print(fast.value)

let ch = nasync.channel(32)
nasync.channel_send(ch, {event: "ready"})
print(nasync.channel_recv(ch))
```

## Task lifecycle

| Method | Description |
|--------|-------------|
| `nasync.spawn(fn, ...args)` | Run a callable on the shared thread pool; returns task id. |
| `nasync.create_task(fn, ...args)` | Alias for `spawn`. |
| `nasync.sleep_async(ms)` | Background sleep task. |
| `nasync.sleep(ms)` | Blocking sleep (current thread). |
| `nasync.done(task)` | `true` when the task finished (success, error, or cancelled). |
| `nasync.poll(task)` | Result if done; `nil` if still pending. |
| `nasync.wait(task)` | Block until done; return result or catchable error. |
| `nasync.result(task)` | Alias for `wait`. |
| `nasync.cancel(task)` | Cancel a pending task; returns whether cancellation was applied. |
| `nasync.shield(task)` | Prevent cancellation until the task completes. |
| `nasync.status(task)` | `"pending"`, `"done"`, `"cancelled"`, or `"error"`. |

`nasync.spawn` integrates with existing async builtins — `io_async_read`, `net_async_http_get`, `nsqlite_async_query`, etc. all share the same task registry.

## Combinators

| Method | Description |
|--------|-------------|
| `nasync.gather(tasks)` | Wait for all tasks; fail fast on first error value. |
| `nasync.gather_exceptions(tasks)` | Wait for all; return every result including errors. |
| `nasync.race(tasks)` | Wait for the first completion; returns `{index, task, value}`. |
| `nasync.wait_any(tasks)` | Alias for `race`. |
| `nasync.as_completed(tasks)` | Wait for all; return results in completion order. |
| `nasync.cancel_all(tasks)` | Cancel every still-pending task; returns count cancelled. |
| `nasync.spawn_all(callables, limit?)` | Spawn many zero-arg callables; optional in-flight cap. |

## Timeouts

| Method | Description |
|--------|-------------|
| `nasync.wait_timeout(task, ms)` | Block up to `ms`; returns `{timed_out, done, value}`. |
| `nasync.timeout(fn, ms, ...args)` | Spawn callable and wait; on timeout cancel task and return `{timed_out, value}`. |

## Async channels

Thread-safe bounded or unbounded channels for producer/consumer patterns between tasks.

| Method | Description |
|--------|-------------|
| `nasync.channel(capacity?)` | Create a channel handle. Omit capacity for unbounded. |
| `nasync.channel_send(ch, value)` | Send a sendable value. |
| `nasync.channel_recv(ch)` | Blocking receive. |
| `nasync.channel_try_recv(ch)` | Non-blocking receive; `nil` if empty. |
| `nasync.channel_recv_timeout(ch, ms)` | Receive with timeout; `nil` on timeout. |
| `nasync.channel_close(ch)` | Close and drop the channel. |
| `nasync.channel_is_closed(ch)` | Whether the channel is closed. |
| `nasync.channel_len(ch)` | Queued message count. |

## Task groups

| Method | Description |
|--------|-------------|
| `nasync.group()` | Create a task group handle. |
| `nasync.group_spawn(group, fn, ...args)` | Spawn into the group. |
| `nasync.group_wait(group)` | Wait for all group tasks; cancel siblings on first error. |
| `nasync.group_cancel(group)` | Cancel all pending tasks in the group. |

## Sendable values

Spawn and channel APIs accept the same cross-thread types as `parallel`: `nil`, `bool`, `int`, `float`, `string`, packed arrays, arrays/objects of sendable values. Functions, native handles, and instances return a type error at the call site.

## Errors

| Code | Meaning |
|------|---------|
| 3473 | Wrong argument count. |
| 3474 | Operation failed (catchable `nasync_error`). |
| 3475 | Type mismatch (hard error). |
| 3476 | Unknown task id. |
| 3477 | Timeout exceeded (`nasync_timeout`). |
| 3478 | Invalid channel or group handle. |

## Deferred / out of scope

- **Native `async`/`await` syntax** — Niao uses explicit task ids; `nasync` is the ergonomic layer on top.
- **Event loop integration** — no single-threaded cooperative scheduler; tasks run on the shared `niao_io` thread pool (and Tokio for I/O futures in `net`).
- **Structured concurrency nursery auto-cancel on scope exit** — use `group_wait` / `group_cancel` explicitly.
- **Semaphore / lock primitives** — use `parallel` mutexes or implement rate limiting via `spawn_all(..., limit)`.
