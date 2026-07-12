# `nazure` — Azure Helper Library

Lightweight native Azure integration for Niao.  
No third-party crates — HMAC-SHA256 via `niao_crypto`, base64 via `niao_codec`, HTTP via `niao_http`.

```niao
import "nazure"
```

---

## Configuration

Every `nazure` operation begins with creating a **config handle**.

```
nazure.config(opts) -> config_id
```

| Field | Type | Description |
|-------|------|-------------|
| `account` | string | **Required.** Storage/Table account name. |
| `key` | string | Base64-encoded storage account key (for SharedKey auth). |
| `sas` | string | SAS token (Blob/Table) **or** function key (Azure Functions). |
| `tenant` | string | Azure AD tenant ID (for OAuth 2.0 Bearer flow). |
| `client_id` | string | Azure AD application ID. |
| `client_secret` | string | Azure AD client secret. |

Auth priority per service:
1. **SharedKey** — `key` present → Azure HMAC-SHA256 signature (Blob uses full SharedKey, Table uses SharedKeyLite).
2. **SAS** — `sas` present, no `key` → SAS token appended to URL.
3. **Bearer** — `tenant`+`client_id`+`client_secret` present → client-credentials OAuth 2.0 token fetched automatically.
4. **Anonymous** — no auth fields → plain HTTP (works with public endpoints).

```niao
let cfg = nazure.config({
    account: "mystorageacct",
    key: "BASE64_ENCODED_STORAGE_KEY"
})
```

---

## Blob Storage

All Blob operations target `https://{account}.blob.core.windows.net`.

### `blob_put`

```
nazure.blob_put(cfg, container, blob, body, content_type?) -> {etag, status}
```

Upload a blob. `body` can be a `string` or `byte_array`.  
`content_type` defaults to `"application/octet-stream"`.

```niao
let result = nazure.blob_put(cfg, "mycontainer", "hello.txt", "Hello, world!", "text/plain")
print(result.status)   // 201
print(result.etag)     // "0x8D9F..."
```

### `blob_get`

```
nazure.blob_get(cfg, container, blob) -> {body, status}
```

Download a blob. `body` is the UTF-8 decoded content as a string.

```niao
let r = nazure.blob_get(cfg, "mycontainer", "hello.txt")
print(r.body)    // "Hello, world!"
print(r.status)  // 200
```

### `blob_delete`

```
nazure.blob_delete(cfg, container, blob) -> true
```

Delete a blob. Returns `true` on success (HTTP 202/204); returns an error value otherwise.

```niao
let ok = nazure.blob_delete(cfg, "mycontainer", "hello.txt")
print(ok)  // true
```

### `blob_list`

```
nazure.blob_list(cfg, container, prefix?) -> names[]
```

List blob names in a container. Optional `prefix` filters results.

```niao
let names = nazure.blob_list(cfg, "mycontainer")
// names = ["foo.txt", "bar/baz.json", ...]

let logs = nazure.blob_list(cfg, "mycontainer", "logs/2024-")
// logs = ["logs/2024-01.csv", "logs/2024-02.csv", ...]
```

---

## Table Storage

All Table operations target `https://{account}.table.core.windows.net`.  
Entities are Niao `object` maps. `PartitionKey` and `RowKey` are required fields for Azure Table entities.

### `table_insert`

```
nazure.table_insert(cfg, table, entity) -> object
```

Insert a single entity. Returns the created entity (or empty object for 204 responses).

```niao
let entity = {
    PartitionKey: "users",
    RowKey: "alice",
    Email: "alice@example.com",
    Score: 42
}
let row = nazure.table_insert(cfg, "Users", entity)
print(row.PartitionKey)  // "users"
```

### `table_query`

```
nazure.table_query(cfg, table, filter?) -> rows[]
```

Query entities. `filter` is an optional OData filter expression.

