# nbrowser — headless browser automation

Headless browser automation for Niao via the Chrome DevTools Protocol: navigate, click, fill, screenshot, PDF. ~playwright, selenium.

v0.1 drives Chromium / Chrome / Edge over **CDP**. Classic WebDriver (Selenium wire protocol) is deferred.

## Import

```niao
import "nbrowser"
```

Paths `import "std/nbrowser"` and `import "nbrowser"` are equivalent. Flat builtins (`nbrowser_launch`, `nbrowser_goto`, …) are also available globally after import.

## Quick start

```niao
import "nbrowser"

let b = nbrowser.launch({headless: true})
let p = nbrowser.new_page(b)
nbrowser.goto(p, "https://example.com")
print(nbrowser.title(p))
nbrowser.fill(p, "input", "hello")
let png = nbrowser.screenshot(p, {full_page: true})
let pdf = nbrowser.pdf(p)
nbrowser.close(b)
```

## Launch & connect

| Method | Description |
|--------|-------------|
| `nbrowser.executable_path()` | Detected Chrome / Chromium / Edge path, or `nil`. Honors `NBROWSER_EXECUTABLE` / `CHROME`. |
| `nbrowser.launch(opts?)` | Spawn a browser. Returns int handle. |
| `nbrowser.connect(endpoint\|opts)` | Attach to an existing DevTools endpoint (`ws://…` or `http://host:port`). |
| `nbrowser.close(browser)` | Close browser and its pages. |
| `nbrowser.is_connected(browser)` | `true` while the handle is open. |
| `nbrowser.version(browser)` | Product / revision / protocol string. |

### Launch options

| Field | Type | Default | Notes |
|-------|------|---------|-------|
| `headless` | bool | `true` | Use new headless mode when true. |
| `executable` / `chrome` | string | auto | Path to browser binary. |
| `width` / `height` | int | 1280 / 720 | Window size. |
| `timeout_ms` | int | 30000 | Launch / request timeout. |
| `no_sandbox` | bool | `false` | Pass `--no-sandbox` (CI containers). |
| `args` | [string] | `[]` | Extra Chromium flags. |
| `user_data_dir` | string | — | Profile directory. |
| `ignore_https_errors` | bool | `true` | Accept bad TLS certs. |

### Connect options

| Field | Type | Required | Notes |
|-------|------|----------|-------|
| `endpoint` / `ws` / `url` | string | yes | DevTools WebSocket or HTTP discovery URL. |
| `timeout_ms` | int | no | Default 30000. |

## Pages

| Method | Description |
|--------|-------------|
| `nbrowser.new_page(browser, url?)` | Open a tab (`about:blank` if no URL). Returns page handle. |
| `nbrowser.pages(browser)` | Array of live page handles. |
| `nbrowser.close_page(page)` | Close one tab. |
| `nbrowser.goto(page, url, opts?)` | Navigate. Returns `{url, title, ok}`. `opts.timeout_ms`, `opts.wait_until`. |
| `nbrowser.reload(page)` | Reload current URL. |
| `nbrowser.url(page)` / `title(page)` / `content(page)` | Inspect document. |
| `nbrowser.eval(page, expression)` | Evaluate JS; JSON result mapped to Niao values. |

## Interaction

| Method | Description |
|--------|-------------|
| `nbrowser.click(page, selector)` | Click first matching element. |
| `nbrowser.fill(page, selector, text)` | Clear + type (Playwright-style fill). |
| `nbrowser.type(page, selector, text)` | Type without clearing. |
| `nbrowser.press(page, key)` | Key press on focused page (`Enter`, `Tab`, …). |
| `nbrowser.hover(page, selector)` / `focus(page, selector)` | Pointer / focus. |
| `nbrowser.select(page, selector, value)` | Set `<select>` value + change events. |
| `nbrowser.check(page, selector)` / `uncheck(page, selector)` | Toggle checkboxes. |
| `nbrowser.wait_for(page, selector, opts?)` | Poll until selector exists. `opts.timeout_ms`. |
| `nbrowser.text(page, selector)` | Inner text. |
| `nbrowser.attr(page, selector, name)` | Attribute string or `nil`. |
| `nbrowser.exists(page, selector)` / `count(page, selector)` | Presence / count. |

## Capture & page config

| Method | Description |
|--------|-------------|
| `nbrowser.screenshot(page, opts?)` | PNG/JPEG/WebP bytes. `full_page`, `format`, `quality`. |
| `nbrowser.pdf(page, opts?)` | PDF bytes. `landscape`, `print_background`, `scale`, `paper_width`, `paper_height`. |
| `nbrowser.set_viewport(page, {width, height, …})` | Emulate device metrics. |
| `nbrowser.set_headers(page, {Name: "value"})` | Extra HTTP headers. |
| `nbrowser.cookies(page)` | Cookie objects. |
| `nbrowser.set_cookie(page, {name, value, …})` | Set one cookie (`url`/`domain`/`path`/…). |
| `nbrowser.clear_cookies(page)` | Delete cookies for the page. |

## Errors

| Code | Meaning |
|------|---------|
| 4510 | Wrong argument count. |
| 4511 | Browser / CDP / I/O error (catchable `nbrowser_error`). |
| 4512 | Wrong type or missing config field. |
| 4513 | Invalid or closed handle. |
| 4514 | Timeout (navigation / wait_for / connect). |

Empty selectors, empty URLs, and missing executables return catchable errors (or type errors for programmer mistakes). Use `is_error(v)` / `ntest.assert_error`.

## Deferred (v0.1)

WebDriver / BiDi wire protocol, network request interception & routing, file chooser / downloads, multi-context (incognito API), frame switching, dialog handlers, tracing / HAR, geolocation & permissions suites.

## See also

- `nhtml` — parse and query HTML offline.
- `nreq` — HTTP client without a browser.
- `nssh` — handle-based session API pattern.
