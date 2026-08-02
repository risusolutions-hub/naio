//! Native `nbrowser` standard library — headless browser automation via CDP
//! (~playwright, selenium).
//!
//! Import with `import "nbrowser"` (or `import "std/nbrowser"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_browser::{
    attr, check, clear_cookies, click, close, close_page, connect, content, cookies, count,
    eval, executable_path, exists, fill, focus, goto, hover, is_connected, launch, new_page, pages,
    pdf, press, reload, screenshot, select_option, set_cookie, set_extra_headers, set_viewport,
    text_content, title, type_text, uncheck, url, version, wait_for, BrowserError, ConnectConfig,
    CookieInput, ImageFormat, LaunchConfig, NavOpts, PdfOpts, ScreenshotOpts, Viewport, WaitUntil,
};
use niao_errors::codes;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::rc::Rc;

const E4500_NBROWSER_ARITY: u32 = codes::E4510_NBROWSER_ARITY;
const E4501_NBROWSER_ERROR: u32 = codes::E4511_NBROWSER_ERROR;
const E4502_NBROWSER_TYPE: u32 = codes::E4512_NBROWSER_TYPE;
const E4503_NBROWSER_INVALID_HANDLE: u32 = codes::E4513_NBROWSER_INVALID_HANDLE;
const E4504_NBROWSER_TIMEOUT: u32 = codes::E4514_NBROWSER_TIMEOUT;

fn nbrowser_err(span: Span, e: BrowserError) -> ValueRef {
    let code = match &e {
        BrowserError::InvalidHandle(_) => E4503_NBROWSER_INVALID_HANDLE,
        BrowserError::Timeout(_) => E4504_NBROWSER_TIMEOUT,
        _ => E4501_NBROWSER_ERROR,
    };
    error_value(code, "nbrowser_error", e.to_string(), span)
}

fn map_res(span: Span, r: Result<ValueRef, BrowserError>) -> NiaoResult<ValueRef> {
    match r {
        Ok(v) => Ok(v),
        Err(e) => Ok(nbrowser_err(span, e)),
    }
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E4500_NBROWSER_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E4500_NBROWSER_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E4502_NBROWSER_TYPE, msg.into())
}

fn int_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an int as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn string_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn opt_object(args: &[ValueRef], idx: usize) -> Option<HashMap<String, ValueRef>> {
    args.get(idx).and_then(|v| match &*v.borrow() {
        Value::Object(m) => Some(m.clone()),
        _ => None,
    })
}

fn map_get_str(m: &HashMap<String, ValueRef>, key: &str) -> Option<String> {
    m.get(key).and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.clone()),
        _ => None,
    })
}

fn map_get_int(m: &HashMap<String, ValueRef>, key: &str) -> Option<i64> {
    m.get(key).and_then(|v| match &*v.borrow() {
        Value::Int(n) => Some(*n),
        _ => None,
    })
}

fn map_get_bool(m: &HashMap<String, ValueRef>, key: &str) -> Option<bool> {
    m.get(key).and_then(|v| match &*v.borrow() {
        Value::Bool(b) => Some(*b),
        _ => None,
    })
}

fn map_get_float(m: &HashMap<String, ValueRef>, key: &str) -> Option<f64> {
    m.get(key).and_then(|v| match &*v.borrow() {
        Value::Float(f) => Some(*f),
        Value::Int(n) => Some(*n as f64),
        _ => None,
    })
}

fn json_to_niao(v: &JsonValue) -> Value {
    match v {
        JsonValue::Null => Value::Nil,
        JsonValue::Bool(b) => Value::Bool(*b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(u) = n.as_u64() {
                Value::Int(u as i64)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::String(n.to_string())
            }
        }
        JsonValue::String(s) => Value::String(s.clone()),
        JsonValue::Array(items) => {
            Value::Array(items.iter().map(json_to_niao).map(|v| v.ref_cell()).collect())
        }
        JsonValue::Object(o) => {
            let mut out = HashMap::new();
            for (k, v) in o.iter() {
                out.insert(k.to_string(), json_to_niao(v).ref_cell());
            }
            Value::Object(out)
        }
    }
}

