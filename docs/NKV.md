# nkv standard library

Embedded ordered key-value store with ACID transactions, prefix scans, and MVCC snapshots (LMDB / redb class). Native Rust via `redb`. Analogues: `lmdb`, Python `shelve` / `diskcache`.

## Import

```niao
import "nkv"
```

Paths `import "std/nkv"` and `import "nkv"` are equivalent. Flat builtins (`nkv_open`, `nkv_put`, …) are also available globally after import.

## Quick start

```niao
import "nkv"

let db = nkv.memory()
nkv.put(db, "user:1", "alice")
nkv.put(db, "user:2", "bob")
print(nkv.get(db, "user:1"))          // "alice"

let rows = nkv.scan(db, {prefix: "user:"})
print(rows[0].key)                    // "user:1"

let snap = nkv.snapshot(db)
nkv.put(db, "user:1", "ALICE")
print(nkv.get(snap, "user:1"))        // still "alice"
nkv.close(snap)
nkv.close(db)
```

File-backed:

```niao
let db = nkv.open("app.nkv", {create: true})
nkv.put(db, "k", 42)
nkv.sync(db)
nkv.close(db)
```

## Lifecycle

| Method | Description |
|--------|-------------|
| `nkv.open(path, opts?)` | Open/create file DB. `opts.create` defaults to `true`. |
| `nkv.memory()` | In-memory DB (not durable). |
| `nkv.close(h)` | Close a DB or transaction handle. |
| `nkv.path(db)` | File path string, or `nil` for memory. |
| `nkv.sync(db)` | Durability checkpoint. |
| `nkv.stats(db)` | Storage stats object. |
| `nkv.DEFAULT_TABLE` | `"main"` — default table name. |

## Key-value ops

First argument is a **DB** (auto-commit) or **write/read txn** handle.

| Method | Description |
|--------|-------------|
| `nkv.put(h, key, value, table?)` | Upsert. Keys: string/bytes. Values: nil/bool/int/float/string/bytes. |
| `nkv.get(h, key, table?)` | Value or `nil`. |
| `nkv.get_or(h, key, default, table?)` | `get` with default. |
| `nkv.has(h, key, table?)` | Membership. |
| `nkv.remove(h, key, table?)` | Delete; `true` if existed. |
| `nkv.clear(h, table?)` | Delete all entries; returns count. |
| `nkv.len(h, table?)` | Entry count. |

## Transactions & snapshots

| Method | Description |
|--------|-------------|
| `nkv.begin(db, mode?)` | `"write"` (default) or `"read"`. |
| `nkv.snapshot(db)` | Read-only MVCC snapshot. |
| `nkv.commit(txn)` | Commit a write transaction. |
| `nkv.abort(txn)` / `nkv.rollback(txn)` | Discard write txn. |

Writes after `snapshot` are invisible to that snapshot (true MVCC).

## Ordered scans

| Method | Description |
|--------|-------------|
| `nkv.scan(h, opts?)` | Array of `{key, value}`. |
| `nkv.keys(h, opts?)` | Keys only. |
| `nkv.values(h, opts?)` | Values only. |
| `nkv.first(h, table?)` / `nkv.last(h, table?)` | Extremal pair or `nil`. |

**Scan options** (`opts` object):

| Field | Meaning |
|-------|---------|
| `table` | Table name (default `main`). |
| `prefix` | Byte/string prefix filter. |
| `start` / `end` | Lexicographic bounds. |
| `end_inclusive` | Include `end` (default `false` = half-open). |
| `limit` | Max pairs. |
| `reverse` | Descending order. |

## Bulk & tables

| Method | Description |
|--------|-------------|
| `nkv.put_many(h, pairs, table?)` | Bulk upsert; pairs are `{key,value}` or `[k,v]`. Returns count. |
| `nkv.get_many(h, keys, table?)` | Array of values/`nil`. |
| `nkv.tables(db)` | Sorted table names. |
| `nkv.drop_table(db, name)` | Drop table; `true` if it existed. |

## Errors

| Code | Kind |
|------|------|
| 4580 | Wrong argument count. |
| 4581 | Store / IO failure — catchable `nkv_error`. |
| 4582 | Wrong argument type. |
| 4583 | Invalid or closed handle — catchable `nkv_error`. |
| 4584 | Transaction misuse (read-only write, already closed) — catchable `nkv_error`. |

## Deferred vs lmdb / shelve / diskcache

Not in v0.1.0: TTL expiry (diskcache), value compression, encryption at rest, nested array/object values (encode with `json` first), custom comparators, async API, change watchers.

## Benchmarks

```text
cargo run -p niao_kv --release --bin kv_bench -- 50000
```
