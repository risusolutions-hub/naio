# nmem — Script Long-Term Memory

In-memory key/value store for script “memory”: optional capacity (LRU eviction),
per-entry TTL with lazy expiry (no background threads), tags, key search, and
export/import. Handles follow the same pattern as `ncache`.

## Import

```niao
import "nmem"
```

Paths `import "std/nmem"` and `import "nmem"` are equivalent. Flat builtins
(`nmem_get`, `nmem_set`, …) are also available globally after import.

## Quick start

```niao
import "nmem"

let mem = nmem.new(1000)                    // keep up to 1000 entries (LRU)
nmem.set(mem, "user:42", {name: "vivek"})
nmem.tag(mem, "user:42", "people")

print(nmem.get(mem, "user:42"))             // {name: "vivek"}
print(nmem.by_tag(mem, "people"))           // ["user:42"]
print(nmem.search(mem, "user:"))            // keys containing "user:"
print(nmem.stats(mem))                      // {len, capacity, hits, misses}

nmem.set(mem, "flash", "gone", 500)         // 500ms TTL
nmem.close(mem)
```

## Creating memory

| Method | Description |
|--------|-------------|
| `nmem.new(capacity?)` | Create a store. Omit or pass `0` for unbounded; positive = max entries (LRU on overflow). |
| `nmem.close(handle)` | Free the store. Returns `true` if the handle was open. |

## Operations

| Method | Description |
|--------|-------------|
| `nmem.set(h, key, value, ttl_ms?)` | Insert/replace; optional per-entry TTL in milliseconds. |
| `nmem.get(h, key)` | Value or `nil`; counts hit/miss; refreshes LRU recency. |
| `nmem.has(h, key)` | Peek; expires lazily; does not count hit/miss. |
| `nmem.remove(h, key)` | `true` if the key existed. |
| `nmem.clear(h)` | Drop all entries (stats survive). |
| `nmem.len(h)` | Entry count (may include not-yet-touched expired entries). |
| `nmem.tag(h, key, tag)` | Attach a string tag to an existing key; `true` on success. |
| `nmem.by_tag(h, tag)` | Sorted array of live keys with that tag. |
| `nmem.search(h, substr)` | Sorted array of live keys containing `substr`. |
| `nmem.stats(h)` | `{len, capacity, hits, misses}` (`capacity` is `0` when unbounded). |
| `nmem.export(h)` | Object of string keys → stored values (expired skipped). |
| `nmem.import(h, obj)` | Merge object entries into the store (no TTL). |

Expired entries are removed lazily on `get` / `has` / `tag` / `by_tag` /
`search` / `export`. There is no background purge thread.

## Errors

| Code | Meaning |
|------|---------|
| 3050 | Wrong argument count (`E3050_NMEM_ARITY`). |
| 3051 | Operation failed — bad capacity/TTL/missing key for tag (`E3051_NMEM_ERROR`). |
| 3052 | Wrong argument type (`E3052_NMEM_TYPE`). |
| 3053 | Invalid or closed memory handle — catchable `error` (`E3053_NMEM_INVALID_HANDLE`). |
