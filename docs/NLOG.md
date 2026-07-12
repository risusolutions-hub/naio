# nlog standard library

Lightweight structured logging: five levels, key-value fields, text or JSON lines, stderr/stdout/file sinks. One atomic level check gates all work — disabled log calls cost about a nanosecond.

## Import

```niao
import "nlog"
```

Paths `import "std/nlog"` and `import "nlog"` are equivalent. Flat builtins (`nlog_info`, `nlog_init`, …) are also available globally after import.

## Quick start

```niao
import "nlog"

nlog.init("info")
nlog.info("server ready", "port", 8080)
nlog.warn("slow query", "ms", 250, "table", "users")
nlog.error("db down", "attempts", 3)
```

Text output (stderr by default):

```
2026-07-12T09:30:12.412Z INFO server ready port=8080
```

## Setup

| Method | Description |
|--------|-------------|
| `nlog.init(level?, opts?)` | Configure. Level default `"info"`. Opts: `{format: "text"\|"json", file: "app.log", stdout: true, timestamps: bool}`. |
| `nlog.set_level(level)` / `nlog.get_level()` | Change/read level at runtime. |
| `nlog.enabled(level)` | `true` if records at `level` would be written. |

Levels: `trace` < `debug` < `info` < `warn` < `error` < `off`.

## Logging

`nlog.trace / debug / info / warn / error(msg, k1, v1, k2, v2, ...)`

Fields are trailing key/value pairs — keys must be strings, values may be any type (rendered via display). An odd number of trailing arguments is an arity error.

## Context fields

Global fields attached to every record (great for request/job ids):

```niao
nlog.context({service: "api", region: "eu"})
nlog.info("boot")            // ... service=api region=eu
nlog.clear_context()
```

## JSON mode

```niao
nlog.init("info", {format: "json", file: "app.log"})
nlog.info("ready", "port", 8080)
// {"ts":"2026-07-12T09:30:12.412Z","level":"info","msg":"ready","port":8080}
```

## Errors

| Code | Meaning |
|------|---------|
| 2640 | Wrong argument count / odd key-value pairs. |
| 2641 | Sink failure or unknown level/format (catchable `error`). |
| 2642 | Type mismatch (non-string field key). |
