# Task 01 — Foundations: niao_codec crate (base64, hex, uuid, dotenv)
Repo: this workspace (Niao language, Rust). Read MASTER_PLAN.md first.

## Build
Create crate `crates/niao_codec` (lib, zero deps) with:
- base64: encode/decode, standard + URL-safe alphabets, no padding option. SIMD-friendly LUT impl.
- hex: encode/decode.
- uuid: v4 (random via own xorshift/os entropy from std) + v7 (timestamp-ordered); to/from string.
- dotenv: parse `.env` (quotes, escapes, comments, export prefix) into Vec<(String,String)>; loader that sets process env.

## Wire up
- Add builtins in niao_runtime (follow the pattern of existing modules like nenv.rs / json.rs).
- Create niao_libs/codec/ Niao module exposing: codec.b64encode/b64decode, codec.hex*, codec.uuid4/uuid7; extend nenv with dotenv loading.
- Replace usage: ahiru_core (base64, uuid, dotenvy), niao_runtime (dotenvy). Delete those deps from Cargo.tomls.

## Acceptance
- RFC 4648 test vectors for base64/hex pass; uuid uniqueness + format tests; dotenv edge cases (quoted values, empty, CRLF).
- Benchmarks: >= old crates' throughput on 1MB payload encode/decode.

## Ground rules (apply to EVERY task)
- ZERO new third-party crates. Only std + existing niao_* workspace crates.
- Lightweight & fast: no allocations in hot loops, prefer &str/&[u8], pre-size Vecs, add #[inline] on small hot fns.
- Public API surface goes to Niao programs via niao_runtime builtins + a niao_libs/<name>/ module (Niao-language wrapper), mirroring how niao_libs/json works today.
- Add unit tests in the crate + at least one .niao example under examples/.
- Add/extend a benchmark under benchmarks/ comparing before vs after.
- After changes: `cargo check --workspace` then `cargo test --workspace` must pass.
- Remove the replaced third-party dependency from every Cargo.toml that used it, and confirm it disappears from `cargo tree`.
- Update CHANGELOG.md with one line.
