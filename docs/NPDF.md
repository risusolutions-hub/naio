# npdf standard library

PDF create (text, images, tables), extract text and pages, merge/split. Native Rust implementation (~reportlab + pypdf subset).

## Import

```niao
import "npdf"
```

Paths `import "std/npdf"` and `import "npdf"` are equivalent. Flat builtins (`npdf_open`, `npdf_merge`, …) are also available globally after import.

## Quick start

```niao
import "npdf"

// Create a one-page PDF
let b = npdf.create({title: "Invoice"})
npdf.text(b, "Hello PDF", {x: 72, y: 720, size: 18})
let bytes = npdf.finish(b)

// Open, inspect, extract
let doc = npdf.open(bytes)
print(npdf.page_count(doc))           // 1
print(npdf.extract_text(doc))         // contains "Hello PDF"
npdf.close(doc)

// Merge two PDFs
let merged = npdf.merge([bytes, bytes])
```

## Open & handles

| Method | Description |
|--------|-------------|
| `npdf.open(source, opts?)` | Open from `byte[]` or path string → document handle. |
| `npdf.close(doc)` | Release a document handle. |
| `npdf.valid(bytes)` | True when bytes parse as PDF. |
| `npdf.page_count(doc)` | Number of pages. |
| `npdf.page_size(doc, page?)` | `{width, height}` in points; `page` is 0-based (default `0`). |
| `npdf.metadata(doc)` | Info dictionary: `title`, `author`, `subject`, … |
| `npdf.save(doc)` | Serialize → `byte[]`. |
| `npdf.write(doc, path)` | Write to filesystem. |
| `npdf.rotate(doc, page, degrees)` | Rotate page (90/180/270). |
| `npdf.remove_pages(doc, pages)` | Delete 0-based page indices. |
| `npdf.copy_pages(doc, pages)` | New handle with selected pages. |

Document handles are positive integers.

## Text extraction

| Method | Description |
|--------|-------------|
| `npdf.extract_text(source, opts?)` | From handle or `byte[]`. `opts`: `{pages: [0,2], page_separator}`. |
| `npdf.extract_page_text(doc, page)` | Single page text (0-based). |
| `npdf.pages_text(doc)` | String array, one entry per page. |
| `npdf.extract_pages(doc, pages)` | Subset as `byte[]`. |
| `npdf.page_bytes(doc, page)` | One page as `byte[]`. |

## Merge & split

| Method | Description |
|--------|-------------|
| `npdf.merge(byte_arrays)` | Concatenate PDFs → `byte[]`. |
| `npdf.merge_docs(handles)` | Merge open documents. |
| `npdf.split(doc, ranges)` | `[[start, end], …]` inclusive 0-based ranges → `byte[][]`. |
| `npdf.split_all(doc)` | One `byte[]` per page. |

## Create (builder)

| Method | Description |
|--------|-------------|
| `npdf.create(opts?)` | New builder. `opts`: `{page_width, page_height, margin, title}`. |
| `npdf.close_builder(b)` | Discard builder. |
| `npdf.add_page(b, opts?)` | Append a page. |
| `npdf.text(b, text, opts?)` | Draw text. `opts`: `{x, y, size, font, color}`. |
| `npdf.image(b, data, opts?)` | Embed PNG/JPEG/GIF/TIFF. `opts`: `{x, y, width, height, scale}`. |
| `npdf.table(b, rows, opts?)` | 2D string array. `opts`: `{x, y, col_widths, header, border, …}`. |
| `npdf.line(b, x1, y1, x2, y2, opts?)` | Line segment. |
| `npdf.rect(b, x, y, w, h, opts?)` | Rectangle; `opts`: `{fill, stroke, stroke_width}`. |
| `npdf.finish(b)` | Finalize → `byte[]`. |
| `npdf.write_new(b, path)` | Write to file. |

Coordinates are PDF points (1/72 inch) from the bottom-left corner. Default page is US Letter (612×792 pt).

### Fonts (`opts.font`)

`helvetica` (default), `helvetica-bold`, `times`, `times-bold`, `courier`, …

## Parallel batch

| Method | Description |
|--------|-------------|
| `npdf.parallel_extract(byte_arrays, opts?)` | Parallel text extraction. `opts.threads` defaults to CPU count. |
| `npdf.parallel_merge(groups, opts?)` | Merge each inner `byte[][]` in parallel. |

## Errors

| Code | Meaning |
|------|---------|
| 3559 | Wrong argument count. |
| 3560 | General PDF error (parse, IO, build). |
| 3561 | Type mismatch. |
| 3562 | Invalid or closed handle. |

## Scope notes

Subset of reportlab/pypdf: no forms, annotations, encryption, or vector drawing beyond lines/rects/tables. Text extraction uses content-stream parsing (works for most text PDFs; scanned/image-only pages return empty text).
