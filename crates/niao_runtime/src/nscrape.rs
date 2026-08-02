//! Native nscrape standard library — polite scraping: robots.txt, rate limits,
//! retries, sitemap crawl, article/readability extraction (~scrapy, trafilatura,
//! newspaper).
//!
//! Import with `import "nscrape"` (or `import "std/nscrape"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use niao_errors::codes;
use niao_parallel::available_threads;
use niao_req::RequestOpts;
use niao_scrape::{
    canonicalize, crawl_sitemap, extract, extract_links, extract_meta, extract_text, extract_title,
    fetch_robots_text, get, get_once, is_html_ct, join, origin, parallel_extract, parse_sitemap,
    same_host, sitemap_urls, Article, Bot, Crawl, ExtractOpts, FetchResponse, Limiter, Page, Robots,
    MAX_BYTES,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const E_ARITY: u32 = codes::E4480_NSCRAPE_ARITY;
const E_ERR: u32 = codes::E4481_NSCRAPE_ERROR;
const E_TYPE: u32 = codes::E4482_NSCRAPE_TYPE;
const E_HANDLE: u32 = codes::E4483_NSCRAPE_INVALID_HANDLE;
const E_ROBOTS: u32 = codes::E4484_NSCRAPE_ROBOTS;

enum Handle {
    Bot(Bot),
    Robots(Robots),
    Limiter(Limiter),
    Crawl(Box<Crawl>),
}

thread_local! {
    static HANDLES: RefCell<HashMap<i64, Handle>> = RefCell::new(HashMap::new());
    static NEXT_ID: RefCell<i64> = const { RefCell::new(1) };
}

fn new_id() -> i64 {
    NEXT_ID.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    })
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E_TYPE, msg.into())
}

fn scrape_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E_ERR, "nscrape_error", msg.into(), span)
}

fn invalid_handle(span: Span, id: i64) -> ValueRef {
    error_value(
        E_HANDLE,
        "nscrape_error",
        format!("invalid or closed nscrape handle {id}"),
        span,
    )
}

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(args: &[ValueRef], min: usize, max: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E_ARITY,
            format!("{name}() expects {min}..={max} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
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

fn handle_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(id) if *id > 0 => Ok(*id),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a positive handle as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn parse_opts(args: &[ValueRef], idx: usize, span: Span) -> NiaoResult<HashMap<String, ValueRef>> {
    if args.len() <= idx {
        return Ok(HashMap::new());
    }
    match &*args[idx].borrow() {
        Value::Object(map) => Ok(map.clone()),
        Value::Nil => Ok(HashMap::new()),
        other => Err(type_err(
            span,
            format!("opts must be an object, got {}", other.type_name()),
        )),
    }
}

fn obj_bool(map: &HashMap<String, ValueRef>, key: &str, default: bool) -> bool {
    map.get(key)
        .and_then(|v| match &*v.borrow() {
            Value::Bool(b) => Some(*b),
            Value::Int(n) => Some(*n != 0),
            _ => None,
        })
        .unwrap_or(default)
}

fn obj_string(map: &HashMap<String, ValueRef>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| match &*v.borrow() {
        Value::String(s) => Some(s.clone()),
        Value::Nil => None,
        _ => None,
    })
}

fn obj_int(map: &HashMap<String, ValueRef>, key: &str, default: i64) -> i64 {
    map.get(key)
        .and_then(|v| match &*v.borrow() {
            Value::Int(n) => Some(*n),
            _ => None,
        })
        .unwrap_or(default)
}

fn obj_headers(map: &HashMap<String, ValueRef>, span: Span) -> NiaoResult<HashMap<String, String>> {
    let Some(v) = map.get("headers") else {
        return Ok(HashMap::new());
    };
    match &*v.borrow() {
        Value::Object(m) => {
            let mut out = HashMap::new();
            for (k, vv) in m {
                match &*vv.borrow() {
                    Value::String(s) => {
                        out.insert(k.clone(), s.clone());
                    }
                    other => {
                        return Err(type_err(
                            span,
                            format!("headers.{k} must be string, got {}", other.type_name()),
                        ));
                    }
                }
            }
            Ok(out)
        }
        Value::Nil => Ok(HashMap::new()),
        other => Err(type_err(
            span,
            format!("headers must be an object, got {}", other.type_name()),
        )),
    }
}

fn bot_from_opts(map: &HashMap<String, ValueRef>, span: Span) -> NiaoResult<Bot> {
    let mut bot = Bot::default();
    if let Some(ua) = obj_string(map, "user_agent") {
        bot.user_agent = ua;
    }
    let delay = obj_int(map, "delay_ms", bot.delay_ms as i64);
    if delay < 0 {
        return Err(type_err(span, "delay_ms must be >= 0"));
    }
    bot.delay_ms = delay as u64;
    bot.limiter.delay_ms = bot.delay_ms;
    let retries = obj_int(map, "retries", bot.retries as i64);
    if retries < 0 {
        return Err(type_err(span, "retries must be >= 0"));
    }
    bot.retries = retries as u32;
    let backoff = obj_int(map, "backoff_ms", bot.backoff_ms as i64);
    if backoff < 0 {
        return Err(type_err(span, "backoff_ms must be >= 0"));
    }
    bot.backoff_ms = backoff as u64;
    let timeout = obj_int(map, "timeout_ms", bot.timeout_ms as i64);
    if timeout < 0 {
        return Err(type_err(span, "timeout_ms must be >= 0"));
    }
    bot.timeout_ms = timeout as u64;
    let redirs = obj_int(map, "max_redirects", bot.max_redirects as i64);
    if redirs < 0 || redirs > 255 {
        return Err(type_err(span, "max_redirects out of range"));
    }
    bot.max_redirects = redirs as u8;
    bot.respect_robots = obj_bool(map, "respect_robots", bot.respect_robots);
    bot.same_host_only = obj_bool(map, "same_host_only", bot.same_host_only);
    let max_pages = obj_int(map, "max_pages", bot.max_pages as i64);
    if max_pages < 0 {
        return Err(type_err(span, "max_pages must be >= 0"));
    }
    bot.max_pages = max_pages as u64;
    bot.headers = obj_headers(map, span)?;
    Ok(bot)
}

fn request_opts_from_map(map: &HashMap<String, ValueRef>, span: Span) -> NiaoResult<RequestOpts> {
    let mut opts = RequestOpts::default();
    let timeout = obj_int(map, "timeout_ms", -1);
    if timeout >= 0 {
        opts.timeout_ms = Some(timeout as u64);
    }
    let retries = obj_int(map, "retries", -1);
    if retries >= 0 {
        opts.retries = Some(retries as u32);
    }
    let backoff = obj_int(map, "backoff_ms", -1);
    if backoff >= 0 {
        opts.backoff_ms = Some(backoff as u64);
    }
    opts.headers = obj_headers(map, span)?;
    Ok(opts)
}

fn string_array(items: Vec<String>) -> ValueRef {
    let arr: Vec<ValueRef> = items
        .into_iter()
        .map(|s| Value::String(s).ref_cell())
        .collect();
    Value::Array(arr).ref_cell()
}

fn fetch_to_value(r: FetchResponse) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("status".into(), Value::Int(r.status as i64).ref_cell());
    map.insert("ok".into(), Value::Bool(r.ok).ref_cell());
    map.insert("url".into(), Value::String(r.url).ref_cell());
    map.insert("body".into(), Value::String(r.body).ref_cell());
    let mut headers = HashMap::new();
    for (k, v) in r.headers {
        headers.insert(k, Value::String(v).ref_cell());
    }
    map.insert("headers".into(), Value::Object(headers).ref_cell());
    map.insert(
        "elapsed_ms".into(),
        Value::Int(r.elapsed_ms as i64).ref_cell(),
    );
    map.insert(
        "robots_allowed".into(),
        Value::Bool(r.robots_allowed).ref_cell(),
    );
    Value::Object(map).ref_cell()
}

