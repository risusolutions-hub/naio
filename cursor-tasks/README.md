# Cursor-Agent Work Orders for Niao
12 self-contained tasks. Run them IN ORDER — later tasks depend on earlier crates
(niao_codec → niao_crypto → niao_http → niao_ws/ndb, etc.).

## How to run (PowerShell, from C:\Risu\Neko)
    cursor-agent -p (Get-Content cursor-tasks\task-01-foundations.md -Raw)

or interactively: `cursor-agent`, then paste the task file content.

## Per-task loop
1. `git add -A; git commit -m "checkpoint before task NN"`  (always checkpoint first)
2. Run the task with cursor-agent.
3. Review: `git diff --stat`, then `cargo check --workspace && cargo test --workspace`.
4. Run benchmarks it touched; compare numbers in its report.
5. Commit or revert. Never start task N+1 on a red workspace.

## Do NOT let the agent rewrite
rustls, cranelift-*, candle/llama-cpp/ort/tokenizers, rusqlite — see MASTER_PLAN.md.

## Order & what each removes
01 codec      → base64, uuid, dotenvy
02 njson      → serde_json (most of it)
03 toml       → toml
04 crypto     → sha2, hmac, jsonwebtoken
05 nregex     → regex
06 ntime      → chrono, chrono-tz
07 nhttp      → httparse, tiny_http, ureq, url, http
08 nws        → tungstenite
09 nio        → tokio (runtime side, phase 1) + ahiru migration map
10 ndb        → redis, postgres, r2d2, r2d2_postgres
11 narchive   → flate2, tar, zip
12 vm-perfect → no removals; GC/JIT/startup/bench CI gates
