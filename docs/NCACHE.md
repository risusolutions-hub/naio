# ncache standard library

In-memory LRU and TTL caches with hit/miss statistics. O(log n) touch/evict via a BTreeMap recency index, lazy TTL expiry (no background threads), string keys, any Niao value.

## Import

```niao
import "ncache"
```

Paths `import "std/ncache"` and `import "ncache"` are equivalent. Flat builtins (`ncache_get`, `ncache_set`, …) are also available globally after import.

## Quick start

```niao
import "ncache"

let lru = ncache.new_lru(1000)               // keep the 1000 hottest entries
ncache.set(lru, "user:42", {name: "vivek"})
let user = ncache.get(lru, "user:42")        // nil on miss

let sessions = ncache.new_ttl(60000)          // 60s default TTL
ncache.set(sessions, "sess:abc", "token")
ncache.set(sessions, "sess:tmp", "x", 500)    // per-entry TTL override (ms)

print(ncache.stats(lru))   // {hits, misses, len, capacity, hit_rate}
```

## Creating caches

| Method | Description |
|--------|-------------|
| `ncache.new_lru(capacity)` | Least-recently-used eviction once `capacity` is exceeded. |
| `ncache.new_ttl(default_ttl_ms, max_size?)` | Entries expire after their TTL; optional size cap (LRU on overflow). |
| `ncache.close(handle)` | Free the cache. |

## Operations

| Method | Description |
|--------|-------------|
| `ncache.set(h, key, value, ttl_ms?)` | Insert/replace; optional per-entry TTL. |
| `ncache.get(h, key)` | Value or `nil`; counts hit/miss; refreshes recency. |
| `ncache.get_or(h, key, fallback)` | `fallback` returned (not stored) on miss. |
| `ncache.has(h, key)` | Peek without touching stats or recency. |
| `ncache.remove(h, key)` | `true` if the key existed. |
| `ncache.clear(h)` | Drop all entries (stats survive). |
| `ncache.len(h)` | Entry count (may include not-yet-purged expired entries). |
| `ncache.keys(h)` | Array of keys. |
| `ncache.purge(h)` | Remove all expired entries now; returns count. |
| `ncache.stats(h)` | `{hits, misses, len, capacity, hit_rate}`. |

Expired entries are removed lazily on `get`/`has` — call `purge()` if you need memory back eagerly.

## Errors

| Code | Meaning |
|------|---------|
| 2670 | Wrong argument count. |
| 2671 | Operation failed (bad capacity/TTL). |
| 2672 | Invalid or closed cache handle (catchable `error`). |
