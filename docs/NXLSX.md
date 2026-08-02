# nxlsx — Excel .xlsx read/write

Excel `.xlsx` read/write with sheets, cell styles, formulas, streaming rows, and `nframe` interop. ~openpyxl / xlsxwriter subset.

Backed by [calamine](https://github.com/tafia/calamine) (read) and [rust_xlsxwriter](https://github.com/jmcnamara/rust_xlsxwriter) (write).

## Import

```niao
import "nxlsx"
```

Paths `import "std/nxlsx"` and `import "nxlsx"` are equivalent.

## Quick start

```niao
import "nxlsx"

let table = {
    id: [1, 2, 3],
    name: ["alice", "bob", "carol"],
    score: [0.1, 0.2, 0.3]
}

// One-shot file I/O
nxlsx.write("out.xlsx", {Data: table})
let book = nxlsx.read("out.xlsx")
let loaded = book["Data"]

// Workbook handle API
let wb = nxlsx.open("out.xlsx")
let names = nxlsx.sheet_names(wb)
let sheet_tbl = nxlsx.read_sheet(wb, names[0])
nxlsx.set_cell(wb, names[0], 5, 1, "footer", {bold: true})
nxlsx.save(wb, "out2.xlsx")
nxlsx.close(wb)

// Streaming write (constant-memory, large exports)
let s = nxlsx.stream_open("big.xlsx", "Rows", ["id", "label"])
nxlsx.stream_row(s, [1, "a"])
nxlsx.stream_row(s, [2, "b"])
nxlsx.stream_close(s)
```

## Table format

A table is a Niao object mapping column names to typed arrays of equal length (same as `nparquet`):

| Column type | Niao array |
|-------------|------------|
| `int` | `int[]` |
| `float` | `float[]` |
| `bool` | `bool[]` |
| `string` | `string[]` |

Nullable columns include a companion `{column}__valid` `bool[]` mask (`1` = present).

Row and column indices are **1-based** (Excel-style): row `1`, column `1` is cell `A1`.

## Workbook functions

| Method | Description |
|--------|-------------|
| `nxlsx.create(opts?)` | New empty workbook; returns handle. |
| `nxlsx.open(path, opts?)` | Open `.xlsx` for reading/editing; returns handle. |
| `nxlsx.close(handle)` | Release workbook handle. |
| `nxlsx.save(handle, path?)` | Write workbook to disk; returns `true`. |
| `nxlsx.to_bytes(handle, opts?)` | Encode workbook to `byte[]`. |
| `nxlsx.sheet_names(handle)` | Sheet name array. |
| `nxlsx.active_sheet(handle)` | Active sheet name. |
| `nxlsx.set_active(handle, name\|index)` | Set active sheet. |
| `nxlsx.add_sheet(handle, name)` | Add worksheet. |
| `nxlsx.remove_sheet(handle, name)` | Remove worksheet. |
| `nxlsx.rename_sheet(handle, old, new)` | Rename worksheet. |

## Read / write

| Method | Description |
|--------|-------------|
| `nxlsx.read(path, opts?)` | Read all sheets → `{sheet_name: table}`. |
| `nxlsx.read_sheet(handle, sheet, opts?)` | Read one sheet to table object. |
| `nxlsx.read_rows(handle, sheet)` | Raw row array-of-arrays (no header inference). |
| `nxlsx.read_chunk(path, sheet?, opts?)` | Streaming chunk read without full load. |
| `nxlsx.write(path, {sheet: table}, opts?)` | Write multi-sheet workbook. |
| `nxlsx.write_sheet(handle, sheet, table, opts?)` | Write/replace one sheet in open workbook. |
| `nxlsx.load(path, opts?)` | Alias for `read`. |
| `nxlsx.load_workbook(path, opts?)` | Alias for `open`. |

### Read options

| Key | Default | Description |
|-----|---------|-------------|
| `header` | `true` | First row is column names for table reads. |
| `start_row` | `1` | 1-based first row to read. |
| `rows` | unlimited | Max rows to materialize. |
| `sheet` | first | Sheet name string. |
| `sheet_index` | — | 1-based sheet index (overrides `sheet`). |
| `infer_types` | `true` | Infer int/float/bool columns on read. |
| `skip_empty` | `false` | Skip all-empty rows in raw reads. |

### Write options

| Key | Default | Description |
|-----|---------|-------------|
| `header` | `true` | Write column names as first row. |
| `constant_memory` | `false` | Use streaming writer (large files). |
| `autofit` | `false` | Set default column widths on write. |
| `freeze_row` | — | Freeze panes row (1-based). |
| `freeze_col` | — | Freeze panes column (1-based). |

## Cells, styles, formulas

| Method | Description |
|--------|-------------|
| `nxlsx.cell(handle, sheet, row, col)` | Read cell value (1-based). |
| `nxlsx.set_cell(handle, sheet, row, col, value, style?)` | Write cell; optional style object. |
| `nxlsx.formula(handle, sheet, row, col, formula)` | Write formula string (no leading `=`). |
| `nxlsx.style(handle, sheet, range, style)` | Apply style to range (`"A1:C10"`). |
| `nxlsx.merge(handle, sheet, range)` | Merge cell range. |
| `nxlsx.freeze(handle, sheet, row, col?)` | Freeze panes. |
| `nxlsx.set_width(handle, sheet, col, width)` | Column width in character units. |
| `nxlsx.rows(handle, sheet)` | Row count. |
| `nxlsx.cols(handle, sheet)` | Column count. |

### Style object keys

| Key | Type | Description |
|-----|------|-------------|
| `bold` | `bool` | Bold font. |
| `italic` | `bool` | Italic font. |
| `underline` | `bool` | Underline. |
| `font_size` | `float` | Font size in points. |
| `font_color` | `string` | Named color or `#RRGGBB`. |
| `bg_color` | `string` | Background color. |
| `number_format` | `string` | Excel number format string. |
| `align` | `string` | `left`, `center`, `right`, `justify`. |
| `valign` | `string` | `top`, `center`, `bottom`. |
| `wrap` | `bool` | Text wrap. |
| `border` | `string` | `thin`, `medium`, `thick`, etc. |

## Streaming

| Method | Description |
|--------|-------------|
| `nxlsx.stream_open(path, sheet, headers?, opts?)` | Begin constant-memory write; returns stream handle. |
| `nxlsx.stream_row(handle, values)` | Append one row (array of cell values). |
| `nxlsx.stream_close(handle)` | Finalize and close stream file. |

Chunk read options: `start_row` (default `1`), `count` (default `1000`).

## nframe interop

| Method | Description |
|--------|-------------|
| `nxlsx.to_nframe(handle, sheet, opts?)` | Sheet → `nframe` handle. |
| `nxlsx.from_nframe(handle, sheet, frame_id)` | `nframe` handle → sheet. |
| `nxlsx.table_rows(table)` | Row count from table object. |
| `nxlsx.table_columns(table)` | Column name array. |

## Utilities

| Method | Description |
|--------|-------------|
| `nxlsx.info(path)` | File metadata: sheets, dimensions, size. |
| `nxlsx.validate(path\|bytes)` | `true` for valid ZIP/xlsx payloads. |
| `nxlsx.column_letter(col)` | 1-based index → `"A"`, `"AB"`, … |
| `nxlsx.column_index(letters)` | `"AB"` → column index. |

## Supported formats

**Read:** `.xlsx`, `.xlsm`, `.xlsb`, `.xls` (via calamine).

**Write:** `.xlsx` only (new workbooks; existing files are read and rewritten).

## Limitations (v0.1.0)

- **Styles on read** are not preserved (calamine reads values only); styles apply on write.
- **Charts, images, macros, conditional formatting, data validation** are not supported.
- **In-place modification** of existing files rewrites the workbook (read → edit → save).
- **Legacy `.xls` write** is not supported.

## Size limits

In-memory workbooks are capped at **5M populated cells** and **256 MiB** encoded payloads.

## Errors

| Code | Meaning |
|------|---------|
| 4410 | Wrong argument count. |
| 4411 | I/O or workbook error (catchable `nxlsx_error`). |
| 4412 | Wrong argument type (hard error). |
| 4413 | Invalid or corrupt xlsx data (catchable). |
| 4414 | Invalid workbook/stream handle (catchable). |

## See also

- [nparquet](NPARQUET.md) — columnar Parquet interchange
- [ncsv](NCSV.md) — lightweight CSV
- [nframe](NFRAME.md) — dataframe handles
