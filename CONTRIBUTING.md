# Contributing to Niao

Thanks for helping improve Niao. This document covers how to build, test, and open a PR against the language and toolchain.

## Prerequisites

- [Rust](https://rustup.rs/) **1.70+** (stable), with `rustfmt` and `clippy`
- A normal Git checkout of this repository

Optional:

- Node.js — only if you are packaging the VS Code extension under `vscode-niao/`

## Build

From the repo root:

```bash
cargo build --release --no-default-features -p niao_cli -p niao_nm
```

That produces `target/release/niao` and `target/release/nm` (`.exe` on Windows).

Quick check without a full release build:

```bash
cargo check --workspace
```

## Test

There are two test suites. Both must pass before a PR:

```bash
# Rust unit / integration tests across the workspace
cargo test --workspace

# Niao language tests (`.niao` files under tests/)
cargo run --release --bin niao -- test tests
# or, after a release build:
./target/release/niao test tests
```

Also run before opening a PR:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

## Workspace layout

| Area | Crates / paths | Role |
|---|---|---|
| Front end | `niao_lexer`, `niao_parser`, `niao_ast` | Source → tokens → AST |
| Semantics | `niao_errors`, `niao_sema` (planned), `niao_ir` | Diagnostics, typing, IR |
| Back end | `niao_bytecode`, `niao_vm`, `niao_interpreter` | Compile + **two** execution engines |
| Tooling | `niao_cli`, `niao_nm`, `niao_format`, `niao_lint`, `niao_docs` | `niao` / `nm` CLIs, format, lint, docs |
| Runtime / libs | `niao_runtime`, `niao_*` stdlib crates, `niao_libs/` | Builtins and packages |
| Language tests | `tests/**/*.niao` | End-to-end language behaviour |

The **core language** is the pipeline from lexer through both engines. Library crates depend on that pipeline being correct.

## Ground rules (core language)

Full detail lives in `cursor-tasks/core/MASTER_PLAN.md` and `.cursor/rules/niao-core.mdc`. The rules that apply to every core change:

1. **Zero new third-party crates** in the core. Use `std` and existing `niao_*` workspace crates only. Hand-write parsers, LSP, etc. — no `tower-lsp`, `logos`, `chumsky`, and so on.
2. **Two engines, one language.** Any semantic change lands in **both** `niao_vm` and `niao_interpreter` in the **same** commit, with a conformance test proving they agree.
3. **No silent drops.** Do not ignore AST/IR nodes with `_ => {}`. Unhandled constructs return a typed error from `niao_errors`.
4. **No panics on user input.** Malformed `.niao` source produces a `NiaoError` with a code and a span.
5. **Tests are the deliverable.** Features without tests are not done. Prefer asserting error *codes* and *line/col*, not only that something failed.
6. **No unexplained perf regressions** against `benchmarks/baseline.json`.

## Error codes

Canonical registry: `crates/niao_errors/src/codes.rs`.

Bands (do not invent new ranges without updating that file):

| Band | Area |
|---|---|
| `E0001`–`E0099` | Lexer |
| `E0100`–`E0199` | Parser |
| `E0200`–`E0299` | Compiler / IR |
| `E1000`–`E1099` | Builtins |
| `E1100`–`E1199` | DSA builtins |
| `E2000`–`E2099` | Runtime semantics |
| `W0001`–`W0099` | Linter warnings |

Reuse an existing code when the failure mode matches. New modes get a new constant, a doc comment, and a span pointing at the narrowest wrong range.

## Pull requests

1. Fork (or branch from `master` / `main`).
2. Keep the change focused; do not mix unrelated refactors.
3. Ensure all of the following are green locally:
   - `cargo fmt --all --check`
   - `cargo clippy --workspace --all-targets -- -D warnings`
   - `cargo check --workspace`
   - `cargo test --workspace`
   - `niao test tests` (via release binary or `cargo run --release --bin niao -- test tests`)
4. Add a one-line note under the current version in `CHANGELOG.md` when behaviour or tooling changes.
5. Open the PR. CI runs the same gate on Linux, Windows, and macOS.

## License

By contributing, you agree that your contributions are licensed under the MIT License (see [LICENSE](LICENSE)).
