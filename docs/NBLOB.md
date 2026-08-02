# nblob — unified object-store VFS

One open/read/write/list API over local directories, in-memory stores, S3, Azure Blob, and GCS (~`fsspec` / `smart_open`, layered over the same REST patterns as `naws` / `nazure` / `ngcp`).

Implemented in the in-house `niao_nblob` crate (`niao_http` + `niao_crypto` / `niao_codec` for cloud signing; no AWS/Azure/GCP SDKs).

## Import

```niao
import "nblob"
```

Paths `import "std/nblob"` and `import "nblob"` are equivalent. Flat builtins (`nblob_read`, `nblob_open`, …) are also available globally after import.

## Quick start

```niao
import "nblob"

// In-memory store (great for tests)
let fs = nblob.memory("demo")
nblob.fs_write(fs, "hello.txt", "hi from nblob")
print(nblob.fs_read(fs, "hello.txt"))

// Same object via URI
print(nblob.read("memory://demo/hello.txt"))

// Local filesystem
let local = nblob.local()
nblob.fs_write(local, "out/note.txt", "saved")
let f = nblob.open("out/note.txt", "r")
print(nblob.read_bytes(f))
nblob.close(f)

// Cloud (credentials required)
// let s3 = nblob.s3({region: "us-east-1", access_key: "...", secret_key: "...", bucket: "my-bucket"})
// nblob.fs_write(s3, "data/x.bin", "payload")
```

## URI helpers

| Method | Description |
|--------|-------------|
| `nblob.parse(uri)` | → `{scheme, netloc, path, bucket, key, uri}` |
| `nblob.join(base, child)` | Join a child path onto a URI/path |
| `nblob.scheme(uri)` | Scheme string (`""` for bare local paths) |

Supported schemes: bare paths / `file://`, `memory://`, `s3://` (`s3a`/`s3n`), `gs://` (`gcs`), `az://` / `azure://`, `abfs://container@account...`.

## Filesystem factories

| Method | Description |
|--------|-------------|
| `nblob.local(root?)` | Local FS handle (default: cwd) |
| `nblob.memory(name?)` | Named or ephemeral in-memory FS |
| `nblob.s3(opts)` | S3 FS (`region`, `access_key`, `secret_key`, `bucket`, optional `session_token` / `endpoint`) |
| `nblob.azure(opts)` | Azure Blob FS (`account`, `container`, `key`/`sas`/`bearer`) |
| `nblob.gcs(opts)` | GCS FS (`access_token`/`token`, `bucket`, optional `project`) |
| `nblob.fs(uri_or_opts)` | Auto factory from URI string or `{scheme, ...}` object |
| `nblob.close_fs(fs)` | Drop an FS handle |

Creating `s3` / `azure` / `gcs` also registers default credentials for URI-level ops (`nblob.read("s3://...")`).

## URI-level ops (~smart_open)

| Method | Description |
|--------|-------------|
| `nblob.open(uri, mode?, opts?)` | Open file handle (`r`/`w`/`a`, with optional `b`/`t` suffix) |
| `nblob.read(uri)` | Read entire object → string |
| `nblob.write(uri, data, opts?)` | Write bytes/string; `opts.content_type` for cloud |
| `nblob.exists(uri)` | Existence check |
| `nblob.info(uri)` | → `{name, type, size, mtime?}` |
| `nblob.ls(uri, opts?)` / `nblob.list(...)` | List entries; `opts.detail` adds mtime when available |
| `nblob.rm(uri)` | Remove file/prefix |
| `nblob.mkdir(uri)` | Create directory (local/memory; no-op on cloud prefixes) |
| `nblob.cp(src, dst)` | Copy (cross-scheme supported) |
| `nblob.mv(src, dst)` | Move = copy + remove |
| `nblob.put(local, uri)` | Upload local → remote URI |
| `nblob.get(uri, local)` | Download remote → local path |

## FS-relative ops

| Method | Description |
|--------|-------------|
| `nblob.fs_read(fs, path)` | Read relative to FS root |
| `nblob.fs_write(fs, path, data, opts?)` | Write |
| `nblob.fs_exists(fs, path)` | Exists |
| `nblob.fs_info(fs, path)` | Metadata |
| `nblob.fs_ls(fs, path?, opts?)` | List |
| `nblob.fs_rm(fs, path)` | Remove |
| `nblob.fs_mkdir(fs, path)` | Mkdir |
| `nblob.fs_open(fs, path, mode?)` | Open relative path |

## File handle ops

| Method | Description |
|--------|-------------|
| `nblob.read_bytes(file, n?)` | Read `n` bytes (or rest) from current position |
| `nblob.write_bytes(file, data)` | Write at position |
| `nblob.tell(file)` | Current offset |
| `nblob.seek(file, offset, whence?)` | Seek (`whence`: 0 start, 1 cur, 2 end) |
| `nblob.flush(file)` | Persist buffer to store |
| `nblob.close(file)` | Flush + free handle |
| `nblob.size(file)` | Buffer size |

## Errors

I/O and store failures return catchable `nblob_error` values (use `ntest.is_error` / `try`).

| Code | Kind | When |
|------|------|------|
| E4570 | hard | Arity mismatch |
| E4571 | `nblob_error` | I/O, HTTP, not found, invalid URI |
| E4572 | hard | Type mismatch |
| E4573 | `nblob_error` | Invalid fs/file handle |
| E4574 | `nblob_error` | Missing cloud credentials (auth) |

## Limitations (v0.1)

- Cloud uploads/downloads are buffered in memory (no multipart / streaming transfer).
- S3 list is a single page (`list-type=2` with delimiter); no continuation-token loop yet.
- No transparent `.gz` / compression layer (unlike full smart_open).
- No cloud glob / versioned objects / soft-delete.

## See also

- `naws` — low-level SigV4 S3 / DynamoDB / Lambda / SSM
- `nazure` — Azure Blob / Table / Functions
- `ngcp` — GCS / Pub/Sub / Firestore / Cloud Functions
- `nfs` — high-level local filesystem helpers
- `nmmap` — memory-mapped local files
- `io` — basic local file I/O
