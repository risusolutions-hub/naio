# njwt — JWT / JWS sign + verify

JWT / JWS sign and verify for HS256/HS384/HS512, RS256/RS384/RS512, PS256/PS384/PS512, ES256/ES384, and EdDSA. Claims validation (exp, nbf, iat, aud, iss, sub), JWKS parse and HTTP fetch. ~PyJWT / python-jose subset.

## Import

```niao
import "njwt"
```

Paths `import "std/njwt"` and `import "njwt"` are equivalent. Flat builtins (`njwt_sign`, `njwt_verify`, …) are also available globally after import.

## Quick start

```niao
import "njwt"

// Sign with shared secret (HS256 default)
let token = njwt.sign({sub: "user-42", exp: njwt.now() + 3600}, "my-secret")

// Verify and read claims
let claims = njwt.verify(token, "my-secret", {validate_exp: true})
print(claims.sub)

// Decode without verification (debug only)
let doc = njwt.decode(token)
print(doc.header.alg, doc.claims.sub)

// JWKS from URL (OIDC providers)
let jwks = njwt.jwks_fetch("https://example.com/.well-known/jwks.json")
let claims = njwt.verify_jwks(token, jwks, {algorithms: ["RS256"]})
```

## Functions

| Method | Description |
|--------|-------------|
| `njwt.sign(claims, key, opts?)` | Create a signed JWT from a claims object. |
| `njwt.verify(token, key, opts?)` | Verify signature and validate claims; returns payload object. |
| `njwt.decode(token)` | Decode header + claims without verification; returns `{header, claims}`. |
| `njwt.header(token)` | Unverified header object only. |
| `njwt.claims(token)` | Unverified payload object only. |
| `njwt.valid(token)` | `true` when token has valid JWS structure and base64url segments. |
| `njwt.now()` | Current Unix timestamp (seconds) for `exp` / `nbf` / `iat`. |
| `njwt.key_from_secret(secret, alg?)` | Build a key object for HMAC algorithms. |
| `njwt.key_from_pem(pem, alg?)` | Build a key object from PEM (RSA / EC / Ed25519). |
| `njwt.jwks_parse(json)` | Parse a JWKS JSON string or object. |
| `njwt.jwks_fetch(url, opts?)` | HTTP GET a JWKS document. |
| `njwt.verify_jwks(token, jwks, opts?)` | Verify using keys from a JWKS (matches `kid` when present). |
| `njwt.verify_all(tokens, key, opts?)` | Parallel batch verify; returns array of claims or errors. |

### `njwt.algorithms`

Object of supported algorithm name strings: `HS256`, `HS384`, `HS512`, `RS256`, `RS384`, `RS512`, `PS256`, `PS384`, `PS512`, `ES256`, `ES384`, `EdDSA`.

### Key formats

| Form | Use |
|------|-----|
| `"shared-secret"` | HMAC (HS*) — string or bytes |
| `"-----BEGIN ..."` | PEM private/public key for asymmetric algs |
| `{secret: "...", alg: "HS256"}` | Explicit HMAC key |
| `{pem: "-----BEGIN...", alg: "RS256"}` | Explicit PEM key |

### Sign options

| Key | Default | Description |
|-----|---------|-------------|
| `alg` | `"HS256"` | Signing algorithm. |
| `kid` | — | Key ID header field. |
| `typ` | `"JWT"` | Type header field. |

### Verify options

| Key | Default | Description |
|-----|---------|-------------|
| `alg` / `algorithms` | `["HS256"]` | Allowed algorithms (header `alg` must match). |
| `validate_exp` | `true` | Reject expired tokens. |
| `validate_nbf` | `false` | Reject tokens before `nbf`. |
| `validate_iat` | `false` | Reject tokens with future `iat`. |
| `leeway` | `0` | Clock skew leeway in seconds. |
| `audience` / `aud` | — | Required audience claim. |
| `issuer` / `iss` | — | Required issuer claim. |
| `subject` / `sub` | — | Required subject claim. |
| `required_claims` | `[]` | Extra claims that must be present. |

### JWKS fetch options

| Key | Default | Description |
|-----|---------|-------------|
| `timeout_ms` | `30000` | HTTP timeout. |
| `user_agent` | — | Custom User-Agent header. |
| `max_bytes` | `1048576` | Maximum JWKS response size. |

## Performance

HS256/HS512 sign and verify use the zero-dependency `niao_crypto` fast path (constant-time HMAC compare). Asymmetric algorithms delegate to `jsonwebtoken` + `ring`. Batch verification uses `niao_parallel`.

## Errors

| Code | Meaning |
|------|---------|
| 4430 | Wrong argument count. |
| 4431 | Sign/verify/JWKS failure (catchable `njwt_error`). |
| 4432 | Wrong argument type (hard error). |

## See also

- `crypto` — SHA-256/512 and HMAC primitives.
- `http` — HTTP client for custom JWKS workflows.
- `json` — JSON parse/stringify for claim builders.
