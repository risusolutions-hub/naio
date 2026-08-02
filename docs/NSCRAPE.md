# nscrape standard library

Polite scraping for Niao: robots.txt, per-host rate limits, retries, sitemap crawl, and article/readability extraction. ~scrapy, trafilatura, newspaper.

## Import

```niao
import "nscrape"
```

Paths `import "std/nscrape"` and `import "nscrape"` are equivalent. Flat builtins (`nscrape_get`, `nscrape_extract`, …) are also available after import.

## Quick start

```niao
import "nscrape"

let html = "<article><h1>Hello</h1><p>Cats are wonderful companions for people everywhere today.</p></article>"
let art = nscrape.extract(html)
print(art.title)
print(art.text)

let robots = nscrape.parse_robots("User-agent: *\nDisallow: /private\nAllow: /private/ok\n")
print(nscrape.allowed(robots, "https://ex.com/private/ok"))
nscrape.close(robots)

let bot = nscrape.bot({delay_ms: 200, retries: 2, respect_robots: true})
let page = nscrape.get(bot, "https://example.com/", {timeout_ms: 10000})
if page.robots_allowed && page.ok {
    let article = nscrape.readable(page.body, {url: page.url})
    print(article.title)
}
nscrape.close(bot)
```

## Bot (politeness policy)

| Method | Description |
|--------|-------------|
| `nscrape.bot(opts?)` | Create a bot handle (UA, delay, retries, robots). |
| `nscrape.close(handle)` | Drop bot / robots / limiter / crawl handle. |
| `nscrape.bot_info(bot)` | Inspect policy fields. |

Bot opts: `user_agent`, `delay_ms`, `retries`, `backoff_ms`, `timeout_ms`, `max_redirects`, `respect_robots`, `same_host_only`, `max_pages`, `headers`.

## robots.txt

| Method | Description |
|--------|-------------|
| `nscrape.parse_robots(text)` | Parse → handle. |
| `nscrape.allowed(robots\|text, url, ua?)` | Allow-check (longest Allow/Disallow match). |
| `nscrape.crawl_delay(robots\|text, ua?)` | Crawl-delay in milliseconds (0 if unset). |
| `nscrape.sitemaps(robots\|text)` | Sitemap URLs listed in the file. |

## Rate limiting

| Method | Description |
|--------|-------------|
| `nscrape.limiter(opts?)` | Per-host delay limiter (`delay_ms`). |
| `nscrape.wait(limiter, host?)` | Block until slot free; returns waited ms. |
| `nscrape.limiter_info(limiter)` | Stats: `delay_ms`, `waits`, `total_wait_ms`, … |

## Polite fetch

| Method | Description |
|--------|-------------|
| `nscrape.get(url\|bot, …)` | GET with rate limit + robots + retries. |
| `nscrape.fetch_robots(url\|bot, …)` | Fetch `/robots.txt` body for an origin. |

Call shapes: `get(url)`, `get(url, opts)`, `get(bot, url)`, `get(bot, url, opts)`.

Response object: `status`, `ok`, `url`, `body`, `headers`, `elapsed_ms`, `robots_allowed`. When robots deny the URL, `robots_allowed` is `false` and `status` is `0` (no network call).

## Sitemap

| Method | Description |
|--------|-------------|
| `nscrape.parse_sitemap(xml)` | `{urls: [{loc, …}], sitemaps: […]}`. |
| `nscrape.sitemap_urls(xml)` | Page URL locs only. |
| `nscrape.crawl_sitemap(url\|bot, …)` | Fetch sitemap (+ nested indexes); return all page URLs. |

## Article / readability

| Method | Description |
|--------|-------------|
| `nscrape.extract(html, opts?)` | Article object (title, text, html, byline, excerpt, …). |
| `nscrape.readable(html, opts?)` | Alias of `extract`. |
| `nscrape.extract_text(html)` | Main text only. |
| `nscrape.extract_title(html)` | Best title guess. |
| `nscrape.extract_links(html, base?)` | `[{href, text}]` (resolves relative if `base`). |
| `nscrape.extract_meta(html)` | Meta name/property map. |
| `nscrape.parallel_extract(htmls, opts?)` | Batch extract (`threads` opt). |

Extract opts: `min_text_length`, `url` / `base`.

## Crawl

| Method | Description |
|--------|-------------|
| `nscrape.crawl(url\|bot, …)` | Start BFS crawl handle. |
| `nscrape.next(crawl)` | Fetch next page, or `nil` when done. |
| `nscrape.results(crawl)` | Pages collected so far. |
| `nscrape.crawl_info(crawl)` | `seed`, `pages`, `pending`, `visited`, `done`, … |

Crawl opts: `max_depth` (default 1), `max_pages`.

## URL helpers

| Method | Description |
|--------|-------------|
| `nscrape.canonicalize(url)` | Strip fragment; normalize path. |
| `nscrape.same_host(a, b)` | Case-insensitive host compare. |
| `nscrape.origin(url)` | `scheme://host[:port]`. |
| `nscrape.join(base, rel)` | Resolve relative URL. |
| `nscrape.is_html_ct(ct)` | Content-Type looks like HTML. |

## Errors

| Code | Kind | When |
|------|------|------|
| E4480 | `nscrape_error` | Wrong arity. |
| E4481 | `nscrape_error` | Soft scrape/parse/network failure. |
| E4482 | `nscrape_error` | Wrong argument type. |
| E4483 | `nscrape_error` | Invalid or closed handle. |
| E4484 | `nscrape_error` | robots.txt parse failure. |

Arity and type mismatches raise hard errors. Parse/network/handle failures return catchable `error` values.

## Notes

- HTTP transport is `niao_req` / `niao_http` (not a separate Python runtime).
- v0.1 does **not** include JS rendering, Scrapy pipelines/middleware, proxy pools, or distributed crawl.
- Related: `nreq`, `nhtml`, `nsanitize`, `nurl`.
