# nvec — Niao Vector Database

`nvec` is a native Niao runtime module providing vector similarity search
(cosine distance) with two backends:

| Backend | When to use |
|---------|-------------|
| **In-memory** (default) | Local embeddings, RAG prototypes, test suites, small corpora (< 1 M vectors) |
| **Qdrant REST** | Production scale, persistent storage, filtered search |

Import with:

```niao
import "nvec"
// or
import "std/nvec"
```

---

## API Reference

### `nvec.open(path?, dim?) -> handle_id`

Creates (or loads) an in-memory vector index.

| Argument | Type | Default | Description |
|----------|------|---------|-------------|
| `path` | string | — | File path to load/save the index. If the file exists it is loaded; otherwise a new index is created at that path. |
| `dim` | int | 0 (auto) | Fixed vector dimension. If omitted, the dimension is inferred from the first `insert`/`upsert`. |

Arguments may appear in either order; both are optional.

```niao
let idx = nvec.open()                    // ephemeral, dim auto-detected
let idx = nvec.open(128)                 // ephemeral, 128-D
let idx = nvec.open("data/embed.nvecd") // load or create file-backed index
let idx = nvec.open("data/embed.nvecd", 384) // load/create with fixed dim
```

---

### `nvec.connect(url, api_key?, collection?) -> handle_id`

Opens a handle to a running **Qdrant** instance over its REST API.

| Argument | Type | Default | Description |
|----------|------|---------|-------------|
| `url` | string | required | Base URL, e.g. `"http://localhost:6333"` |
| `api_key` | string | — | Qdrant API key (sent as both `api-key` and `x-api-key` headers) |
| `collection` | string | `"niao_default"` | Collection name |

No network call is made until the first `insert`/`upsert`/`search`. The
collection is created automatically (with the vector dimension of the first
upserted vector) if it does not exist.

```niao
let q = nvec.connect("http://localhost:6333")
let q = nvec.connect("https://xyz.qdrant.io", "my-api-key", "products")
```

---

### `nvec.insert(id, vec_id, vector, metadata) -> true | error`

Inserts a new vector. Returns an **error value** if `vec_id` already exists
in an in-memory index (use `nvec.upsert` to overwrite).

| Argument | Type | Description |
|----------|------|-------------|
| `id` | handle | Handle from `open` or `connect` |
| `vec_id` | string | Unique identifier for this vector |
| `vector` | float[] | Embedding values (all elements must be numbers) |
| `metadata` | object | Arbitrary key-value metadata attached to the vector |

```niao
nvec.insert(idx, "doc:42", [0.1, 0.9, 0.3], {source: "wiki", lang: "en"})
```

---

### `nvec.upsert(id, vec_id, vector, metadata) -> true | error`

Like `insert` but silently replaces any existing vector with the same `vec_id`.

```niao
nvec.upsert(idx, "user:7", embeddings, {role: "admin", active: true})
```

---

### `nvec.search(id, query, top_k?, threshold?) -> hits[]`

Returns up to `top_k` vectors closest to `query` by **cosine similarity**,
filtered to `score >= threshold`.

| Argument | Type | Default | Description |
|----------|------|---------|-------------|
| `id` | handle | required | |
| `query` | float[] | required | Query embedding |
| `top_k` | int | `10` | Maximum hits to return |
| `threshold` | float | `0.0` | Minimum cosine score (0.0–1.0) |

Each hit is an **object** with three fields:

```niao
{
    id:       "doc:42",     // string vec_id
    score:    0.9742,       // cosine similarity (0.0–1.0)
    metadata: {source: "wiki", lang: "en"}
}
```

Results are sorted by descending score.

```niao
let hits = nvec.search(idx, query_embedding, 5)
let hits = nvec.search(idx, query_embedding, 10, 0.7)  // only score >= 0.7

for hit in hits {
    print(hit.id, hit.score, hit.metadata.source)
}
```

---

### `nvec.delete(id, vec_id) -> bool`

Removes a vector by its `vec_id`. Returns `true` if the vector existed,
`false` if not found.

```niao
let removed = nvec.delete(idx, "doc:42")
```

---

### `nvec.count(id) -> int`

Returns the number of live (non-deleted) vectors in the index.

```niao
print("vectors:", nvec.count(idx))
```

---

### `nvec.save(id, path) -> true | error`

Persists an in-memory index to a binary file (`.nvecd` format). Not
applicable to Qdrant handles.

```niao
nvec.save(idx, "data/embed.nvecd")
```

---

