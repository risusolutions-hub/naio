# nexpr standard library

Safe sandboxed expression evaluator for user formulas and config logic. Native implementation — Python `simpleeval` / `asteval` subset (expressions only: no statements, imports, or assignment).

## Import

```niao
import "nexpr"
```

Paths `import "std/nexpr"` and `import "nexpr"` are equivalent. Flat builtins (`nexpr_eval`, `nexpr_compile`, …) are also available globally after import.

## Quick start

```niao
import "nexpr"

// One-shot
print(nexpr.eval("2 + 3 * 4"))                    // 14
print(nexpr.eval("price * qty", { "price": 9.99, "qty": 3 }))

// Compile once, run many times
let formula = nexpr.compile("price * qty * (1 + tax) + ship")
let ev = nexpr.evaluator({ "tax": 0.08, "ship": 4.99 })
let i = 0
while i < rows.len() {
    nexpr.set(ev, "price", rows[i].price)
    nexpr.set(ev, "qty", rows[i].qty)
    print(nexpr.execute_compiled(ev, formula))
    i = i + 1
}
```

## Expression language

| Feature | Supported |
|---------|-----------|
| Arithmetic | `+ - * / // % **` |
| Compare | `== != < > <= >=` |
| Boolean | `and or not` (also `&& \|\| !`) |
| Ternary | `a if cond else b` |
| Membership | `needle in haystack` (string, array, object keys) |
| Literals | int, float, `"string"`, `'string'`, `true`/`false`, `nil` |
| Collections | `[1, 2]`, `{ "x": 1, y: 2 }` |
| Access | `obj.field`, `arr[i]`, `obj["key"]` |
| Calls | builtins + custom fns (`abs(x)`, `min(a,b)`, …) |
| Comments | `# line comment` |

**Blocked for safety:** assignment (`=`), imports, statements, lambdas, arbitrary code execution.

### Default builtins

`abs`, `round`, `min`, `max`, `len`, `sum`, `all`, `any`, `int`, `float`, `str`, `bool`, `pow`, `ord`, `chr`, `hex`, `oct`, `bin`

Use `nexpr.functions()` and `nexpr.operators()` to list them at runtime.

## One-shot API

| Method | Description |
|--------|-------------|
| `nexpr.eval(expr, vars?)` | Parse and evaluate; optional variable object. |
| `nexpr.valid(expr)` | `true` when `expr` parses. |
| `nexpr.compile(expr)` | Parse once; returns opaque `int` handle (or catchable parse `error`). |
| `nexpr.run(compiled, vars?)` | Execute compiled expression with optional variable overlay. |
| `nexpr.referenced(compiled)` | Variable names referenced by a compiled expression. |

## Evaluator handles

| Method | Description |
|--------|-------------|
| `nexpr.evaluator(vars?, fns?)` | Create reusable evaluator handle. |
| `nexpr.set(ev, name, value)` | Bind a variable. |
| `nexpr.set_fn(ev, name, fn)` | Register a Niao callable. |
| `nexpr.get(ev, name)` | Read a variable (catchable `error` if missing). |
| `nexpr.names(ev)` | List bound variable names. |
| `nexpr.clear(ev)` | Remove all variables. |
| `nexpr.clear_fns(ev)` | Remove custom functions. |
| `nexpr.execute(ev, expr)` | Parse + evaluate on the evaluator. |
| `nexpr.execute_compiled(ev, compiled)` | Run a compiled handle. |
| `nexpr.batch(ev, compiled, rows, threads?)` | Parallel per-row eval; `rows` is an array of variable objects. `threads` defaults to auto. |
| `nexpr.allow_op(ev, op, allowed?)` | Enable/disable an operator token (e.g. `"**"`). |
| `nexpr.allow_fn(ev, name, allowed?)` | Enable/disable a function by name. |
| `nexpr.free(ev)` | Release evaluator handle. |
| `nexpr.free_compiled(handle)` | Release compiled handle. |

## Errors

| Code | Meaning |
|------|---------|
| 4200 | Wrong argument count. |
| 4201 | Semantic error (undefined name, division by zero, disabled op, catchable). |
| 4202 | Type mismatch in arguments or expression values. |
| 4203 | Lex/parse error (catchable from `compile`). |
| 4204 | Invalid or released handle. |

## Notes

- Expression values are `nil`, `bool`, `int`, `float`, `string`, `array`, and `object` — not arbitrary Niao handles.
- Custom functions are Niao callables invoked through the runtime; keep them pure for predictable formulas.
- For high-volume row evaluation, prefer `compile` + `batch` over repeated `eval` strings.
