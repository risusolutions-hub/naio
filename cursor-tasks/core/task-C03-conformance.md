# Task C03 — Kill Silent Drops, Build the Conformance Harness

**Goal:** make it impossible for the compiler to silently ignore code, and impossible for
the two engines to diverge without a test failing.

**Read first:** `cursor-tasks/core/MASTER_PLAN.md`, `.cursor/rules/niao-core.mdc`.
**Prerequisite:** C02 green.

This is the most important task in C01–C04. Every language feature after this one depends
on the harness built here. Do not skip it to get to C05 faster.

---

## Part 1 — `niao_ir` silently discards top-level constructs

`crates/niao_ir/src/lib.rs`, the top-level lowering loop, handles `TopLevel::Trait`,
`Class`, `Fn`, `Import`, and `Stmt` — then ends with:

```rust
_ => {}
```

That catch-all swallows `TopLevel::Struct`, `TopLevel::Server`, and `TopLevel::Route`.
Consequences, all silent, all exit code 0:

1. **`struct` definitions do not exist in VM mode.** `Expr::StructInit` lowers to
   `IrInstr::MakeInstance` using only the field *count*, so a misspelled or missing field
   is never caught — you get a malformed object instead of an error.
2. **`niao run server.niao` on a file containing a `server { }` block does nothing.**
   No output, no diagnostic, exit 0. The user's program silently did not run.

Fix, in this order:

- **Replace `_ => {}` with an exhaustive match.** List every `TopLevel` variant explicitly.
  Anything not yet lowerable returns `E0200_UNSUPPORTED` naming the construct and its span.
  From now on, adding an AST variant must break the build until it is handled.
- **Lower `TopLevel::Struct` properly.** Register the struct name, its ordered field names,
  and its declared field types in the IR module alongside `classes` and `traits`. Make
  `MakeInstance` validate against that: unknown field → `E1010`-band error; missing field →
  a distinct error naming which field; duplicate field → its own error. Wire the same
  validation into `niao_interpreter` so both engines agree.
- **Make `server`/`route` blocks explicit.** Either lower them, or — the smaller change —
  have `niao run` detect a `ServerBlock`/`RouteBlock` in the parsed program and either
  dispatch to the serve path automatically or fail with a clear message: *"this file
  defines a web server; run it with `niao serve <file>`"*. Silence is the one option that
  is not acceptable.

Then sweep the rest of the pipeline for the same pattern:

```
rg '_ => \{\}|_ => \{ \}|_ => Ok\(\(\)\)' crates/niao_ir crates/niao_bytecode crates/niao_vm crates/niao_interpreter crates/niao_ast
```

Every hit on an AST/IR/opcode match is a potential silent miscompile. Convert each to an
exhaustive match or a typed error. Where a catch-all is genuinely correct, leave a comment
saying why.

## Part 2 — Engine selection is a string match

`crates/niao_cli/src/main.rs`:

- `run_file()` (line ~408) picks the engine with `mode == "interp" || has_file_imports(file)`.
- `has_file_imports()` (line ~390) **returns `false` when the file fails to parse**, so a
  syntax error silently routes to the VM and reports from there.
- `test_execution_mode()` (line ~509) picks the engine for `niao test` with
  `source.contains("interpreter mode")` — a substring match against the source text,
  including inside comments and string literals.

Fix:

- Parse once. Thread the resulting `Program` through instead of parsing three times.
- On parse failure, **report the parse error** — never fall through to a default engine.
- Replace the `"interpreter mode"` substring match with an explicit directive on the first
  line of the file, e.g. `#!niao-engine: interp`, parsed properly. Migrate the affected
  test files.
- Make engine selection explicit and greppable: one function returning an
  `enum Engine { Vm, Interp }`, with the reason recorded so `--verbose` can print
  *why* an engine was chosen.

## Part 3 — The conformance harness

**This is the deliverable.** There is currently no test anywhere that runs the same program
through both engines and compares the results. With 49 core tests across 11.9k lines,
divergence is not a hypothetical.

Build `tests/conformance/`:

- Each case is a `.niao` file plus an expected-output `.txt` (and, for error cases, an
  expected error code and line/col).
- A Rust integration test — `crates/niao_cli/tests/conformance.rs` — that, for every case:
  1. runs it under the VM,
  2. runs it under the interpreter,
  3. asserts **VM output == interpreter output**,
  4. asserts both match the expected file.
- A divergence failure must print a readable diff naming both engines. This message will be
  read a lot — make it good.
- Wire it into `.github/workflows/ci.yml` from C01.
- `UPDATE_EXPECT=1 cargo test` regenerates expected files, so adding a case is cheap.
  Regenerated files still have to be reviewed in the diff.

Seed it with at least 60 cases covering what the language has **today** — this is a
characterisation suite, so it captures current behaviour, bugs included:

- literals, all operators, precedence, short-circuit `&&`/`||`
- `if`/`else if`/`else`, `while`, `for`-`in`, `break`, `continue`
- functions: recursion, mutual recursion, early return, wrong arity
- closures over loop variables — **document whatever each engine does today**
- classes: fields, methods, static methods/fields, `extends`, `super`, `implements`
- structs: construction, field access, and the error cases from Part 1
- objects, arrays, nested indexing, out-of-bounds
- `try`/`catch`/`throw`, including throwing from a nested call
- string escapes and all numeric literal forms from C02
- integer overflow, division by zero, float edge cases (`nan`, `inf`, `-0.0`)
- every builtin reachable without an import
- top-level statements with and without `fn main()`
- one case per error code you can trigger from source

**Expect this to fail on first run.** Every divergence it finds is a real bug that was
already shipping. Triage: fix what is clearly wrong, and for anything ambiguous, record the
current behaviour in the expected file plus a `# DIVERGENCE:` comment and a line in
`docs/DECISIONS.md`. Do not paper over a difference by making the test lenient.

---

## Acceptance

- [ ] No `_ => {}` remains on any AST/IR/opcode match in the core crates.
- [ ] `struct` is lowered in VM mode; unknown/missing/duplicate fields are typed errors in
      **both** engines.
- [ ] `niao run` on a file with a `server` block either serves or errors — never silence.
- [ ] Parse failures always surface; no silent engine fallback.
- [ ] `test_execution_mode`'s substring match is gone.
- [ ] `tests/conformance/` has 60+ cases; both engines agree on all of them.
- [ ] Conformance runs in CI.
- [ ] Every divergence found is either fixed or documented in `docs/DECISIONS.md`.
- [ ] `cargo check --workspace`, `cargo test --workspace`, `niao test tests` green.
- [ ] `CHANGELOG.md` updated.

**Write the divergence list into the task summary.** It is the most valuable output of this
task and it feeds directly into C05.
