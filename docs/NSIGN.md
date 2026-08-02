# nsign — signed + expiring tokens, cookies, URLs

Tamper-proof signed values with optional expiry (~`itsdangerous` subset). Native Rust HMAC implementation for session tokens, signed cookies, email verification links, and URL parameters.

## Import

```niao
import "nsign"
```

Paths `import "std/nsign"` and `import "nsign"` are equivalent. Flat builtins (`nsign_dumps`, `nsign_sign`, …) are also available globally after import.

## Quick start

```niao
import "nsign"

let secret = "change-me-in-production"
let token = nsign.dumps({user_id: 42, role: "admin"}, secret, {salt: "session", max_age: 3600})
let data = nsign.loads(token, secret, {salt: "session", max_age: 3600})

let url = nsign.sign_url("https://app.example/verify", {email: "a@b.com"}, secret, {max_age: 86400})
let params = nsign.unsign_url(url, secret, {max_age: 86400})

let signed = nsign.cookie_sign("sid", {uid: 1}, secret, {max_age: 3600})
let session = nsign.cookie_unsign("sid=" + signed, secret, {max_age: 3600})
```

## One-shot helpers

| Method | Description |
|--------|-------------|
| `nsign.sign(value, secret, opts?)` | Sign a raw string (`value + sep + hmac`). |
| `nsign.unsign(signed, secret, opts?)` | Verify and return the original string. |
| `nsign.dumps(value, secret, opts?)` | JSON-encode, sign with timestamp (URL-safe by default). |
| `nsign.loads(token, secret, opts?)` | Verify signature + expiry, decode JSON. |
| `nsign.loads_unsafe(token, secret, opts?)` | Decode without requiring valid signature; returns `{valid, value, timestamp, expired}`. |
| `nsign.validate(token, secret, opts?)` | Fast boolean signature + expiry check. |

### Options (`opts` object)

| Option | Default | Description |
|--------|---------|-------------|
| `salt` | `"itsdangerous.Signer"` / `"itsdangerous"` | Context salt for key derivation. |
| `sep` | `"."` | Separator between payload and signature. |
| `digest` | `"sha1"` | HMAC digest: `"sha1"`, `"sha256"`, `"sha512"`. |
| `key_derivation` | `"django-concat"` | `"django-concat"`, `"concat"`, `"hmac"`, `"none"`. |
| `max_age` | `nil` | Max token age in seconds (timed serializers). |
| `url_safe` | `true` | Use base64url payload encoding (`dumps`/`loads`). |
| `secret_keys` | — | Array of secrets for key rotation (oldest → newest). |
| `max_payload` | `1048576` | Max signed payload bytes. |

## Handles

### Signer (raw strings)

| Method | Description |
|--------|-------------|
| `nsign.signer(secret, opts?)` | Low-level HMAC signer handle. |
| `.sign(value)` | Sign string. |
| `.unsign(signed)` | Verify and return value. |
| `.validate(signed)` | Boolean validity. |

### Timed signer

| Method | Description |
|--------|-------------|
| `nsign.timed(secret, opts?)` | Timestamped string signer. |
| `.sign(value)` | Sign with embedded Unix timestamp. |
| `.unsign(signed, max_age?)` | Returns `{value, timestamp}`. |
| `.validate(signed, max_age?)` | Boolean check including expiry. |

### Serializers

| Method | Description |
|--------|-------------|
| `nsign.serializer(secret, opts?)` | JSON payload signer (raw JSON bytes). |
| `nsign.url_safe(secret, opts?)` | URL-safe base64 JSON + timestamp (Flask-style). |
| `.dumps(value)` | Encode + sign. |
| `.loads(token, max_age?)` | Verify + decode. |
| `.loads_unsafe(token, max_age?)` | Decode without enforcing signature. |
| `.validate(token, max_age?)` | Boolean check. |

## Cookies

| Method | Description |
|--------|-------------|
| `nsign.cookie_sign(name, value, secret, opts?)` | Sign JSON cookie value (URL-safe timed). |
| `nsign.cookie_unsign(cookie, secret, opts?)` | Parse `name=value` or `Set-Cookie` fragment and verify. |
| `nsign.set_cookie(name, signed_value, opts?)` | Format `Set-Cookie` header (`path`, `max_age`, `http_only`, `secure`, `same_site`). |

## Signed URLs

| Method | Description |
|--------|-------------|
| `nsign.sign_url(base_url, params, secret, opts?)` | Append signed query param (default `token`). |
| `nsign.unsign_url(url, secret, opts?)` | Extract and verify param; returns decoded object. |
| `nsign.DEFAULT_PARAM` | Default query key: `"token"`. |

Opts: `param` overrides the query parameter name; `max_age` enforces expiry.

## Token format

Compatible with [itsdangerous](https://itsdangerous.palletsprojects.com/) layout:

- **Signer:** `payload.base64url(hmac(payload))`
- **TimestampSigner:** `payload.timestamp.base64url(hmac(payload.timestamp))`
- **URLSafeSerializer:** base64url(JSON) as payload, then timestamp signing
- Key derivation default: `SHA1(salt + "signer" + secret)` (django-concat)

## Errors

| Code | Kind | Meaning |
|------|------|---------|
| 3581 | hard | Wrong argument count. |
| 3582 | catchable `nsign_error` | Bad signature, malformed token, bad payload. |
| 3583 | hard | Wrong argument type. |
| 3584 | catchable `nsign_error` | Invalid or closed handle. |
| 3585 | catchable `nsign_expired` | Token past `max_age`. |

## See also

- **crypto** — SHA-256/512 and HMAC primitives
- **njwt** — JWT HS256/HS512 (structured claims)
- **nvalid** — schema validation for decoded payloads
