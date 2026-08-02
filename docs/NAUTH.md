# nauth — web auth kit (~flask-login, django.contrib.auth subset)

Sessions, login/logout, password reset, RBAC role hierarchies, and CSRF tokens.
Built on **npass** (password hashing) and **nsign** (timed signed cookies/tokens).

## Import

```niao
import "nauth"
```

Paths `import "std/nauth"` and `import "nauth"` are equivalent. Flat builtins (`nauth_auth`, `nauth_hash_password`, …) are also available globally after import.

## Quick start

```niao
import "nauth"

let auth = nauth.auth("change-me-to-a-long-random-secret", {
    scheme: "bcrypt",
    bcrypt_cost: 12,
    roles: {admin: ["editor", "viewer"], editor: ["viewer"]},
})

let hash = auth.hash_password("correct-horse")
let result = auth.login("alice", "correct-horse", hash, {roles: ["admin"]})
let cookie = result.session.cookie()          // Set-Cookie header
let csrf = auth.csrf_token(result.session)    // bind CSRF to session id

// later request
let session = auth.session_from_cookie(cookie)
if session != nil and auth.validate_csrf(csrf, session) and auth.allows(session.roles, "viewer") {
    print("welcome " + session.user_id)
}
```

## Constants

| Name | Value | Description |
|------|-------|-------------|
| `nauth.DEFAULT_SESSION_LIFETIME` | `86400` | Session max age in seconds (24h). |
| `nauth.DEFAULT_COOKIE_NAME` | `"session"` | Default cookie name. |
| `nauth.DEFAULT_RESET_MAX_AGE` | `3600` | Password-reset token lifetime (1h). |
| `nauth.DEFAULT_TOKEN_BYTES` | `32` | Default random token size. |

## Module functions

| Function | Description |
|----------|-------------|
| `nauth.auth(secret, opts?)` | Create an Auth handle. |
| `nauth.hash_password(password, opts?)` | Hash a password (default argon2id). |
| `nauth.verify_password(password, hash)` | Verify against a stored hash. |
| `nauth.verify_and_update(password, hash, opts?)` | Verify; return `{ok, hash, updated}` if rehash needed. |
| `nauth.user(id, opts?)` | User object (`roles`, `permissions`, `active`). |
| `nauth.anonymous()` | Anonymous user object. |
| `nauth.compare(a, b)` | Constant-time string equality. |
| `nauth.token(nbytes?)` | Cryptographic URL-safe random token. |
| `nauth.has_role(roles, role)` | Exact role membership. |
| `nauth.has_permission(perms, perm)` | Permission check (`"*"` grants all). |
| `nauth.roles_expand(hierarchy, roles)` | Expand roles through a hierarchy. |
| `nauth.roles_allows(hierarchy, roles, required)` | Inherited role check. |
| `nauth.extract_cookie(header, name)` | Extract cookie value or `nil`. |

**Auth opts** (all optional): `cookie_name`, `session_lifetime`, `reset_max_age`, `cookie_path`, `cookie_http_only`, `cookie_secure`, `cookie_same_site`, `scheme` (`"argon2id"` \| `"bcrypt"` \| `"scrypt"`), `bcrypt_cost`, `memory_kib`, `time_cost`, `roles` (hierarchy object: role → child role array).

## Auth handle

| Method | Description |
|--------|-------------|
| `.hash_password(password)` | Hash with the auth password context. |
| `.verify_password(password, hash)` | Verify password. |
| `.verify_and_update(password, hash)` | Verify + optional rehash. |
| `.login(user_id, password, stored_hash, extra?)` | `{ok, session, hash, updated}` or error. |
| `.login_user(user_id, extra?)` | Create session without password check. |
| `.create_session(user_id, extra?)` | Alias for `login_user`. |
| `.logout()` | Clear-cookie `Set-Cookie` header. |
| `.load_session(token)` | Unsign token → Session handle. |
| `.session_from_cookie(header)` | Session, `nil`, or error. |
| `.cookie(session)` | Build `Set-Cookie` for a session. |
| `.reset_token(user_id)` | Timed password-reset token. |
| `.verify_reset(token, max_age?)` | Returns `user_id` or error. |
| `.complete_reset(token, new_password, max_age?)` | `{user_id, hash}`. |
| `.csrf_token(session_or_sid)` | Double-submit CSRF token. |
| `.validate_csrf(token, session_or_sid)` | Constant-time CSRF check. |
| `.allows(roles, required)` | Hierarchy-aware role check. |
| `.expand_roles(roles)` | Expand via configured hierarchy. |
| `.has_permission(perms, perm)` | Permission check. |

**Login `extra`:** `{roles: [...], permissions: [...]}`.

## Session handle

| Method / field | Description |
|----------------|-------------|
| `.token()` | Signed session token string. |
| `.cookie(opts?)` | `Set-Cookie` header value. |
| `.get(key)` / `.set(key, value)` | Session flash/data bag. |
| `.refresh()` | New session handle with same data. |
| `.to_object()` | Plain `{user_id, session_id, roles, permissions, data, …}`. |
| `.user_id` / `.session_id` / `.roles` / `.permissions` | Fields. |
| `.is_authenticated` | Always `true` for Session handles. |

## Password reset flow

```niao
let tok = auth.reset_token(user_id)
// email tok to user…
let done = auth.complete_reset(tok, "new-password")
// persist done.hash for done.user_id
```

## CSRF

Tokens are `nonce.mac` HMAC-SHA256 bindings to the session id. Validate on every state-changing request:

```niao
let ok = auth.validate_csrf(form_csrf, session)
```

## Errors

Catchable `nauth_error` values (codes `e4494`–`e4501`): arity/type mistakes abort; domain failures (bad credentials, bad signature, expired reset, CSRF mismatch, empty secret) return Error values usable with `ntest.assert_error` / `catch`.

## Deferred vs flask-login / django.auth

**Implemented:** sessions (signed cookies), login/logout, password hash/verify/rehash, reset tokens, RBAC hierarchy, CSRF, user/anonymous objects, constant-time compare.

**Out of scope for v0.1.0:** OAuth (use `noauth`), JWT API tokens (use `njwt`), TOTP/MFA (use `notp`), persistent session stores / Redis, remember-me cookies, account lockout, email delivery, decorator/middleware binding to a web framework.

## Install

Shipped with the Niao toolchain. Also listed in `niao_libs/catalog.json` for `nm install --global`.

## See also

`npass`, `nsign`, `njwt`, `noauth`, `notp`, `ncrypt`