fn article_to_value(a: Article) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("title".into(), Value::String(a.title).ref_cell());
    map.insert("text".into(), Value::String(a.text).ref_cell());
    map.insert("html".into(), Value::String(a.html).ref_cell());
    map.insert("byline".into(), Value::String(a.byline).ref_cell());
    map.insert("excerpt".into(), Value::String(a.excerpt).ref_cell());
    map.insert("site_name".into(), Value::String(a.site_name).ref_cell());
    map.insert("lang".into(), Value::String(a.lang).ref_cell());
    map.insert("published".into(), Value::String(a.published).ref_cell());
    map.insert("top_image".into(), Value::String(a.top_image).ref_cell());
    map.insert("url".into(), Value::String(a.url).ref_cell());
    let mut meta = HashMap::new();
    for (k, v) in a.meta {
        meta.insert(k, Value::String(v).ref_cell());
    }
    map.insert("meta".into(), Value::Object(meta).ref_cell());
    Value::Object(map).ref_cell()
}

fn page_to_value(p: Page) -> ValueRef {
    let mut map = HashMap::new();
    map.insert("url".into(), Value::String(p.url).ref_cell());
    map.insert("status".into(), Value::Int(p.status as i64).ref_cell());
    map.insert("title".into(), Value::String(p.title).ref_cell());
    map.insert("text".into(), Value::String(p.text).ref_cell());
    map.insert("html".into(), Value::String(p.html).ref_cell());
    map.insert("links".into(), string_array(p.links));
    map.insert("depth".into(), Value::Int(p.depth as i64).ref_cell());
    map.insert(
        "robots_allowed".into(),
        Value::Bool(p.robots_allowed).ref_cell(),
    );
    map.insert(
        "elapsed_ms".into(),
        Value::Int(p.elapsed_ms as i64).ref_cell(),
    );
    Value::Object(map).ref_cell()
}

fn soft_robots_from_arg(
    args: &[ValueRef],
    idx: usize,
    name: &str,
    span: Span,
) -> NiaoResult<Result<Robots, ValueRef>> {
    match &*args[idx].borrow() {
        Value::String(s) => {
            if s.len() > MAX_BYTES {
                return Ok(Err(scrape_err(
                    span,
                    format!("input size {} exceeds limit {MAX_BYTES}", s.len()),
                )));
            }
            match Robots::parse(s) {
                Ok(r) => Ok(Ok(r)),
                Err(e) => Ok(Err(error_value(
                    E_ROBOTS,
                    "nscrape_error",
                    e.message().to_string(),
                    span,
                ))),
            }
        }
        Value::Int(id) if *id > 0 => HANDLES.with(|h| match h.borrow().get(id) {
            Some(Handle::Robots(r)) => Ok(Ok(r.clone())),
            Some(_) => Ok(Err(scrape_err(span, format!("{name}() handle is not robots")))),
            None => Ok(Err(invalid_handle(span, *id))),
        }),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects robots handle or text, got {}",
                other.type_name()
            ),
        )),
    }
}

