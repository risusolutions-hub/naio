# ngcp — Native Google Cloud Helper

`ngcp` is a lightweight Google Cloud client for the Niao runtime. It talks to
**GCS**, **Pub/Sub**, **Firestore REST**, and **Cloud Functions** over HTTPS
using the built-in `niao_http` client. Service-account JWTs are signed with
RS256 via the existing `rsa` dependency (same stack as `ncrypt`); tokens are
exchanged at Google's OAuth2 endpoint.

Peers: [`naws`](NAWS.md) (AWS), [`nazure`](NAZURE.md) (Azure).

## Import

```niao
import "ngcp"
```

or

```niao
import "std/ngcp"
```

---

## Configuration

All operations require a **config handle** from `ngcp.config`. The handle is a
thread-local integer ID.

```niao
let cfg = ngcp.config({
    project:       "my-gcp-project",
    client_email:  "sa@my-gcp-project.iam.gserviceaccount.com",
    private_key:   "-----BEGIN PRIVATE KEY-----\n...\n-----END PRIVATE KEY-----\n",
    // access_token: "ya29....",   // optional: skip JWT exchange
    // scopes: "https://www.googleapis.com/auth/cloud-platform",
    // token_uri: "https://oauth2.googleapis.com/token",
})
```

| Field | Required | Type | Description |
|-------|----------|------|-------------|
| `project` | yes | string | GCP project id |
| `client_email` | for SA auth | string | Service account email |
| `private_key` | for SA auth | string | PEM private key (`\n` escapes OK) |
| `access_token` | alt auth | string | Pre-fetched OAuth2 Bearer token |
| `scopes` / `scope` | no | string | OAuth scope (default cloud-platform) |
| `token_uri` | no | string | Token endpoint (default Google OAuth2) |

Auth priority: explicit `access_token` → cached SA token → fresh JWT exchange.

---

## GCS

### `ngcp.gcs_put(cfg, bucket, object, body, content_type?) → {etag, status}`

Upload an object (JSON media upload API).

```niao
let r = ngcp.gcs_put(cfg, "my-bucket", "data/hello.txt", "Hello, GCS!", "text/plain")
print(r.status)  // 200
print(r.etag)
```

### `ngcp.gcs_get(cfg, bucket, object) → {body, status, headers{}}`

Download object bytes as a UTF-8 string body.

### `ngcp.gcs_delete(cfg, bucket, object) → true`

Delete an object.

### `ngcp.gcs_list(cfg, bucket, prefix?) → names[]`

List object names (optional prefix filter).

---

## Pub/Sub

### `ngcp.pubsub_publish(cfg, topic, data, attrs?) → {message_ids[]}`

Publish one message. `data` is base64-encoded on the wire. Optional `attrs` is
a string→string object.

```niao
let r = ngcp.pubsub_publish(cfg, "events", "{\"ok\":true}", {source: "niao"})
print(r.message_ids)
```

### `ngcp.pubsub_pull(cfg, subscription, max?) → messages[]`

Pull up to `max` messages (default 10). Each item:
`{ack_id, data, message_id, attributes{}}`.

### `ngcp.pubsub_ack(cfg, subscription, ack_ids[]) → true`

Acknowledge pulled messages.

---

## Firestore REST

### `ngcp.firestore_get(cfg, collection, doc_id) → fields{} | nil`

Fetch a document's fields (typed values decoded to Niao primitives). Missing
docs return `nil`.

### `ngcp.firestore_set(cfg, collection, doc_id, fields{}) → true`

Create or overwrite fields (PATCH + updateMask).

```niao
ngcp.firestore_set(cfg, "users", "u1", {name: "Ada", age: 36})
let doc = ngcp.firestore_get(cfg, "users", "u1")
print(doc.name)  // "Ada"
```

### `ngcp.firestore_delete(cfg, collection, doc_id) → true`

Delete a document.

### `ngcp.firestore_query(cfg, collection, opts?) → docs[]`

Run a structured query (`opts.limit` default 100). Each result:
`{id, fields{}}`.

---

## Cloud Functions

### `ngcp.function_invoke(cfg, url, payload, method?) → {status, body}`

HTTP-invoke a Cloud Function / Cloud Run URL. Default method `POST`. Object
payloads are JSON-serialised.

```niao
let r = ngcp.function_invoke(cfg, "https://REGION-PROJECT.cloudfunctions.net/fn", {ping: 1})
print(r.status, r.body)
```

---

## Errors

Catchable `ngcp_error` values (also `ngcp_gcs_error`, `ngcp_pubsub_error`,
`ngcp_firestore_error`, `ngcp_function_error` kinds on the wire message):

| Code | Meaning |
|------|---------|
| E4540 | Arity mismatch |
| E4541 | API / network error |
| E4542 | Type mismatch |
| E4543 | Auth / invalid config handle |

```niao
import "ntest"
let r = ngcp.gcs_get(cfg, "missing", "x")
if ntest.is_error(r) {
    print(r.message)
}
```

---

## Flat builtins

After import, flat names are also available globally:
`ngcp_config`, `ngcp_gcs_put`, `ngcp_gcs_get`, `ngcp_gcs_delete`, `ngcp_gcs_list`,
`ngcp_pubsub_publish`, `ngcp_pubsub_pull`, `ngcp_pubsub_ack`,
`ngcp_firestore_get`, `ngcp_firestore_set`, `ngcp_firestore_delete`,
`ngcp_firestore_query`, `ngcp_function_invoke`.
