# ncolumnar standard library

Column-major binary codec for tables. Wire format magic `NCOL1`.

A table is a Niao object mapping column names to typed arrays of equal length: `int[]`, `float[]`, `bool[]`, `string[]`, or an all-`nil` array for sparse columns.

## Import

```niao
import "ncolumnar"
```

Paths `import "std/ncolumnar"` and `import "ncolumnar"` are equivalent.

## Quick start

```niao
import "ncolumnar"

let table = {
    id: [1, 2, 3],
    score: [0.1, 0.2, 0.3],
    name: ["alice", "bob", "carol"]
}

let wire = ncolumnar.encode(table)              // byte[]
let back = ncolumnar.decode(wire)               // round-trip object
print(ncolumnar.rows(table))                    // 3
print(ncolumnar.info(wire))                     // {magic, version, rows, cols, columns, types}
print(ncolumnar.validate(wire))                 // true
```

## Functions

| Method | Description |
|--------|-------------|
| `ncolumnar.encode(table)` | Encode object table to `byte[]` (`NCOL1`). |
| `ncolumnar.decode(bytes)` | Decode `byte[]` back to column object. |
| `ncolumnar.validate(bytes)` | `true` for valid `NCOL1` payloads. |
| `ncolumnar.info(bytes)` | `{magic, version, rows, cols, columns, types}` without full materialization. |
| `ncolumnar.rows(table)` | Row count (length of first sorted column). |

## Column types

| Type tag | Niao column | Encoding |
|----------|-------------|----------|
| `int` | `int[]` | `i64` per row, little-endian |
| `float` | `float[]` | `f64` per row |
| `bool` | `bool[]` | `u8` 0/1 per row |
| `string` | `string[]` | offset table + UTF-8 blob |
| `nil` | `[nil, nil, …]` | row count only |

All columns must have the same row count. Column names are sorted lexicographically in the wire format.

## Wire format

```
NCOL1 | version:1 | rows:u32 | cols:u16 | (name_len, name, column_data)*
```

Each column stores its type tag followed by column-major packed data.

## Errors

| Code | Meaning |
|------|---------|
| 3430 | Wrong argument count. |
| 3431 | Table shape/type error (catchable). |
| 3432 | Type mismatch (hard error). |
| 3433 | Invalid or corrupt `NCOL1` data (catchable). |
