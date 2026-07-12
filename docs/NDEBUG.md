# ndebug standard library

Checkpoint time-travel over Niao values with deep structural diff. **Opcode-level scrubbing** in the VM is a roadmap item — this library snapshots and compares value trees only.

## Import

```niao
import "ndebug"
```

Paths `import "std/ndebug"` and `import "ndebug"` are equivalent.

## Quick start

```niao
import "ndebug"

let h = ndebug.start()
ndebug.checkpoint(h, "init", {count: 0, items: [1, 2]})
// ... mutate state ...
ndebug.checkpoint(h, "later", {count: 2, items: [1, 2, 3]})

let d = ndebug.diff(h, "init", "later")
print(d.equal)       // false
print(d.changes)

let restored = ndebug.travel(h, "init")
print(restored)      // deep clone of init snapshot

ndebug.close(h)
```

## Functions

| Method | Description |
|--------|-------------|
| `ndebug.start()` | New debug session handle. |
| `ndebug.checkpoint(h, label, value)` | Save named deep snapshot. |
| `ndebug.checkpoint(h, value)` | Auto-label `cp_1`, `cp_2`, … |
| `ndebug.labels(h)` | Ordered label strings. |
| `ndebug.len(h)` | Checkpoint count. |
| `ndebug.get(h, label)` | Snapshot value (shared ref in session). |
| `ndebug.latest(h)` | Most recent checkpoint value. |
| `ndebug.at(h, i)` | Checkpoint by index. |
| `ndebug.diff(h, a, b)` | Deep diff between two checkpoints (`{equal, changes}`). |
| `ndebug.diff_value(h, label, value)` | Diff checkpoint vs live value. |
| `ndebug.travel(h, label)` | Deep clone of checkpoint (time-travel read). |
| `ndebug.clear(h)` | Remove all checkpoints. |
| `ndebug.clear(h, label)` | Remove one label. |
| `ndebug.close(h)` | Release handle. |

Functions and native handles are stored as descriptive strings in snapshots.

## Errors

| Code | Meaning |
|------|---------|
| 3280 | Wrong argument count. |
| 3281 | Unknown label / empty session (catchable `ndebug_error`). |
| 3282 | Wrong argument type. |
| 3283 | Invalid or closed handle (catchable `ndebug_error`). |
