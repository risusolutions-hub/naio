# Task C06 — Closures and First-Class Functions

**Goal:** close the largest expressiveness gap in the language.

**Read first:** `cursor-tasks/core/MASTER_PLAN.md`, `.cursor/rules/niao-core.mdc`.
**Prerequisite:** C05 Phase 2 (symbol resolution) at minimum — you need scope analysis to
compute captures. C05 Phase 1's `Ty::Fn` should already exist.

---

## The problem

`niao_ast::Expr` has 16 variants. None of them is a function literal. `TopLevel::Fn` is the
only way to make a function, and a function name is not a value.

So none of this is expressible in Niao today:

```niao
let doubled = map(nums, fn(x) { return x * 2 })
let sorted  = sort_by(people, fn(a, b) { return a.age < b.age })
on_click(fn() { print("clicked") })
let counter = make_counter()          // returns a closure over its own state
```

No `map`, no `filter`, no `reduce`, no callbacks, no strategy objects, no lazy
initialisation, no custom comparators. Every higher-order pattern is currently blocked, and
every library that wants a callback has to invent a workaround.

This is the single feature most likely to change how the language *feels*.

---

## Design decisions — make these first, write them in `docs/DECISIONS.md`

1. **Syntax.** Recommended: `fn(x, y) { ... }` as an expression — reuses the existing `fn`
   keyword and the existing parser path, no new tokens, no `=>` ambiguity with the object
   literal `{`. A concise arrow form can come later; do not do both now.
2. **Capture semantics.** Recommended: **capture by reference with shared mutable
   environment** (JS/Python semantics) — it matches user expectation and makes
   `make_counter()` work. Capture-by-value is simpler and faster but surprises people.
   Whichever you choose, the loop-variable case is the one everybody hits:
   ```niao
   let fns = []
   for i in [1,2,3] { fns.push(fn() { return i }) }
   ```
   Decide whether `i` is fresh per iteration (Rust/modern JS `let`) or shared (old JS
   `var`). **Recommended: fresh per iteration** — it is what people mean. Write the test
   first, then implement to it.
3. **Recursion.** Can a closure call itself? Needs either a self-binding or named function
   expressions. Pick one; do not leave it undefined.
4. **`self` inside a closure in a method.** Recommended: lexical — the enclosing method's
   `self`. Do not rebind.
5. **Are top-level `fn`s now values?** Recommended yes — `let f = add` should work, and it
   is what users will try first.

## Implementation

- `niao_ast`: `Expr::Lambda { params, return_type, body, span }`.
- `niao_parser`: parse `fn(` in expression position. Watch the ambiguity with `fn` in
  statement position — a `fn` at the start of a statement is still a declaration.
- `niao_sema`: **compute the capture set per lambda.** This is the real work. For each
  lambda, walk the body, find every free variable, resolve it to an enclosing scope, and
  record whether it is captured by value or by reference. Type it as `Ty::Fn`.
- `niao_ir` / `niao_bytecode`: a `MakeClosure` instruction taking a function index and the
  capture list; an upvalue representation. Read the closure chapter of *Crafting
  Interpreters* — the flat-closure/upvalue design there is the right shape for this VM.
- `niao_vm`: closure objects, upvalues, and **GC integration**. `crates/niao_vm/src/gc.rs`
  is mark-and-compact; closures introduce new roots and new cycles. A closure capturing an
  object that references the closure is a cycle — the collector must handle it, and there
  must be a test that allocates a large number of such cycles and asserts the heap does not
  grow.
- `niao_interpreter`: the same semantics. Environment-chain closures are the natural fit
  here; **they must observably match the VM's flat closures.**
- `niao_format`, `niao_lint`, `niao_docs`, `vscode-niao/syntaxes/`.
- `docs/grammar.ebnf`.

## Then use it

Once closures work, add the higher-order builtins that were impossible before. Put them in
`niao_runtime` and expose them on arrays:

`map` · `filter` · `reduce` / `fold` · `for_each` · `find` · `any` · `all` · `sort_by` ·
`min_by` · `max_by` · `group_by` · `partition` · `flat_map` · `take_while` · `drop_while`

Each gets a conformance test and appears in a new `examples/higher_order.niao`.

---

## Tests

- Capture by reference: a counter closure that mutates captured state across calls.
- The loop-variable case from Design Decision 2 — assert the decided behaviour explicitly.
- Nested closures, three levels deep, each capturing from a different enclosing scope.
- A closure returned from a function, called after the defining frame is gone. This is the
  test that catches a broken upvalue implementation.
- A closure passed to a native builtin and invoked from Rust.
- Closures capturing `self` inside a method.
- Recursive closure, per Design Decision 3.
- Arity mismatch on a closure call → typed error with a span.
- **GC:** allocate 100k closures capturing large objects, force collection, assert the heap
  shrinks. Then allocate closure/object reference cycles and assert the same.
- **Conformance cases for every one of the above** — VM and interpreter must agree,
  especially on capture semantics, where they are most likely to diverge.

---

## Acceptance

- [ ] `fn(x) { ... }` works as an expression in both engines with identical semantics.
- [ ] Captured state is mutable and shared per the documented rule; the counter test passes.
- [ ] The loop-variable case behaves as documented, with a test asserting it.
- [ ] Closures survive the defining frame; the returned-closure test passes.
- [ ] GC handles closures and closure cycles; heap tests pass.
- [ ] `Ty::Fn` is checked by sema — arity and argument types on closure calls.
- [ ] 14 higher-order builtins implemented with tests.
- [ ] `examples/higher_order.niao` runs and demonstrates them.
- [ ] Conformance cases added; both engines agree.
- [ ] `docs/DECISIONS.md` records all five design decisions with rationale.
- [ ] `docs/grammar.ebnf` updated.
- [ ] `cargo check --workspace`, `cargo test --workspace`, `niao test tests` green.
- [ ] `CHANGELOG.md` updated.
- [ ] No regression in `benchmarks/baseline.json`. Closures add an indirection to calls —
      if the non-closure call path slows down, keep it separate and say so with numbers.
