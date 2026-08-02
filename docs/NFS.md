# nfs — high-level filesystem helpers

High-level file operations above `io` handles and `nos` basics: recursive copy/move, atomic writes, secure temp files/directories, disk usage, and send-to-trash (~`shutil` + `tempfile` + `send2trash` subset).

Implemented in the in-house `niao_nfs` crate (`std::fs`, `tempfile`, `trash`, `niao_parallel` for tree copies).

## Import

```niao
import "nfs"
```

Paths `import "std/nfs"` and `import "nfs"` are equivalent. Flat builtins (`nfs_copy`, `nfs_copytree`, …) are also available globally after import.

## Quick start

```niao
import "nfs"

// Atomic config write
nfs.write_atomic("app.toml", "key = 1")

// Secure temp file (deleted on close unless keep: true)
let f = nfs.tempfile({suffix: ".json"})
nfs.tempfile_write(f, "{\"ok\": true}")
print(nfs.tempfile_path(f))
nfs.tempfile_close(f)

// Copy a project tree in parallel
nfs.copytree("src_pkg", "dst_pkg", {dirs_exist_ok: true, threads: 8})

// Move to recycle bin / trash
nfs.trash("old_report.pdf")
```

## Copy & move

| Method | Description |
|--------|-------------|
| `nfs.copy(src, dst, opts?)` | Copy file; returns bytes copied or catchable `nfs_error`. |
| `nfs.copy2(src, dst)` | Copy file with metadata (mode, timestamps). |
| `nfs.copyfile(src, dst)` | Copy contents only. |
| `nfs.copymode(src, dst)` | Copy permission bits. |
| `nfs.copystat(src, dst)` | Copy stat metadata. |
| `nfs.copytree(src, dst, opts?)` | Recursive directory copy (parallel file copies). |
| `nfs.move(src, dst)` | Rename or cross-device move via copy+delete. |
| `nfs.rmtree(path, opts?)` | Remove directory tree. |
| `nfs.samefile(a, b)` | `true` when paths resolve to the same file. |
| `nfs.which(cmd)` | Locate executable on `PATH`; `nil` if missing. |
| `nfs.walk(root, opts?)` | Directory walk → `[{root, dirs, files}, …]`. |
| `nfs.commonprefix(paths)` | Longest shared path prefix. |

### `copy` / `copytree` options

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `metadata` | bool | `false` / `true` (tree) | Preserve mode and timestamps. |
| `follow_symlinks` | bool | `true` | Follow symlinks when copying a file. |
| `dirs_exist_ok` | bool | `false` | Allow existing destination dir (`copytree`). |
| `symlinks` | bool | `false` | Copy symlinks as symlinks (`copytree`). |
| `ignore` | string[] | `[]` | Glob patterns to skip (`copytree`, `rmtree`). |
| `threads` | int | CPU count | Parallel workers for `copytree` file copies. |

### `rmtree` options

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `ignore_errors` | bool | `false` | Continue on per-entry errors. |
| `ignore` | string[] | `[]` | Skip matching entry names. |

### `walk` options

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `topdown` | bool | `true` | Pre-order directory traversal. |
| `follow_symlinks` | bool | `false` | Descend into symlinked directories. |

## Disk usage

| Method | Description |
|--------|-------------|
| `nfs.disk_usage(path)` | Volume stats → `{total, used, free}` (bytes). |
| `nfs.tree_size(path, opts?)` | Sum of file sizes under `path`; `opts.threads` for parallel sum. |

## Temp files & directories

| Method | Description |
|--------|-------------|
| `nfs.temp_dir()` | System temp directory path. |
| `nfs.mkstemp(opts?)` | Secure temp file → `{handle, path}`. |
| `nfs.mktemp(opts?)` | **Insecure** name generation (race-prone; prefer `tempfile`). |
| `nfs.tempfile(opts?)` | Temp file handle (auto-deleted on `tempfile_close`). |
| `nfs.tempdir(opts?)` | Temp directory handle. |
| `nfs.tempfile_path(h)` | Path for handle. |
| `nfs.tempfile_write(h, data)` | Write bytes/string. |
| `nfs.tempfile_read(h, max?)` | Read up to `max` bytes (default 65536). |
| `nfs.tempfile_close(h, opts?)` | Close; `opts.keep: true` retains file. |
| `nfs.tempdir_path(h)` | Path for temp dir handle. |
| `nfs.tempdir_close(h, opts?)` | Remove dir unless `keep: true`. |

### Temp options

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `dir` | string | system temp | Parent directory. |
| `prefix` | string | `".tmp"` | Name prefix. |
| `suffix` | string | `""` | Name suffix (e.g. `".json"`). |

## Atomic write & trash

| Method | Description |
|--------|-------------|
| `nfs.write_atomic(path, text, opts?)` | Write via temp file + rename in target directory. |
| `nfs.write_bytes_atomic(path, data, opts?)` | Atomic binary write. |
| `nfs.trash(path)` | Move file/dir to OS recycle bin / Freedesktop trash. |
| `nfs.trash_all(paths)` | Trash multiple paths. |

### Atomic write options

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `dir` | string | parent of `path` | Directory for temp file (same volume recommended). |
| `fsync` | bool | `true` | `fsync` before rename. |
| `mode` | int | — | Unix file mode after write. |

## Errors

| Code | Meaning |
|------|---------|
| 3530 | Wrong argument count. |
| 3531 | I/O or trash failure (catchable `nfs_error`). |
| 3532 | Wrong argument type (hard error). |
| 3533 | Invalid or closed temp handle. |

## See also

- `io` — streaming handles, whole-file read/write, async I/O.
- `nos` — lightweight `stat`, `mkdir`, `rename`, `exists`.
- `nmmap` — zero-copy mmap reads for large files.
- `nwatch` — mtime polling watchers.
