# njpath standard library

JSONPath / JMESPath-style queries, JSON Pointer and JSON Patch over in-memory JSON-like values. Native Rust implementation (~jmespath + jsonpath-ng + RFC 6901/6902 subset).

## Import

```niao
import "njpath"
```

Paths `import "std/njpath"` and `import "njpath"` are equivalent. Flat builtins (`njpath_find`, `njpath_pointer_get`, …) are also available globally after import.

## Quick start

```niao
import "njpath"

let doc = {
    store: {
        book: [
            {title: "A", price: 8.95},
            {title: "B", price: 12.99},
        ],
    },
}

// JSONPath
print(njpath.find(doc, "$.store.book[*].title"))   // ["A", "B"]
print(njpath.find_one(doc, "$.store.book[0].price")) // 8.95

// JMESPath
print(njpath.jmes(doc, "store.book[*].price | max(@)"))

// JSON Pointer
print(njpath.pointer_get(doc, "/store/book/0/title")) // "A"
let updated = njpath.pointer_set(doc, "/store/book/0/price", 9.99)

// JSON Patch
let patch = [{"op": "replace", "path": "/store/book/1/title", "value": "Revised"}]
print(njpath.patch_apply(doc, patch))

// Merge patch (RFC 7396)
print(njpath.merge(doc, {store: {bicycle: {color: "red"}}}))

// Diff two documents → RFC 6902 patch
print(njpath.diff(doc, updated))
```

## JSON Pointer (RFC 6901)

| Method | Description |
|--------|-------------|
| `njpath.pointer_get(doc, pointer)` | Value at pointer, or `nil` when missing. |
| `njpath.pointer_resolve(doc, pointer)` | Value at pointer; error if missing. |
| `njpath.pointer_exists(doc, pointer)` | Whether pointer resolves. |
| `njpath.pointer_set(doc, pointer, value)` | Immutable set — returns new document. |
| `njpath.pointer_remove(doc, pointer)` | Immutable remove — returns new document. |
| `njpath.pointer_test(doc, pointer, expected)` | RFC 6902 test semantics. |
| `njpath.pointer_join(base, token)` | Append escaped token. |
| `njpath.pointer_parent(pointer)` | Parent pointer (`""` at root). |
| `njpath.pointer_escape(token)` | Escape `/` and `~`. |
| `njpath.pointer_unescape(token)` | Unescape token. |

## JSON Patch (RFC 6902) & merge patch (RFC 7396)

| Method | Description |
|--------|-------------|
| `njpath.patch_apply(doc, ops)` | Apply patch array; returns new document. |
| `njpath.patch_test(doc, ops)` | Dry-run: `true` if patch would succeed. |
| `njpath.patch_valid(ops)` | Whether ops parse as valid patch. |
| `njpath.diff(before, after)` | Generate RFC 6902 diff. |
| `njpath.merge(doc, patch)` | Apply merge patch. |
| `njpath.patch_op_names(ops)` | Operation names in order (`add`, `remove`, …). |

## JSONPath (~jsonpath-ng subset)

| Method | Description |
|--------|-------------|
| `njpath.find(doc, query)` | All matches as array. |
| `njpath.find_one(doc, query)` | First match or `nil`. |
| `njpath.find_paths(doc, query)` | JSON Pointer strings for matches (best-effort). |
| `njpath.path_replace(doc, query, value)` | Replace matches. |
| `njpath.path_delete(doc, query)` | Delete matches (null replacement). |
| `njpath.path_valid(query)` | Whether query parses. |
| `njpath.compile_path(query)` | Compile → handle for reuse. |
| `njpath.path_search(handle, doc)` | Search with compiled handle. |
| `njpath.path_query(handle)` | Original query string. |
| `njpath.close(handle)` | Free compiled handle. |

Supports `$`, `.`, `[]`, `[*]`, slices, filters `[?(@.field)]`, and recursive descent `..`.

## JMESPath (~Python jmespath subset)

| Method | Description |
|--------|-------------|
| `njpath.jmes(doc, expression)` | Evaluate expression. |
| `njpath.jmes_valid(expression)` | Whether expression parses. |
| `njpath.compile_jmes(expression)` | Compile → handle. |
| `njpath.jmes_search(handle, doc)` | Search with compiled handle. |
| `njpath.jmes_expression(handle)` | Original expression string. |

Pipe expressions, projections, filters, and builtin functions (`length`, `sort`, `max`, `sum`, …) follow standard JMESPath semantics.

## Parallel batch

| Method | Description |
|--------|-------------|
| `njpath.parallel_find(docs, query, opts?)` | JSONPath over many docs. Returns array of match arrays. |
| `njpath.parallel_find_one(docs, query, opts?)` | First match per doc. |
| `njpath.parallel_jmes(docs, expression, opts?)` | JMESPath over many docs. |

`opts`: `{threads}` — defaults to CPU count.

## Errors

| Code | Meaning |
|------|---------|
| 4380 | Wrong argument count. |
| 4381 | Operation failed — catchable `njpath_error`. |
| 4382 | Wrong argument type. |
| 4383 | Invalid or closed handle — catchable `njpath_error`. |

## Deferred vs Python jsonpath-ng / jmespath / jsonpointer

Not in v0.1.0: JSONPath **update-or-create** auto-vivification beyond pointer `set`, custom JMESPath functions from Niao callbacks, JSON Patch **atomic** multi-doc transactions, JSONPath **AS/CSV** output modes, and streaming/file-backed query engines (use in-memory values; pair with `nmmap` + `json.parse` for large files). RFC 9535-only syntax not in jsonpath_lib2 is unsupported.
