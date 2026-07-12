# nurl — URL utilities

Hand-rolled URL parse, build, join, query helpers, and RFC 3986 percent encoding (std only).

## Import

```niao
import "nurl"
```

Paths `import "std/nurl"` and `import "nurl"` are equivalent. Flat builtins (`nurl_parse`, `nurl_encode`, …) are also available globally after import.

## Quick start

```niao
import "nurl"

let u = nurl.parse("https://api.example.com/v1/users?page=1#top")
print(u.scheme)    // "https"
print(u.host)      // "api.example.com"
print(u.port)      // nil (default 443 omitted)
print(u.path)      // "/v1/users"

let q = nurl.get_query("https://ex.com/?a=1&b=hello%20world")
print(q.a)         // "1"
print(q.b)         // "hello world"

let url = nurl.set_query("https://ex.com/path", {q: "search", limit: 10})
let joined = nurl.join("https://ex.com/a/b/", "c")
```

## Functions

| Method | Description |
|--------|-------------|
| `nurl.parse(url)` | Parse absolute URL → `{scheme, host, port, path, query, fragment, userinfo}`. Missing optional fields are `nil`. |
| `nurl.build(parts)` | Build URL string from a parts object (inverse of `parse`). Requires `scheme` and `host`. |
| `nurl.encode(s)` | RFC 3986 percent-encode a string. |
| `nurl.decode(s)` | Decode percent-escapes (strict; `+` is not treated as space). |
| `nurl.get_query(url)` | Parse query string → object of decoded key/value pairs. |
| `nurl.set_query(url_or_parts, query_obj)` | Replace query from an object; accepts URL string or parts object. |
| `nurl.join(base, path)` | RFC 3986 relative resolution of `path` against `base`. |

## Parts object

`parse` returns:

| Field | Type | Notes |
|-------|------|-------|
| `scheme` | string | Always present (lowercased). |
| `host` | string | Always present. |
| `port` | int \| nil | Omitted when default for scheme (80/443/21). |
| `path` | string | Always present (`/` minimum). |
| `query` | string \| nil | Raw query without `?`. |
| `fragment` | string \| nil | Without `#`. |
| `userinfo` | string \| nil | `user` or `user:password` before `@`. |

`build` accepts the same fields. `userinfo` is split on the first `:` into username and password.

## Errors

| Code | Meaning |
|------|---------|
| 2880 | Wrong argument count. |
| 2881 | URL/encoding error (catchable `nurl_error` from `parse`, `decode`, `join`, etc.). |
| 2882 | Wrong argument type. |

## See also

- `net` — HTTP client; includes basic URL helpers via `niao_http`.
- `nvalid` — `is_url()` fast validation check.
