# Task 10 — ndb: own Redis + Postgres drivers (replace redis, postgres, r2d2*)
Read MASTER_PLAN.md first.

## Build
Create `crates/niao_db` (deps: niao_crypto, niao_codec; TLS via rustls):
- RESP2/RESP3 codec + sync redis client: GET/SET/DEL/EXPIRE/INCR, hashes, lists, pub/sub optional, pipelining, auth, our own generic pool (replaces r2d2: simple Mutex<Vec<Conn>> + max/idle/health-check).
- Postgres wire protocol v3 (sync): startup, cleartext+MD5+SCRAM-SHA-256 auth (SCRAM needs niao_crypto sha256/hmac — already there), simple + extended query, text format decoding for common types (bool,int2/4/8,float4/8,text,varchar,timestamp,uuid,json), parameterized queries, errors with SQLSTATE.
- Same pool reused.

## Wire up
- Replace redis in ahiru_core; postgres + r2d2 + r2d2_postgres in ahiru_core and niao_runtime/npg. Keep Niao `npg` lib API identical. rusqlite/r2d2_sqlite stay (C binding, allowed) — but move them onto our pool so r2d2 itself is gone.
- mongodb/sqlx stay for now; list their exact usage surface in the report for a future task.

## Acceptance
- Protocol unit tests from captured byte fixtures (hand-write frames); integration tests behind env-guard (NIAO_TEST_PG_URL / NIAO_TEST_REDIS_URL) so CI without servers still passes.
- Bench: simple-query round-trip latency ~ postgres crate.

## Ground rules (apply to EVERY task)
- ZERO new third-party crates. Only std + existing niao_* workspace crates.
- Lightweight & fast: no allocations in hot loops, prefer &str/&[u8], pre-size Vecs, add #[inline] on small hot fns.
- Public API surface goes to Niao programs via niao_runtime builtins + a niao_libs/<name>/ module (Niao-language wrapper), mirroring how niao_libs/json works today.
- Add unit tests in the crate + at least one .niao example under examples/.
- Add/extend a benchmark under benchmarks/ comparing before vs after.
- After changes: `cargo check --workspace` then `cargo test --workspace` must pass.
- Remove the replaced third-party dependency from every Cargo.toml that used it, and confirm it disappears from `cargo tree`.
- Update CHANGELOG.md with one line.
