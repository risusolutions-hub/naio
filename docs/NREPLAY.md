# nreplay standard library

Deterministic event record / replay sessions. Capture labeled events with relative timestamps (`t_ms` since `start`), inspect them, and persist to a simple line format.

## Import

```niao
import "nreplay"
```

Paths `import "std/nreplay"` and `import "nreplay"` are equivalent. Flat builtins (`nreplay_start`, `nreplay_record`, …) are also available globally after import.

## Quick start

```niao
import "nreplay"

fn main() {
    let h = nreplay.start()
    nreplay.record(h, "rng", "42")
    nreplay.record(h, "tick", 1)
    print(nreplay.running(h))   // true
    nreplay.stop(h)
    print(nreplay.len(h))       // 2

    let ev = nreplay.play(h, 0) // {kind: "rng", data: "42", t_ms: ...}
    print(ev.kind, ev.data, ev.t_ms)

    nreplay.save(h, "session.nrep")
    nreplay.close(h)

    let h2 = nreplay.load("session.nrep")
    print(nreplay.running(h2))  // false (loaded sessions are stopped)
    print(nreplay.events(h2))
    nreplay.close(h2)
}
```

## Functions

| Method | Description |
|--------|-------------|
| `nreplay.start()` | Begin a new recording session. Returns a positive integer handle. `running` is `true`. |
| `nreplay.stop(h)` | Mark the session stopped (`running` → `false`). Returns `true`. Does not clear events. |
| `nreplay.record(h, kind, data)` | Append an event. `kind` is a non-empty string; `data` is any value. `t_ms` is milliseconds since `start`. Returns `true`. |
| `nreplay.events(h)` | Array of `{kind, data, t_ms}` for every recorded event. |
| `nreplay.len(h)` | Number of events. |
| `nreplay.play(h, i)` | Event at index `i`, or `nil` if out of range. |
| `nreplay.save(h, path)` | Write events to `path` (one line per event). Returns `true`, or catchable `nreplay_error` on I/O failure. |
| `nreplay.load(path)` | Load a saved file into a new handle (`running` is `false`). Data fields are strings. Catchable `nreplay_error` on I/O or parse failure. |
| `nreplay.clear(h)` | Remove all events; keep the handle. Returns `true`. |
| `nreplay.close(h)` | Release the handle. Returns `true` if it existed, else `false`. |
| `nreplay.running(h)` | `true` after `start` until `stop` (or for a loaded session: always `false` until you call `start` on a new handle). |

Invalid handles return a catchable `nreplay_error` (code 3143) rather than a hard abort, except where arity/type checks fail first.

## File format

Each line:

```
kind|||stringdata|||t_ms
```

- Separator is the literal string `|||`.
- `stringdata` is `Value.to_string()` of the recorded data (so loaded data is always a string).
- `t_ms` is a decimal integer (milliseconds since session `start`).
- Blank lines are ignored on load.

Example:

```
rng|||42|||3
tick|||1|||15
```

## Errors

| Code | Meaning |
|------|---------|
| 3140 | Wrong argument count. |
| 3141 | Semantic / I/O / parse error (catchable `nreplay_error`). |
| 3142 | Wrong argument type. |
| 3143 | Invalid or closed handle (catchable `nreplay_error`). |
