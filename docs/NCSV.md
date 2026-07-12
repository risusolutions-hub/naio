# ncsv — CSV Parse and Stringify

`ncsv` is a lightweight native library for reading and writing CSV text.
It handles quoted fields, escaped quotes, and optional header rows — without
pulling in a dataframe layer (see `ncl` for typed column inference).

Import with:

```niao
import "ncsv"
// or
import "std/ncsv"
```

---

## Options

All functions accept an optional `opts` object:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `header` | bool | `false` | When `true`, parse returns objects; stringify can emit a header row |
| `delimiter` | string | `","` | Field separator (single character) |
| `quote` | string | `"\"` | Quote character (single character) |
| `names` | string[] | — | Column names for header mode |

---

## API

### `ncsv.parse(text, opts?) -> rows`

Parse CSV text in memory.

- Default: array of string arrays — one inner array per row.
- With `header: true` and `names`: array of objects keyed by `names` (all rows are data).
- With `header: true` and no `names`: first row becomes column names; remaining rows are objects.

```niao
ncsv.parse("a,b\n1,2")
// [["a","b"], ["1","2"]]

ncsv.parse("name,age\nalice,30", {header: true})
// [{name: "alice", age: "30"}]

ncsv.parse("1,2\n3,4", {header: true, names: ["x", "y"]})
// [{x: "1", y: "2"}, {x: "3", y: "4"}]
```

Parse failures return a catchable `ncsv_error` value (code 2853).

---

### `ncsv.read(path, opts?) -> rows`

Read a file and parse it with the same rules as `parse`.
I/O failures return a catchable `ncsv_error` value (code 2851).

```niao
let rows = ncsv.read("data/users.csv", {header: true})
```

---

### `ncsv.stringify(rows, opts?) -> string`

Serialize rows to CSV text.

- Input rows may be arrays of values or objects.
- With `header: true`, a header row is written from `opts.names` or sorted object keys.
- Fields containing the delimiter, quote, or newline are quoted; quotes are doubled.

```niao
ncsv.stringify([["a", "b, c"], ["1", "2"]])
// "a,\"b, c\"\n1,2"

ncsv.stringify([{name: "alice", age: "30"}], {header: true})
// "age,name\n30,alice"   (keys sorted)
```

---

### `ncsv.write(path, rows, opts?) -> true`

Write CSV text to a file. Returns `true` on success.
I/O failures return a catchable `ncsv_error` value (code 2851).

```niao
ncsv.write("out.csv", rows, {header: true, names: ["id", "label"]})
```

---

## Error codes

| Code | Kind |
|------|------|
| 2850 | Wrong argument count |
| 2851 | I/O or general recoverable error |
| 2852 | Type mismatch |
| 2853 | Parse error (e.g. unclosed quote) |

Use `is_error(result)` and `error_message(result)` to handle recoverable failures.

---

## Notes

- All cell values are strings on parse; stringify converts numbers and other types with `to_string()`.
- Empty input parses to `[]`.
- Line endings `\n`, `\r`, and `\r\n` are accepted.
- Quoted fields may span multiple lines (RFC 4180-style).