### `nvec.load(path) -> handle_id | error`

Loads a `.nvecd` file and returns a new in-memory handle.

```niao
let idx = nvec.load("data/embed.nvecd")
```

---

### `nvec.close(id) -> bool`

Releases the handle and its resources. Returns `true` if the handle existed.

```niao
nvec.close(idx)
```

---

## Error Handling

All functions that operate on an index handle return an **error value**
(detectable with `error_value()` / `type(v) == "error"`) on failure rather
than raising a hard runtime error. Hard errors (arity violations, type errors)
are still thrown.

```niao
let hits = nvec.search(idx, query, 5)
if type(hits) == "error" {
    print("search failed:", hits.message)
} else {
    for h in hits { print(h.id, h.score) }
}
```

| Error code | Meaning |
|-----------|---------|
| `E2790` | Wrong number of arguments |
| `E2791` | Index / search / I/O operation failed |
| `E2792` | Type mismatch (e.g. non-numeric vector element) |
| `E2793` | Invalid or already-closed handle |

---

## In-memory Search Algorithm

| Index size | Algorithm | Complexity |
|-----------|-----------|------------|
| N ≤ 256 | Brute-force cosine | O(N·D) exact |
| N > 256 | NSW graph (HNSW-lite) | Sub-linear approximate |

The **NSW graph** (Navigable Small World — the single-layer base of HNSW) is
built incrementally as vectors are upserted. Parameters:

| Parameter | Value | Description |
|-----------|-------|-------------|
| `M` | 16 | Max bidirectional neighbours per node |
| `ef_construction` | 64 | Candidate list size during graph build |
| `ef_search` | 64 | Candidate list size during query |

For exact results at any scale, reduce `threshold` to `0.0` and use
brute-force by setting the NSW threshold above your corpus size (currently
requires recompilation). For production exact search at millions of vectors,
use the **Qdrant backend**.

---

## Persistence Format (`.nvecd`)

The binary format is version-tagged and self-describing:

```
[4 bytes] magic: "NVEC"
[1 byte]  version: 1
[4 bytes] dim: u32 LE
[4 bytes] count: u32 LE
per entry:
  [4+N bytes] id string (u32 len + UTF-8 bytes)
  [dim × 4]   f32 vector (LE)
  [4 bytes]   meta count: u32 LE
  per meta k/v:
    [4+N bytes] key string
    [1 byte]    value type (0=nil, 1=bool, 2=i64, 3=f64, 4=str)
    [variable]  value bytes
```

---

## Performance Notes

- **Dimension overhead**: each f32 occupies 4 bytes; a 1 536-D OpenAI ada-002
  embedding uses ~6 KB per vector. 100 K vectors ≈ 600 MB RAM.
- **Cosine brute-force** (N ≤ 256): typically < 0.1 ms on modern hardware.
- **NSW search** (N > 256): sub-linear average case; depends heavily on
  intrinsic dimensionality and `M`. For well-clustered data it is ~10–50×
  faster than brute-force at N = 1 M.
- **Qdrant backend**: latency is network-bound (LAN: 1–5 ms). Set `api_key`
  to a non-empty string even for local Qdrant if TLS is enabled.
- **Save/load** is a sequential write/read of the raw binary format; 100 K
  vectors at 384-D takes ≈ 150 ms on a spinning disk.

---

## Full Example

```niao
import "nvec"

// Build an in-memory index and persist it.
let idx = nvec.open("model_cache.nvecd", 4)

nvec.upsert(idx, "apple",  [0.9, 0.1, 0.2, 0.0], {category: "fruit", calories: 52})
nvec.upsert(idx, "banana", [0.8, 0.2, 0.1, 0.1], {category: "fruit", calories: 89})
nvec.upsert(idx, "carrot", [0.1, 0.8, 0.6, 0.2], {category: "veggie", calories: 41})
nvec.upsert(idx, "steak",  [0.0, 0.3, 0.9, 0.8], {category: "meat",  calories: 271})

print("count:", nvec.count(idx))

let query = [0.85, 0.15, 0.15, 0.05]
let hits = nvec.search(idx, query, 3, 0.5)
print("Top-3 nearest to query:")
for h in hits {
    print(" ", h.id, "score:", h.score, "meta:", h.metadata)
}

nvec.save(idx, "model_cache.nvecd")
nvec.close(idx)

// Reload later.
let idx2 = nvec.load("model_cache.nvecd")
print("Reloaded count:", nvec.count(idx2))
nvec.close(idx2)
```
