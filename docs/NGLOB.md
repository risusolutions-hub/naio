# nglob standard library

Glob patterns, `**` recursion, gitignore-style matching, walk with filters. Native Rust implementation (~glob + fnmatch + pathspec subset).

## Import

```niao
import "nglob"
```

Paths `import "std/nglob"` and `import "nglob"` are equivalent. Flat builtins (`nglob_match`, `nglob_glob`, …) are also available globally after import.

## Quick start

```niao
import "nglob"

print(nglob.match("foo.py", "*.py"))           // true
print(nglob.filter(["a.py", "b.txt"], "*.py")) // ["a.py"]

let hits = nglob.glob("**/*.rs", {root: "crates", recursive: true})
print(len(hits))

let m = nglob.compile(["*.py", "!tests/*"], {gitignore: true})
print(nglob.ignored(m, "vendor/x.py"))         // true
print(nglob.ignored(m, "tests/x.py"))         // false

let entries = nglob.walk(".", {
    include: ["**/*.niao"],
    exclude: ["**/target/**"],
})
print(entries[0].path)

nglob.close(m)
```

## fnmatch-style

| Method | Description |
|--------|-------------|
| `nglob.match(path, pattern, opts?)` | Unix fnmatch (`*` crosses `/`). `opts`: `{case_sensitive, case_insensitive, basename_only}`. |
| `nglob.match_case(path, pattern)` | Case-sensitive shorthand. |
| `nglob.filter(paths, pattern, opts?)` | Return paths matching `pattern`. |
| `nglob.has_magic(pattern)` | True if pattern contains `*`, `?`, or `[`. |
| `nglob.escape(text)` | Escape metacharacters for literal matching. |
| `nglob.translate(pattern, opts?)` | Regex equivalent (Python `fnmatch.translate` shape). |

## Filesystem glob

| Method | Description |
|--------|-------------|
| `nglob.glob(pattern, opts?)` | Expand pattern on disk. `opts`: `{root, recursive, hidden, follow_links, case_sensitive}`. |

Non-recursive `*.rs` searches only `root`. Use `**` or `{recursive: true}` for tree walks.

## Compiled matchers

| Method | Description |
|--------|-------------|
| `nglob.compile(patterns, opts?)` | Compile one or more patterns → handle. `opts`: `{gitignore, root, case_sensitive}`. |
| `nglob.close(handle)` | Free handle. |
| `nglob.matches(handle, path, is_dir?)` | Glob/include match. |
| `nglob.ignored(handle, path, is_dir?)` | Gitignore mode: true when path is ignored. |
| `nglob.classify(handle, path, is_dir?)` | `"whitelist"`, `"ignore"`, or `"none"`. |
| `nglob.filter_paths(handle, paths)` | Filter in-memory path list. |
| `nglob.match_any(path, patterns, opts?)` | OR match without compiling a handle. |
| `nglob.pattern_count(handle)` | Number of compiled patterns. |
| `nglob.is_gitignore(handle)` | True when compiled as gitignore/pathspec. |

## Walk & parallel

| Method | Description |
|--------|-------------|
| `nglob.walk(root, opts?)` | Walk tree; returns `[{path, is_dir, depth}, …]`. |
| `nglob.walk_paths(root, opts?)` | Paths only. |
| `nglob.parallel_filter(paths, pattern, opts?)` | Parallel fnmatch over many paths. `opts.threads` defaults to CPU count. |
| `nglob.paths_matching(paths, patterns, opts?)` | Filter paths by glob patterns (no I/O). |
| `nglob.parallel_classify(paths, patterns, opts?)` | Parallel compiled match. |

Walk `opts`: `{include, exclude, gitignore, hidden, max_depth, follow_links, files_only, case_sensitive, threads}`.

- `include` — glob patterns; empty means all files (after excludes).
- `exclude` — glob or gitignore patterns skipped during walk.
- `gitignore` (default `true`) — honor `.gitignore` files on disk.

## Errors

| Code | Meaning |
|------|---------|
| 3526 | Wrong argument count. |
| 3527 | Operation failed (invalid pattern, I/O) — catchable `nglob_error`. |
| 3528 | Wrong argument type. |
| 3529 | Invalid or closed handle — catchable `nglob_error`. |

## Deferred vs Python glob/pathspec

Not in v0.1.0: brace expansion (`{a,b}`), `dir_fd` / `root_dir` POSIX semantics, `pathlib`-style pure paths, `.git/info/exclude` custom files beyond the `ignore` crate defaults, and streaming `iglob` iterators (use `walk` and filter in batches instead).
