# nsoa — Struct-of-Arrays Tables

`nsoa` stores columnar data with typed columns (`int`, `float`, `bool`, `string`). Each column is a packed native array; rows are appended as objects keyed by column name.

Import with:

```niao
import "nsoa"
// or
import "std/nsoa"
```

---

## Quick start

```niao
import "nsoa"

let t = nsoa.new({id: "int", score: "float", active: "bool", name: "string"})
nsoa.push(t, {id: 1, score: 9.5, active: true, name: "alice"})
nsoa.push(t, {id: 2, score: 8.0, active: false, name: "bob"})

print(nsoa.column(t, "id"))     // int_array
print(nsoa.get(t, 0, "name"))   // "alice"
print(nsoa.get(t, 1))            // full row object
print(nsoa.stats(t))
nsoa.close(t)
```

---

## Functions

| Method | Description |
|--------|-------------|
| `nsoa.new(schema)` | Create a table from `{col: "int"|"float"|"bool"|"string", ...}`. |
| `nsoa.close(handle)` | Free the table. |
| `nsoa.len(handle)` | Row count. |
| `nsoa.push(handle, row)` | Append a row object with all schema columns. |
| `nsoa.column(handle, name)` | Packed array for a column. |
| `nsoa.get(handle, row, col?)` | Cell value, or full row object when `col` omitted. |
| `nsoa.names(handle)` | Sorted column names. |
| `nsoa.stats(handle)` | `{rows, columns: [{name, type, len}, ...]}`. |

---

## Errors

| Code | Meaning |
|------|---------|
| 3380 | Wrong argument count. |
| 3381 | Schema/row/column error — catchable `nsoa_error`. |
| 3382 | Wrong argument type. |
| 3383 | Invalid or closed handle — catchable `nsoa_error`. |