```niao
let all = nazure.table_query(cfg, "Users")

let filtered = nazure.table_query(cfg, "Users", "PartitionKey eq 'users' and Score gt 10")
for row in filtered {
    print(row.RowKey, row.Score)
}
```

### `table_delete`

```
nazure.table_delete(cfg, table, entity) -> true
```

Delete a single entity. `entity` must contain `PartitionKey` and `RowKey`.

```niao
let ok = nazure.table_delete(cfg, "Users", {PartitionKey: "users", RowKey: "alice"})
print(ok)  // true
```

---

## Azure Functions

### `function_invoke`

```
nazure.function_invoke(cfg, app, fn_name, payload) -> {status, body}
```

Call an Azure Function HTTP trigger via POST.

- `app` — Function App name (without `.azurewebsites.net`)
- `fn_name` — Function name as registered in Azure
- `payload` — string or object (objects are JSON-serialised automatically)
- If `cfg.sas` is set, it is used as the function key (`?code=...`)
- If `cfg.tenant`/`client_id`/`client_secret` are set, a Bearer token is fetched

```niao
let fn_cfg = nazure.config({
    account: "myfuncapp",
    sas: "MY_FUNCTION_KEY"
})

let r = nazure.function_invoke(fn_cfg, "myfuncapp", "ProcessOrder", {
    order_id: "ORD-001",
    amount: 99.99
})
print(r.status)  // 200
print(r.body)    // response JSON string
```

---

## Error values

All functions return a runtime `error` value on failure instead of raising. Check with `type(result) == "error"`.

| Code | Constant | Meaning |
|------|----------|---------|
| E2810 | `NAZURE_ARITY` | Wrong number of arguments |
| E2811 | `NAZURE_ERROR` | HTTP or Azure API error |
| E2812 | `NAZURE_TYPE` | Argument type mismatch |
| E2813 | `NAZURE_AUTH` | Authentication failure (bad key, token error) |

```niao
let result = nazure.blob_get(cfg, "mycontainer", "missing.txt")
if type(result) == "error" {
    print("Error:", result.message)
}
```

---

## SAS token example

```niao
let cfg = nazure.config({
    account: "mystorageacct",
    sas: "sv=2020-08-04&ss=b&srt=co&sp=rwdlacuptfx&se=2025-01-01T00:00:00Z&sig=..."
})
let names = nazure.blob_list(cfg, "mycontainer")
```

---

## Bearer token example (Azure Functions with AD auth)

```niao
let cfg = nazure.config({
    account: "myfuncapp",
    tenant: "00000000-0000-0000-0000-000000000000",
    client_id: "11111111-1111-1111-1111-111111111111",
    client_secret: "my-client-secret"
})
let r = nazure.function_invoke(cfg, "myfuncapp", "SecureFunc", "{}")
```

---

## Flat builtins (without namespace import)

When using `import "nazure"` the flat builtins are also available globally:

| Flat name | Namespace alias |
|-----------|----------------|
| `nazure_config` | `nazure.config` |
| `nazure_blob_put` | `nazure.blob_put` |
| `nazure_blob_get` | `nazure.blob_get` |
| `nazure_blob_delete` | `nazure.blob_delete` |
| `nazure_blob_list` | `nazure.blob_list` |
| `nazure_table_insert` | `nazure.table_insert` |
| `nazure_table_query` | `nazure.table_query` |
| `nazure_table_delete` | `nazure.table_delete` |
| `nazure_function_invoke` | `nazure.function_invoke` |

---

## Implementation notes

- Config handles are thread-local; each thread maintains its own registry.
- **No new Cargo dependencies** — auth uses `niao_crypto::hmac_sha256` and `niao_codec::base64`.
- Blob API version: `2020-08-04`. Table API version: `2019-02-02`.
- Azure China / Government clouds: not yet supported (hardcoded to `core.windows.net`).
- Large blob bodies: streamed in one HTTP request; chunked upload not yet supported.
