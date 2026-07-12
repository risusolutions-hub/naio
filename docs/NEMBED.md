# nembed standard library

Content-hash embedding cache with a local deterministic embedder. Vectors are SHA-256 seeded, L2-normalized float arrays — no external model or network calls.

## Import

```niao
import "nembed"
```

Paths `import "std/nembed"` and `import "nembed"` are equivalent. Flat builtins (`nembed_open`, `nembed_embed`, …) are also available globally after import.

## Quick start

```niao
import "nembed"

let cache = nembed.open(128)                    // embedding dimension (default 384)
let v1 = nembed.get_or_embed(cache, "hello")    // float[] — cached by content hash
let v2 = nembed.get_or_embed(cache, "hello")    // cache hit
print(nembed.cosine(v1, v2))                    // 1.0

let raw = nembed.embed("world", 64)             // pure function, no cache
print(nembed.hash("world"))                     // SHA-256 hex fingerprint
print(nembed.stats(cache))                      // {hits, misses, len, dim, hit_rate}
nembed.close(cache)
```

## Cache lifecycle

| Method | Description |
|--------|-------------|
| `nembed.open(dim?)` | Open a cache handle. `dim` defaults to 384 (range 8..=4096). |
| `nembed.close(handle)` | Free the cache. Returns `true` if the handle existed. |
| `nembed.dim(handle)` | Configured embedding dimension. |

## Embedding

| Method | Description |
|--------|-------------|
| `nembed.hash(text)` | SHA-256 hex digest of `text` (cache key). |
| `nembed.embed(text, dim?)` | Deterministic L2-normalized vector without caching. |
| `nembed.get(handle, text)` | Cached `float[]` or `nil` on miss (counts miss). |
| `nembed.get_or_embed(handle, text)` | Return cached vector or compute, store, and return. |
| `nembed.embed_batch(handle, texts)` | Array of `float[]` for each string (with cache). |
| `nembed.has(handle, text)` | `true` when the content hash is cached (no stat change). |
| `nembed.cosine(a, b)` | Cosine similarity of two equal-length `float[]` vectors. |

## Cache management

| Method | Description |
|--------|-------------|
| `nembed.clear(handle)` | Drop all cached vectors (stats survive). |
| `nembed.len(handle)` | Number of cached embeddings. |
| `nembed.stats(handle)` | `{hits, misses, len, dim, hit_rate}`. |

## Errors

| Code | Meaning |
|------|---------|
| 3310 | Wrong argument count. |
| 3311 | Invalid dimension or cosine mismatch (catchable). |
| 3312 | Type mismatch (hard error). |
| 3313 | Invalid or closed cache handle (catchable). |
