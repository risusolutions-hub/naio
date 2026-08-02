# notp — TOTP/HOTP two-factor authentication (~pyotp subset)

High-performance native one-time password generation and verification: RFC 4226 HOTP, RFC 6238 TOTP, base32 secrets, `otpauth://` provisioning URIs, constant-time compare, parallel bulk generation.

## Import

```niao
import "notp"
```

Paths `import "std/notp"` and `import "notp"` are equivalent. Flat builtins (`notp_totp`, `notp_hotp_at`, …) are also available globally after import.

## Quick start

```niao
import "notp"

let secret = notp.random_base32()
let t = notp.totp(secret, {issuer: "MyApp", name: "user@example.com"})
let code = t.now()
let ok = t.verify(code, nil, 1)

let uri = t.provisioning_uri("user@example.com", "MyApp")
let parsed = notp.parse_uri(uri)
```

## Constants

| Name | Value | Description |
|------|-------|-------------|
| `notp.DEFAULT_DIGITS` | `6` | Standard OTP length. |
| `notp.DEFAULT_INTERVAL` | `30` | TOTP step size in seconds. |
| `notp.MIN_DIGITS` | `1` | Minimum code length. |
| `notp.MAX_DIGITS` | `10` | Maximum code length. |

## Secret & base32

| Function | Description |
|----------|-------------|
| `notp.random_base32(length?)` | Generate a random base32 secret (default length 32). |
| `notp.base32_decode(s)` | Decode base32 to byte array (0..255 ints), or catchable `notp_error`. |
| `notp.base32_encode(bytes)` | Encode byte array to unpadded uppercase base32. |
| `notp.compare(a, b)` | Constant-time string equality (for OTP codes). |

## TOTP (time-based)

| Function | Description |
|----------|-------------|
| `notp.totp(secret, opts?)` | TOTP handle object (`kind: "totp"`). |
| `notp.totp_at(secret, unix_s, opts?)` | Code at Unix timestamp (seconds). |
| `notp.totp_now(secret, opts?)` | Code at current system time. |
| `notp.totp_at_bulk(secret, times, opts?)` | Parallel batch codes for many timestamps. |

**Opts object** (all optional): `digits` (default 6), `interval` (default 30), `digest` (`"sha1"` \| `"sha256"` \| `"sha512"`), `issuer`, `name`.

**Handle methods:**

| Method | Description |
|--------|-------------|
| `.now()` | Current OTP using system clock. |
| `.at(unix_s)` | OTP at Unix seconds. |
| `.verify(token, time?, window?)` | Verify code; default `time` = now, `window` = 0 steps. |
| `.provisioning_uri(name, issuer?)` | Build `otpauth://totp/...` URI for QR enrollment. |

**Handle fields:** `secret`, `digits`, `interval`, `digest`, `kind`, `id`.

## HOTP (counter-based)

| Function | Description |
|----------|-------------|
| `notp.hotp(secret, opts?)` | HOTP handle object (`kind: "hotp"`). |
| `notp.hotp_at(secret, counter, opts?)` | Code at counter value. |
| `notp.hotp_at_bulk(secret, counters, opts?)` | Parallel batch codes for many counters. |

**Handle methods:**

| Method | Description |
|--------|-------------|
| `.at(counter)` | OTP at counter. |
| `.verify(token, counter)` | Verify at exact counter. |
| `.verify_window(token, counter, window?)` | Verify with look-ahead; returns matched counter or `nil`. |
| `.provisioning_uri(name, issuer?, counter?)` | Build `otpauth://hotp/...` URI. |

## URI parsing

| Function | Description |
|----------|-------------|
| `notp.parse_uri(uri)` | `{ok, value}` handle or `{ok: false, error}` — parses Google Key URI format. |

Supports `secret`, `issuer`, `algorithm`, `digits`, `period` (TOTP), and `counter` (HOTP) query parameters.

## Test vectors

RFC 4226/6238 secret (base32 `GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ`, ASCII `12345678901234567890`):

| Type | Input | Code (6 digits) |
|------|-------|-----------------|
| HOTP | counter `0` | `755224` |
| HOTP | counter `1` | `287082` |
| TOTP | Unix `59` | `287082` |
| TOTP | Unix `1111111111` | `050471` |

Google/pyotp example secret `JBSWY3DPEHPK3PXP` (used in QR URI docs):

| Type | Input | Code (6 digits) |
|------|-------|-----------------|
| HOTP | counter `0` | `282760` |
| TOTP | Unix `59` | `996554` |

## Errors

Operations return catchable `notp_error` values (codes `e3572`–`e3575`) for invalid secrets, base32, digits, intervals, digests, URIs, and closed handles.

## Deferred vs pyotp

Implemented: TOTP/HOTP generate & verify, provisioning URIs, URI parse, SHA-1/256/512, configurable digits/interval, verify windows, bulk parallel generation, constant-time compare.

Not implemented (out of scope for v0.1.0): Steam Guard variant, QR image rendering (use an external QR library with `provisioning_uri` output), async/timezone-aware datetime objects (pass Unix seconds instead).

## Install

```bash
nm install notp
```

Native module — no extra runtime dependencies beyond the Niao toolchain.