fn check_str_len(s: &str, span: Span) -> Option<ValueRef> {
    if s.len() > MAX_BYTES {
        Some(scrape_err(
            span,
            format!("input size {} exceeds limit {MAX_BYTES}", s.len()),
        ))
    } else {
        None
    }
}

fn extract_opts_from_map(map: &HashMap<String, ValueRef>) -> (ExtractOpts, Option<String>) {
    let mut opts = ExtractOpts::default();
    let min_len = obj_int(map, "min_text_length", opts.min_text_length as i64);
    if min_len >= 0 {
        opts.min_text_length = min_len as usize;
    }
    let base = obj_string(map, "url").or_else(|| obj_string(map, "base"));
    (opts, base)
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

// >>> typeof(nscrape.bot()) == "int"
fn nscrape_bot(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nscrape_bot", span)?;
    let map = parse_opts(args, 0, span)?;
    let bot = bot_from_opts(&map, span)?;
    let id = new_id();
    HANDLES.with(|h| h.borrow_mut().insert(id, Handle::Bot(bot)));
    Ok(Value::Int(id).ref_cell())
}

// >>> nscrape.close(nscrape.bot()) == nil
fn nscrape_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nscrape_close", span)?;
    let id = handle_arg(args, 0, "nscrape_close", span)?;
    let removed = HANDLES.with(|h| h.borrow_mut().remove(&id).is_some());
    if !removed {
        return Ok(invalid_handle(span, id));
    }
    Ok(Value::Nil.ref_cell())
}

// >>> nscrape.bot_info(nscrape.bot()).delay_ms >= 0
fn nscrape_bot_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nscrape_bot_info", span)?;
    let id = handle_arg(args, 0, "nscrape_bot_info", span)?;
    HANDLES.with(|h| match h.borrow().get(&id) {
        Some(Handle::Bot(bot)) => {
            let mut map = HashMap::new();
            map.insert(
                "user_agent".into(),
                Value::String(bot.user_agent.clone()).ref_cell(),
            );
            map.insert(
                "delay_ms".into(),
                Value::Int(bot.delay_ms as i64).ref_cell(),
            );
            map.insert("retries".into(), Value::Int(bot.retries as i64).ref_cell());
            map.insert(
                "backoff_ms".into(),
                Value::Int(bot.backoff_ms as i64).ref_cell(),
            );
            map.insert(
                "timeout_ms".into(),
                Value::Int(bot.timeout_ms as i64).ref_cell(),
            );
            map.insert(
                "max_redirects".into(),
                Value::Int(bot.max_redirects as i64).ref_cell(),
            );
            map.insert(
                "respect_robots".into(),
                Value::Bool(bot.respect_robots).ref_cell(),
            );
            map.insert(
                "same_host_only".into(),
                Value::Bool(bot.same_host_only).ref_cell(),
            );
            map.insert(
                "max_pages".into(),
                Value::Int(bot.max_pages as i64).ref_cell(),
            );
            Ok(Value::Object(map).ref_cell())
        }
        Some(_) => Ok(scrape_err(span, "handle is not a bot")),
        None => Ok(invalid_handle(span, id)),
    })
}

// >>> typeof(nscrape.parse_robots("User-agent: *\nDisallow:\n")) == "int"
fn nscrape_parse_robots(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nscrape_parse_robots", span)?;
    let text = string_arg(args, 0, "nscrape_parse_robots", span)?;
    if let Some(e) = check_str_len(&text, span) {
        return Ok(e);
    }
    match Robots::parse(&text) {
        Ok(r) => {
            let id = new_id();
            HANDLES.with(|h| h.borrow_mut().insert(id, Handle::Robots(r)));
            Ok(Value::Int(id).ref_cell())
        }
        Err(e) => Ok(error_value(
            E_ROBOTS,
            "nscrape_error",
            e.message().to_string(),
            span,
        )),
    }
}

// >>> nscrape.allowed("User-agent: *\nDisallow: /private\n", "https://ex.com/")
fn nscrape_allowed(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 2, 3, "nscrape_allowed", span)?;
    let robots = match soft_robots_from_arg(args, 0, "nscrape_allowed", span)? {
        Ok(r) => r,
        Err(e) => return Ok(e),
    };
    let url = string_arg(args, 1, "nscrape_allowed", span)?;
    let ua = if args.len() >= 3 {
        string_arg(args, 2, "nscrape_allowed", span)?
    } else {
        niao_scrape::DEFAULT_USER_AGENT.to_string()
    };
    match robots.allowed(&url, &ua) {
        Ok(b) => Ok(Value::Bool(b).ref_cell()),
        Err(e) => Ok(scrape_err(span, e.message())),
    }
}

