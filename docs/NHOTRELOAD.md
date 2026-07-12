# nhotreload standard library

File watch plus per-function body diff via `niao_parser`. **Live VM swap** (hot-patching running code) is a roadmap item — this library detects and reports function-level source changes only.

## Import

```niao
import "nhotreload"
```

Paths `import "std/nhotreload"` and `import "nhotreload"` are equivalent. Flat builtins (`nhotreload_watch`, `nhotreload_poll`, …) are also available globally after import.

## Quick start

```niao
import "nhotreload"

let h = nhotreload.watch("src/app.niao")
if nhotreload.poll(h) {
    let changes = nhotreload.diff(h)
    print("changed functions:", changes)
}
nhotreload.close(h)

let fns = nhotreload.parse("fn add(a: int) -> int { return a + 1 }")
print(fns[0].name, fns[0].line)
```

## Watch session

| Method | Description |
|--------|-------------|
| `nhotreload.watch(path)` | Parse `path`, cache function bodies, return a handle. |
| `nhotreload.changed(h)` | `true` when file mtime differs from last stored (no update). |
| `nhotreload.poll(h)` | Update mtime + function cache; `true` when the file changed. |
| `nhotreload.diff(h)` | After `poll`, array of `{name, old, new}` for changed/added/removed functions. |
| `nhotreload.path(h)` | Watched file path. |
| `nhotreload.close(h)` | Free handle; returns `true` if it existed. |

## Parse / diff (no handle)

| Method | Description |
|--------|-------------|
| `nhotreload.functions(path)` | Parse file; return `[{name, body, line}, …]`. |
| `nhotreload.parse(source)` | Parse source string; same shape as `functions`. |
| `nhotreload.diff_sources(old, new)` | Compare two source strings; return change array. |

Top-level `fn` items and class `Method` / `StaticMethod` members are extracted. Class methods are named `ClassName.method`.

## Roadmap

- In-process live VM function swap after `diff` (requires VM support).

## Errors

| Code | Meaning |
|------|---------|
| 3200 | Wrong argument count. |
| 3201 | I/O, parse, or semantic error (catchable `nhotreload_error`). |
| 3202 | Wrong argument type. |
| 3203 | Invalid or closed handle (catchable `nhotreload_error`). |
