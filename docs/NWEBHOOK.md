# nwebhook — Standard Webhooks HMAC sign / verify

Send and receive webhooks with [Standard Webhooks](https://www.standardwebhooks.com/) HMAC-SHA256 signatures (`v1`), timestamp tolerance, and optional message-id replay defense (~`svix` / `standard-webhooks` subset).

## Import

```niao
import "nwebhook"
```

Paths `import "std/nwebhook"` and `import "nwebhook"` are equivalent. Flat builtins (`nwebhook_sign`, `nwebhook_verify`, …) are also available after import.

## Quick start

```niao
import "nwebhook"

let secret = "whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw"
let payload = "{\"event\":\"invoice.paid\",\"amount\":1999}"

# Sender: build signed headers
let req = nwebhook.sign_request(secret, payload)
# POST req.payload with req.headers to your consumers

# Receiver: verify signature + JSON
let wh = nwebhook.webhook(secret)
let data = wh.verify(payload, req.headers)
print(data.event)   # "invoice.paid"

# Replay defense by webhook-id
let guard = nwebhook.guard()
if !guard.check(req.id) {
    print("duplicate delivery — ignore")
}
```

## Official sign vector

```niao
import "nwebhook"

let sig = nwebhook.sign(
    "whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw",
    "msg_p5jXN8AQM9LWM0D4loKWxJek",
    1614265330,
    "{\"test\": 2432232314}"
)
# => "v1,g0hM9SsE+OTPJTGt/tmIKtSyZlE3uFJELVlNIOLJ1OE="
```

## Webhook handle

| Method | Description |
|--------|-------------|
| `nwebhook.webhook(secret, opts?)` | Create a signer/verifier. `secret` may be a string or an array (key rotation). |
| `.sign(msg_id, timestamp, payload)` | Return `v1,<base64>` signature. |
| `.verify(payload, headers, opts?)` | Verify; return parsed JSON (or `{id,timestamp,payload,data}` when `meta: true`). |
| `.verify_raw(payload, headers, opts?)` | Verify; return the raw payload string. |
| `.valid(payload, headers, opts?)` | Boolean check (no JSON parse). |

### Options

| Option | Default | Description |
|--------|---------|-------------|
| `format` | `"standard"` | `"standard"` strips optional `whsec_` and base64-decodes; `"raw"` uses the secret string bytes. |
| `tolerance` | `300` | Allowed clock skew in seconds (±). |
| `now` | wall clock | Override Unix seconds (tests). |
| `parse_json` | `true` | Parse body as JSON on `verify`. |
| `meta` | `false` | Return `{id, timestamp, payload, data}` instead of just JSON. |

## One-shot helpers

| Method | Description |
|--------|-------------|
| `nwebhook.sign(secret, msg_id, timestamp, payload, opts?)` | Sign without a handle. |
| `nwebhook.verify(secret, payload, headers, opts?)` | Verify + JSON parse. |
| `nwebhook.verify_raw(secret, payload, headers, opts?)` | Verify, return raw string. |
| `nwebhook.valid(secret, payload, headers, opts?)` | Boolean validity. |
| `nwebhook.sign_request(secret, payload, opts?)` | Allocate id/timestamp/signature/headers for sending (`id` / `timestamp` overrides). |
| `nwebhook.headers(msg_id, timestamp, signature)` | Build the three header fields. |
| `nwebhook.new_id()` | Svix-style `msg_…` id. |
| `nwebhook.now()` | Current Unix seconds. |
| `nwebhook.parse_secret(secret, opts?)` | Validate secret → `{ok, len, encoded}`. |
| `nwebhook.check_timestamp(ts, opts?)` | Tolerance window check (`true` or error). |

## Replay guard

In-memory sliding set of recently seen `webhook-id` values (idempotency).

| Method | Description |
|--------|-------------|
| `nwebhook.guard(opts?)` | Create guard (`max_age` default 300, `capacity` default 10000). |
| `.check(msg_id, now?)` | `true` if first sighting (records it); `false` if replay. |
| `.seen(msg_id, now?)` | Peek without recording. |
| `.forget(msg_id)` / `.clear()` / `.size()` | Manage the set. |

## Constants

| Name | Value |
|------|-------|
| `nwebhook.TOLERANCE` | `300` |
| `nwebhook.SECRET_PREFIX` | `"whsec_"` |
| `nwebhook.HDR_ID` | `"webhook-id"` |
| `nwebhook.HDR_TIMESTAMP` | `"webhook-timestamp"` |
| `nwebhook.HDR_SIGNATURE` | `"webhook-signature"` |

## Wire format

Signed content is `{msg_id}.{timestamp}.{body}` over UTF-8 bytes, HMAC-SHA256, standard base64, version prefix `v1,`. The `webhook-signature` header may list several space-delimited signatures (secret rotation); any matching `v1` signature accepts.

## Errors

| Code | Kind | Meaning |
|------|------|---------|
| 4460 | hard | Wrong argument count. |
| 4461 | catchable `nwebhook_error` | Bad secret, missing headers, bad/expired timestamp, no matching signature, JSON parse. |
| 4462 | hard | Wrong argument type. |
| 4463 | catchable `nwebhook_error` | Invalid or closed handle. |

## See also

- `nsign` — general HMAC token / cookie / URL signing (~itsdangerous)
- `njwt` — JWT / JWS
- `ncrypt` / `crypto` — lower-level crypto primitives