// >>> nscrape.crawl_delay("User-agent: *\nCrawl-delay: 1.5\n") == 1500
fn nscrape_crawl_delay(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nscrape_crawl_delay", span)?;
    let robots = match soft_robots_from_arg(args, 0, "nscrape_crawl_delay", span)? {
        Ok(r) => r,
        Err(e) => return Ok(e),
    };
    let ua = if args.len() >= 2 {
        string_arg(args, 1, "nscrape_crawl_delay", span)?
    } else {
        niao_scrape::DEFAULT_USER_AGENT.to_string()
    };
    Ok(Value::Int(robots.crawl_delay_ms(&ua) as i64).ref_cell())
}

// >>> len(nscrape.sitemaps("Sitemap: https://ex.com/s.xml\n")) == 1
fn nscrape_sitemaps(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nscrape_sitemaps", span)?;
    let robots = match soft_robots_from_arg(args, 0, "nscrape_sitemaps", span)? {
        Ok(r) => r,
        Err(e) => return Ok(e),
    };
    Ok(string_array(robots.sitemaps.clone()))
}

// >>> typeof(nscrape.limiter({delay_ms: 0})) == "int"
fn nscrape_limiter(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 0, 1, "nscrape_limiter", span)?;
    let map = parse_opts(args, 0, span)?;
    let delay = obj_int(&map, "delay_ms", 500);
    if delay < 0 {
        return Err(type_err(span, "delay_ms must be >= 0"));
    }
    let id = new_id();
    HANDLES.with(|h| {
        h.borrow_mut()
            .insert(id, Handle::Limiter(Limiter::new(delay as u64)))
    });
    Ok(Value::Int(id).ref_cell())
}

// >>> nscrape.wait(nscrape.limiter({delay_ms: 0}), "ex.com") == 0
fn nscrape_wait(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nscrape_wait", span)?;
    let id = handle_arg(args, 0, "nscrape_wait", span)?;
    let host = if args.len() >= 2 {
        string_arg(args, 1, "nscrape_wait", span)?
    } else {
        String::new()
    };
    HANDLES.with(|h| match h.borrow_mut().get_mut(&id) {
        Some(Handle::Limiter(lim)) => {
            let ms = lim.wait(&host);
            Ok(Value::Int(ms as i64).ref_cell())
        }
        Some(_) => Ok(scrape_err(span, "handle is not a limiter")),
        None => Ok(invalid_handle(span, id)),
    })
}

// >>> nscrape.limiter_info(nscrape.limiter()).delay_ms >= 0
fn nscrape_limiter_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nscrape_limiter_info", span)?;
    let id = handle_arg(args, 0, "nscrape_limiter_info", span)?;
    HANDLES.with(|h| match h.borrow().get(&id) {
        Some(Handle::Limiter(lim)) => {
            let info = lim.info();
            let mut map = HashMap::new();
            map.insert(
                "delay_ms".into(),
                Value::Int(info.delay_ms as i64).ref_cell(),
            );
            map.insert(
                "hosts_tracked".into(),
                Value::Int(info.hosts_tracked as i64).ref_cell(),
            );
            map.insert("waits".into(), Value::Int(info.waits as i64).ref_cell());
            map.insert(
                "total_wait_ms".into(),
                Value::Int(info.total_wait_ms as i64).ref_cell(),
            );
            Ok(Value::Object(map).ref_cell())
        }
        Some(_) => Ok(scrape_err(span, "handle is not a limiter")),
        None => Ok(invalid_handle(span, id)),
    })
}

fn bot_or_url_start(
    args: &[ValueRef],
    span: Span,
    name: &str,
) -> NiaoResult<(Option<i64>, String, usize)> {
    // Returns (optional bot handle, url, opts_index)
    match &*args[0].borrow() {
        Value::Int(id) if *id > 0 => {
            if args.len() < 2 {
                return Err(RuntimeError::at(
                    span,
                    E_ARITY,
                    format!("{name}() with bot handle requires a url"),
                ));
            }
            let url = string_arg(args, 1, name, span)?;
            Ok((Some(*id), url, 2))
        }
        Value::String(url) => Ok((None, url.clone(), 1)),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects url string or bot handle, got {}",
                other.type_name()
            ),
        )),
    }
}

// >>> typeof(nscrape.get) == "function"
fn nscrape_get(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nscrape_get", span)?;
    let (bot_id, url, opts_idx) = bot_or_url_start(args, span, "nscrape_get")?;
    let map = parse_opts(args, opts_idx, span)?;
    let mut req = request_opts_from_map(&map, span)?;
    let respect_override = map.get("respect_robots").map(|_| obj_bool(&map, "respect_robots", true));

    let result = if let Some(id) = bot_id {
        HANDLES.with(|h| match h.borrow_mut().get_mut(&id) {
            Some(Handle::Bot(bot)) => {
                if let Some(r) = respect_override {
                    bot.respect_robots = r;
                }
                get(bot, &url, &req)
            }
            Some(_) => Err(niao_scrape::ScrapeError::new("handle is not a bot")),
            None => Err(niao_scrape::ScrapeError::new(format!(
                "invalid or closed nscrape handle {id}"
            ))),
        })
    } else {
        // one-shot: optionally disable robots
        if let Some(false) = respect_override {
            let mut bot = Bot::default();
            bot.respect_robots = false;
            get(&mut bot, &url, &req)
        } else {
            let _ = &mut req;
            get_once(&url, &req)
        }
    };

    match result {
        Ok(r) => Ok(fetch_to_value(r)),
        Err(e) => {
            let msg = e.message().to_string();
            if msg.contains("invalid or closed") {
                Ok(invalid_handle(span, bot_id.unwrap_or(0)))
            } else {
                Ok(scrape_err(span, msg))
            }
        }
    }
}

