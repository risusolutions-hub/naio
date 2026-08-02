# npass — Password hashing and strength policy

Native password hashing (~`passlib`, `argon2-cffi`, `bcrypt` subsets): **argon2id**, **bcrypt**, and **scrypt** with PHC-encoded strings, multi-scheme `context`, auto-identify verify, rehash detection, and configurable strength policies.

## Import

```niao
import "npass"
```

Paths `import "std/npass"` and `import "npass"` are equivalent. Flat builtins (`npass_hash`, `npass_verify`, …) are also registered globally after import.

## Quick start

```niao
import "npass"

let hash = npass.hash("user-password")
let ok = npass.verify("user-password", hash)

let ctx = npass.context({
    schemes: ["argon2id", "bcrypt", "scrypt"],
    default: "argon2id",
    deprecated: ["bcrypt"]
})
let stored = ctx.hash("secret")
ctx.verify("secret", stored)

let report = npass.check("MyP@ssw0rd123!")
if !report.ok {
    print(report.issues)
}
```

## Hashing

| Method | Description |
|--------|-------------|
| `npass.hash(password, scheme?, opts?)` | Hash with scheme (`argon2id` default) and optional context opts. |
| `npass.verify(password, hash)` | Constant-time verify; auto-detects scheme from hash prefix. |
| `npass.identify(hash)` | `"argon2id"`, `"bcrypt"`, `"scrypt"`, or `nil`. |
| `npass.needs_update(hash, opts?)` | `true` if hash uses deprecated scheme or below configured work factors. |
| `npass.verify_and_update(password, hash, opts?)` | `{valid, new_hash, scheme}` — `new_hash` set when rehash recommended. |

### Scheme-specific helpers

| Method | Description |
|--------|-------------|
| `npass.argon2_hash(password, opts?)` | Argon2id PHC string. Opts: `memory_kib`, `time_cost`, `parallelism`. |
| `npass.bcrypt_hash(password, cost?)` | bcrypt string (default cost `12`). |
| `npass.scrypt_hash(password, opts?)` | scrypt PHC string. Opts: `log_n`, `r`, `p`. |

Default work factors follow OWASP-style recommendations (argon2id m=19 456 KiB, t=2, p=1; bcrypt cost 12; scrypt ln=15, r=8, p=1). Use lower costs only in tests.

## CryptContext handle

`npass.context(opts?)` returns `{id, kind: "context", hash, verify, verify_and_update, needs_update}`.

Context opts (same object shape as `hash` / `verify_and_update`):

| Field | Description |
|-------|-------------|
| `schemes` | Allowed schemes array (default all three). |
| `default` | Default scheme for new hashes. |
| `deprecated` | Schemes that trigger `needs_update`. |
| `memory_kib`, `time_cost`, `parallelism` | Argon2id parameters. |
| `bcrypt_cost` | bcrypt work factor. |
| `log_n`, `r`, `p` | scrypt parameters. |

```niao
let ctx = npass.context({default: "argon2id", deprecated: ["bcrypt"]})
let h = ctx.hash("login")
let r = ctx.verify_and_update("login", old_hash)
if r.valid && type(r.new_hash) == "string" {
    // store r.new_hash
}
```

## Strength and policy

| Method | Description |
|--------|-------------|
| `npass.check(password, policy?)` | `{ok, score, entropy, length, issues, classes}` with optional inline policy fields. |
| `npass.policy(opts?)` | Reusable handle with `.validate(password)` / `.check(password)`. |
| `npass.entropy(password)` | Estimated entropy bits. |
| `npass.is_common(password)` | `true` if password matches built-in top-common list. |
| `npass.generate(length?, alphabet?)` | CSPRNG password (default length 16). |

Policy fields: `min_length`, `max_length`, `min_upper`, `min_lower`, `min_digit`, `min_special`, `min_entropy`, `min_score` (0–4), `forbid_common`, `forbid_sequential`, `forbid_repeated`, `forbidden` (substring blocklist).

Score 0 = very weak, 4 = very strong. Checks include length, character classes, entropy, common-password blocklist, sequential/repeated runs.

## Constants

| Name | Value |
|------|-------|
| `npass.DEFAULT_SCHEME` | `"argon2id"` |
| `npass.DEFAULT_BCRYPT_COST` | `12` |
| `npass.MIN_BCRYPT_COST` / `MAX_BCRYPT_COST` | `4` / `31` |
| `npass.MAX_PASSWORD_BYTES` | `1024` |
| `npass.DEFAULT_ALPHABET` | URL-safe + symbols for `generate`. |

## Errors

| Code | Meaning |
|------|---------|
| 3567 | Wrong argument count. |
| 3568 | Hash/verify/policy error (catchable `npass_error`). |
| 3569 | Wrong argument type (hard error). |
| 3570 | Invalid or closed handle. |
| 3571 | Unknown or unsupported scheme. |

## See also

- `crypto` — SHA/HMAC/JWT (not password storage).
- `nvalid` — general field validation.
- `nid` — ID generation (not secrets).

## Deferred (not in 0.1.0)

- Unix `crypt` / DES schemes (deprecated in passlib).
- Apache `htpasswd` file helpers.
- Full `zxcvbn` dictionary scoring (subset entropy + common list only).
- Hardware security module / external KMS integration.
