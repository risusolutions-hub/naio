# nrepl standard library

Subprocess expression-evaluation sessions. Each `eval` writes a tiny wrapper script and runs `niao run` in an isolated process. A **`--watch-expr` CLI flag** for in-process REPL is a roadmap item.

## Import

```niao
import "nrepl"
```

Paths `import "std/nrepl"` and `import "nrepl"` are equivalent.

## Quick start

```niao
import "nrepl"

if nrepl.available() {
    let h = nrepl.start()
    let r = nrepl.eval(h, "1 + 2")
    print(r.stdout)          // "3"
    print(r.ok)              // true
    print(nrepl.len(h))      // 1
    print(nrepl.history(h))
    nrepl.close(h)
}
```

## Session options (`start`)

Optional object passed to `nrepl.start(opts)`:

| Field | Default | Description |
|-------|---------|-------------|
| `cwd` | process cwd | Working directory for subprocess runs. |
| `mode` | `"interp"` | `"interp"` or `"vm"` passed to `niao run --mode`. |
| `timeout_ms` | none | Kill eval subprocess after this many milliseconds. |

## Functions

| Method | Description |
|--------|-------------|
| `nrepl.start(opts?)` | New session handle. |
| `nrepl.eval(h, expr)` | Run expression; returns `{expr, stdout, stderr, code, ok}`. |
| `nrepl.history(h)` | Array of prior eval result objects. |
| `nrepl.cwd(h)` | Session working directory string. |
| `nrepl.len(h)` | Number of evals recorded. |
| `nrepl.available()` | `true` when `niao` binary is found (`PATH` or `NIAO_BIN`). |
| `nrepl.close(h)` | Release session; returns `true` if it existed. |

Invalid or closed handles return catchable `nrepl_error` (code 3271).

## Errors

| Code | Meaning |
|------|---------|
| 3270 | Wrong argument count. |
| 3271 | Semantic / I/O / timeout / missing binary / invalid session (catchable). |
| 3272 | Wrong argument type. |
