# Task 03 — Own TOML/config parser (replace `toml` crate)
Read MASTER_PLAN.md first.

## Build
In `crates/niao_json_core` add a `toml` module (reuse Value type): tables, arrays-of-tables, inline tables, all string kinds, ints (hex/oct/bin/underscores), floats, bools, RFC3339 datetimes (as string for now), dotted keys.

## Wire up
- Replace `toml` crate usage in ahiru_core and niao_cli (niao.config parsing, package manifests).
- niao_pkg/nm manifest parsing switched too if it uses toml.

## Acceptance
- Round-trip tests on niao.config and a representative package manifest; error messages include line/col.
- `toml` gone from Cargo.lock.

## Ground rules (apply to EVERY task)
- ZERO new third-party crates. Only std + existing niao_* workspace crates.
- Lightweight & fast: no allocations in hot loops, prefer &str/&[u8], pre-size Vecs, add #[inline] on small hot fns.
- Public API surface goes to Niao programs via niao_runtime builtins + a niao_libs/<name>/ module (Niao-language wrapper), mirroring how niao_libs/json works today.
- Add unit tests in the crate + at least one .niao example under examples/.
- Add/extend a benchmark under benchmarks/ comparing before vs after.
- After changes: `cargo check --workspace` then `cargo test --workspace` must pass.
- Remove the replaced third-party dependency from every Cargo.toml that used it, and confirm it disappears from `cargo tree`.
- Update CHANGELOG.md with one line.
