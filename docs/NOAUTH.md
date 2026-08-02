# noauth standard library

OAuth2 + OIDC **client** flows: authorization code (+ PKCE), client credentials, refresh token, revocation, introspection, OIDC discovery, ID token validation, and userinfo. Native Rust implementation (~authlib / oauthlib / requests-oauthlib subset).

## Import

```niao
import "noauth"
```

Paths `import "std/noauth"` and `import "noauth"` are equivalent. Flat builtins (`noauth_client`, `noauth_auth_url`, …) are also available globally after import.

## Quick start

```niao
import "noauth"

let cfg = {
    client_id: "my-app",
    client_secret: "secret",
    authorization_endpoint: "https://idp.example.com/oauth/authorize",
    token_endpoint: "https://idp.example.com/oauth/token",
    redirect_uri: "https://app.example.com/callback",
    scopes: ["openid", "profile", "email"],
    issuer: "https://idp.example.com",
    jwks_uri: "https://idp.example.com/.well-known/jwks.json",
}

let client = noauth.client(cfg)
let state = noauth.random_state()
let pkce = noauth.pkce()

let url = noauth.auth_url(client, {
    state: state,
    code_challenge: pkce.challenge,
    nonce: noauth.random_nonce(),
})
print(url)  // redirect user here

// After redirect:
let cb = noauth.parse_callback("https://app.example.com/callback?code=...&state=...")
noauth.validate_state(state, cb.state)

let tokens = noauth.exchange_code(client, cb.code, {code_verifier: pkce.verifier})
if !noauth.token_expired(tokens) {
    let claims = noauth.verify_id_token(client, tokens.id_token, {nonce: nonce})
    print(claims.claims.sub)
}

noauth.close(client)
```

## OIDC discovery

```niao
let meta = noauth.discover("https://accounts.google.com")
let client = noauth.client_from_discovery(meta, "client-id", {
    client_secret: "secret",
    redirect_uri: "https://app/cb",
    scopes: ["openid", "email"],
})
```

## Client configuration object

| Field | Description |
|-------|-------------|
| `client_id` | OAuth client identifier (required). |
| `token_endpoint` | Token URL (required). |
| `client_secret` | Confidential client secret. |
| `authorization_endpoint` | Authorize URL (auth-code flow). |
| `redirect_uri` | Registered redirect URI. |
| `issuer` | Expected OIDC issuer (`iss` claim). |
| `jwks_uri` | JWKS URL for RS256 ID tokens. |
| `userinfo_endpoint` | OIDC userinfo URL. |
| `revocation_endpoint` | RFC 7009 revocation URL. |
| `introspection_endpoint` | RFC 7662 introspection URL. |
| `scopes` | Default scope list (array or space-separated string). |
| `client_auth_method` | `"body"` (default), `"basic"`, or `"none"`. |
| `timeout_ms` | HTTP timeout for token/userinfo calls. |

Returns a **handle** (`int`) stored in the runtime; call `noauth.close(handle)` when done.

## PKCE & random values

| Method | Description |
|--------|-------------|
| `noauth.pkce(opts?)` | `{verifier, challenge, method}` with S256 by default. |
| `noauth.pkce_challenge(verifier, method?)` | Compute challenge (`"S256"` or `"plain"`). |
| `noauth.random_state()` | URL-safe CSRF `state` parameter. |
| `noauth.random_nonce()` | URL-safe OIDC `nonce`. |

## Authorization redirect

| Method | Description |
|--------|-------------|
| `noauth.auth_url(client, opts?)` | Build authorize URL. |
| `noauth.parse_callback(url)` | Parse full redirect URL → `{code, state, error, …}`. |
| `noauth.parse_query(query)` | Parse query string only. |
| `noauth.validate_state(expected, received)` | Constant-time state check. |

`opts` for `auth_url`: `state`, `nonce`, `code_challenge`, `code_challenge_method`, `scopes`, `response_mode`, `prompt`, `login_hint`, `audience`.

## Token grants

| Method | Description |
|--------|-------------|
| `noauth.exchange_code(client, code, opts?)` | Authorization code (+ optional `code_verifier`). |
| `noauth.client_credentials(client, opts?)` | Client credentials grant. |
| `noauth.refresh(client, refresh_token, opts?)` | Refresh access token. |
| `noauth.revoke(client, token, opts?)` | Revoke token (RFC 7009). |
| `noauth.introspect(client, token)` | Token introspection (RFC 7662). |
| `noauth.userinfo(client, access_token)` | Fetch OIDC userinfo. |
| `noauth.parse_token(json_string)` | Parse token JSON without HTTP. |

Token objects include: `access_token`, `token_type`, `expires_in`, `refresh_token`, `scope`, `id_token`, `obtained_at`, `raw`.

## ID tokens

| Method | Description |
|--------|-------------|
| `noauth.decode_id_token(jwt)` | Decode without verification → `{header, claims}`. |
| `noauth.verify_id_token(client, jwt, opts?)` | Verify HS256 (client secret) or RS256 (JWKS) + validate `iss`/`aud`/`nonce`/`exp`. |

## Token helpers

| Method | Description |
|--------|-------------|
| `noauth.token_expired(token, opts?)` | Whether access token is past `expires_in` (+ optional `leeway` seconds). |
| `noauth.token_expires_in(token)` | Remaining lifetime seconds or `nil`. |
| `noauth.is_bearer(token)` | Whether `token_type` is Bearer. |
| `noauth.basic_auth(client_id, secret)` | `Authorization: Basic …` header value. |
| `noauth.client_info(handle)` | Inspect client config. |

## Errors

| Code | Meaning |
|------|---------|
| 4440 | Wrong argument count. |
| 4441 | Operation failed — catchable `noauth_error`. |
| 4442 | Wrong argument type. |
| 4443 | Invalid or closed client handle. |

## Deferred (not in 0.1.0)

- Device authorization grant (RFC 8628)
- Resource-owner password grant (deprecated in OAuth 2.1)
- JWT bearer / SAML bearer grants
- Automatic async HTTP — all token calls are synchronous via `niao_http`
- ES256 / PS256 ID token algs (RS256 + HS256 only)
