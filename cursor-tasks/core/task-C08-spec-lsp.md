# Task C08 — Language Spec and LSP

**Goal:** make Niao learnable and make writing it feel modern. This is the adoption task.

**Read first:** `cursor-tasks/core/MASTER_PLAN.md`, `.cursor/rules/niao-core.mdc`.
**Prerequisite:** C05 (sema) — an LSP without a resolver and type checker can only do
syntax highlighting, which you already have.

---

## Part 1 — The grammar is wrong

`docs/grammar.ebnf` is 91 lines and is the **only** language specification in the repo.
It contradicts the implementation in at least eight places:

| # | `grammar.ebnf` says | The parser actually does |
|---|---|---|
| 1 | `program = { import_stmt \| top_level }` | `TopLevel::Stmt` exists — top-level statements are legal, and **the README's headline feature** |
| 2 | `type_name` includes `"error"` | The lexer has **no** `error` token; `error` lexes as an identifier |
| 3 | `type_name` omits `array` | The lexer emits `TokenKind::TypeArray` |
| 4 | `int_lit = digit { digit }` | Hex `0x` is supported (and after C02, `0b`, `0o`, `_`, exponents) |
| 5 | `;` required after most statements | Optional in practice |
| 6 | `assign_stmt = ident assign_op expr` | `AssignTarget` also allows `Member` and `Index` |
| 7 | `primary` has no `self` / `super` | `Expr::SuperCall` exists; `self` is a keyword |
| 8 | Comments undocumented | Both `#` and `//` are comments (see C02) |

Rewrite it **from the parser**, not from memory. Then keep it honest:

- Add a test that parses every construct in the EBNF and asserts it is accepted, and that
  a curated list of near-miss strings is rejected. A grammar that can drift silently will.
- Regenerate for every feature added in C02, C04, C06, C07.

## Part 2 — Write the language reference

There are 205 files in `docs/`. Every one is a per-library API reference (`NJSON.md`,
`NPG.md`, `NMONGO.md`, …). **There is no document that teaches the language.** A newcomer
has the README and then falls off a cliff.

Write two documents:

**`docs/LANGUAGE.md`** — the reference. One section per construct, each with a working
example that is **extracted and run in CI** (write a small harness that pulls fenced
```niao blocks and executes them — documentation that rots is worse than none):

values and types · variables and scope · operators and precedence · control flow ·
functions and closures · structs · classes, inheritance, traits · enums and `match` ·
errors: `try`/`catch`/`throw` · imports and modules · the type system and where `Unknown`
applies · the web DSL · execution modes (VM vs interpreter) and when each is used

**`docs/TUTORIAL.md`** — 45 minutes, zero to a working HTTP service. Install → hello world
→ variables and functions → collections → a struct and a class → error handling → reading a
file → an HTTP call → an ahiru endpoint. Every step runnable, every step's output shown.

Also fix `README.md`: it links `docs/grammar.ebnf` as the language documentation. After
this task it should link `docs/LANGUAGE.md` and `docs/TUTORIAL.md` first.

## Part 3 — The LSP

`vscode-niao/` is a TextMate grammar and a file icon. No completion, no go-to-definition,
no hover, no inline errors. Every modern language user expects all four within a minute of
opening a file.

Create `crates/niao_lsp`, a binary speaking Language Server Protocol over stdio.
**Zero new third-party crates** — that includes `tower-lsp`. LSP is JSON-RPC over stdio;
you already have `niao_json_core` and `niao_rpc`. Write the framing layer by hand.

Ship in this order — each step is independently useful:

1. **Diagnostics.** Run lexer → parser → sema on change (debounced) and publish errors with
   spans. This alone is 80% of the value and it works the moment C05 lands.
2. **Hover.** Type and doc comment for the symbol under the cursor — straight from sema.
3. **Go-to-definition** and **find-references.** Both are symbol-table lookups from C05
   Phase 2.
4. **Completion.** In-scope symbols; members after `.` using the receiver's inferred type;
   keywords.
5. **Document symbols** (outline) and **signature help**.
6. **Rename**, using the reference index from step 3.
7. **Formatting**, delegating to `niao_format` — which by then has the round-trip property
   tests from C04.

Requirements:

- Incremental where it is cheap; a full re-analysis on change is acceptable at this codebase
  size, but **measure it.** Sub-100ms on a 1,000-line file, or fix it.
- Never panic. An LSP crash takes the editor experience down with it. Catch at the boundary
  and log.
- A crash log the user can find and attach to a bug report.

Then update `vscode-niao/` to launch it: client activation, `.niao` language contribution,
bundled server binary or a discovery path, and a settings section. Bump the extension
version, package it as `niao-language-*.vsix` (the `neko-*` artifacts are removed in C01),
and write a short `vscode-niao/README.md` section on what the extension now does.

## Part 4 — REPL and debugger (scope check)

`docs/NREPL.md` and `docs/NDEBUG.md` exist. Verify whether the implementations behind them
do. If they are docs for planned features, say so in the summary and open follow-up tasks
rather than quietly leaving the docs implying otherwise.

---

## Acceptance

- [ ] `docs/grammar.ebnf` matches the parser; all eight contradictions fixed; a drift test
      guards it.
- [ ] `docs/LANGUAGE.md` covers every construct, examples extracted and run in CI.
- [ ] `docs/TUTORIAL.md` takes a newcomer to a working HTTP service; every step runs.
- [ ] `README.md` links both before the grammar.
- [ ] `crates/niao_lsp` exists, zero new third-party dependencies.
- [ ] Diagnostics, hover, go-to-definition, find-references, and completion all work in
      VS Code against a real project.
- [ ] Analysis is sub-100ms on a 1,000-line file, with the measurement recorded.
- [ ] The server does not panic on malformed input — fuzz it with the C02 lexer corpus.
- [ ] Extension packages as `niao-language-*.vsix` and launches the server.
- [ ] REPL and debugger status confirmed and documented honestly.
- [ ] `cargo check --workspace`, `cargo test --workspace`, `niao test tests` green.
- [ ] `CHANGELOG.md` updated.
