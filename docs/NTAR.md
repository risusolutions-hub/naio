# ntar — tar archives read/write

POSIX tar archive read/write with `.tar`, `.tar.gz`, and `.tar.zst` support (~Python `tarfile` subset).

Implemented in the in-house `niao_tar` crate (`tar`, `flate2`, `zstd`, `walkdir`).

## Import

```niao
import "ntar"
```

Paths `import "std/ntar"` and `import "ntar"` are equivalent. Flat builtins (`ntar_open`, `ntar_unpack`, …) are also available globally after import.

## Quick start

```niao
import "ntar"
import "nfs"

// Pack a directory tree
ntar.pack_tree("src/", "release.tar.gz", {arcname: "pkg", level: 6})

// Open and inspect
let arc = ntar.open("release.tar.gz")
print(ntar.names(arc.handle))
let info = ntar.get(arc.handle, "pkg/main.niao")
print(info.size)

// Extract one member or everything
ntar.extract(arc.handle, "pkg/main.niao", "out/")
ntar.extract_all(arc.handle, "out/")
ntar.close(arc.handle)

// One-shot unpack
ntar.unpack("release.tar.gz", "dest/")
```

## Open & close

| Method | Description |
|--------|-------------|
| `ntar.open(path, opts?)` | Open archive for read/write/append. Returns `{handle, mode, compression, path}`. |
| `ntar.close(handle)` | Finalize a write handle (flushes tar + compression) or release a reader. |

### Open options

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `mode` | string | `"r"` | Python-style mode: `r`, `r:gz`, `r:zst`, `w`, `w:gz`, `w:zst`, `a` (uncompressed append only). |
| `compression` | string | auto from path | Override: `none`, `gz`, `zst`. |
| `level` | int | `6` | Compression level for write (`gzip` 0–9, `zstd` 1–22). |

## Read API

| Method | Description |
|--------|-------------|
| `ntar.names(handle)` | Member path names as a string array. |
| `ntar.members(handle)` | Full metadata objects for every member. |
| `ntar.get(handle, name)` | Metadata for one member (`name`, `size`, `mode`, `mtime`, `type`, …). |
| `ntar.contains(handle, name)` | `true` when a member exists. |
| `ntar.read(handle, name, max?)` | Raw member bytes (default max 512 MiB). |
| `ntar.next(handle)` | Iterator-style next member metadata; `nil` at end. |
| `ntar.rewind(handle)` | Reset iterator to first member. |
| `ntar.extract(handle, member, dest, opts?)` | Extract one member under `dest`. |
| `ntar.extract_all(handle, dest, opts?)` | Extract all (or filtered) members; returns extracted names. |

### Member metadata object

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Archive path (POSIX `/` separators). |
| `size` | int | Uncompressed size in bytes. |
| `mode` | int | Permission bits. |
| `mtime` | int | Unix modification time (seconds). |
| `uid` / `gid` | int | Owner ids when present in header. |
| `type` | string | `file`, `dir`, `symlink`, `link`, `fifo`, `chr`, `blk`, or `unknown`. |
| `link_target` | string? | Symlink/hard-link target when applicable. |
| `index` | int | Zero-based member index. |

### Extract options

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `members` | string[] | all | Extract only these member paths. |
| `numeric_owner` | bool | `false` | Reserved for future owner restoration. |
| `max_entry_bytes` | int | 512 MiB | Per-member size cap while extracting. |
| `threads` | int | `1` | Reserved for parallel extract tuning. |

## Write API

| Method | Description |
|--------|-------------|
| `ntar.add(handle, path, opts?)` | Add file or directory from disk. |
| `ntar.add_bytes(handle, arcname, data, mode?)` | Add in-memory file contents. |
| `ntar.add_dir(handle, path, opts?)` | Add directory entry. |
| `ntar.add_tree(handle, root, opts?)` | Recursively add a directory tree. |

### Add options

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `arcname` | string | basename / tree prefix | Path inside the archive. |
| `mode` | int | `0o644` / `0o755` | Permission bits. |
| `mtime` | int | source mtime | Unix seconds override. |
| `recursive` | bool | `false` | For `add()`: recurse into directories. |

## Convenience functions

| Method | Description |
|--------|-------------|
| `ntar.unpack(archive, dest, opts?)` | Open, extract all, close in one call. |
| `ntar.pack_tree(src, archive, opts?)` | Create archive from directory tree. |
| `ntar.create(paths, archive, opts?)` | Create archive from path list (files or dirs). |
| `ntar.is_tar(path)` | `true` when path looks like a tar archive. |
| `ntar.detect(path)` | Compression from extension: `none`, `gz`, or `zst`. |

## Errors

| Code | Meaning |
|------|---------|
| 4364 | Wrong argument count. |
| 4365 | Archive I/O or format failure (catchable `ntar_error`). |
| 4366 | Wrong argument type (hard error). |
| 4367 | Invalid or closed handle. |
| 4368 | Member not found. |

Unsafe paths (`..`, absolute paths) are rejected on extract.

## See also

- [`archive`](ARCHIVE.md) — low-level gzip/deflate helpers
- [`nfs`](NFS.md) — high-level filesystem copy/move
- [`nzip`](NZIP.md) — ZIP archives
