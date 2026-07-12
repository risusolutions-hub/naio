# nmmap — Memory-Mapped File I/O

`nmmap` maps files into memory with `memmap2` for zero-copy reads, builds a lazy line index on demand, and supports byte-pattern search over the mapped region.

Import with:

```niao
import "nmmap"
// or
import "std/nmmap"
```

---

## Quick start

```niao
import "nmmap"

let h = nmmap.open("data/large.log")
print(nmmap.len(h))
print(nmmap.line_count(h))       // builds line index on first call
print(nmmap.line(h, 0))
print(nmmap.find(h, "ERROR"))
nmmap.close(h)
```

---

## Functions

| Method | Description |
|--------|-------------|
| `nmmap.open(path)` | Memory-map a file read-only. Returns an integer handle. |
| `nmmap.close(handle)` | Unmap and free the handle. Returns `true` if it existed. |
| `nmmap.len(handle)` | Mapped byte length. |
| `nmmap.bytes(handle, start, end?)` | Copy a byte slice as `byte_array`. |
| `nmmap.text(handle, start, end?)` | Read a UTF-8 slice as string. |
| `nmmap.line_count(handle)` | Count lines (lazy index; first call scans the file). |
| `nmmap.line(handle, index)` | Line text without trailing newline. |
| `nmmap.find(handle, needle, start?)` | First byte offset of `needle` (string or `byte_array`), or `-1`. |
| `nmmap.stats(handle)` | `{path, len, lines_indexed, line_count}`. |

Line indexing treats `\n`, `\r`, and `\r\n` as line breaks.

---

## Errors

| Code | Meaning |
|------|---------|
| 3360 | Wrong argument count. |
| 3361 | I/O, mmap, UTF-8, or range error — catchable `nmmap_error`. |
| 3362 | Wrong argument type. |
| 3363 | Invalid or closed handle — catchable `nmmap_error`. |
