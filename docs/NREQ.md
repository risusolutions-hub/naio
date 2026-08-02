# nreq standard library

Ergonomic HTTP client for Niao: sessions, cookie jar, retries, redirects, multipart upload, file download, HTTP proxies, and helpers for forms/JSON/auth. Built as a thin requests/httpx-style layer over the in-tree `niao_http` transport (the same stack `net` uses) — like Python's `requests` over `urllib`.

## Import

```niao
import "nreq"
```

Paths `import "std/nreq"` and `import "nreq"` are equivalent. Flat builtins (`nreq_get`, `nreq_session`, …) are also available after import.

## Quick start

```niao
import "nreq"

let r = nreq.get("https://httpbin.org/get", {
    headers: {"Accept": "application/json"},
    timeout_ms: 10000,
})
if nreq.ok(r) {
    print(r.status)
    print(nreq.json(r))
}

let s = nreq.session({
    base_url: "https://api.example.com",
    headers: {"X-App": "demo"},
    retries: 3,
    backoff_ms: 100,
})
let login = nreq.post(s, "/login", {json: {user: "a", pass: "b"}})
nreq.raise_for_status(login)
print(nreq.cookies(s))
nreq.close(s)
```

## Module verbs

| Method | Description |
|--------|-------------|
| `nreq.get(url\|session, …)` | GET (optional session handle first). |
| `nreq.post(…)` | POST |
| `nreq.put(…)` | PUT |
| `nreq.patch(…)` | PATCH |
| `nreq.delete(…)` | DELETE |
| `nreq.head(…)` | HEAD |
| `nreq.request(method, url\|session, …)` | Arbitrary method. |
| `nreq.download(url\|session, path, opts?)` | Write response body to a file path. |

Call shapes: `nreq.get(url)`, `nreq.get(url, opts)`, `nreq.get(session, url)`, `nreq.get(session, url, opts)`.

## Sessions

| Method | Description |
|--------|-------------|
| `nreq.session(opts?)` | Create a session handle (cookies, defaults). |
| `nreq.close(session)` | Drop the session. |
| `nreq.session_info(session)` | Inspect defaults (`base_url`, `timeout_ms`, …). |
| `nreq.cookies(session)` | Cookie jar as `{name: {value, domain, path, …}}`. |
| `nreq.set_cookie(session, name, value, opts?)` | Set a cookie (`domain`, `path`, `secure`, `http_only`). |
| `nreq.clear_cookies(session)` | Empty the jar. |

Session opts: `base_url`, `headers`, `params`, `auth` / `bearer`, `timeout_ms`, `max_redirects`, `allow_redirects`, `retries`, `retry_statuses`, `backoff_ms`, `proxy`, `user_agent`, `cookies`.

## Request options

| Field | Description |
|-------|-------------|
| `headers` | Extra headers object. |
| `params` | Query string map. |
| `data` | Form object or raw string body. |
| `json` | JSON body (object or string). |
| `files` | Multipart parts `[{name, filename?, data\|content, content_type?}]`. |
| `body_bytes` | Raw byte array body. |
| `auth` | `[user, pass]` or `{user, pass}`. |
| `bearer` | Bearer token string. |
| `timeout_ms` | Socket timeout. |
| `retries` / `backoff_ms` / `retry_statuses` | Retry policy. |
| `proxy` | HTTP proxy URL (HTTPS CONNECT deferred). |
| `cookies` | Extra cookies for this request. |

## Response object

| Field | Description |
|-------|-------------|
| `status` | HTTP status code. |
| `ok` | `true` for 2xx. |
| `url` | Final URL. |
| `body` | UTF-8 lossy text. |
| `body_bytes` | Raw bytes as int array. |
| `headers` | Lowercased header map. |
| `set_cookies` | Raw `Set-Cookie` values. |
| `elapsed_ms` | Wall time for the call (incl. retries). |
| `reason` | Short reason phrase. |

Helpers: `nreq.ok(r)`, `nreq.json(r)`, `nreq.raise_for_status(r)`.

## Forms, multipart, auth, URL

| Method | Description |
|--------|-------------|
| `nreq.encode_form(obj)` | `application/x-www-form-urlencoded`. |
| `nreq.decode_form(str)` | Parse form body → object. |
| `nreq.multipart(parts, boundary?)` | Build `{content_type, body, body_bytes, boundary}`. |
| `nreq.boundary()` | Random multipart boundary. |
| `nreq.basic_auth(user, pass)` | `Basic …` value. |
| `nreq.bearer(token)` | `Bearer …` value. |
| `nreq.join(base, path)` | Resolve relative URL. |
| `nreq.url(base, path?, params?)` | Build URL with query. |
| `nreq.parse_set_cookie(header)` | Parse one `Set-Cookie`. |
| `nreq.cookie_header(map)` | Build `Cookie` request header. |
| `nreq.default_headers()` | Default `User-Agent` map. |

## Errors

| Code | Kind | When |
|------|------|------|
| E4490 | `nreq_error` | Wrong arity. |
| E4491 | `nreq_error` | HTTP / config / status errors (catchable value). |
| E4492 | `nreq_error` | Type mismatch. |
| E4493 | `nreq_error` | Invalid / closed session handle. |

Network failures return catchable error values (`is_error(r)`), not panics.

## Notes

- **Redirects** follow inside `niao_http` (default on).
- **Connection pooling** uses the shared `niao_http` keepalive pool infrastructure; keep-alive is transport-dependent today.
- **Streaming download**: `download` writes the received body to disk (receive-then-write in v0.1; true chunked stream deferred).
- **Proxies**: HTTP absolute-form proxying supported; HTTPS-over-CONNECT deferred.
- Compose with **`nretry`** for call-level retry wrappers around arbitrary Niao functions; `nreq` retries are HTTP status/IO oriented.

## Related

- `net` — broader networking surface (`net_http_*`)
- `noauth` — OAuth2/OIDC on the same HTTP stack
- `nretry` — general retry policies
