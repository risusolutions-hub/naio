# nmime standard library

File-type detection by magic bytes, extension↔MIME maps. Native Rust implementation (~python-magic + filetype + mimetypes subset).

## Import

```niao
import "nmime"
```

Paths `import "std/nmime"` and `import "nmime"` are equivalent. Flat builtins (`nmime_from_bytes`, `nmime_guess_type`, …) are also available globally after import.

## Quick start

```niao
import "nmime"

let png = byte_array[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]
let hit = nmime.from_bytes(png)
print(hit.mime)           // image/png
print(hit.extension)      // png
print(hit.kind)           // image

print(nmime.guess_type("report.pdf").mime)     // application/pdf
print(nmime.extension_to_mime("png"))          // image/png
print(nmime.extension_for("image/jpeg"))       // jpg

let sniffed = nmime.sniff("Cargo.toml")
print(sniffed.source)     // combined, magic, or extension

let det = nmime.compile({max_bytes: 8192})
let _ = nmime.add_magic(det, byte_array[0xCA, 0xFE, 0xBA, 0xBE], "application/x-custom", "cust")
print(nmime.detect(det, byte_array[0xCA, 0xFE, 0xBA, 0xBE]).mime)
nmime.close(det)
```

## Magic-byte detection (filetype-style)

| Method | Description |
|--------|-------------|
| `nmime.from_bytes(data)` | Detect from header bytes. Returns `{mime, extension, kind, source, confidence}` or `nil`. |
| `nmime.guess_mime(data)` | MIME string or `nil`. |
| `nmime.guess_extension(data)` | Extension from magic or `nil`. |
| `nmime.match_mime(data, mime)` | True when bytes match the given MIME. |
| `nmime.is_image(data)` | True for image magic. |
| `nmime.is_video(data)` | True for video magic. |
| `nmime.is_audio(data)` | True for audio magic. |
| `nmime.is_archive(data)` | True for archive magic. |
| `nmime.is_text(data)` | True for text-like magic. |
| `nmime.is_font(data)` | True for font magic. |

## Path sniffing

| Method | Description |
|--------|-------------|
| `nmime.from_path(path, opts?)` | Read file header and detect via magic. `opts`: `{max_bytes, prefer_magic}`. |
| `nmime.from_file(path, opts?)` | Alias of `from_path`. |
| `nmime.sniff(path, opts?)` | Combined magic + filename extension guess. |
| `nmime.extension(path)` | Lowercase extension from a path string. |

Default `max_bytes` is 4096; ceiling is `nmime.max_sniff_bytes()` (65536).

## Extension maps (mimetypes-style)

| Method | Description |
|--------|-------------|
| `nmime.guess_type(filename, strict?)` | Returns `{mime, encoding}`. |
| `nmime.extension_for(mime, strict?)` | Primary extension for a MIME type. |
| `nmime.guess_all_extensions(mime, strict?)` | All known extensions. |
| `nmime.extension_to_mime(ext, strict?)` | Lookup by extension (with or without dot). |
| `nmime.mime_to_extensions(mime, strict?)` | Reverse lookup. |
| `nmime.add_type(mime, ext, strict?)` | Register a custom mapping (session-local). |
| `nmime.known_extensions(strict?)` | Sorted list of known extensions. |
| `nmime.known_types(strict?)` | Sorted list of known MIME types. |
| `nmime.common_types()` | Object map of extension → MIME. |

## MIME parsing & classification

| Method | Description |
|--------|-------------|
| `nmime.parse(mime)` | `{type, subtype, suffix, parameters, canonical}` or `nmime_error`. |
| `nmime.is_valid(mime)` | True when syntactically valid. |
| `nmime.normalize(mime)` | Canonical lowercase form. |
| `nmime.matches(mime, pattern)` | Wildcards: `image/*`, `*/json`, `*/*`. |
| `nmime.kind(mime)` | `"image"`, `"video"`, `"audio"`, `"text"`, `"archive"`, `"font"`, `"application"`, `"unknown"`. |
| `nmime.is_mime_image(mime)` | Category helpers on MIME strings. |
| `nmime.is_mime_video(mime)` | |
| `nmime.is_mime_audio(mime)` | |
| `nmime.is_mime_archive(mime)` | |
| `nmime.is_mime_text(mime)` | |
| `nmime.is_mime_font(mime)` | |

## Compiled detector handles

| Method | Description |
|--------|-------------|
| `nmime.compile(opts?)` | Create detector with optional `{max_bytes, prefer_magic}`. |
| `nmime.close(handle)` | Free handle. |
| `nmime.detect(handle, data)` | Magic detect with custom rules. |
| `nmime.sniff_handle(handle, path)` | Combined sniff using handle settings. |
| `nmime.add_magic(handle, bytes, mime, ext?, offset?)` | Add custom magic rule. |

## Parallel batch

| Method | Description |
|--------|-------------|
| `nmime.parallel_from_bytes(batches, opts?)` | Parallel magic detect. `opts.threads` defaults to CPU count. |
| `nmime.parallel_detect(paths, opts?)` | Parallel path sniff. |
| `nmime.parallel_guess_types(filenames, strict?, opts?)` | Parallel extension lookup. |

## Constants

| Method | Description |
|--------|-------------|
| `nmime.max_sniff_bytes()` | 65536 |
| `nmime.default_sniff_bytes()` | 4096 |
| `nmime.signature_count()` | Built-in magic signature count |

## Errors

Operations return catchable `nmime_error` objects (codes `E3550`–`E3553`) on invalid MIME syntax, I/O failure, bad handles, or malformed magic rules.

## See also

- [`nglob`](NGLOB.md) — glob patterns and filesystem walks
- [`nbinary`](NBINARY.md) — binary packing and CRC
- [`nencoding`](NENCODING.md) — charset detection and transcoding
