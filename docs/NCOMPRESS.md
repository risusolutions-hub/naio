# ncompress — modern compression (zstd, lz4, brotli, xz)

Block and streaming compression for zstd, LZ4 frame, Brotli, and XZ/LZMA. Complements `archive` (gzip/deflate). ~zstandard, lz4, brotli subset.

## Import

```niao
import "ncompress"
```

Paths `import "std/ncompress"` and `import "ncompress"` are equivalent. Flat builtins (`ncompress_compress`, `ncompress_decompress`, …) are also available globally after import.

## Quick start

```niao
import "ncompress"

let raw = byte_array[72, 101, 108, 108, 111]
let z = ncompress.compress(raw, "zstd", {level: 3})
let back = ncompress.decompress(z, "zstd")
print(back)

// Codec shortcuts
let fast = ncompress.lz4_compress(raw)
let small = ncompress.zstd_compress(raw, {level: 19})

// Auto-detect codec from frame magic
let codec = ncompress.detect(z)   // "zstd"
let auto = ncompress.decompress_auto(z)

// Streaming
let h = ncompress.stream_open("compress", "zstd", {level: 1})
ncompress.stream_write(h, raw)
let frame = ncompress.stream_finish(h)
```

## Codecs

| Name | Aliases | Default level | Level range |
|------|---------|---------------|-------------|
| `zstd` | `zstandard`, `zst` | 3 | 1–22 |
| `lz4` | `lz4frame` | 0 | 0–12 |
| `brotli` | `br` | 6 | 0–11 |
| `xz` | `lzma` | 6 | 0–9 |

Constants: `ncompress.codecs.ZSTD`, `.LZ4`, `.BROTLI`, `.XZ`.

For gzip/deflate, use `import "archive"`.

## Block API

| Method | Description |
|--------|-------------|
| `ncompress.compress(data, codec, opts?)` | Compress a byte buffer to `byte[]`. |
| `ncompress.decompress(data, codec, opts?)` | Decompress a frame to `byte[]`. |
| `ncompress.decompress_auto(data, hint?, opts?)` | Decompress with auto-detected codec; optional `hint` string. |
| `ncompress.detect(data)` | Return codec name or `nil` from frame magic bytes. |
| `ncompress.frame_info(data, codec?)` | Return `{codec, content_size, compressed_size, has_checksum}`. |
| `ncompress.is_valid(data, codec)` | `true` when `data` decodes successfully. |
| `ncompress.compress_file(src, dst, codec, opts?)` | Read `src`, write compressed `dst`; returns `true`. |
| `ncompress.decompress_file(src, dst, codec, opts?)` | Decompress file; returns `true`. |
| `ncompress.parallel_compress(blocks, codec, opts?)` | Compress many independent blocks in parallel; returns `byte[][]`. |
| `ncompress.parallel_decompress(blocks, codec, opts?)` | Parallel batch decompress. |

### Codec shortcuts

| Method | Description |
|--------|-------------|
| `ncompress.zstd_compress(data, opts?)` | ZSTD compress (default level 3). |
| `ncompress.zstd_decompress(data, opts?)` | ZSTD decompress. |
| `ncompress.lz4_compress(data, opts?)` | LZ4 frame compress. |
| `ncompress.lz4_decompress(data, opts?)` | LZ4 frame decompress. |
| `ncompress.brotli_compress(data, opts?)` | Brotli compress. |
| `ncompress.brotli_decompress(data, opts?)` | Brotli decompress. |
| `ncompress.xz_compress(data, opts?)` | XZ compress. |
| `ncompress.xz_decompress(data, opts?)` | XZ decompress. |

### Compress options

| Key | Default | Description |
|-----|---------|-------------|
| `level` | codec default | Compression level (see table above). |
| `content_size` | `true` | Embed uncompressed size in frame (zstd, lz4). |
| `checksum` | `false` | Enable frame checksum (zstd, lz4 block checksums). |
| `window_log` | `0` | Brotli window size log2 (10–24); `0` = library default. |
| `independent_blocks` | `false` | LZ4 independent block mode. |
| `threads` | CPU count | Parallel batch `threads` (parallel_* only). |

### Decompress options

| Key | Default | Description |
|-----|---------|-------------|
| `max_output` | 256 MiB | Maximum allowed decompressed bytes (`0` = use `MAX_BYTES`). |
| `verify_content_size` | `true` | Verify declared content size when present. |

## Stream API

| Method | Description |
|--------|-------------|
| `ncompress.stream_open(mode, codec, opts?)` | Open a stream; `mode` is `"compress"` or `"decompress"`. Returns handle `int`. |
| `ncompress.stream_write(handle, chunk)` | Feed bytes. Compress mode may return early compressed output; decompress mode returns `true`. |
| `ncompress.stream_read(handle, max?)` | Read decompressed output (decompress handles only). |
| `ncompress.stream_finish(handle)` | Finalize and return remaining output; closes handle. |
| `ncompress.stream_close(handle)` | Abort and close without finishing; returns whether handle existed. |

## Limits

`ncompress.MAX_BYTES` is 256 MiB per buffer (input and output guard).

## Errors

Operations return `{__error: true, code, kind: "ncompress_error", message}` on failure. Corrupt frames use code `3558`.

## See also

- `archive` — gzip/deflate one-shot helpers
- `nbinary` — byte packing and hex
- `npar` — parallel utilities used internally for batch compression
