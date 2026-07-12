# naws — Native AWS Helper

`naws` is a zero-dependency AWS client library for the Niao runtime. It signs every
request using AWS Signature Version 4 (HMAC-SHA256 via the built-in `niao_crypto`
crate) and sends HTTP/HTTPS traffic through the built-in `niao_http` client.

Supported services: **S3**, **DynamoDB**, **Lambda**, **SSM Parameter Store**.

## Import

```niao
import "naws"
```

or

```niao
import "std/naws"
```

---

## Configuration

All operations require a **config handle** produced by `naws.config`. The handle is a
thread-local integer ID; it is not serialisable or portable across threads.

```niao
let cfg = naws.config({
    region:        "us-east-1",
    access_key:    "AKIAIOSFODNN7EXAMPLE",
    secret_key:    "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
    session_token: nil          // optional; omit or set nil for IAM user keys
})
```

Fields:

| Field | Required | Type | Description |
|-------|----------|------|-------------|
| `region` | yes | string | AWS region, e.g. `"us-east-1"` |
| `access_key` | yes | string | AWS Access Key ID |
| `secret_key` | yes | string | AWS Secret Access Key |
| `session_token` | no | string \| nil | Temporary session token (STS/AssumeRole) |

---

## S3

### `naws.s3_put(cfg, bucket, key, body, content_type?) → {etag, status}`

Upload an object to S3.

```niao
let r = naws.s3_put(cfg, "my-bucket", "data/hello.txt", "Hello, world!", "text/plain")
print(r.etag)    // "\"d8e8fca2dc0f896fd7cb4cb0031ba249\""
print(r.status)  // 200
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `cfg` | int | Config handle |
| `bucket` | string | S3 bucket name |
| `key` | string | Object key (path) |
| `body` | string \| bytes | Object content |
| `content_type` | string? | MIME type (default `application/octet-stream`) |

**Returns** `{etag: string, status: int}` or an error value on failure.

### `naws.s3_get(cfg, bucket, key) → {body, status, headers{}}`

Download an object from S3.

```niao
let r = naws.s3_get(cfg, "my-bucket", "data/hello.txt")
print(r.body)    // "Hello, world!"
print(r.status)  // 200
```

**Returns** `{body: string, status: int, headers: {}}`.

### `naws.s3_delete(cfg, bucket, key) → true`

Delete an object from S3.

```niao
naws.s3_delete(cfg, "my-bucket", "data/hello.txt")
```

**Returns** `true` on success.

### `naws.s3_list(cfg, bucket, prefix?) → keys[]`

List object keys in a bucket (uses the S3 List Objects v2 API).

```niao
let keys = naws.s3_list(cfg, "my-bucket", "data/")
for k in keys {
    print(k)
}
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `prefix` | string? | Key prefix filter; omit to list all objects |

**Returns** `string[]` of object keys.

---

## DynamoDB

DynamoDB item types are mapped transparently:

| Niao type | DynamoDB type |
|-----------|---------------|
| `string` | S |
| `int` / `float` | N |
| `bool` | BOOL |
| `nil` | NULL |
| `array` | L |
| `object` | M |

### `naws.dynamodb_put(cfg, table, item{}) → true`

Write (or replace) an item.

```niao
naws.dynamodb_put(cfg, "Users", {
    id:    "u-001",
    name:  "Alice",
    score: 42
})
```

### `naws.dynamodb_get(cfg, table, key{}) → item{} | nil`

Read an item by primary key.

```niao
let user = naws.dynamodb_get(cfg, "Users", {id: "u-001"})
if user != nil {
    print(user.name)
}
```

### `naws.dynamodb_delete(cfg, table, key{}) → true`

Delete an item by primary key.

```niao
naws.dynamodb_delete(cfg, "Users", {id: "u-001"})
```

### `naws.dynamodb_query(cfg, table, opts{}) → items[]`

Query using a `KeyConditionExpression`.

```niao
let rows = naws.dynamodb_query(cfg, "Orders", {
    key_condition: "user_id = :uid",
    values: {":uid": "u-001"},
    limit: 20,
    ascending: false
})
```

`opts` fields:

| Field | Type | Description |
|-------|------|-------------|
| `key_condition` | string | KeyConditionExpression |
| `filter` | string | FilterExpression |
| `names` | object | ExpressionAttributeNames `{"#n": "name"}` |
| `values` | object | ExpressionAttributeValues `{":v": value}` |
| `index` | string | Secondary index name |
| `limit` | int | Max items returned |
| `ascending` | bool | Scan order (default `true`) |

---

## Lambda

### `naws.lambda_invoke(cfg, fn_name, payload) → {status, body}`

Invoke a Lambda function synchronously.

```niao
let r = naws.lambda_invoke(cfg, "my-function", {message: "ping"})
print(r.status)  // 200
print(r.body)    // "{\"result\":\"pong\"}"
```

`payload` may be:
- A **string** — passed as-is (must be valid JSON).
- An **object** — serialised to JSON automatically.

---

## SSM Parameter Store

### `naws.ssm_get(cfg, name, decrypt?) → string`

Retrieve a parameter value.

```niao
let db_url = naws.ssm_get(cfg, "/prod/database/url")
let api_key = naws.ssm_get(cfg, "/prod/api/key", true)   // decrypt SecureString
```

| Parameter | Type | Description |
|-----------|------|-------------|
| `name` | string | Full parameter name or path |
| `decrypt` | bool? | Decrypt SecureString (default `true`) |

---

## Error handling

All functions return an **error value** (not a runtime exception) on failure. Use
`ntest.is_error(v)` or check `v.code` to detect and handle errors.

```niao
let r = naws.s3_get(cfg, "my-bucket", "missing.txt")
if ntest.is_error(r) {
    print("failed:", r.message)
}
```

Error codes:

| Code | Constant | Meaning |
|------|----------|---------|
| 2800 | `E2800_NAWS_ARITY` | Wrong number of arguments |
| 2801 | `E2801_NAWS_ERROR` | API or network error |
| 2802 | `E2802_NAWS_TYPE` | Type mismatch in argument |
| 2803 | `E2803_NAWS_AUTH` | Invalid config handle or auth failure |

---

## SigV4 internals

Signing is performed entirely in std Rust:

1. **Canonical request** — method, URI-encoded path, sorted query string, signed headers, SHA-256 of body.
2. **String to sign** — `AWS4-HMAC-SHA256` \n datetime \n credential scope \n hash(canonical request).
3. **Signing key** — `HMAC(HMAC(HMAC(HMAC("AWS4"+secret, date), region), service), "aws4_request")`.
4. **Authorization header** — `AWS4-HMAC-SHA256 Credential=…, SignedHeaders=…, Signature=…`.

`x-amz-content-sha256` is always included (required by S3, harmless elsewhere).

---

## Limitations

- **Pagination**: `s3_list` returns the first page (up to 1 000 keys by default). Full
  pagination via continuation tokens is not yet implemented.
- **Multipart upload**: Not supported; use `s3_put` for objects up to ~5 GB.
- **Pre-signed URLs**: Not implemented.
- **DynamoDB batch/transact**: Not implemented (use separate put/get calls).
