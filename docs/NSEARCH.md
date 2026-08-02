# nsearch standard library

Hosted search-engine clients for Niao: Elasticsearch/OpenSearch, Meilisearch, and Typesense. Built as a thin client layer over the in-tree `niao_http` transport — analogous to Python's `elasticsearch` and `meilisearch` SDKs.

## Import

```niao
import "nsearch"
```

Paths `import "std/nsearch"` and `import "nsearch"` are equivalent. Flat builtins (`nsearch_search`, `nsearch_es_query`, …) are also available after import.

## Quick start

```niao
import "nsearch"

let es = nsearch.elasticsearch({
    url: "http://localhost:9200",
    // api_key: "...",
    // username: "elastic", password: "...",
    // cloud_id: "my-cluster:base64…",
    timeout_ms: 10000,
})

let body = nsearch.es_query({q: "niao", fields: ["title", "body"], size: 10})
let r = nsearch.search(es, {index: "docs", q: "niao", size: 10})
if nsearch.ok(r) {
    let hits = nsearch.hits({engine: "elasticsearch", body: r.body})
    print(len(hits))
}
nsearch.close(es)

let meili = nsearch.meilisearch({url: "http://localhost:7700", key: "MASTER_KEY"})
let mr = nsearch.search(meili, {
    index: "movies",
    q: "dune",
    filter: nsearch.meili_filter(["year > 2000", "genres = Action"]),
    limit: 20,
})
nsearch.close(meili)
```

## Clients

| Method | Description |
|--------|-------------|
| `nsearch.elasticsearch(opts?)` | Elasticsearch client handle. |
| `nsearch.opensearch(opts?)` | OpenSearch client (same REST shape as ES). |
| `nsearch.meilisearch(opts?)` | Meilisearch client. |
| `nsearch.typesense(opts?)` | Typesense client. |
| `nsearch.close(client)` | Drop the handle. |
| `nsearch.info(client)` | `{engine, url, timeout_ms, auth}`. |
| `nsearch.engine(client)` | Engine name string. |

Client opts: `url`, `cloud_id` (Elastic Cloud), `api_key`, `key` (Meili), `bearer`, `username`/`password` (or `user`/`pass`), `timeout_ms`.

## Document & search ops

| Method | Description |
|--------|-------------|
| `nsearch.search(client, opts)` | Search. Opts: `index`/`collection`, `q`/`query`, `fields`, `query_by`, `filter`/`filter_by`, `sort`, `limit`/`size`, `offset`/`from`, `body`/`json`. |
| `nsearch.index(client, index, doc, opts?)` | Index/upsert a document (`opts.id` or trailing id string). |
| `nsearch.get(client, index, id)` | Fetch one document. |
| `nsearch.delete(client, index, id)` | Delete one document. |
| `nsearch.update(client, index, id, doc)` | Partial update / upsert. |
| `nsearch.bulk(client, ops)` | Bulk ops `[{action, index, id?, doc?}]`. |
| `nsearch.create_index(client, name, settings?)` | Create index/collection. |
| `nsearch.delete_index(client, name)` | Delete index/collection. |
| `nsearch.list_indexes(client)` | List indexes/collections. |
| `nsearch.index_exists(client, name)` | Bool existence check. |
| `nsearch.request(client, method, path, opts?)` | Raw REST escape hatch (`body`/`json`, `params`). |

## Helpers (offline)

| Method | Description |
|--------|-------------|
| `nsearch.es_query(opts?)` | Build Elasticsearch query DSL JSON. |
| `nsearch.es_bulk_ndjson(ops)` | Build `_bulk` NDJSON body. |
| `nsearch.meili_filter(parts)` | Join filter clauses with ` AND `. |
| `nsearch.ts_filter(parts)` | Join Typesense `filter_by` with ` && `. |
| `nsearch.join(base, path)` | Resolve relative URL. |
| `nsearch.encode_params(map)` | Query string encode. |
| `nsearch.ok(r)` | `true` for 2xx responses. |
| `nsearch.json(r)` | Parse response `body` as JSON. |
| `nsearch.raise_for_status(r)` | Return error value if not ok. |
| `nsearch.hits(r, engine?)` | Normalize hits across engines. |

## Response object

| Field | Description |
|-------|-------------|
| `status` | HTTP status code. |
| `ok` | `true` for 2xx. |
| `url` | Final URL. |
| `body` | Response body text. |
| `elapsed_ms` | Wall time for the call. |

## Errors

| Code | Kind | When |
|------|------|------|
| E4550 | `nsearch_error` | Wrong arity. |
| E4551 | `nsearch_error` | Transport / engine / parse failure (catchable value). |
| E4552 | `nsearch_error` | Wrong argument type. |
| E4553 | `nsearch_error` | Invalid client handle. |

Hard errors (arity/type) throw. HTTP and engine failures return catchable error values so `is_error()` / try-catch work.

## Notes

- OpenSearch uses the same client verbs as Elasticsearch (compatible REST paths for the covered ops).
- Meilisearch and Typesense bulk calls require a single index/collection per `bulk()` invocation.
- Deferred in v0.1: scroll/PIT, multi-search, full settings/synonyms CRUD, task polling.
