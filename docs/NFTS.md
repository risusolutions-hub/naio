# nfts — Embedded Full-Text Search

`nfts` is a native Niao module for **embedded full-text search**: inverted index,
BM25 ranking, phrase and prefix queries, and facets. Designed as a Whoosh-class
API that pairs with `nvec` for hybrid keyword + vector RAG.

Import with:

```niao
import "nfts"
// or
import "std/nfts"
```

---

## Quick start

```niao
import "nfts"

let idx = nfts.open()
nfts.add(idx, "doc1", {title: "Rust book", body: "systems programming safety"}, {lang: "en"})
nfts.add(idx, "doc2", {title: "Search", body: "full text search with bm25"}, {lang: "en"})
nfts.add(idx, "doc3", {title: "Cats", body: "fluffy cats meow"}, {lang: "fr"})

let hits = nfts.search(idx, "systems programming", 5)
for h in hits {
    print(h.id, h.score, h.fields.title)
}

let buckets = nfts.facets(idx, "lang", "search OR programming")
for b in buckets {
    print(b.value, b.count)
}

nfts.close(idx)
```

---

## Query syntax

| Form | Meaning |
|------|---------|
| `term` | Match token (BM25) |
| `field:term` | Match in one field |
| `"exact phrase"` | Ordered phrase |
| `prefix*` | Prefix expansion |
| `a AND b` / juxtaposition | Conjunction |
| `a OR b` | Disjunction |
| `NOT a` / `a NOT b` | Negation |
| `(a OR b) AND c` | Grouping |

---

## API Reference

### `nfts.open(path?) -> handle_id`

Create an ephemeral index, or load from `path` if the file exists.

### `nfts.close(id) -> bool`

Close a handle. Returns `true` if it was open.

### `nfts.add(id, doc_id, fields, facets?) -> true | error`

Index a new document. `fields` is an object of string (or scalar) values to
tokenize. `facets` are exact-value facet dimensions (not tokenized). Returns a
catchable error if `doc_id` already exists.

### `nfts.update(id, doc_id, fields, facets?) -> true | error`

Insert or replace a document.

### `nfts.delete(id, doc_id) -> bool`

Remove a document. Returns `false` if missing.

### `nfts.get(id, doc_id) -> {id, fields{}, facets{}} | nil`

Fetch a stored document.

### `nfts.count(id) -> int`

Number of documents in the index.

### `nfts.search(id, query, top_k?, field_or_opts?) -> hits[]`

BM25-ranked search. `top_k` defaults to `10`. Optional 4th argument is a default
field name (`string`) or `{field: "body"}`.

Each hit: `{id, score, fields{}, facets{}}`.

### `nfts.suggest(id, prefix, field?, limit?) -> string[]`

Prefix completion over the term dictionary.

### `nfts.facets(id, facet_field, query?, limit?) -> {value, count}[]`

Facet value counts, optionally restricted to documents matching `query`.

### `nfts.schema(id) -> {fields: string[], facet_fields: string[]}`

Known indexed field and facet names.

### `nfts.save(id, path) -> true | error`

Persist the index to JSON (`.nfts`).

### `nfts.load(path) -> handle_id | error`

Load a previously saved index.

### `nfts.clear(id) -> true`

Drop all documents from the index.

### `nfts.analyze(text) -> string[]`

Tokenize text with the same analyzer used at index time (debug / preview).

---

## Errors

| Code | Kind | Meaning |
|------|------|---------|
| 4140 | `nfts_error` | Wrong arity |
| 4141 | `nfts_error` | Index / I/O / duplicate-id error (catchable) |
| 4142 | `nfts_error` | Type mismatch |
| 4143 | `nfts_error` | Invalid or closed handle (catchable) |

Hard errors (arity / type) throw. Recoverable failures return error values.

---

## Related

- [`nvec`](NVEC.md) — vector similarity search (combine for hybrid RAG)
- [`nnlp`](NNLP.md) — tokenization / BM25 helpers outside a durable index
