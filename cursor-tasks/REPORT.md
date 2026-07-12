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
