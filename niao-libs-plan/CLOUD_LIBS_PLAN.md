# Niao Cloud & Data Libraries — 6 New Native Modules

Goal: lightweight, fast, std-only runtime modules for vector DB, Redis, AWS, Azure,
Supabase, and a Prisma-style ORM — all following the `n*` naming convention and the
8-step integration checklist from `NEW_STDLIB_PLAN.md`.

Design pillars: **zero new third-party crates**, **handle-based connections** (thread-local
registry like `npg`/`ncache`), **HTTP via existing `net`/`niao_http`**, **recoverable errors**
via `error_value()`, flat builtins + namespace object.

## Library naming map

| Domain | Niao name | Import | Replaces (conceptually) | Backend |
|--------|-----------|--------|-------------------------|---------|
| Vector database | **`nvec`** | `import "nvec"` | Pinecone / Qdrant / Weaviate client | In-memory HNSW index + optional Qdrant REST |
| Redis cache/KV | **`nredis`** | `import "nredis"` | redis-py / ioredis | `niao_db::redis::Client` (RESP2, already built) |
| AWS helper | **`naws`** | `import "naws"` | boto3 (subset) | SigV4 signing + REST (S3, DynamoDB, Lambda, SSM) |
| Azure helper | **`nazure`** | `import "nazure"` | azure-sdk (subset) | SharedKey/Bearer auth + REST (Blob, Table, Functions) |
| Supabase helper | **`nsupa`** | `import "nsupa"` | @supabase/supabase-js | PostgREST + Auth + Storage REST |
| Prisma-style ORM | **`nmodel`** | `import "nmodel"` | Prisma / Drizzle | Schema DSL over `nsqlite` + `npg` handles |

## Error code map (2780–2839)

```
nredis  2780 arity, 2781 error, 2782 type, 2783 invalid handle
nvec    2790 arity, 2791 error, 2792 type, 2793 invalid handle
naws    2800 arity, 2801 error, 2802 type, 2803 auth
nazure  2810 arity, 2811 error, 2812 type, 2813 auth
nsupa   2820 arity, 2821 error, 2822 type, 2823 auth
nmodel  2830 arity, 2831 error, 2832 type, 2833 schema
```

## Per-library API surface

### `nredis` — Redis client

```
nredis.connect(url) -> handle_id
nredis.ping(id) -> "PONG"
nredis.get(id, key) -> string | nil
nredis.set(id, key, value) -> true
nredis.del(id, key) -> true
nredis.incr(id, key, by?) -> int
nredis.expire(id, key, secs) -> bool
nredis.mget(id, keys[]) -> array
nredis.mset(id, pairs{}) -> true
nredis.hget/hset/hdel/hgetall(id, key, ...)
nredis.close(id) -> true
nredis.cmd(id, parts[]) -> value   // raw RESP command
```

Wrap `crates/niao_db/src/redis/mod.rs`. Extend with hash ops + mget/mset if missing.

### `nvec` — Vector database

```
nvec.open(path?, dim?) -> handle_id          // in-memory index
nvec.connect(url, api_key?) -> handle_id      // Qdrant REST backend
nvec.insert(id, vec_id, vector[], metadata{}) -> true
nvec.upsert(id, vec_id, vector[], metadata{}) -> true
nvec.search(id, query[], top_k?, threshold?) -> hits[]
nvec.delete(id, vec_id) -> true
nvec.count(id) -> int
nvec.save(id, path) / nvec.load(path) -> handle_id
nvec.close(id) -> true
```

In-memory: flat cosine similarity + optional simple HNSW (std-only). Qdrant mode: HTTP via net.

### `naws` — AWS helper

```
naws.config({region, access_key, secret_key, session_token?}) -> config_id
naws.s3_put(config, bucket, key, body, content_type?) -> {etag, status}
naws.s3_get(config, bucket, key) -> {body, status, headers{}}
naws.s3_delete(config, bucket, key) -> true
naws.s3_list(config, bucket, prefix?) -> keys[]
naws.dynamodb_put/get/delete/query(config, table, item/keys) -> object
naws.lambda_invoke(config, fn_name, payload) -> {status, body}
naws.ssm_get(config, name, decrypt?) -> string
```

SigV4 in std (HMAC-SHA256 via existing `crypto` module). No AWS SDK crate.

### `nazure` — Azure helper

```
nazure.config({account, key?, sas?, tenant?, client_id?, client_secret?}) -> config_id
nazure.blob_put(config, container, blob, body, content_type?) -> {etag, status}
nazure.blob_get(config, container, blob) -> {body, status}
nazure.blob_delete(config, container, blob) -> true
nazure.blob_list(config, container, prefix?) -> names[]
nazure.table_insert/query/delete(config, table, entity) -> object
nazure.function_invoke(config, app, fn_name, payload) -> {status, body}
```

SharedKey HMAC + optional Bearer (client credentials). REST only.

### `nsupa` — Supabase helper

```
nsupa.connect(url, anon_key, service_key?) -> client_id
nsupa.from(client, table).select(cols?).eq(col, val)... -> rows[]
nsupa.from(client, table).insert(row{}) -> row
nsupa.from(client, table).update(row{}).eq(col, val) -> row
nsupa.from(client, table).delete().eq(col, val) -> true
nsupa.auth_sign_up(client, email, password) -> session{}
nsupa.auth_sign_in(client, email, password) -> session{}
nsupa.storage_upload(client, bucket, path, body) -> {path}
nsupa.storage_download(client, bucket, path) -> body
nsupa.rpc(client, fn_name, args{}) -> value
```

PostgREST query builder as chained filter objects (return intermediate handle or build URL).

### `nmodel` — Prisma-style ORM

```
nmodel.schema({models: {User: {fields: {id: "int@id", email: "string@unique", ...}}}}) -> schema_id
nmodel.bind(schema_id, db_handle, dialect?) -> client_id   // nsqlite or npg handle
nmodel.migrate(client) -> applied_count
nmodel.create(client, "User", {email: "..."}) -> row
nmodel.find_many(client, "User", {where: {email: "..."}, limit?, order?}) -> rows[]
nmodel.find_unique(client, "User", {where: {id: 1}}) -> row | nil
nmodel.update(client, "User", {where: {...}, data: {...}}) -> row
nmodel.delete(client, "User", {where: {...}}) -> count
nmodel.raw(client, sql, params?) -> rows[] | int
```

Generates SQL from schema; migrations stored in `_nmodel_migrations` table.

## Integration checklist (each lib)

1. `crates/niao_runtime/src/<name>.rs` or `<name>/mod.rs`
2. Error codes in `crates/niao_errors/src/codes.rs` (pre-allocated above)
3. Wire in `crates/niao_runtime/src/lib.rs`
4. `crates/niao_pkg/src/catalog.rs`
5. `niao_libs/<name>/package.json` + `0.2.2/lib.json` + `catalog.json`
6. `docs/<NAME>.md`
7. `examples/<name>_demo.niao`
8. `#[cfg(test)]` unit tests in module

## Sub-agent rules

- Work ONLY inside your assigned module + its niao_libs wrapper + demo + docs + tests.
- Do NOT edit `lib.rs`, `codes.rs`, or `catalog.rs` — report exact wiring snippets.
- ZERO new third-party crates. std + existing `niao_*` only.
- Fast + lightweight: handle registries, buffer reuse, `#[inline]` hot paths.
- Acceptance: module compiles with `cargo check -p niao_runtime` after orchestrator wires it.

## Build order (parallel)

All six are independent — spawn one sub-agent per library simultaneously.