fn parse_launch(opts: Option<&HashMap<String, ValueRef>>, span: Span) -> NiaoResult<LaunchConfig> {
    let mut c = LaunchConfig::default();
    let Some(m) = opts else {
        return Ok(c);
    };
    if let Some(b) = map_get_bool(m, "headless") {
        c.headless = b;
    }
    c.executable = map_get_str(m, "executable").or_else(|| map_get_str(m, "chrome"));
    if let Some(w) = map_get_int(m, "width") {
        if w <= 0 {
            return Err(type_err(span, "launch() width must be > 0"));
        }
        c.width = w as u32;
    }
    if let Some(h) = map_get_int(m, "height") {
        if h <= 0 {
            return Err(type_err(span, "launch() height must be > 0"));
        }
        c.height = h as u32;
    }
    if let Some(t) = map_get_int(m, "timeout_ms") {
        if t < 0 {
            return Err(type_err(span, "launch() timeout_ms must be >= 0"));
        }
        c.timeout_ms = t as u64;
    }
    if let Some(b) = map_get_bool(m, "no_sandbox") {
        c.no_sandbox = b;
    }
    if let Some(b) = map_get_bool(m, "ignore_https_errors") {
        c.ignore_https_errors = b;
    }
    c.user_data_dir = map_get_str(m, "user_data_dir");
    if let Some(v) = m.get("args") {
        match &*v.borrow() {
            Value::Array(items) => {
                for it in items {
                    match &*it.borrow() {
                        Value::String(s) => c.args.push(s.clone()),
                        other => {
                            return Err(type_err(
                                span,
                                format!("launch() args must be strings, got {}", other.type_name()),
                            ));
                        }
                    }
                }
            }
            _ => return Err(type_err(span, "launch() args must be an array of strings")),
        }
    }
    Ok(c)
}

fn parse_connect(cfg: &HashMap<String, ValueRef>, span: Span) -> NiaoResult<ConnectConfig> {
    let endpoint = map_get_str(cfg, "endpoint")
        .or_else(|| map_get_str(cfg, "ws"))
        .or_else(|| map_get_str(cfg, "url"))
        .ok_or_else(|| type_err(span, "connect() requires endpoint (or ws/url)"))?;
    let mut c = ConnectConfig::new(endpoint);
    if let Some(t) = map_get_int(cfg, "timeout_ms") {
        if t < 0 {
            return Err(type_err(span, "connect() timeout_ms must be >= 0"));
        }
        c.timeout_ms = t as u64;
    }
    Ok(c)
}

fn parse_nav(opts: Option<&HashMap<String, ValueRef>>, span: Span) -> NiaoResult<NavOpts> {
    let mut n = NavOpts::default();
    let Some(m) = opts else {
        return Ok(n);
    };
    if let Some(t) = map_get_int(m, "timeout_ms") {
        if t < 0 {
            return Err(type_err(span, "timeout_ms must be >= 0"));
        }
        n.timeout_ms = t as u64;
    }
    if let Some(s) = map_get_str(m, "wait_until") {
        n.wait_until = WaitUntil::parse(&s).ok_or_else(|| {
            type_err(
                span,
                "wait_until must be load, networkidle, or domcontentloaded",
            )
        })?;
    }
    Ok(n)
}

fn parse_screenshot(opts: Option<&HashMap<String, ValueRef>>, span: Span) -> NiaoResult<ScreenshotOpts> {
    let mut o = ScreenshotOpts::default();
    let Some(m) = opts else {
        return Ok(o);
    };
    if let Some(b) = map_get_bool(m, "full_page") {
        o.full_page = b;
    }
    if let Some(s) = map_get_str(m, "format") {
        o.format = ImageFormat::parse(&s)
            .ok_or_else(|| type_err(span, "screenshot format must be png, jpeg, or webp"))?;
    }
    if let Some(q) = map_get_int(m, "quality") {
        if !(0..=100).contains(&q) {
            return Err(type_err(span, "screenshot quality must be 0..=100"));
        }
        o.quality = Some(q as u32);
    }
    Ok(o)
}

fn parse_pdf(opts: Option<&HashMap<String, ValueRef>>, span: Span) -> NiaoResult<PdfOpts> {
    let mut o = PdfOpts::default();
    let Some(m) = opts else {
        return Ok(o);
    };
    if let Some(b) = map_get_bool(m, "landscape") {
        o.landscape = b;
    }
    if let Some(b) = map_get_bool(m, "print_background") {
        o.print_background = b;
    }
    if let Some(s) = map_get_float(m, "scale") {
        if s <= 0.0 {
            return Err(type_err(span, "pdf scale must be > 0"));
        }
        o.scale = s;
    }
    o.paper_width = map_get_float(m, "paper_width");
    o.paper_height = map_get_float(m, "paper_height");
    Ok(o)
}

