# nwatch standard library

Reactive poll watchers for filesystem mtimes and in-memory values. No background threads — call `poll` / `take_changed` from your own loop.

## Import

```niao
import "nwatch"
```

Paths `import "std/nwatch"` and `import "nwatch"` are equivalent. Flat builtins (`nwatch_file`, `nwatch_poll`, …) are also available globally after import.

## Quick start

```niao
import "nwatch"

// File mtime watch
let f = nwatch.file("config.toml")
if nwatch.poll(f) {
    print("config changed:", nwatch.path(f))
}

// In-memory value watch
let v = nwatch.value(0)
nwatch.set(v, 1)
print(nwatch.take_changed(v))   // 1
print(nwatch.take_changed(v))   // nil (already taken)
```

## File watches

| Method | Description |
|--------|-------------|
| `nwatch.file(path)` | Create a handle storing `path` and the current mtime (`nil` mtime if the path is missing). |
| `nwatch.changed(h)` | `true` when the current mtime differs from the last stored one (does **not** update). |
| `nwatch.poll(h)` | Like `changed`, then updates the stored mtime. |
| `nwatch.path(h)` | Watched path string (file handles). |

Mtime is read with `std::fs::metadata` / `modified`. Missing → present, present → missing, or a real timestamp change all count as changed.

## Value watches

| Method | Description |
|--------|-------------|
| `nwatch.value(init)` | Create an in-memory watch with initial value (starts clean). |
| `nwatch.set(h, v)` | Replace the value; marks dirty when `v` differs from the previous value. |
| `nwatch.take_changed(h)` | Current value if dirty since the last take, else `nil`. Clears the dirty flag. |

## Shared

| Method | Description |
|--------|-------------|
| `nwatch.kind(h)` | `"file"` or `"value"`. |
| `nwatch.close(h)` | Free the handle; returns `true` if it existed. |
| `nwatch.path(h)` | Path for file watches; `nil` for value watches. |

Handles are opaque ints (`Value::Int`).

## Errors

| Code | Meaning |
|------|---------|
| 3100 | Wrong argument count. |
| 3101 | Semantic error — e.g. wrong watch kind for the operation (catchable). |
| 3102 | Wrong argument type. |
| 3103 | Invalid or closed handle (catchable). |
