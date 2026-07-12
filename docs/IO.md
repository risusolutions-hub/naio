# io standard library

File and filesystem I/O: read/write whole files, streaming file handles, path manipulation, directory
listing, filesystem metadata, and async background file tasks. Backed by the native `niao_io` crate.

## Import

```niao
import "io"
```

`import "std/io"` and `import "io"` are equivalent. Flat builtins (`io_read_file`, …) are also
available globally after import.

## Quick start

```niao
import "io"

io.write_file("out.txt", "hello\nworld\n")
let text = io.read_file("out.txt")          // whole file as string
let bytes = io.read_bytes("out.txt")        // whole file as byte array

for line in io.read_lines("out.txt") {      // line iterator
    print(line)
}

let f = io.open("big.log", "r")             // streaming handle
while !io.eof(f) { print(io.read_line(f)) }
io.close(f)
```

## Whole-file operations

| Method | Description |
|--------|-------------|
| `io.read_file(path)` | Entire file as a string. |
| `io.read_bytes(path)` | Entire file as a byte array. |
| `io.read_all(handle)` | Read the rest of an open handle. |
| `io.write_file(path, text)` | Write/replace a file with text. |
| `io.write_bytes(path, bytes)` | Write/replace a file with bytes. |
| `io.append_file(path, text)` | Append text to a file. |

## Streaming handles

| Method | Description |
|--------|-------------|
| `io.open(path, mode)` | Open a handle. Modes: `r`, `rb`, `w`, `wb`, `a`, `ab`. |
| `io.read(handle, n)` / `io.read_bytes(handle, n)` | Read up to `n` chars/bytes. |
| `io.read_line(handle)` / `io.read_lines(path)` | One line / all lines. |
| `io.write(handle, text)` / `io.write_bytes(handle, bytes)` | Write to handle. |
| `io.seek(handle, pos)` / `io.tell(handle)` | Move / report cursor. |
| `io.flush(handle)` · `io.eof(handle)` · `io.close(handle)` | Flush, test end, close. |

## Paths

`io.join(a, b)`, `io.join_many([...])`, `io.dirname`, `io.basename`, `io.stem`, `io.extension`,
`io.is_absolute`, `io.canonical`, `io.cwd`, `io.chdir`, `io.home_dir`, `io.temp_dir`.

## Filesystem

`io.exists`, `io.is_file`, `io.is_dir`, `io.is_symlink`, `io.file_size`, `io.created_ms`,
`io.modified_ms`, `io.list_dir`, `io.list_dir_recursive`, `io.create_dir`, `io.create_dir_all`,
`io.remove_file`, `io.remove_dir`, `io.remove_dir_all`, `io.rename`, `io.copy`.

## Async file tasks

| Method | Description |
|--------|-------------|
| `io.async_read(path)` / `io.async_read_bytes(path)` | Start a background read; returns a task handle. |
| `io.async_write(path, text)` / `io.async_write_bytes(path, bytes)` | Background write. |
| `io.async_copy(src, dst)` | Background copy. |
| `io.task_poll(task)` · `io.task_wait(task)` · `io.task_done(task)` · `io.task_cancel(task)` | Manage tasks. |

## v0.2.4 notes

- Buffered readers/writers with configurable capacity (default 64 KiB) and a reused line-read scratch buffer.
- Atomic write helper (`write_temp` + rename) and temp-file RAII handles.

> **Status:** drafted from the runtime registration in `crates/niao_runtime/src/io.rs` (55 builtins).
> Verify signatures/return shapes against source before publishing.
