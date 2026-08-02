# nparquet — Parquet + Arrow IPC

Parquet and Arrow IPC (streaming) read/write with `nframe` interop for high-performance columnar data interchange. ~pyarrow subset (beside `ncolumnar`'s NCOL1 wire format).

Backed by [Apache Arrow / Parquet](https://arrow.apache.org/) via **arrow-rs**.

## Import

```niao
import "nparquet"
```

Paths `import "std/nparquet"` and `import "nparquet"` are equivalent.

## Quick start

```niao
import "nparquet"

let table = {
    id: [1, 2, 3],
    score: [0.1, 0.2, 0.3],
    name: ["alice", "bob", "carol"]
}

// In-memory Parquet (Snappy by default)
let wire = nparquet.encode(table)
let back = nparquet.decode(wire)

// File I/O
nparquet.write_file("out.parquet", table)
let loaded = nparquet.read_file("out.parquet")

// Arrow IPC (Feather-style streaming)
let ipc = nparquet.encode_ipc(table)
let from_ipc = nparquet.decode_ipc(ipc)

// nframe interop
import "nframe"
let csv_id = nframe.read_csv("data.csv")
let tbl = nparquet.from_nframe(csv_id)
nparquet.save("data.parquet", tbl)
```

## Table format

A table is a Niao object mapping column names to typed arrays of equal length:

| Column type | Niao array |
|-------------|------------|
| `int` | `int[]` |
| `float` | `float[]` |
| `bool` | `bool[]` |
| `string` | `string[]` |
| `date` | `int[]` (days since Unix epoch) |

Nullable columns include a companion `{column}__valid` `bool[]` mask (`true` = present).

## Functions

| Method | Description |
|--------|-------------|
| `nparquet.encode(table, opts?)` | Encode table to Parquet `byte[]`. |
| `nparquet.decode(bytes, opts?)` | Decode Parquet `byte[]` to table object. |
| `nparquet.read_file(path, opts?)` | Read Parquet file from disk. |
| `nparquet.write_file(path, table, opts?)` | Write table to Parquet file; returns `true`. |
| `nparquet.encode_ipc(table)` | Encode table to Arrow IPC stream `byte[]`. |
| `nparquet.decode_ipc(bytes, opts?)` | Decode Arrow IPC stream to table. |
| `nparquet.read_ipc_file(path, opts?)` | Read Arrow IPC file. |
| `nparquet.write_ipc_file(path, table)` | Write Arrow IPC file; returns `true`. |
| `nparquet.schema(source)` | Schema from path or `byte[]` without full decode. |
| `nparquet.info(source)` | File metadata: rows, columns, types, row groups, sizes. |
| `nparquet.validate(bytes)` | `true` for valid Parquet payloads. |
| `nparquet.validate_ipc(bytes)` | `true` for valid Arrow IPC streams. |
| `nparquet.rows(table)` | Row count. |
| `nparquet.columns(table)` | Column name array. |
| `nparquet.to_nframe(table)` | Convert table to `nframe` handle (`int`). |
| `nparquet.from_nframe(handle)` | Convert `nframe` handle to table object. |
| `nparquet.load(path, opts?)` | Alias for `read_file` (pyarrow compat). |
| `nparquet.save(path, table, opts?)` | Alias for `write_file` (pyarrow compat). |

### Read options

| Key | Default | Description |
|-----|---------|-------------|
| `columns` | all | `string[]` column projection (read subset). |
| `rows` | unlimited | Max rows to materialize. |

### Write options

| Key | Default | Description |
|-----|---------|-------------|
| `compression` | `"snappy"` | `"snappy"`, `"gzip"`, `"zstd"`, `"lz4"`, `"brotli"`, `"none"`. |
| `row_group_size` | `1048576` | Target rows per Parquet row group. |

## Supported Arrow types

On read, these Parquet/Arrow physical types map to Niao columns:

- `Int8/16/32/64` → `int[]`
- `Float32/64` → `float[]`
- `Boolean` → `bool[]`
- `Utf8` / `LargeUtf8` → `string[]`
- `Date32` → `int[]` (date)
- `Timestamp` → `int[]` (epoch milliseconds)

Nested types (struct, list, map), decimals, and binary are **not** supported in v0.1.0.

## Size limits

In-memory encode/decode is capped at **256 MiB** per operation.

## Errors

| Code | Meaning |
|------|---------|
| 4400 | Wrong argument count. |
| 4401 | I/O or table shape error (catchable `nparquet_error`). |
| 4402 | Wrong argument type (hard error). |
| 4403 | Invalid or corrupt Parquet/IPC data (catchable). |

## vs ncolumnar

| | `ncolumnar` | `nparquet` |
|---|-------------|------------|
| Format | Custom `NCOL1` | Industry-standard Parquet / Arrow IPC |
| Interop | Niao-only | pandas, Polars, Spark, DuckDB, pyarrow |
| Compression | none | Snappy, Gzip, Zstd, … |
| Schema evolution | fixed | Parquet schema metadata |

Use `ncolumnar` for fast in-process Niao↔Niao exchange; use `nparquet` for cross-language pipelines and analytics tooling.