// >>> typeof(nscrape.fetch_robots) == "function"
fn nscrape_fetch_robots(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nscrape_fetch_robots", span)?;
    let (bot_id, url, _) = bot_or_url_start(args, span, "nscrape_fetch_robots")?;
    let result = if let Some(id) = bot_id {
        HANDLES.with(|h| match h.borrow_mut().get_mut(&id) {
            Some(Handle::Bot(bot)) => fetch_robots_text(bot, &url),
            Some(_) => Err(niao_scrape::ScrapeError::new("handle is not a bot")),
            None => Err(niao_scrape::ScrapeError::new(format!(
                "invalid or closed nscrape handle {id}"
            ))),
        })
    } else {
        let mut bot = Bot::default();
        bot.respect_robots = false;
        fetch_robots_text(&mut bot, &url)
    };
    match result {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(scrape_err(span, e.message())),
    }
}

// >>> len(nscrape.parse_sitemap("<urlset><url><loc>https://ex.com/</loc></url></urlset>").urls) == 1
fn nscrape_parse_sitemap(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nscrape_parse_sitemap", span)?;
    let xml = string_arg(args, 0, "nscrape_parse_sitemap", span)?;
    if let Some(e) = check_str_len(&xml, span) {
        return Ok(e);
    }
    match parse_sitemap(&xml) {
        Ok(doc) => {
            let mut map = HashMap::new();
            let urls: Vec<ValueRef> = doc
                .urls
                .into_iter()
                .map(|u| {
                    let mut m = HashMap::new();
                    m.insert("loc".into(), Value::String(u.loc).ref_cell());
                    if let Some(v) = u.lastmod {
                        m.insert("lastmod".into(), Value::String(v).ref_cell());
                    }
                    if let Some(v) = u.changefreq {
                        m.insert("changefreq".into(), Value::String(v).ref_cell());
                    }
                    if let Some(v) = u.priority {
                        m.insert("priority".into(), Value::String(v).ref_cell());
                    }
                    Value::Object(m).ref_cell()
                })
                .collect();
            map.insert("urls".into(), Value::Array(urls).ref_cell());
            map.insert("sitemaps".into(), string_array(doc.sitemaps));
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(scrape_err(span, e.message())),
    }
}

// >>> len(nscrape.sitemap_urls("<urlset><url><loc>https://ex.com/a</loc></url></urlset>")) == 1
fn nscrape_sitemap_urls(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nscrape_sitemap_urls", span)?;
    let xml = string_arg(args, 0, "nscrape_sitemap_urls", span)?;
    if let Some(e) = check_str_len(&xml, span) {
        return Ok(e);
    }
    match sitemap_urls(&xml) {
        Ok(u) => Ok(string_array(u)),
        Err(e) => Ok(scrape_err(span, e.message())),
    }
}

// >>> typeof(nscrape.crawl_sitemap) == "function"
fn nscrape_crawl_sitemap(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nscrape_crawl_sitemap", span)?;
    let (bot_id, url, opts_idx) = bot_or_url_start(args, span, "nscrape_crawl_sitemap")?;
    let map = parse_opts(args, opts_idx, span)?;
    let max_sm = obj_int(&map, "max_sitemaps", 20);
    if max_sm < 0 {
        return Err(type_err(span, "max_sitemaps must be >= 0"));
    }
    let result = if let Some(id) = bot_id {
        HANDLES.with(|h| match h.borrow_mut().get_mut(&id) {
            Some(Handle::Bot(bot)) => crawl_sitemap(bot, &url, max_sm as usize),
            Some(_) => Err(niao_scrape::ScrapeError::new("handle is not a bot")),
            None => Err(niao_scrape::ScrapeError::new(format!(
                "invalid or closed nscrape handle {id}"
            ))),
        })
    } else {
        let mut bot = Bot::default();
        bot.respect_robots = false;
        crawl_sitemap(&mut bot, &url, max_sm as usize)
    };
    match result {
        Ok(u) => Ok(string_array(u)),
        Err(e) => Ok(scrape_err(span, e.message())),
    }
}

fn do_extract(args: &[ValueRef], span: Span, name: &str) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, name, span)?;
    let html = string_arg(args, 0, name, span)?;
    if let Some(e) = check_str_len(&html, span) {
        return Ok(e);
    }
    let map = parse_opts(args, 1, span)?;
    let (opts, base) = extract_opts_from_map(&map);
    match extract(&html, base.as_deref(), &opts) {
        Ok(a) => Ok(article_to_value(a)),
        Err(e) => Ok(scrape_err(span, e.message())),
    }
}

// >>> nscrape.extract("<article><p>Cats are wonderful companions for people everywhere.</p></article>").text.contains("Cats")
fn nscrape_extract(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    do_extract(args, span, "nscrape_extract")
}

