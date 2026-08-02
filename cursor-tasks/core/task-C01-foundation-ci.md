# Task C01 — Foundation & CI

**Goal:** a safety net, and a repo that is legally and cosmetically shippable.
Nothing in C02–C08 is safe to attempt without CI. Do this first.

**Read first:** `cursor-tasks/core/MASTER_PLAN.md`, `.cursor/rules/niao-core.mdc`.

---

## 1. Add the LICENSE file

`README.md` says "MIT — see [LICENSE]" and `Cargo.toml` declares `license = "MIT"`.
**There is no LICENSE file in the repo.** Nothing currently grants anyone the rights the
README claims to grant. Fix it:

- Create `LICENSE` at the repo root: standard MIT text, `Copyright (c) 2026 Niao Contributors`.
- Confirm `vscode-niao/LICENSE` says the same thing and the same copyright holder.

## 2. Add CI

There is no `.github/` directory. Nothing runs the 2,056 tests in this repo on push.

Create `.github/workflows/ci.yml`:

- Triggers: `push` and `pull_request` on `master`/`main`.
- Matrix: `ubuntu-latest`, `windows-latest`, `macos-latest`. Stable Rust.
- Cache `~/.cargo` and `target/` keyed on `Cargo.lock`.
- Steps, in order, each a hard failure:
  1. `cargo fmt --all --check`
  2. `cargo clippy --workspace --all-targets -- -D warnings`
  3. `cargo check --workspace`
  4. `cargo test --workspace`
  5. `cargo build --release --no-default-features -p niao_cli -p niao_nm`
  6. `./target/release/niao test tests`
- `concurrency: cancel-in-progress` so pushes supersede.

**If step 1 or 2 fails on the existing tree, do not weaken the step.** Fix the code, or if
the volume is genuinely unmanageable, split it into a separate non-blocking `lint` job and
open a follow-up task file — but say so explicitly in the summary. Do not silently drop
`-D warnings`.

Add a CI status badge to the top of `README.md`.

## 3. Purge the `neko` legacy

Niao was previously called `neko`. Artifacts under the old name are **tracked in git**:

```
windows/NekoSetup.exe                       <- a committed binary installer
benchmarks/dsa_bench.nekobc
examples/dsa_demo.nekobc
examples/factorial.nekobc
examples/io_demo.nekobc
examples/.neko-build/ahiru_hello.nekobc
examples/.niao/neko_libs/catalog.json
examples/.niao/neko_libs/io/0.1.0/lib.json
examples/.niao/neko_libs/io/package.json
examples/.niao/neko_libs/json/0.1.0/lib.json
examples/.niao/neko_libs/json/package.json
tests/dsa_arrays.nekobc      tests/dsa_graph.nekobc     tests/dsa_heap.nekobc
tests/dsa_list.nekobc        tests/dsa_set_map.nekobc   tests/dsa_stack_queue.nekobc
tests/errors_typed.nekobc    tests/io.nekobc            tests/json.nekobc
tests/obj_lit.nekobc
vscode-niao/icons/neko-file-icon.png
windows/.neko-build/examples_hello.nekobc
windows/.neko-build/examples_libs_smoke.nekobc
windows/.neko-build/examples_re_demo.nekobc
```

These are stale build caches and an old installer binary. Do:

- `git rm` all of the above. They are regenerable build output, not source.
- Rename `vscode-niao/icons/neko-file-icon.png` → `niao-file-icon.png` and update every
  reference in `vscode-niao/package.json`.
- Rebuild the VS Code extension so it packages as `niao-language-*.vsix`, not
  `neko-language-0.1.0.vsix` / `neko-language-0.1.1.vsix`. Delete the two stale `.vsix`
  files from the repo — build artifacts do not belong in git.
- **Keep** the legacy-shim removal logic in `install.ps1` (`Remove-LegacyNekoShims`) and
  `crates/niao_nm/src/main.rs` (`remove_legacy_neko_shim`). Those exist to clean up
  *users'* machines that still have the old install. Leave them, and add a comment on each
  saying it is a migration path for pre-rename installs and can be removed after v0.3.

Extend `.gitignore` — it currently ignores `.niao-build/` and `*.niaobc` but not the old
names, which is exactly why the above got committed:

```
.neko-build/
*.nekobc
**/.niao/neko_libs/
*.vsix
```

## 4. Clean the working tree

The repo root has ~80 untracked scratch files from past debugging sessions:
`nview_*.log`, `nview_*.txt`, `ngcp_*.txt`, `ngcp_*.log`, `rt_*.txt`, `rt_*.log`,
`ns_*.txt`, `iso_rt_test.*`, `nsearch_*.log`, `nmverify.txt`, `cargo_rt_nreq_check*.txt`,
`_xlsx_build*.txt`, `_nmqtt_check.txt`, `build_*.txt`, `err*.txt`, `final_rt_test.log`,
`cli_log.txt`, `niao_cli_dbg.log`, `target-*.log`, `nreq_bench_*.txt`, `niao_rust_deps.txt`.

- Delete them.
- Add a `.gitignore` block so they cannot come back:
  ```
  # scratch build/debug output
  *.log
  *.pid
  /nview_*
  /ngcp_*
  /rt_*
  /ns_*
  /iso_*
  /_*.txt
  /nmverify.txt
  /cargo_rt_*.txt
  ```
- `niao_rust_deps.txt` **is** tracked — `git rm` it, it is a stale `cargo tree` dump.
- Two helper scripts at the root, `_defer_broken_libs.py` and `_wire_expansion_libs.py`,
  look like one-off migration tooling. If they are still used, move them to `scripts/` and
  add a header comment saying what they do. If not, delete them.

## 5. Add CONTRIBUTING.md

`README.md` says "Contributions are welcome" and describes a 4-step flow that is documented
nowhere. Write `CONTRIBUTING.md` covering:

- Prerequisites (Rust 1.70+), how to build, how to run both test suites.
- The workspace layout and which crate does what.
- The ground rules from `cursor-tasks/core/MASTER_PLAN.md` — especially **zero new
  third-party crates** and **semantics land in both engines**.
- Error-code conventions and the band allocation from `crates/niao_errors/src/codes.rs`.
- That `cargo fmt`, `cargo clippy`, `cargo test --workspace`, and `niao test tests` must
  all pass before a PR.

---

## Acceptance

- [ ] `LICENSE` exists at repo root with MIT text.
- [ ] `.github/workflows/ci.yml` exists and is green on all three platforms.
- [ ] `git ls-files | grep -i neko` returns **nothing** except the intentional legacy-shim
      code in `install.ps1` and `crates/niao_nm/src/main.rs`.
- [ ] `git status --short` is clean after a fresh `cargo build`.
- [ ] `CONTRIBUTING.md` exists.
- [ ] `cargo check --workspace`, `cargo test --workspace`, `niao test tests` all green.
- [ ] `CHANGELOG.md` updated.

**Do not change any language behaviour in this task.** If a test fails, that is a finding —
write it down in the summary, do not fix it here.
