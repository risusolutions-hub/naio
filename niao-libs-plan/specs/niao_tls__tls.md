# Library spec: `tls`  →  crate `niao_tls`

| | |
|---|---|
| Category | TLS |
| Replaces Rust crate(s) | `rustls,rustls-pemfile,rustls-native-certs,rustls-pki-types,tokio-rustls` (v0.23) |
| Target Niao crate | `crates/niao_tls` |
| Difficulty | 5/5 — Extreme |
| Status | TO BUILD — high risk, read the risk note |
| Depends on Niao libs | crypto |

## Goal
Reimplement the functionality of `rustls,rustls-pemfile,rustls-native-certs,rustls-pki-types,tokio-rustls` as a **zero-external-dependency**, lightweight, high-performance native Niao/Rust module. Only `std` and existing `niao_*` crates allowed.

## Spec references
RFC 8446 (TLS1.3), RFC 5246 (1.2)

## Implementation blueprint (make it FAST + LIGHT)
FULL crypto stack: AEAD (AES-GCM, ChaCha20-Poly1305), ECDHE (X25519, P-256), signature verify (RSA-PSS, ECDSA), HKDF, X.509 parse+chain validation, cert store access per-OS, PEM parse. TLS1.3 handshake state machine.

### Performance rules
- No heap allocation inside hot loops; reuse buffers, pre-size Vecs.
- Prefer `&[u8]` / `&str` and `Cow` over owned copies.
- `#[inline]` small hot functions; batch/SIMD where the spec allows.
- Add `#[cfg]` scalar fallbacks for any intrinsics so it builds on all targets.

## Public API surface
`TlsConnector/TlsAcceptor`
Expose to Niao programs through a `niao_libs/tls/` wrapper mirroring how `niao_libs/json` works today, plus runtime builtins.

## Performance target
handshake+bulk within 2x rustls

## Tests required
interop vs openssl s_server, RFC test vectors, cert chain cases
Plus: unit tests in the crate, one `.niao` example under `examples/`, and a benchmark under `benchmarks/` comparing against the crate being replaced (generate fixtures from the old crate BEFORE removing it).

## Risk / notes
SECURITY-CRITICAL & HUGE. Hand-rolled crypto/TLS is the single riskiest item here — a subtle bug = remote exploit. Strong recommendation: keep rustls. If pursued, isolate, get external audit, constant-time primitives mandatory.

## Done criteria
- `cargo check --workspace` and `cargo test --workspace` green.
- Replaced crate(s) removed from every `Cargo.toml` and gone from `cargo tree`.
- Benchmark meets the target above; numbers logged in `niao-libs-plan/REPORT.md`.
- `CHANGELOG.md` updated.