// >>> nscrape.readable("<article><p>Cats are wonderful companions for people everywhere.</p></article>").title
fn nscrape_readable(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    do_extract(args, span, "nscrape_readable")
}

// >>> nscrape.extract_text("<p>Hello world from nscrape text extraction path.</p>").contains("Hello")
fn nscrape_extract_text(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nscrape_extract_text", span)?;
    let html = string_arg(args, 0, "nscrape_extract_text", span)?;
    if let Some(e) = check_str_len(&html, span) {
        return Ok(e);
    }
    match extract_text(&html) {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(scrape_err(span, e.message())),
    }
}

// >>> nscrape.extract_title("<title>Hi</title>")
// => "Hi"
fn nscrape_extract_title(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nscrape_extract_title", span)?;
    let html = string_arg(args, 0, "nscrape_extract_title", span)?;
    if let Some(e) = check_str_len(&html, span) {
        return Ok(e);
    }
    match extract_title(&html) {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(scrape_err(span, e.message())),
    }
}

// >>> len(nscrape.extract_links("<a href=\"/a\">A</a>", "https://ex.com/")) == 1
fn nscrape_extract_links(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nscrape_extract_links", span)?;
    let html = string_arg(args, 0, "nscrape_extract_links", span)?;
    if let Some(e) = check_str_len(&html, span) {
        return Ok(e);
    }
    let base = if args.len() >= 2 {
        Some(string_arg(args, 1, "nscrape_extract_links", span)?)
    } else {
        None
    };
    match extract_links(&html, base.as_deref()) {
        Ok(links) => {
            let arr: Vec<ValueRef> = links
                .into_iter()
                .map(|l| {
                    let mut m = HashMap::new();
                    m.insert("href".into(), Value::String(l.href).ref_cell());
                    m.insert("text".into(), Value::String(l.text).ref_cell());
                    Value::Object(m).ref_cell()
                })
                .collect();
            Ok(Value::Array(arr).ref_cell())
        }
        Err(e) => Ok(scrape_err(span, e.message())),
    }
}

// >>> nscrape.extract_meta("<meta name=\"author\" content=\"Ada\">").author
// => "Ada"
fn nscrape_extract_meta(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nscrape_extract_meta", span)?;
    let html = string_arg(args, 0, "nscrape_extract_meta", span)?;
    if let Some(e) = check_str_len(&html, span) {
        return Ok(e);
    }
    match extract_meta(&html) {
        Ok(meta) => {
            let mut map = HashMap::new();
            for (k, v) in meta {
                map.insert(k, Value::String(v).ref_cell());
            }
            Ok(Value::Object(map).ref_cell())
        }
        Err(e) => Ok(scrape_err(span, e.message())),
    }
}

// >>> len(nscrape.parallel_extract(["<p>One paragraph of extractable text content here.</p>", "<p>Two paragraph of extractable text content here.</p>"])) == 2
fn nscrape_parallel_extract(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "nscrape_parallel_extract", span)?;
    let htmls = match &*args[0].borrow() {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::String(s) => {
                        if s.len() > MAX_BYTES {
                            return Ok(scrape_err(
                                span,
                                format!("item {} size {} exceeds limit {MAX_BYTES}", i + 1, s.len()),
                            ));
                        }
                        out.push(s.clone());
                    }
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "nscrape_parallel_extract() expects string array; item {} is {}",
                                i + 1,
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            out
        }
        other => {
            return Err(type_err(
                span,
                format!(
                    "nscrape_parallel_extract() expects string array, got {}",
                    other.type_name()
                ),
            ));
        }
    };
    let map = parse_opts(args, 1, span)?;
    let (opts, base) = extract_opts_from_map(&map);
    let threads = obj_int(&map, "threads", available_threads() as i64);
    if threads <= 0 {
        return Err(type_err(span, "threads must be > 0"));
    }
    match parallel_extract(&htmls, base.as_deref(), &opts, threads as usize) {
        Ok(arts) => {
            let arr: Vec<ValueRef> = arts.into_iter().map(article_to_value).collect();
            Ok(Value::Array(arr).ref_cell())
        }
        Err(e) => Ok(scrape_err(span, e.message())),
    }
}

// >>> typeof(nscrape.crawl) == "function"
fn nscrape_crawl(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 3, "nscrape_crawl", span)?;
    let (bot_id, url, opts_idx) = bot_or_url_start(args, span, "nscrape_crawl")?;
    let map = parse_opts(args, opts_idx, span)?;
    let max_depth = obj_int(&map, "max_depth", 1);
    if max_depth < 0 {
        return Err(type_err(span, "max_depth must be >= 0"));
    }
    let max_pages_opt = obj_int(&map, "max_pages", -1);

    let bot = if let Some(id) = bot_id {
        HANDLES.with(|h| match h.borrow().get(&id) {
            Some(Handle::Bot(bot)) => {
                let mut bot = bot.clone();
                if max_pages_opt >= 0 {
                    bot.max_pages = max_pages_opt as u64;
                }
                Ok(bot)
            }
            Some(_) => Err(niao_scrape::ScrapeError::new("handle is not a bot")),
            None => Err(niao_scrape::ScrapeError::new(format!(
                "invalid or closed nscrape handle {id}"
            ))),
        })
    } else {
        let mut bot = bot_from_opts(&map, span)?;
        if max_pages_opt >= 0 {
            bot.max_pages = max_pages_opt as u64;
        }
        Ok(bot)
    };

    let bot = match bot {
        Ok(b) => b,
        Err(e) => return Ok(scrape_err(span, e.message())),
    };

    match Crawl::start(bot, &url, max_depth as u32) {
        Ok(crawl) => {
            let id = new_id();
            HANDLES.with(|h| h.borrow_mut().insert(id, Handle::Crawl(Box::new(crawl))));
            Ok(Value::Int(id).ref_cell())
        }
        Err(e) => Ok(scrape_err(span, e.message())),
    }
}

