# Task C01b — Make lint blocking again

**Why this exists:** Task C01 required `cargo fmt --all --check` and
`cargo clippy --workspace --all-targets -- -D warnings` as hard CI failures.
On the Aug 2026 tree that is not yet viable:

- `cargo fmt --all` was applied during C01 (and a duplicate `mod linux` /
  `mod macos` declaration in `crates/niao_io/src/poller/unix.rs` was removed so
  rustfmt could resolve modules). Fmt should stay green.
- `cargo clippy --workspace --all-targets -- -D warnings` fails immediately on
  existing style lints (e.g. `clippy::manual_div_ceil` and
  `clippy::redundant_closure` in `niao_parallel`) across a 130-crate workspace.
  Clearing that under `-D warnings` is a dedicated cleanup, not foundation work.

C01 therefore put workspace fmt+clippy in a **non-blocking** `lint` job
(`continue-on-error: true`) in `.github/workflows/ci.yml`, while keeping
check / test / release build / `niao test tests` required. **`-D warnings` was
not dropped** — it still runs in the lint job.

A middle-ground guardrail is already in the required `ci` job: clippy with
`-D warnings` for the **core crates only** (`niao_lexer`, `niao_parser`,
`niao_ast`, `niao_ir`, `niao_bytecode`, `niao_vm`, `niao_interpreter`,
`niao_errors`). This task is about making the **workspace-wide** lint job
blocking.

## Goal

Make the `lint` job required (remove `continue-on-error`) with both steps green
on Ubuntu, Windows, and macOS.

## Work

1. Run `cargo clippy --workspace --all-targets -- -D warnings` and fix every
   finding. Prefer real fixes over `#[allow(...)]`; allow only with a one-line
   justification when the lint is wrong for hot paths.
2. Confirm `cargo fmt --all --check` is still green.
3. In `.github/workflows/ci.yml`:
   - Remove `continue-on-error: true` from the `lint` job, **or** fold fmt/clippy
     back into the main `ci` job as hard failures (preferred end state from C01).
4. Do **not** weaken `-D warnings`.
5. `CHANGELOG.md` one-liner under Unreleased.

## Acceptance

- [ ] `cargo fmt --all --check` green
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` green
- [ ] Lint is a hard CI failure again on all three platforms
- [ ] `cargo check --workspace`, `cargo test --workspace`, `niao test tests` still green
