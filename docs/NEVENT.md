# nevent standard library

In-process event emitter and pub-sub with dot-separated typed topics and `*` / `**` wildcard patterns. A scoped port of Python **blinker** / **pyee** patterns for Niao.

Handlers are native-backed; dispatch runs synchronously on the calling thread (no background delivery queue).

## Import

```niao
import "nevent"
```

Paths `import "std/nevent"` and `import "nevent"` are equivalent. Flat builtins (`nevent_on`, `nevent_emit`, …) are also available globally after import.

## Quick start

```niao
import "nevent"

let bus = nevent.new()
let hits = 0

nevent.on(bus, "user.created", fn(user) {
    hits = hits + 1
    print("created", user.id)
})

nevent.on(bus, "user.*", fn(topic, user) {
    print("wildcard", topic, user.id)
})

let r = nevent.emit(bus, "user.created", {id: 42})
print(r.called)   // 2 — exact + wildcard listener

nevent.once(bus, "boot", fn() { print("once") })
nevent.emit(bus, "boot")
nevent.emit(bus, "boot")   // once handler not called again

nevent.off(bus, "user.created")
nevent.close(bus)
```

Use `nevent.global()` for a process-wide default bus (lazy singleton).

## Topics & wildcards

Topics are dot-separated segments: `order.paid`, `app.http.request`.

| Pattern | Matches |
|---------|---------|
| `user.created` | Exactly `user.created` |
| `user.*` | One segment: `user.login`, `user.logout` |
| `user.**` | Zero or more: `user`, `user.admin.login` |
| `a.**.c` | `a.b.c`, `a.x.y.c` |

Helpers:

| Method | Description |
|--------|-------------|
| `nevent.match_topic(pattern, topic)` | Whether `topic` matches `pattern`. |
| `nevent.parse_topic(topic)` | Split a literal topic into segment array. |
| `nevent.join_topic(segments)` | Join segments into a topic string. |
| `nevent.normalize_topic(s)` | Trim and collapse duplicate dots. |
| `nevent.is_valid_topic(s)` | Literal topic validation (no wildcards). |
| `nevent.is_valid_pattern(s)` | Pattern validation (may include wildcards). |

**Emit** requires a **literal** topic (no wildcards). **Subscribe** accepts patterns.

Wildcard listeners receive the **actual topic** as the first argument, then emit payload arguments. Exact listeners receive only the payload.

## Emitter API

| Method | Description |
|--------|-------------|
| `nevent.new(opts?)` | Create emitter handle. |
| `nevent.global()` | Lazy process-wide bus handle. |
| `nevent.close(handle)` | Destroy emitter and subscriptions. |
| `nevent.on(handle, pattern, fn)` | Subscribe; returns subscription id. |
| `nevent.once(handle, pattern, fn)` | Subscribe for one delivery. |
| `nevent.off(handle, pattern, fn?)` | Unsubscribe by pattern (and optional handler). |
| `nevent.off_id(handle, sub_id)` | Unsubscribe by subscription id. |
| `nevent.emit(handle, topic, ...args)` | Dispatch synchronously; returns `{called, queued, errors?}`. |
| `nevent.pause(handle)` | Queue emits until `resume` / `flush`. |
| `nevent.resume(handle)` | Allow live dispatch again. |
| `nevent.flush(handle)` | Drain queued emits; returns `{called, batches}`. |
| `nevent.clear(handle, pattern?)` | Remove all (or pattern) subscriptions. |
| `nevent.listener_count(handle, topic?)` | Count matching subscriptions. |
| `nevent.has_listeners(handle, topic?)` | Whether any subscription matches. |
| `nevent.topics(handle)` | Distinct active pattern strings. |
| `nevent.stats(handle)` | `{emits, deliveries, subscriptions, paused, pending}`. |

### Options (`nevent.new`)

| Key | Default | Meaning |
|-----|---------|---------|
| `max_listeners` | `128` | Per-pattern cap (`0` = unlimited). |

### Handler errors

If a listener throws, `emit` collects error strings in `errors` and continues with remaining listeners (pyee-style best effort). Other listeners still run.

### Pause / flush

While paused, `emit` queues `{topic, args}` tuples. `flush` replays them in order after `resume` (or while still paused).

## Errors

| Code | Meaning |
|------|---------|
| 3508 | Wrong argument count. |
| 3509 | Operation failed (catchable `nevent_error`). |
| 3510 | Wrong argument type (hard error). |
| 3511 | Invalid or closed emitter handle. |
| 3512 | Invalid topic / pattern. |

## Deferred / not in v0.1.0

- Weak listener references and automatic GC cleanup (blinker `weak=True`)
- Per-listener sender filtering (blinker `sender=` / `connect(sender=…)`)
- Async / threaded delivery (`emit_async`) — use `nasync` for background work inside handlers
- Cross-process or network pub-sub

## See also

- `nasync` — structured async tasks and channels
- `nwatch` — poll-based reactive watchers
- `nsignal` — OS signal delivery
- `nfunc` — debounce/throttle for handler rate control