// >>> typeof(nscrape.next) == "function"
fn nscrape_next(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nscrape_next", span)?;
    let id = handle_arg(args, 0, "nscrape_next", span)?;
    HANDLES.with(|h| match h.borrow_mut().get_mut(&id) {
        Some(Handle::Crawl(crawl)) => match crawl.next() {
            Ok(Some(page)) => Ok(page_to_value(page)),
            Ok(None) => Ok(Value::Nil.ref_cell()),
            Err(e) => Ok(scrape_err(span, e.message())),
        },
        Some(_) => Ok(scrape_err(span, "handle is not a crawl")),
        None => Ok(invalid_handle(span, id)),
    })
}

// >>> typeof(nscrape.results) == "function"
fn nscrape_results(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nscrape_results", span)?;
    let id = handle_arg(args, 0, "nscrape_results", span)?;
    HANDLES.with(|h| match h.borrow().get(&id) {
        Some(Handle::Crawl(crawl)) => {
            let arr: Vec<ValueRef> = crawl.results.iter().cloned().map(page_to_value).collect();
            Ok(Value::Array(arr).ref_cell())
        }
        Some(_) => Ok(scrape_err(span, "handle is not a crawl")),
        None => Ok(invalid_handle(span, id)),
    })
}

// >>> typeof(nscrape.crawl_info) == "function"
fn nscrape_crawl_info(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nscrape_crawl_info", span)?;
    let id = handle_arg(args, 0, "nscrape_crawl_info", span)?;
    HANDLES.with(|h| match h.borrow().get(&id) {
        Some(Handle::Crawl(crawl)) => {
            let mut map = HashMap::new();
            map.insert("seed".into(), Value::String(crawl.seed.clone()).ref_cell());
            map.insert(
                "max_depth".into(),
                Value::Int(crawl.max_depth as i64).ref_cell(),
            );
            map.insert(
                "max_pages".into(),
                Value::Int(crawl.max_pages as i64).ref_cell(),
            );
            map.insert(
                "pages".into(),
                Value::Int(crawl.results.len() as i64).ref_cell(),
            );
            map.insert(
                "pending".into(),
                Value::Int(crawl.pending() as i64).ref_cell(),
            );
            map.insert(
                "visited".into(),
                Value::Int(crawl.visited_count() as i64).ref_cell(),
            );
            map.insert("done".into(), Value::Bool(crawl.done).ref_cell());
            Ok(Value::Object(map).ref_cell())
        }
        Some(_) => Ok(scrape_err(span, "handle is not a crawl")),
        None => Ok(invalid_handle(span, id)),
    })
}

// >>> nscrape.canonicalize("https://ex.com/a#frag").contains("ex.com")
fn nscrape_canonicalize(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nscrape_canonicalize", span)?;
    let url = string_arg(args, 0, "nscrape_canonicalize", span)?;
    match canonicalize(&url) {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(scrape_err(span, e.message())),
    }
}

// >>> nscrape.same_host("https://a.com/1", "http://A.com/2")
// => true
fn nscrape_same_host(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nscrape_same_host", span)?;
    let a = string_arg(args, 0, "nscrape_same_host", span)?;
    let b = string_arg(args, 1, "nscrape_same_host", span)?;
    match same_host(&a, &b) {
        Ok(v) => Ok(Value::Bool(v).ref_cell()),
        Err(e) => Ok(scrape_err(span, e.message())),
    }
}

// >>> nscrape.origin("https://ex.com/path")
// => "https://ex.com"
fn nscrape_origin(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nscrape_origin", span)?;
    let url = string_arg(args, 0, "nscrape_origin", span)?;
    match origin(&url) {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(scrape_err(span, e.message())),
    }
}

// >>> nscrape.join("https://ex.com/a/", "b").contains("/a/b")
fn nscrape_join(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "nscrape_join", span)?;
    let base = string_arg(args, 0, "nscrape_join", span)?;
    let rel = string_arg(args, 1, "nscrape_join", span)?;
    match join(&base, &rel) {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(scrape_err(span, e.message())),
    }
}

