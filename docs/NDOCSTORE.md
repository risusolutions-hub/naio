# ndocstore standard library

Embedded JSON document store with queries and secondary indexes. Native Rust implementation (~tinydb subset).

## Import

```niao
import "ndocstore"
```

Paths `import "std/ndocstore"` and `import "ndocstore"` are equivalent. Flat builtins (`ndocstore_memory`, `ndocstore_insert`, …) are also available globally after import.

## Quick start

```niao
import "ndocstore"

let db = ndocstore.memory()
ndocstore.insert(db, {name: "Ada", age: 36, tags: ["math", "cs"]})
ndocstore.insert(db, {name: "Bob", age: 25, tags: ["art"]})

ndocstore.create_index(db, "age")

let adults = ndocstore.search(db, {gte: {age: 30}})
print(adults[0].name)                 // Ada

ndocstore.update(db, {age: 37}, {name: "Ada"})
print(ndocstore.get(db, 1).age)       // 37

let users = ndocstore.table(db, "users")
ndocstore.insert(users, {login: "ada"})
print(ndocstore.tables(db))           // ["_default", "users"]

ndocstore.close(db)
```

File-backed stores use TinyDB-compatible JSON (`{"_default": {"1": {...}, ...}}`) plus an `_ndocstore` metadata object for indexes:

```niao
let db = ndocstore.open("data.json")
ndocstore.insert(db, {ok: true})
ndocstore.flush(db)
ndocstore.close(db)
```

## Constructors & lifecycle

| Method | Description |
|--------|-------------|
| `ndocstore.memory()` | In-memory store (default table `_default`). |
| `ndocstore.open(path)` | Open or create a JSON file store. |
| `ndocstore.from_json(text)` | Load a store from a JSON string (memory). |
| `ndocstore.close(h)` | Close handle; last handle to a store flushes file-backed data. |
| `ndocstore.flush(h)` | Persist to disk (no-op for memory). |
| `ndocstore.path(h)` | File path or `nil` for memory stores. |
| `ndocstore.to_json(h, pretty?)` | Serialize the whole store (`pretty` default `true`). |

## Tables

| Method | Description |
|--------|-------------|
| `ndocstore.tables(h)` | Sorted list of table names. |
| `ndocstore.table(h, name)` | Table-view handle sharing the same store. |
| `ndocstore.drop_table(h, name)` | Drop a named table (`_default` cannot be dropped). |
| `ndocstore.default_table(h)` | Active table name for this handle. |
| `ndocstore.set_default_table(h, name)` | Set the store's default table (creates if missing). |

## CRUD

| Method | Description |
|--------|-------------|
| `ndocstore.insert(h, doc)` | Insert object; returns numeric doc id. |
| `ndocstore.insert_many(h, docs)` | Bulk insert; returns int array of ids. |
| `ndocstore.get(h, id)` | Document with `_id` field, or `nil`. |
| `ndocstore.all(h)` | All documents (each includes `_id`). |
| `ndocstore.search(h, query)` | Documents matching query. |
| `ndocstore.update(h, fields, query_or_ids)` | Merge-patch fields; returns count. Third arg: query object, id, or id array. |
| `ndocstore.upsert(h, fields, query)` | Update first match or insert; returns doc id. |
| `ndocstore.remove(h, query_or_ids)` | Delete matching docs; returns count. |
| `ndocstore.truncate(h)` | Clear the active table. |

## Queries & counts

| Method | Description |
|--------|-------------|
| `ndocstore.len(h)` | Document count in the active table. |
| `ndocstore.count(h, query?)` | Count all, or matching query. |
| `ndocstore.contains(h, query)` | `true` if any document matches. |
| `ndocstore.exists(h, id)` | `true` if doc id is present. |

### Query object shapes

- Shorthand equality (AND): `{name: "Ada", age: 36}`
- Comparisons: `{eq|ne|gt|gte|lt|lte: {field: value}}`
- Membership: `{in|nin: {field: [a, b]}}`
- `{contains: {field: "sub"}}` — string substring, or array membership
- `{exists: "field"}` or `{exists: {field: true|false}}`
- Compose: `{and: [q1, q2]}`, `{or: [...]}`, `{not: q}`

Dotted paths work: `{eq: {"address.city": "Kyoto"}}`.

Returned documents always include `_id`. User-supplied `_id` on insert is stripped.

## Secondary indexes

| Method | Description |
|--------|-------------|
| `ndocstore.create_index(h, field)` | Index a scalar field (dotted paths ok). |
| `ndocstore.drop_index(h, field)` | Remove an index. |
| `ndocstore.indexes(h)` | List indexed fields. |

Equality queries on indexed fields use the index (intersected for multi-field AND). Large unindexed scans parallelize automatically.

## Errors

| Code | Meaning |
|------|---------|
| 4560 | Wrong argument count. |
| 4561 | Operation failed (bad query, reserved name, …) — catchable `ndocstore_error`. |
| 4562 | Wrong argument type. |
| 4563 | Invalid or closed handle — catchable `ndocstore_error`. |
| 4564 | File IO failure — catchable `ndocstore_error`. |

## Deferred vs TinyDB

Not in v0.1.0: custom storage middlewares, `Query().matches(regex)` / callable `test()`, multi-process file locking, transactions/rollback, and non-JSON document types.
