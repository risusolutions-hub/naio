# nfeed — RSS / Atom / JSON Feed parse & generate

RSS / Atom / JSON Feed parse + generate with feedparser-shaped results, charset detection, HTML sanitization, and parallel batch parsing. ~feedparser subset.

## Import

```niao
import "nfeed"
```

Paths `import "std/nfeed"` and `import "nfeed"` are equivalent. Flat builtins (`nfeed_parse`, `nfeed_emit`, …) are also available globally after import.

## Quick start

```niao
import "nfeed"

let doc = nfeed.parse_file("https://example.com/feed.xml")
print(doc.feed.title, doc.feed.link)
for e in nfeed.entries(doc) {
    print(nfeed.get(e, "title"), nfeed.get(e, "link"))
}

// Build and emit RSS 2.0
let feed = nfeed.build({
    title: "My Blog",
    link: "https://blog.example/",
    entries: [{
        title: "Hello",
        link: "https://blog.example/post-1",
        guid: "post-1",
        summary: "<p>First post</p>",
        published_ms: 1283730060000
    }]
})
let xml = nfeed.emit(feed, {format: "rss2"})
```

## Parse

| Method | Description |
|--------|-------------|
| `nfeed.parse(text, opts?)` | Parse feed text → document object. |
| `nfeed.parse_bytes(bytes, opts?)` | Parse raw bytes with charset detection. |
| `nfeed.parse_file(path, opts?)` | Read file and parse. |
| `nfeed.parse_many(texts, opts?)` | Parallel parse of string array. |
| `nfeed.valid(text)` | `true` when input is a valid syndication feed. |
| `nfeed.detect(text)` | Sniff container: `"rss"`, `"atom"`, `"json"`, or `nil`. |
| `nfeed.detect_version(text)` | Version string: `rss20`, `atom10`, `json10`, … |

### Parse options

| Key | Default | Description |
|-----|---------|-------------|
| `sanitize` | `false` | Sanitize HTML in summary/content via allowlist policy. |
| `relaxed` | `false` | On parse errors, retry leniently and set `bozo=true`. |

## Document object

Parsed feeds return an object shaped like feedparser:

| Field | Description |
|-------|-------------|
| `version` | Detected format (`rss20`, `atom10`, `json10`, …). |
| `bozo` | `true` when the source was malformed but partially recovered. |
| `bozo_exception` | Error message when `bozo` is true. |
| `encoding` | Detected charset label. |
| `feed` | Feed-level metadata object (`title`, `link`, `subtitle`, `authors`, …). |
| `entries` | Array of entry objects. |

Entry objects expose `title`, `link`, `id`, `summary`, `summary_detail`, `content` (array of `{value, type, …}`), `published`, `published_ms`, `updated`, `updated_ms`, `author`, `authors`, `tags`, `enclosures`, `guid`, `guid_is_permalink`.

Access helpers:

| Method | Description |
|--------|-------------|
| `nfeed.entries(doc)` | Entry array from a document. |
| `nfeed.get(obj, field)` | Case-insensitive field lookup; `nil` when missing. |

## Emit

| Method | Description |
|--------|-------------|
| `nfeed.emit(doc, opts?)` | Serialize to RSS 2.0, Atom 1.0, or JSON Feed. |
| `nfeed.emit_file(path, doc, opts?)` | Write serialized feed; returns `true`. |

### Emit options

| Key | Default | Description |
|-----|---------|-------------|
| `format` | `"rss2"` | `"rss2"`, `"atom"`, or `"json"`. |
| `pretty` | `false` | Pretty-print JSON output. |
| `indent` | `0` | XML indent width (0 = compact). |

## Build

| Method | Description |
|--------|-------------|
| `nfeed.build(opts?)` | Build a document from `{title?, link?, entries?: [...]}`. |
| `nfeed.build_entry(opts?)` | Build a single entry object. |

## HTML & dates

| Method | Description |
|--------|-------------|
| `nfeed.strip_html(html)` | Fast tag strip (no DOM). |
| `nfeed.sanitize_html(html, opts?)` | XSS-safe allowlist HTML cleanup. |
| `nfeed.parse_date(text)` | Parse RFC 822 / RFC 3339 → `{raw, iso, unix_ms}`. |
| `nfeed.format_date(unix_ms)` | Format milliseconds as RFC 3339 UTC. |

## Errors

| Code | Kind | When |
|------|------|------|
| E4420 | `nfeed_error` | Wrong argument count. |
| E4421 | `nfeed_error` | General feed error (I/O, emit, build). |
| E4422 | `nfeed_error` | Type mismatch. |
| E4423 | `nfeed_error` | Parse / date / format sniff failure. |

## Supported formats

**Parse:** RSS 0.9x / 1.0 / 2.0, Atom 1.0, JSON Feed 1.0/1.1 (via native `feed-rs` engine), Media RSS enclosures, iTunes extensions (metadata only).

**Emit:** RSS 2.0, Atom 1.0, JSON Feed 1.0.

**Deferred:** HTTP URL fetching (use `http` + `nfeed.parse`), RSS 0.9x emit, CDF, full extension round-trip for iTunes/Media RSS namespaces.

## See also

- [`NICAL.md`](NICAL.md) — calendar/vCard parsing (similar parse/emit pattern)
- [`NHTML.md`](NHTML.md) — HTML parsing for embedded feed content