// >>> nscrape.is_html_ct("text/html; charset=utf-8")
// => true
fn nscrape_is_html_ct(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nscrape_is_html_ct", span)?;
    let ct = string_arg(args, 0, "nscrape_is_html_ct", span)?;
    Ok(Value::Bool(is_html_ct(&ct)).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nscrape_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nscrape_fns![
    ("nscrape_bot", "bot", nscrape_bot),
    ("nscrape_close", "close", nscrape_close),
    ("nscrape_bot_info", "bot_info", nscrape_bot_info),
    ("nscrape_parse_robots", "parse_robots", nscrape_parse_robots),
    ("nscrape_allowed", "allowed", nscrape_allowed),
    ("nscrape_crawl_delay", "crawl_delay", nscrape_crawl_delay),
    ("nscrape_sitemaps", "sitemaps", nscrape_sitemaps),
    ("nscrape_limiter", "limiter", nscrape_limiter),
    ("nscrape_wait", "wait", nscrape_wait),
    ("nscrape_limiter_info", "limiter_info", nscrape_limiter_info),
    ("nscrape_get", "get", nscrape_get),
    ("nscrape_fetch_robots", "fetch_robots", nscrape_fetch_robots),
    ("nscrape_parse_sitemap", "parse_sitemap", nscrape_parse_sitemap),
    ("nscrape_sitemap_urls", "sitemap_urls", nscrape_sitemap_urls),
    ("nscrape_crawl_sitemap", "crawl_sitemap", nscrape_crawl_sitemap),
    ("nscrape_extract", "extract", nscrape_extract),
    ("nscrape_extract_text", "extract_text", nscrape_extract_text),
    ("nscrape_extract_title", "extract_title", nscrape_extract_title),
    ("nscrape_extract_links", "extract_links", nscrape_extract_links),
    ("nscrape_extract_meta", "extract_meta", nscrape_extract_meta),
    ("nscrape_readable", "readable", nscrape_readable),
    ("nscrape_parallel_extract", "parallel_extract", nscrape_parallel_extract),
    ("nscrape_crawl", "crawl", nscrape_crawl),
    ("nscrape_next", "next", nscrape_next),
    ("nscrape_results", "results", nscrape_results),
    ("nscrape_crawl_info", "crawl_info", nscrape_crawl_info),
    ("nscrape_canonicalize", "canonicalize", nscrape_canonicalize),
    ("nscrape_same_host", "same_host", nscrape_same_host),
    ("nscrape_origin", "origin", nscrape_origin),
    ("nscrape_join", "join", nscrape_join),
    ("nscrape_is_html_ct", "is_html_ct", nscrape_is_html_ct),
];

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nscrape";
pub const MODULE_PATHS: &[&str] = &["nscrape", "std/nscrape"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs()
        .into_iter()
        .map(|(flat, _, f)| (flat, f))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_ast::Span;

    fn span() -> Span {
        Span::dummy()
    }

    #[test]
    fn extract_title_doctest() {
        let v = nscrape_extract_title(
            &[Value::String("<title>Hi</title>".into()).ref_cell()],
            span(),
        )
        .unwrap();
        assert_eq!(*v.borrow(), Value::String("Hi".into()));
    }

    #[test]
    fn allowed_doctest() {
        let robots = "User-agent: *\nDisallow: /private\n";
        let v = nscrape_allowed(
            &[
                Value::String(robots.into()).ref_cell(),
                Value::String("https://ex.com/".into()).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        assert_eq!(*v.borrow(), Value::Bool(true));
        let v2 = nscrape_allowed(
            &[
                Value::String(robots.into()).ref_cell(),
                Value::String("https://ex.com/private".into()).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        assert_eq!(*v2.borrow(), Value::Bool(false));
    }

    #[test]
    fn parse_sitemap_doctest() {
        let xml = "<urlset><url><loc>https://ex.com/</loc></url></urlset>";
        let v = nscrape_parse_sitemap(&[Value::String(xml.into()).ref_cell()], span()).unwrap();
        match &*v.borrow() {
            Value::Object(m) => match &*m.get("urls").unwrap().borrow() {
                Value::Array(a) => assert_eq!(a.len(), 1),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn is_html_ct_doctest() {
        let v = nscrape_is_html_ct(
            &[Value::String("text/html; charset=utf-8".into()).ref_cell()],
            span(),
        )
        .unwrap();
        assert_eq!(*v.borrow(), Value::Bool(true));
    }

    #[test]
    fn same_host_doctest() {
        let v = nscrape_same_host(
            &[
                Value::String("https://a.com/1".into()).ref_cell(),
                Value::String("http://A.com/2".into()).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        assert_eq!(*v.borrow(), Value::Bool(true));
    }

    #[test]
    fn join_doctest() {
        let v = nscrape_join(
            &[
                Value::String("https://ex.com/a/".into()).ref_cell(),
                Value::String("b".into()).ref_cell(),
            ],
            span(),
        )
        .unwrap();
        match &*v.borrow() {
            Value::String(s) => assert!(s.contains("/a/b")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn extract_article_doctest() {
        let html = r#"<article class="post-content"><p>Cats are wonderful companions for people everywhere today.</p></article>"#;
        let v = nscrape_extract(&[Value::String(html.into()).ref_cell()], span()).unwrap();
        match &*v.borrow() {
            Value::Object(m) => match &*m.get("text").unwrap().borrow() {
                Value::String(s) => assert!(s.contains("Cats")),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn invalid_handle_soft() {
        let v = nscrape_close(&[Value::Int(999999).ref_cell()], span()).unwrap();
        match &*v.borrow() {
            Value::Error(_) => {}
            other => panic!("expected error, got {other:?}"),
        }
    }
}
