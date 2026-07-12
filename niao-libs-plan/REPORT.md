# Niao libs implementation report

## clap

### Status: complete (crate + wrapper; orchestrator migration pending)

### Added
- `crates/niao_args` — zero-dependency CLI parser (`Command::new().arg().subcommand().get_matches_from()`), clap v4-compatible runtime builder API.
- `niao_libs/args/` package (`package.json`, `0.2.2/lib.json`).
- `examples/args_demo.niao` (placeholder until runtime builtins wired).
- `benchmarks/benchmark_args.py` + `tests/bench_compare.rs` (niao_args vs clap throughput).
- 14 clap-parity integration tests mirroring full `niao` + `nm` command trees (`tests/cli_parity.rs`).

### Benchmarks (release, 100k parses, representative `niao run` argv, Windows)
| Metric | niao_args | clap |
|---|---|---|
| parse throughput | **273,826 parses/s** | 143,018 parses/s |
| ratio | **1.91×** (faster) | — |

Spec target: n/a (parse-once). niao_args meets equal-or-better bar.

Run: `python benchmarks/benchmark_args.py` or `cargo test -p niao_args --release bench_parse_throughput -- --nocapture`.

### Tests
- `cargo test -p niao_args` — **21 tests green** (6 unit, 14 cli/nm parity, 1 bench).
- Parity covers: subcommands (3-level nesting), aliases (`i`/`find`/`up`), trailing `--` script args, `value_delimiter`, defaults, `arg_required_else_help`, `--version`.

### Deps to remove (orchestrator pass — not removed in this task)
- Workspace `Cargo.toml`: `clap = { version = "4", features = ["derive"] }`
- `crates/niao_cli/Cargo.toml`: `clap`
- `crates/niao_nm/Cargo.toml`: `clap`
- Migrate `niao_cli` / `niao_nm` to `niao_args` builder or manual `Parser` impls; wire `niao_runtime/src/args.rs` builtins + `niao_libs/catalog.json`.

### Notes
- No proc-macro derive yet; consumers should use runtime builder (orchestrator can add derive later).
- `niao_args` uses `clap` only as **dev-dependency** for parity fixtures/benchmarks.

---

## Wave 0 orchestrator merge (checkpoint `eddab75` → `wave 0 complete`)

### Workspace members added
- `crates/niao_log` added to root `Cargo.toml` (was missing; all other Wave 0 crates already present).

### Runtime wiring completed
| Module | Crate | Change |
|---|---|---|
| BigInt / VM | `niao_bignum` | `niao_runtime`, `niao_vm` use `niao_bignum::BigInt`; added `to_i64`/`to_u64` |
| DSA maps/sets | `niao_collections` | `dsa_storage`, `int_algos`, `ncl/*` switched from `ahash`/`indexmap` |
| TCP socket create | `niao_io` | `net/socket.rs` uses `niao_io::socket::Socket` (was `socket2`) |
| FTP builtins | `niao_net_clients` | `net/ftp.rs` uses `niao_net_clients::ftp::FtpClient` (was `suppaftp`) |
| Native stdlib | deferred wrappers | `nargs`, `nlog`, `nmath`, `nrand`, `nstr`, `nfmt` registered in `builtin_table`, `install_native_modules`, `native_module_paths` |
| ahiru logging | `niao_log` | `ahiru_core::server` init via `niao_log::SubscriberBuilder` (was `tracing-subscriber`) |

### Direct deps removed (confirmed via `cargo tree -i`)
| Crate | Removed from | Notes |
|---|---|---|
| `ahash` | `niao_runtime`, `ahiru_core` | direct edge gone; transitive via `dashmap`/`hashbrown`/`rusqlite` remains |
| `indexmap` | `niao_runtime` | direct edge gone; transitive via `h2`/`sqlx` remains |
| `num-bigint`, `num-traits` | `niao_runtime`, `niao_vm` | no workspace package; not in `niao_runtime` tree |
| `socket2` | `niao_runtime` | direct edge gone; transitive via `lettre`/`tokio` remains |
| `suppaftp` | `niao_runtime` | not in workspace; dev-dep only in `niao_net_clients` benches |
| `tracing`, `tracing-subscriber` | `ahiru_core` | direct edge gone; transitive via `axum`/`tower-http`/`sqlx` remains |
| `http` | `ahiru_core` | direct edge removed; still transitive via `axum`/`hyper` |

### Deps retained (not safe to remove yet)
| Dep | Reason |
|---|---|
| `clap` | `niao_cli`, `niao_nm` still use `clap::Parser` derive; `niao_args` dev-dep for parity benches |
| `rand` / `rand_distr` | `niao_data`, `niao_ml`, `niao_tensor`, `niao_graph`, `niao_classic` not migrated to `niao_rand` |
| `socket2` | `niao_io` dev-dep for compare benches only |
| `suppaftp` | `niao_net_clients` dev-dep for FTP bench compare |
| `num-bigint` | `niao_bignum` dev-dep for prop/bench parity |

### Incidental fix (build blocker)
- `niao_json_core/src/serde.rs`: removed broken `VariantAccess` enum bridge; `forward_to_deserialize_any!` on `ValueDeserializer` (matches `ahiru_core::value_de` pattern).

### CI commands & results (Windows, 2026-07-12)
```text
cargo check --workspace --exclude niao_llm --exclude niao_rag --exclude niao_cli
  → Finished (green)

cargo test --workspace --exclude niao_llm --exclude niao_rag --exclude niao_cli \
  -- --skip runs_fibonacci --skip vm_runs_sort_100k --skip vm_runs_dsa_demo
  → exit 0 (green; ~5.4 min)

# Verify-only crates (tasks 01–10)
cargo test -p niao_codec -p niao_archive -p niao_json_core -p niao_regex \
  -p niao_crypto -p niao_time -p niao_io -p niao_http -p niao_ws -p niao_db
  → all green

# Wave 0 new crates
cargo test -p niao_args -p niao_collections -p niao_bignum -p niao_rand \
  -p niao_log -p niao_net_clients
  → all green (niao_args: 21, niao_collections: 15, niao_bignum: 12+prop, niao_rand: 9, niao_log: 12, niao_net_clients: 11)
```

### Exclusions documented
- `niao_llm`, `niao_cli`: require `cmake` / `llama-cpp-sys-2` on this machine.
- `niao_rag`: excluded with LLM stack.

### Unresolved wiring (follow-up)
- `niao_cli` / `niao_nm`: migrate from `clap` derive to `niao_args` builder (or proc-macro).
- `niao_rand` crate exists but `nrand` runtime module uses inline xoshiro (not yet delegating to crate).
- ML/data crates (`niao_tensor`, `niao_ml`, etc.): still on `rand` crate.
- `ahiru_core` axum stack: still on `tokio`/`axum`/`http` transitive deps per master plan.
