# nsignal — OS signal handlers & graceful shutdown

Cross-platform OS signal registration, deferred handler delivery, and graceful-shutdown helpers (~Python `signal` stdlib subset). OS handlers only enqueue signal numbers; user callbacks run when you call `nsignal.poll()` (or blocking `wait` / `pause`) from normal code.

## Import

```niao
import "nsignal"
```

Paths `import "std/nsignal"` and `import "nsignal"` are equivalent. Flat builtins (`nsignal_on`, `nsignal_poll`, …) are also available globally after import.

## Quick start

```niao
import "nsignal"

nsignal.on(nsignal.SIGINT, fn(info) {
    print("caught", info.name)
})

// In your main loop:
while running {
    let handled = nsignal.poll()
    if len(handled) > 0 { running = false }
}

// Or register a one-shot graceful shutdown:
let guard = nsignal.shutdown(fn() {
    print("flushing buffers…")
    nos.exit(0)
})
// guard.id — pass to nsignal.shutdown_cancel(guard.id) to unregister
```

## Constants

| Name | Meaning |
|------|---------|
| `nsignal.SIGINT`, `SIGTERM`, `SIGHUP`, … | Platform signal numbers (uppercase aliases on the namespace). |
| `nsignal.SIG_DFL` | Sentinel (`-1`) — OS default disposition. |
| `nsignal.SIG_IGN` | Sentinel (`-2`) — ignore signal. |

Use `nsignal.valid()` for the full list on the current OS.

## Functions

| Method | Description |
|--------|-------------|
| `nsignal.on(sig, handler)` | Register a callable; `handler(info)` receives `{signum, name, description}`. Returns `true` or catchable `nsignal_error`. |
| `nsignal.off(sig)` | Remove handler and restore OS default. |
| `nsignal.get(sig)` | Current handler: callable, `SIG_DFL`, or `SIG_IGN`. |
| `nsignal.ignore(sig)` | Ignore at OS level. |
| `nsignal.default(sig)` | Restore OS default. |
| `nsignal.poll()` | Drain pending signals, invoke handlers, return array of handled signums. |
| `nsignal.pending()` | Peek pending signums without invoking handlers. |
| `nsignal.pause()` | Block until any watched signal; invoke handler; return signum. |
| `nsignal.wait(sig, timeout_ms?)` | Block for `sig` (`timeout_ms` default `-1` = infinite); return signum or `nil`. |
| `nsignal.raise(sig)` | Raise signal in current process (platform permitting). |
| `nsignal.alarm(seconds)` | Unix `alarm(2)`; returns previous seconds (errors on Windows). |
| `nsignal.name(sig)` | Lowercase name (`"sigint"`). |
| `nsignal.number(name)` | Parse `"SIGINT"` / `"int"` → signum. |
| `nsignal.strsignal(sig)` | Human-readable label. |
| `nsignal.valid()` | Array of valid platform signums. |
| `nsignal.info(sig)` | `{signum, name, description, handler}`. |
| `nsignal.shutdown(handler, signals?)` | Register `handler` on `SIGINT`+`SIGTERM` (or custom array); returns `{id, kind}`. |
| `nsignal.shutdown_cancel(id)` | Unregister a shutdown guard. |
| `nsignal.reset()` | Clear all handlers and restore OS defaults. |

### Signal arguments

Functions accepting `sig` take either an **int** signum or a **string** name (`"SIGTERM"`, `"term"`).

### Delivery model

Like Python's signal module, handlers are **not** invoked inside the OS signal handler. Call `nsignal.poll()` from your event loop (or use `pause` / `wait`) to run user code safely.

## Errors

| Code | Meaning |
|------|---------|
| 3480 | Wrong argument count. |
| 3481 | Registration / raise / platform error (catchable `nsignal_error`). |
| 3482 | Wrong argument type (hard error). |
| 3483 | Invalid signal number or name. |

## See also

- `nos` — process exit, PID, lightweight OS helpers.
- `nwatch` — explicit poll watchers for files and values.
- `ncrash` — structured crash reports and `wrap(fn)` guards.
