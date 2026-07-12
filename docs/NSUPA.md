# nsupa — Supabase Client Library

`nsupa` is the Niao native library for interacting with a [Supabase](https://supabase.com) project.
It covers the three main Supabase REST APIs:

| Layer | Supabase endpoint | nsupa API |
|-------|-------------------|-----------|
| Database (PostgREST) | `/rest/v1/<table>` | `from` + query builder |
| Auth (GoTrue) | `/auth/v1/signup`, `/auth/v1/token` | `auth_sign_up`, `auth_sign_in` |
| Storage | `/storage/v1/object/<bucket>/<path>` | `storage_upload`, `storage_download` |

**No third-party crates.** All HTTP is done via the built-in `niao_http` layer.

---

## Import

```niao
import "nsupa"
// or
import "std/nsupa"
```

---

## Connection

### `nsupa.connect(url, anon_key, service_key?) -> client_id`

Registers a Supabase client and returns a numeric handle.

| Argument | Type | Notes |
|----------|------|-------|
| `url` | string | Project URL, e.g. `https://xyz.supabase.co` |
| `anon_key` | string | Public anonymous JWT key |
| `service_key` | string? | Optional service-role key (bypasses Row Level Security) |

```niao
let client = nsupa.connect("https://xyz.supabase.co", env.get("SUPA_ANON_KEY"))
```

### `nsupa.close(client_id) -> true`

Releases the client handle. All query handles associated with this client become invalid.

---

## Query Builder

Query handles are created with `nsupa.from` and are consumed by terminal operations
(`select`, `insert`, `update`, `delete`).  Filter helpers (`eq`, `gt`, `lt`, `order`,
`limit`, `offset`) mutate the handle **in place** and return the same handle id so they
can be chained.

### `nsupa.from(client_id, table) -> query_id`

Allocates a new query handle targeting `table`.

```niao
let q = nsupa.from(client, "users")
```

### Filter helpers (all return `query_id`)

| Function | PostgREST operator | Example |
|----------|--------------------|---------|
| `nsupa.eq(q, col, val)` | `col=eq.val` | `nsupa.eq(q, "role", "admin")` |
| `nsupa.neq(q, col, val)` | `col=neq.val` | `nsupa.neq(q, "status", "banned")` |
| `nsupa.gt(q, col, val)` | `col=gt.val` | `nsupa.gt(q, "age", 18)` |
| `nsupa.lt(q, col, val)` | `col=lt.val` | `nsupa.lt(q, "price", 100)` |
| `nsupa.gte(q, col, val)` | `col=gte.val` | `nsupa.gte(q, "score", 90)` |
| `nsupa.lte(q, col, val)` | `col=lte.val` | `nsupa.lte(q, "stock", 5)` |
| `nsupa.order(q, col, dir?)` | `order=col.dir` | `nsupa.order(q, "created_at", "desc")` |
| `nsupa.limit(q, n)` | `limit=n` | `nsupa.limit(q, 20)` |
| `nsupa.offset(q, n)` | `offset=n` | `nsupa.offset(q, 40)` |

`dir` defaults to `"asc"` if omitted.

### Terminal: `nsupa.select(query_id, cols?) -> rows[]`

Fires a `GET /rest/v1/<table>` request and returns the JSON-decoded array of rows.

```niao
let q = nsupa.from(client, "products")
nsupa.gt(q, "price", 10)
nsupa.order(q, "price", "asc")
nsupa.limit(q, 5)
let rows = nsupa.select(q, "id,name,price")
```

### Terminal: `nsupa.insert(query_id, row{}) -> row`

Fires a `POST /rest/v1/<table>` and returns the created row.

```niao
let q = nsupa.from(client, "todos")
let todo = nsupa.insert(q, {"title": "Buy milk", "done": false})
```

### Terminal: `nsupa.update(query_id, data{}) -> rows[]`

Fires a `PATCH /rest/v1/<table>?<filters>` with `Prefer: return=representation`.

```niao
let q = nsupa.from(client, "todos")
nsupa.eq(q, "id", "42")
let updated = nsupa.update(q, {"done": true})
```

### Terminal: `nsupa.delete(query_id) -> true`

Fires a `DELETE /rest/v1/<table>?<filters>`.

```niao
let q = nsupa.from(client, "todos")
nsupa.eq(q, "id", "42")
nsupa.delete(q)
```

### `nsupa.drop_query(query_id) -> true`

Discards a query handle without sending any request (useful in error paths).

---

## Auth (GoTrue)

### `nsupa.auth_sign_up(client_id, email, password) -> session{}`

Creates a new user account and returns the session object.  The `access_token` is
automatically stored in the client for subsequent database / storage calls.

```niao
let session = nsupa.auth_sign_up(client, "alice@example.com", "supersecret")
print(session.access_token)
```

### `nsupa.auth_sign_in(client_id, email, password) -> session{}`

Signs in an existing user.  The returned `session{}` contains at least:

| Field | Type |
|-------|------|
| `access_token` | string |
| `refresh_token` | string |
| `expires_in` | int |
| `user` | object |

### `nsupa.auth_sign_out(client_id) -> true`

Clears the stored JWT so subsequent requests use the anonymous key again.

---

## Storage

### `nsupa.storage_upload(client_id, bucket, path, body) -> {path}`

Uploads `body` (UTF-8 string) to `<bucket>/<path>`.

```niao
nsupa.storage_upload(client, "avatars", "alice/avatar.txt", "hello world")
```

### `nsupa.storage_download(client_id, bucket, path) -> body`

Downloads the object at `<bucket>/<path>` and returns its content as a string.

```niao
let content = nsupa.storage_download(client, "avatars", "alice/avatar.txt")
```

---

## RPC

### `nsupa.rpc(client_id, fn_name, args?) -> value`

Calls a PostgREST RPC function at `/rest/v1/rpc/<fn_name>`.

```niao
let result = nsupa.rpc(client, "get_user_stats", {"user_id": 42})
```

---

## Error handling

All functions return an `error_value` on failure rather than throwing.  Check with `is_error()`:

```niao
let rows = nsupa.select(q)
if is_error(rows) {
    print("query failed: " + rows.message)
}
```

Error codes:

| Code | Constant | Meaning |
|------|----------|---------|
| 2820 | `E2820_NSUPA_ARITY` | Wrong number of arguments |
| 2821 | `E2821_NSUPA_ERROR` | HTTP / API error |
| 2822 | `E2822_NSUPA_TYPE` | Type mismatch |
| 2823 | `E2823_NSUPA_AUTH` | Authentication error |

---

## Wiring (orchestrator)

Add to `crates/niao_runtime/src/lib.rs`:

```rust
// After the existing module declarations (around line 28):
mod nsupa;

// In install_native_modules():
env.define(nsupa::MODULE_NAME.to_string(), nsupa::namespace().ref_cell());

// In builtin_table():
builtins.extend(nsupa::builtins());

// In native_module_paths() (both cfg branches):
"nsupa", "std/nsupa",

// In native_module_export_name():
if nsupa::MODULE_PATHS.contains(&path) {
    return Some(nsupa::MODULE_NAME);
}
```
