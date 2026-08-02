# nzip — ZIP archives

ZIP archive read/write, per-entry compression (stored, deflate, bzip2, LZMA, zstd), AES-256 encryption, streaming entry reads, parallel extract, and integrity checks (~Python `zipfile` subset).

Implemented in the in-house `niao_zip` crate (`zip` 2.x with deflate/bzip2/lzma/zstd/aes-crypto; `niao_parallel` for parallel `extract_all`).

## Import

```niao
import "nzip"
```

Paths `import "std/nzip"` and `import "nzip"` are equivalent. Flat builtins (`nzip_open`, `nzip_read`, …) are also available globally after import.

## Quick start

```niao
import "nzip"

// Create an archive
let w = nzip.create("bundle.zip", {compression: "deflated", level: 6})
nzip.write_bytes(w.handle, "hello.txt", "Hello, ZIP!")
nzip.mkdir(w.handle, "data")
nzip.close(w.handle)

// Read it back
let z = nzip.open("bundle.zip")
print(nzip.namelist(z.handle))          // ["hello.txt", "data/"]
let bytes = nzip.read(z.handle, "hello.txt")
print(bytes)                            // Hello, ZIP!
nzip.close(z.handle)

// Extract everything in parallel
let z2 = nzip.open("bundle.zip")
nzip.extract_all(z2.handle, "./out", {threads: 8})
nzip.close(z2.handle)
```

## Compression constants

| Constant | Value | Description |
|----------|-------|-------------|
| `nzip.STORED` | `"stored"` | No compression |
| `nzip.DEFLATED` | `"deflated"` | DEFLATE (default) |
| `nzip.BZIP2` | `"bzip2"` | Bzip2 |
| `nzip.LZMA` | `"lzma"` | LZMA |
| `nzip.ZSTD` | `"zstd"` | Zstandard |

## Open & create

| Method | Description |
|--------|-------------|
| `nzip.is_zipfile(path)` | `true` when `path` looks like a ZIP archive. |
| `nzip.is_zipfile_bytes(bytes)` | `true` when bytes begin with a valid ZIP header. |
| `nzip.open(path, opts?)` | Open for reading; returns `{handle, mode: "r"}`. |
| `nzip.create(path, opts?)` | Create/truncate for writing; returns `{handle, mode: "w"}`. |
| `nzip.append(path, opts?)` | Open for append; returns `{handle, mode: "a"}`. |
| `nzip.close(handle)` | Close handle; finalizes writers. |

### Open / create options

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `password` | string | — | Decryption password (read) or AES encryption password (write). |
| `compression` | string | `"deflated"` | Default compression for new entries (`create`/`append`). |
| `level` | int | `6` | Compression level for deflate/bzip2/lzma/zstd. |
| `comment` | string | — | Archive comment (writers). |
| `large_file` | bool | `true` | Enable ZIP64 for large entries. |

## Read operations

| Method | Description |
|--------|-------------|
| `nzip.namelist(handle)` | Entry names in archive order. |
| `nzip.infolist(handle)` | Array of entry metadata objects (see below). |
| `nzip.getinfo(handle, name)` | Metadata for one entry. |
| `nzip.read(handle, name)` | Decompress and return entry bytes. |
| `nzip.comment(handle)` | Archive comment string, or `nil`. |
| `nzip.set_password(handle, password?)` | Set/clear decryption password for subsequent reads. |
| `nzip.test(handle)` | Verify CRC/decompression of every entry; `true` or catchable `nzip_error`. |

### Entry metadata (`infolist` / `getinfo`)

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Archive path |
| `size` | int | Uncompressed size (bytes) |
| `compressed_size` | int | Compressed size |
| `compression` | string | `stored`, `deflated`, `bzip2`, `lzma`, `zstd`, … |
| `is_dir` | bool | Directory entry |
| `is_symlink` | bool | Symlink entry (when present) |
| `crc32` | int | CRC-32 checksum |
| `modified` | int / nil | Unix timestamp when available |
| `encrypted` | bool | Entry uses encryption |
| `comment` | string / nil | Per-entry comment |

## Streaming reads

| Method | Description |
|--------|-------------|
| `nzip.open_entry(handle, name)` | Open one entry for chunked reads. |
| `nzip.entry_read(handle, max?)` | Read up to `max` bytes (default 65536); returns `byte[]`. |
| `nzip.entry_close(handle)` | Close the active entry stream. |

Only one entry stream may be open per archive handle at a time.

## Extract

| Method | Description |
|--------|-------------|
| `nzip.extract(handle, name, dest?, opts?)` | Extract one entry under `dest` (default `"."`); returns output path. |
| `nzip.extract_all(handle, dest?, opts?)` | Extract all entries; returns array of output paths. |

### Extract options

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `password` | string | — | Decryption password |
| `threads` | int | CPU count | Parallel workers for `extract_all` |
| `overwrite` | bool | `false` | Replace existing files |

Path traversal (`..` segments) is rejected (zip-slip protection).

## Write operations

| Method | Description |
|--------|-------------|
| `nzip.write_file(handle, path, opts?)` | Add a file from disk; returns bytes written. |
| `nzip.write_bytes(handle, arcname, data, opts?)` | Add bytes under `arcname`. |
| `nzip.writestr(handle, arcname, text, opts?)` | Alias for `write_bytes`. |
| `nzip.mkdir(handle, arcname)` | Add a directory entry (`arcname/`). |
| `nzip.set_comment(handle, comment)` | Set archive comment before `close`. |

### Per-entry write options

| Field | Type | Description |
|-------|------|-------------|
| `arcname` / `name` | string | Path inside the archive |
| `compression` | string | Override default compression |
| `level` | int | Override compression level |
| `comment` | string | Per-entry comment |

## Errors

Catchable `nzip_error` values use codes `4390`–`4395`:

| Code | Meaning |
|------|---------|
| `4390` | Wrong argument count |
| `4391` | General ZIP / I/O error |
| `4392` | Type mismatch |
| `4393` | Invalid or closed handle |
| `4394` | Entry not found |
| `4395` | Password required or incorrect |

## Deferred / limitations

- Legacy **ZipCrypto** (weak pre-AES encryption) is not supported for writing; reading may fail on ZipCrypto-only archives. AES-256 (WinZip-style) is supported.
- **Multi-disk** and **spanned** archives are not supported.
- Entry streaming buffers decompressed data internally (chunked API, not O(1) memory streaming).
- Unix permission bits and extended timestamps are preserved when present in the archive but not fully exposed in metadata yet.

## See also

- [`archive`](ARCHIVE.md) — raw gzip/deflate helpers
- [`nfs`](NFS.md) — high-level filesystem copy/move
- [`nmmap`](NMMAP.md) — memory-mapped file reads
