# nhub — model/dataset hub downloads

Download models and datasets from Hugging Face Hub or any direct URL, with local cache, resume, and SHA-256 verification. Native Rust implementation (~huggingface-hub subset) for feeding `nllm` and `nonnx`.

## Import

```niao
import "nhub"
```

Paths `import "std/nhub"` and `import "nhub"` are equivalent. Flat builtins (`nhub_client`, `nhub_download`, …) are also available globally after import.

## Quick start

```niao
import "nhub"

let hub = nhub.client({
    cache_dir: "/data/hf-cache",
    token: nil,           // or HF token string; also reads ~/.cache/huggingface/token
    retries: 3,
})

let repo = nhub.model(hub, "gpt2")
let cfg = nhub.download(repo, "config.json")
print(cfg.path)
print(cfg.cached)   // true when served from cache

let weights = nhub.snapshot(repo, {
    allow_patterns: ["*.json", "tokenizer*"],
})

nhub.download_url(
    "https://example.com/model.gguf",
    "/tmp/model.gguf",
    {expected_sha256: "abc123...", resume: true},
)

nhub.close_repo(repo)
nhub.close(hub)
```

## Cache & client

| Method | Description |
|--------|-------------|
| `nhub.version()` | Library version string. |
| `nhub.cache_dir()` | Effective cache path (`$HF_HOME/hub` or `~/.cache/huggingface/hub`). |
| `nhub.default_cache_dir()` | Default cache without `$HF_HOME` override. |
| `nhub.client(opts?)` | Create hub client handle. |
| `nhub.close(client)` | Release client handle. |
| `nhub.token(client?)` | Read HF token from cache/env. |

Client opts: `cache_dir`, `token`, `endpoint`, `retries`, `progress`.

## Repositories (HF Hub)

| Method | Description |
|--------|-------------|
| `nhub.model(client, repo_id, opts?)` | Model repo handle. |
| `nhub.dataset(client, repo_id, opts?)` | Dataset repo handle. |
| `nhub.close_repo(repo)` | Release repo handle. |
| `nhub.file_url(repo, filename)` | Resolve HF resolve URL. |
| `nhub.repo_info(repo)` | `{sha, files, repo_id, revision, kind}`. |
| `nhub.list_files(repo)` | Remote filenames from repo metadata. |
| `nhub.cached(repo, filename)` | Local cache path or `nil`. |
| `nhub.download(repo, filename)` | `{path, bytes, cached}`. |
| `nhub.snapshot(repo, opts?)` | Download filtered files → `{paths, count, bytes}`. |

Repo opts: `revision` (default `"main"`). Snapshot opts: `allow_patterns`, `ignore_patterns` (glob syntax).

## Direct URLs & checksums

| Method | Description |
|--------|-------------|
| `nhub.download_url(url, dest, opts?)` | Resumable HTTP GET → `{path, bytes, resumed}`. |
| `nhub.sha256(data)` | Hex digest of string, file path, or byte array. |
| `nhub.verify(data, expected, opts?)` | Compare digest; returns `true` or catchable checksum error. |

Direct opts: `timeout_ms`, `retries`, `resume`, `expected_sha256`, `headers`. Verify opts: `algo` (`sha256` / `sha512`).

## Errors

Catchable `nhub_error` objects with `code`, `message`, and `type`. Checksum failures use code `4622`.

## See also

- [`nllm`](NLLM.md) — GGUF inference (uses HF cache for tokenizers)
- [`nonnx`](NONNX.md) — ONNX inference
- [`nreq`](NREQ.md) — general HTTP client
