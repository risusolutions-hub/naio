# ntest standard library

A tiny, fast test framework: register cases, run them with per-test isolation, and use rich assertions. The runner prints `PASS/FAIL/SKIP` lines and returns a summary object.

## Import

```niao
import "ntest"
```

Paths `import "std/ntest"` and `import "ntest"` are equivalent. Flat builtins (`ntest_case`, `ntest_assert_eq`, …) are also available globally after import.

## Quick start

```niao
import "ntest"
import "nstr"

fn test_snake() {
    ntest.assert_eq(nstr.snake("HelloWorld"), "hello_world")
}

fn test_math() {
    ntest.assert_near(0.1 + 0.2, 0.3, 0.000001)
}

fn test_todo() {
    ntest.fail("todo")
}

ntest.case("snake_case works", test_snake)
ntest.case("math holds", test_math)
ntest.skip("not ready yet", test_todo)

let summary = ntest.run()
if !summary.ok { nos.exit(1) }
```

Output:

```
PASS  snake_case works
PASS  math holds
SKIP  not ready yet

3 test(s): 2 passed, 0 failed, 1 skipped in 1ms
```

## Registration & running

| Method | Description |
|--------|-------------|
| `ntest.case(name, fn)` | Register a test. |
| `ntest.skip(name, fn)` | Register but don't run (counted as skipped). |
| `ntest.run(filter?)` | Run all (or names containing `filter`). Returns summary. |
| `ntest.count()` | Registered test count. |
| `ntest.clear()` | Reset the registry. |

A test **fails** when it throws (any assertion failure or runtime error) or returns an `error` value. Everything else passes.

**Summary object:** `{total, passed, failed, skipped, duration_ms, ok, failures: [{name, message}]}`.

## Assertions

| Method | Description |
|--------|-------------|
| `ntest.assert_true(v, msg?)` / `assert_false(v, msg?)` | Must be exactly `true`/`false`. |
| `ntest.assert_eq(a, b, msg?)` / `assert_ne(a, b, msg?)` | Deep value equality. |
| `ntest.assert_near(a, b, eps?)` | Numeric closeness (default eps `1e-9`). |
| `ntest.assert_contains(hay, needle, msg?)` | String substring, array element, or object key. |
| `ntest.assert_error(v, msg?)` / `assert_not_error(v, msg?)` | Error-value checks. |
| `ntest.fail(msg?)` | Unconditional failure. |

## Errors

| Code | Meaning |
|------|---------|
| 2660 | Wrong argument count. |
| 2661 | Runner error. |
| 2662 | Assertion failure (propagates like a thrown error; caught per-test by the runner). |
