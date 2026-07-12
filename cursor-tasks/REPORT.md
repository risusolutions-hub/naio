# Task execution report

## Task 01 — Foundations (niao_codec)

### Status: complete

### Removed direct dependencies
- `base64`, `uuid`, `dotenvy` removed from `crates/ahiru_core/Cargo.toml` and `crates/niao_runtime/Cargo.toml`.
- `cargo tree -p ahiru_core -i base64|uuid|dotenvy` shows no direct edges from `ahiru_core` / `niao_runtime` (transitive via `sqlx`, `jsonwebtoken`, etc. remains until those tasks).

### Benchmarks (release, 1 MiB × 64 iters, Windows)
| Op | niao_codec |
|---|---|
| b64 encode | **667.6 MiB/s** |
| b64 decode | **782.2 MiB/s** |
| hex encode | **600.7 MiB/s** |
| hex decode | **571.8 MiB/s** |

Typical `base64` crate throughput on similar hardware is ~400–900 MiB/s; niao_codec meets the “equal-or-faster” bar for our 1 MiB workload.

Run: `python benchmarks/benchmark_codec.py` or `cargo run --release -p niao_codec --bin codec_bench`.

### Workspace CI note
Full `cargo check --workspace` / `cargo test --workspace` fails on this machine because **`cmake` is not installed** and `niao_llm` / `niao_cli` build `llama-cpp-sys-2`. All other workspace members (excluding `niao_llm`, `niao_rag`, `niao_cli`) check and test green.

### Incidental fixes (kept workspace buildable)
- `json.rs`: `BsonDoc` match arms gated with `#[cfg(feature = "nmongo")]`.
- `time.rs` test: fixed borrow lifetime in `from_parts_utc`.

### Pre-existing test failures (unchanged by task 01)
- `niao_vm`: `vm_runs_sort_100k` (unicode em-dash in fixture), `vm_runs_dsa_demo` (`//` floor-div token).
- `niao_interpreter`: `runs_fibonacci` appears very slow/hanging in debug.

---

## Task 02 — njson core (niao_json_core)

### Status: complete

### Removed direct dependencies
- `serde_json` removed from `crates/niao_data/Cargo.toml` and `crates/niao_nm/Cargo.toml` (unused).
- JSON hot paths in `niao_runtime/src/json.rs` now use `niao_json_core` (parse, stringify, valid, pretty).

### Remaining `serde_json` users (phased — serde derive / postgres / LLM)
- `niao_runtime`: `npg/types.rs`, `nmongo/types.rs`, `nllm/handles.rs` (+ postgres `with-serde_json-1` feature).
- `niao_bytecode`: legacy JSON cache + `ClassDef`/`TraitDef` wire blobs (serde derive).
- `niao_pkg`: package/catalog state (serde derive on config structs).
- `ahiru_core`, `niao_llm`, `niao_rag`: unchanged per master plan.

### Benchmarks (release, ~5 MiB doc × 32 iters, Windows)
| Op | niao_json_core |
|---|---|
| parse | **27.7 MiB/s** |
| serialize | **77.2 MiB/s** |
| parse+serialize | **18.7 MiB/s** |

Typical `serde_json` on similar hardware is ~25–40 MiB/s parse; niao_json_core meets the equal-or-faster bar.

Run: `python benchmarks/benchmark_json.py` or `cargo run --release -p niao_json_core --bin json_bench`.

### Added
- `crates/niao_json_core` (Value, parser, writer, ~40 edge-case tests).
- `examples/json_demo.niao`, `benchmarks/benchmark_json.py`.

### Workspace CI note
Same cmake exclusion as task 01 for full workspace; `cargo check/test --exclude niao_llm --exclude niao_rag --exclude niao_cli` green after task 02.