// >>> nbrowser.executable_path()
// => "C:\\Program Files\\...\\msedge.exe"   # or nil
fn nbrowser_executable_path(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "nbrowser_executable_path", span)?;
    Ok(match executable_path() {
        Some(p) => Value::String(p).ref_cell(),
        None => Value::Nil.ref_cell(),
    })
}

// >>> let b = nbrowser.launch({headless: true})
// => 1
fn nbrowser_launch(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nbrowser_launch", span)?;
    let opts = opt_object(args, 0);
    let cfg = parse_launch(opts.as_ref(), span)?;
    map_res(span, launch(&cfg).map(|id| Value::Int(id).ref_cell()))
}

// >>> let b = nbrowser.connect({endpoint: "http://127.0.0.1:9222"})
// => 1
fn nbrowser_connect(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nbrowser_connect", span)?;
    let cfg = match &*args[0].borrow() {
        Value::Object(m) => parse_connect(m, span)?,
        Value::String(s) => ConnectConfig::new(s.clone()),
        other => {
            return Err(type_err(
                span,
                format!(
                    "connect() expects object or string endpoint, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    map_res(span, connect(&cfg).map(|id| Value::Int(id).ref_cell()))
}

// >>> nbrowser.close(b)
// => nil
fn nbrowser_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nbrowser_close", span)?;
    let id = int_arg(args, 0, "nbrowser_close", span)?;
    map_res(span, close(id).map(|_| Value::Nil.ref_cell()))
}

// >>> nbrowser.is_connected(b)
// => true
fn nbrowser_is_connected(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nbrowser_is_connected", span)?;
    let id = int_arg(args, 0, "nbrowser_is_connected", span)?;
    Ok(Value::Bool(is_connected(id)).ref_cell())
}

// >>> nbrowser.version(b)
// => "HeadlessChrome/..."
fn nbrowser_version(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nbrowser_version", span)?;
    let id = int_arg(args, 0, "nbrowser_version", span)?;
    map_res(span, version(id).map(|s| Value::String(s).ref_cell()))
}

// >>> let p = nbrowser.new_page(b, "about:blank")
// => 2
fn nbrowser_new_page(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nbrowser_new_page", span)?;
    let bid = int_arg(args, 0, "nbrowser_new_page", span)?;
    let url = if args.len() > 1 {
        Some(string_arg(args, 1, "nbrowser_new_page", span)?)
    } else {
        None
    };
    map_res(
        span,
        new_page(bid, url.as_deref()).map(|id| Value::Int(id).ref_cell()),
    )
}

// >>> nbrowser.pages(b)
// => [2]
fn nbrowser_pages(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nbrowser_pages", span)?;
    let bid = int_arg(args, 0, "nbrowser_pages", span)?;
    map_res(
        span,
        pages(bid).map(|ids| {
            Value::Array(ids.into_iter().map(|i| Value::Int(i).ref_cell()).collect()).ref_cell()
        }),
    )
}

// >>> nbrowser.close_page(p)
// => nil
fn nbrowser_close_page(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nbrowser_close_page", span)?;
    let id = int_arg(args, 0, "nbrowser_close_page", span)?;
    map_res(span, close_page(id).map(|_| Value::Nil.ref_cell()))
}

// >>> nbrowser.goto(p, "https://example.com")
// => {url: "...", title: "Example Domain", ok: true}
fn nbrowser_goto(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nbrowser_goto", span)?;
    let pid = int_arg(args, 0, "nbrowser_goto", span)?;
    let u = string_arg(args, 1, "nbrowser_goto", span)?;
    let opts = parse_nav(opt_object(args, 2).as_ref(), span)?;
    map_res(
        span,
        goto(pid, &u, &opts).map(|r| {
            let mut o = HashMap::new();
            o.insert("url".into(), Value::String(r.url).ref_cell());
            o.insert("title".into(), Value::String(r.title).ref_cell());
            o.insert("ok".into(), Value::Bool(r.ok).ref_cell());
            Value::Object(o).ref_cell()
        }),
    )
}

// >>> nbrowser.reload(p)
// => nil
fn nbrowser_reload(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nbrowser_reload", span)?;
    let pid = int_arg(args, 0, "nbrowser_reload", span)?;
    map_res(span, reload(pid).map(|_| Value::Nil.ref_cell()))
}

// >>> nbrowser.url(p)
// => "about:blank"
fn nbrowser_url(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nbrowser_url", span)?;
    let pid = int_arg(args, 0, "nbrowser_url", span)?;
    map_res(span, url(pid).map(|s| Value::String(s).ref_cell()))
}

// >>> nbrowser.title(p)
// => ""
fn nbrowser_title(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nbrowser_title", span)?;
    let pid = int_arg(args, 0, "nbrowser_title", span)?;
    map_res(span, title(pid).map(|s| Value::String(s).ref_cell()))
}

// >>> nbrowser.content(p)
// => "<html>...</html>"
fn nbrowser_content(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nbrowser_content", span)?;
    let pid = int_arg(args, 0, "nbrowser_content", span)?;
    map_res(span, content(pid).map(|s| Value::String(s).ref_cell()))
}

// >>> nbrowser.eval(p, "1 + 2")
// => 3
fn nbrowser_eval(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nbrowser_eval", span)?;
    let pid = int_arg(args, 0, "nbrowser_eval", span)?;
    let expr = string_arg(args, 1, "nbrowser_eval", span)?;
    map_res(
        span,
        eval(pid, &expr).map(|j| json_to_niao(&j).ref_cell()),
    )
}

// >>> nbrowser.click(p, "#btn")
// => nil
fn nbrowser_click(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nbrowser_click", span)?;
    let pid = int_arg(args, 0, "nbrowser_click", span)?;
    let sel = string_arg(args, 1, "nbrowser_click", span)?;
    map_res(span, click(pid, &sel).map(|_| Value::Nil.ref_cell()))
}

// >>> nbrowser.fill(p, "#q", "hello")
// => nil
fn nbrowser_fill(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nbrowser_fill", span)?;
    let pid = int_arg(args, 0, "nbrowser_fill", span)?;
    let sel = string_arg(args, 1, "nbrowser_fill", span)?;
    let text = string_arg(args, 2, "nbrowser_fill", span)?;
    map_res(span, fill(pid, &sel, &text).map(|_| Value::Nil.ref_cell()))
}

// >>> nbrowser.type(p, "#q", " more")
// => nil
fn nbrowser_type(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nbrowser_type", span)?;
    let pid = int_arg(args, 0, "nbrowser_type", span)?;
    let sel = string_arg(args, 1, "nbrowser_type", span)?;
    let text = string_arg(args, 2, "nbrowser_type", span)?;
    map_res(
        span,
        type_text(pid, &sel, &text).map(|_| Value::Nil.ref_cell()),
    )
}

// >>> nbrowser.press(p, "Enter")
// => nil
fn nbrowser_press(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nbrowser_press", span)?;
    let pid = int_arg(args, 0, "nbrowser_press", span)?;
    let key = string_arg(args, 1, "nbrowser_press", span)?;
    map_res(span, press(pid, &key).map(|_| Value::Nil.ref_cell()))
}

// >>> nbrowser.hover(p, "#link")
// => nil
fn nbrowser_hover(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nbrowser_hover", span)?;
    let pid = int_arg(args, 0, "nbrowser_hover", span)?;
    let sel = string_arg(args, 1, "nbrowser_hover", span)?;
    map_res(span, hover(pid, &sel).map(|_| Value::Nil.ref_cell()))
}

// >>> nbrowser.focus(p, "#q")
// => nil
fn nbrowser_focus(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nbrowser_focus", span)?;
    let pid = int_arg(args, 0, "nbrowser_focus", span)?;
    let sel = string_arg(args, 1, "nbrowser_focus", span)?;
    map_res(span, focus(pid, &sel).map(|_| Value::Nil.ref_cell()))
}

// >>> nbrowser.select(p, "#country", "us")
// => nil
fn nbrowser_select(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nbrowser_select", span)?;
    let pid = int_arg(args, 0, "nbrowser_select", span)?;
    let sel = string_arg(args, 1, "nbrowser_select", span)?;
    let val = string_arg(args, 2, "nbrowser_select", span)?;
    map_res(
        span,
        select_option(pid, &sel, &val).map(|_| Value::Nil.ref_cell()),
    )
}

// >>> nbrowser.check(p, "#agree")
// => nil
fn nbrowser_check(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nbrowser_check", span)?;
    let pid = int_arg(args, 0, "nbrowser_check", span)?;
    let sel = string_arg(args, 1, "nbrowser_check", span)?;
    map_res(span, check(pid, &sel).map(|_| Value::Nil.ref_cell()))
}

// >>> nbrowser.uncheck(p, "#agree")
// => nil
fn nbrowser_uncheck(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nbrowser_uncheck", span)?;
    let pid = int_arg(args, 0, "nbrowser_uncheck", span)?;
    let sel = string_arg(args, 1, "nbrowser_uncheck", span)?;
    map_res(span, uncheck(pid, &sel).map(|_| Value::Nil.ref_cell()))
}

// >>> nbrowser.wait_for(p, "#ready", {timeout_ms: 5000})
// => nil
fn nbrowser_wait_for(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nbrowser_wait_for", span)?;
    let pid = int_arg(args, 0, "nbrowser_wait_for", span)?;
    let sel = string_arg(args, 1, "nbrowser_wait_for", span)?;
    let mut timeout_ms = 30_000u64;
    if let Some(m) = opt_object(args, 2) {
        if let Some(t) = map_get_int(&m, "timeout_ms") {
            if t < 0 {
                return Err(type_err(span, "timeout_ms must be >= 0"));
            }
            timeout_ms = t as u64;
        }
    }
    map_res(
        span,
        wait_for(pid, &sel, timeout_ms).map(|_| Value::Nil.ref_cell()),
    )
}

// >>> nbrowser.text(p, "h1")
// => "Hello"
fn nbrowser_text(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nbrowser_text", span)?;
    let pid = int_arg(args, 0, "nbrowser_text", span)?;
    let sel = string_arg(args, 1, "nbrowser_text", span)?;
    map_res(
        span,
        text_content(pid, &sel).map(|s| Value::String(s).ref_cell()),
    )
}

// >>> nbrowser.attr(p, "a", "href")
// => "https://..."
fn nbrowser_attr(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "nbrowser_attr", span)?;
    let pid = int_arg(args, 0, "nbrowser_attr", span)?;
    let sel = string_arg(args, 1, "nbrowser_attr", span)?;
    let name = string_arg(args, 2, "nbrowser_attr", span)?;
    map_res(
        span,
        attr(pid, &sel, &name).map(|o| match o {
            Some(s) => Value::String(s).ref_cell(),
            None => Value::Nil.ref_cell(),
        }),
    )
}

// >>> nbrowser.exists(p, "#missing")
// => false
fn nbrowser_exists(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nbrowser_exists", span)?;
    let pid = int_arg(args, 0, "nbrowser_exists", span)?;
    let sel = string_arg(args, 1, "nbrowser_exists", span)?;
    map_res(span, exists(pid, &sel).map(|b| Value::Bool(b).ref_cell()))
}

// >>> nbrowser.count(p, "li")
// => 3
fn nbrowser_count(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nbrowser_count", span)?;
    let pid = int_arg(args, 0, "nbrowser_count", span)?;
    let sel = string_arg(args, 1, "nbrowser_count", span)?;
    map_res(span, count(pid, &sel).map(|n| Value::Int(n).ref_cell()))
}

// >>> nbrowser.screenshot(p, {full_page: true})
// => <bytes>
fn nbrowser_screenshot(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nbrowser_screenshot", span)?;
    let pid = int_arg(args, 0, "nbrowser_screenshot", span)?;
    let opts = parse_screenshot(opt_object(args, 1).as_ref(), span)?;
    map_res(
        span,
        screenshot(pid, &opts).map(|b| Value::ByteArray(b).ref_cell()),
    )
}

// >>> nbrowser.pdf(p)
// => <bytes>
fn nbrowser_pdf(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nbrowser_pdf", span)?;
    let pid = int_arg(args, 0, "nbrowser_pdf", span)?;
    let opts = parse_pdf(opt_object(args, 1).as_ref(), span)?;
    map_res(
        span,
        pdf(pid, &opts).map(|b| Value::ByteArray(b).ref_cell()),
    )
}

// >>> nbrowser.set_viewport(p, {width: 800, height: 600})
// => nil
fn nbrowser_set_viewport(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nbrowser_set_viewport", span)?;
    let pid = int_arg(args, 0, "nbrowser_set_viewport", span)?;
    let m = match &*args[1].borrow() {
        Value::Object(m) => m.clone(),
        other => {
            return Err(type_err(
                span,
                format!(
                    "set_viewport() expects object, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    let width = map_get_int(&m, "width").ok_or_else(|| type_err(span, "set_viewport() requires width"))?;
    let height =
        map_get_int(&m, "height").ok_or_else(|| type_err(span, "set_viewport() requires height"))?;
    if width <= 0 || height <= 0 {
        return Err(type_err(span, "set_viewport() width/height must be > 0"));
    }
    let mut vp = Viewport {
        width: width as u32,
        height: height as u32,
        ..Viewport::default()
    };
    if let Some(dsf) = map_get_float(&m, "device_scale_factor") {
        if dsf <= 0.0 {
            return Err(type_err(span, "device_scale_factor must be > 0"));
        }
        vp.device_scale_factor = dsf;
    }
    if let Some(b) = map_get_bool(&m, "mobile") {
        vp.mobile = b;
    }
    map_res(span, set_viewport(pid, &vp).map(|_| Value::Nil.ref_cell()))
}

// >>> nbrowser.set_headers(p, {"Accept-Language": "en"})
// => nil
fn nbrowser_set_headers(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nbrowser_set_headers", span)?;
    let pid = int_arg(args, 0, "nbrowser_set_headers", span)?;
    let m = match &*args[1].borrow() {
        Value::Object(m) => m.clone(),
        other => {
            return Err(type_err(
                span,
                format!("set_headers() expects object, got {}", other.type_name()),
            ));
        }
    };
    let mut headers = HashMap::new();
    for (k, v) in m {
        match &*v.borrow() {
            Value::String(s) => {
                headers.insert(k, s.clone());
            }
            other => {
                return Err(type_err(
                    span,
                    format!(
                        "set_headers() values must be strings, got {}",
                        other.type_name()
                    ),
                ));
            }
        }
    }
    map_res(
        span,
        set_extra_headers(pid, headers).map(|_| Value::Nil.ref_cell()),
    )
}

// >>> nbrowser.cookies(p)
// => [{name: "...", value: "...", ...}]
fn nbrowser_cookies(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nbrowser_cookies", span)?;
    let pid = int_arg(args, 0, "nbrowser_cookies", span)?;
    map_res(
        span,
        cookies(pid).map(|list| {
            Value::Array(
                list.into_iter()
                    .map(|c| {
                        let mut o = HashMap::new();
                        o.insert("name".into(), Value::String(c.name).ref_cell());
                        o.insert("value".into(), Value::String(c.value).ref_cell());
                        o.insert("domain".into(), Value::String(c.domain).ref_cell());
                        o.insert("path".into(), Value::String(c.path).ref_cell());
                        o.insert("secure".into(), Value::Bool(c.secure).ref_cell());
                        o.insert("http_only".into(), Value::Bool(c.http_only).ref_cell());
                        o.insert("expires".into(), Value::Float(c.expires).ref_cell());
                        Value::Object(o).ref_cell()
                    })
                    .collect(),
            )
            .ref_cell()
        }),
    )
}

// >>> nbrowser.set_cookie(p, {name: "a", value: "b", url: "https://example.com"})
// => nil
fn nbrowser_set_cookie(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nbrowser_set_cookie", span)?;
    let pid = int_arg(args, 0, "nbrowser_set_cookie", span)?;
    let m = match &*args[1].borrow() {
        Value::Object(m) => m.clone(),
        other => {
            return Err(type_err(
                span,
                format!("set_cookie() expects object, got {}", other.type_name()),
            ));
        }
    };
    let name = map_get_str(&m, "name").ok_or_else(|| type_err(span, "set_cookie() requires name"))?;
    let value =
        map_get_str(&m, "value").ok_or_else(|| type_err(span, "set_cookie() requires value"))?;
    let cookie = CookieInput {
        name,
        value,
        url: map_get_str(&m, "url"),
        domain: map_get_str(&m, "domain"),
        path: map_get_str(&m, "path"),
        secure: map_get_bool(&m, "secure").unwrap_or(false),
        http_only: map_get_bool(&m, "http_only").unwrap_or(false),
        expires: map_get_float(&m, "expires"),
    };
    map_res(
        span,
        set_cookie(pid, &cookie).map(|_| Value::Nil.ref_cell()),
    )
}

// >>> nbrowser.clear_cookies(p)
// => nil
fn nbrowser_clear_cookies(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nbrowser_clear_cookies", span)?;
    let pid = int_arg(args, 0, "nbrowser_clear_cookies", span)?;
    map_res(span, clear_cookies(pid).map(|_| Value::Nil.ref_cell()))
}

macro_rules! nbrowser_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nbrowser_fns![
    ("nbrowser_executable_path", "executable_path", nbrowser_executable_path),
    ("nbrowser_launch", "launch", nbrowser_launch),
    ("nbrowser_connect", "connect", nbrowser_connect),
    ("nbrowser_close", "close", nbrowser_close),
    ("nbrowser_is_connected", "is_connected", nbrowser_is_connected),
    ("nbrowser_version", "version", nbrowser_version),
    ("nbrowser_new_page", "new_page", nbrowser_new_page),
    ("nbrowser_pages", "pages", nbrowser_pages),
    ("nbrowser_close_page", "close_page", nbrowser_close_page),
    ("nbrowser_goto", "goto", nbrowser_goto),
    ("nbrowser_reload", "reload", nbrowser_reload),
    ("nbrowser_url", "url", nbrowser_url),
    ("nbrowser_title", "title", nbrowser_title),
    ("nbrowser_content", "content", nbrowser_content),
    ("nbrowser_eval", "eval", nbrowser_eval),
    ("nbrowser_click", "click", nbrowser_click),
    ("nbrowser_fill", "fill", nbrowser_fill),
    ("nbrowser_type", "type", nbrowser_type),
    ("nbrowser_press", "press", nbrowser_press),
    ("nbrowser_hover", "hover", nbrowser_hover),
    ("nbrowser_focus", "focus", nbrowser_focus),
    ("nbrowser_select", "select", nbrowser_select),
    ("nbrowser_check", "check", nbrowser_check),
    ("nbrowser_uncheck", "uncheck", nbrowser_uncheck),
    ("nbrowser_wait_for", "wait_for", nbrowser_wait_for),
    ("nbrowser_text", "text", nbrowser_text),
    ("nbrowser_attr", "attr", nbrowser_attr),
    ("nbrowser_exists", "exists", nbrowser_exists),
    ("nbrowser_count", "count", nbrowser_count),
    ("nbrowser_screenshot", "screenshot", nbrowser_screenshot),
    ("nbrowser_pdf", "pdf", nbrowser_pdf),
    ("nbrowser_set_viewport", "set_viewport", nbrowser_set_viewport),
    ("nbrowser_set_headers", "set_headers", nbrowser_set_headers),
    ("nbrowser_cookies", "cookies", nbrowser_cookies),
    ("nbrowser_set_cookie", "set_cookie", nbrowser_set_cookie),
    ("nbrowser_clear_cookies", "clear_cookies", nbrowser_clear_cookies),
];

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nbrowser";
pub const MODULE_PATHS: &[&str] = &["nbrowser", "std/nbrowser"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs()
        .into_iter()
        .map(|(flat, _, f)| (flat, f))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::dummy()
    }

    #[test]
    fn launch_arity_ok() {
        // May fail to find browser in CI; arity with wrong types still checked.
        let err = nbrowser_launch(
            &[Value::String("nope".into()).ref_cell()],
            span(),
        );
        // string opts is ignored as non-object → treated as no opts; may succeed or error.
        let _ = err;
    }

    #[test]
    fn connect_missing_endpoint() {
        let err = nbrowser_connect(&[Value::Object(HashMap::new()).ref_cell()], span()).unwrap_err();
        assert_eq!(err.code(), E4502_NBROWSER_TYPE);
    }

    #[test]
    fn goto_invalid_handle() {
        let v = nbrowser_goto(
            &[
                Value::Int(999_999).ref_cell(),
                Value::String("about:blank".into()).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn click_empty_selector() {
        let v = nbrowser_click(
            &[
                Value::Int(1).ref_cell(),
                Value::String("".into()).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        assert!(matches!(&*v.borrow(), Value::Error(_)));
    }

    #[test]
    fn is_connected_false() {
        let v = nbrowser_is_connected(&[Value::Int(999_999).ref_cell()], span()).unwrap();
        assert!(matches!(&*v.borrow(), Value::Bool(false)));
    }

    #[test]
    fn set_viewport_requires_dims() {
        let err = nbrowser_set_viewport(
            &[
                Value::Int(1).ref_cell(),
                Value::Object(HashMap::new()).ref_cell(),
            ],
            span(),
        )
        .unwrap_err();
        assert_eq!(err.code(), E4502_NBROWSER_TYPE);
    }
}
