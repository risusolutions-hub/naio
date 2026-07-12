# Task 02 — njson: own JSON engine (replace serde_json)
Read MASTER_PLAN.md first.

## Build
Create crate `crates/niao_json_core` (zero deps):
- Value enum (Null/Bool/I64/F64/Str/Array/Object) — Object backed by our own small-vec-of-pairs map (preserve insertion order, linear scan <16 keys, hash above).
- Parser: single-pass, byte-level, SIMD-ish whitespace skip, proper \u escapes + surrogate pairs, integer fast path w/o float roundtrip, depth limit.
- Writer: to_string / to_string_pretty, streaming into a reusable Vec<u8> buffer.
- Optional zero-copy str slices where no escapes present (Cow).

## Wire up
- niao_runtime/json.rs and niao_vm/json_fast.rs: switch to niao_json_core.
- Migrate crates that use serde_json ONLY for Value/parse/to_string (niao_bytecode? niao_pkg? niao_nm? niao_data? — check each). Where serde derive is load-bearing (config structs), add manual to/from Value impls.
- serde/serde_json may remain temporarily in ahiru_core + niao_llm/niao_rag; note remaining users at the end of your report.

## Acceptance
- Pass JSONTestSuite-style edge cases (write ~40 representative cases inline: bad unicode, trailing data, deep nesting, big ints, -0, 1e309...).
- Bench: parse+serialize of a 5MB mixed document >= serde_json throughput; VM json_fast paths not regressed (run benchmarks/).

## Ground rules (apply to EVERY task)
- ZERO new third-party crates. Only std + existing niao_* workspace crates.
- Lightweight & fast: no allocations in hot loops, prefer &str/&[u8], pre-size Vecs, add #[inline] on small hot fns.
- Public API surface goes to Niao programs via niao_runtime builtins + a niao_libs/<name>/ module (Niao-language wrapper), mirroring how niao_libs/json works today.
- Add unit tests in the crate + at least one .niao example under examples/.
- Add/extend a benchmark under benchmarks/ comparing before vs after.
- After changes: `cargo check --workspace` then `cargo test --workspace` must pass.
- Remove the replaced third-party dependency from every Cargo.toml that used it, and confirm it disappears from `cargo tree`.
- Update CHANGELOG.md with one line.
