//! Polite HTTP fetch using niao_req (retries) + robots/rate policy.

use crate::bot::Bot;
use crate::error::{ScrapeError, ScrapeResult};
use crate::robots::Robots;
use crate::urlutil::{host_of, origin, robots_url_for};
use niao_req::{execute, RequestOpts, Session};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct FetchResponse {
    pub status: u16,
    pub ok: bool,
    pub url: String,
    pub body: String,
    pub headers: HashMap<String, String>,
    pub elapsed_ms: u64,
    pub robots_allowed: bool,
}

/// Ensure robots for origin are loaded (fetch if missing).
pub fn ensure_robots(bot: &mut Bot, page_url: &str) -> ScrapeResult<()> {
    let orig = origin(page_url)?;
    if bot.robots_cache.contains_key(&orig) {
        return Ok(());
    }
    let ru = robots_url_for(page_url)?;
    let text = match raw_get(&ru, bot) {
        Ok(r) if r.status == 200 => r.body,
        _ => String::new(),
    };
    let parsed = Robots::parse(&text).unwrap_or_default();
    bot.robots_cache.insert(orig, parsed);
    Ok(())
}

/// Polite GET: rate-limit, robots check, retries via niao_req.
pub fn get(bot: &mut Bot, url: &str, extra: &RequestOpts) -> ScrapeResult<FetchResponse> {
    if bot.respect_robots {
        ensure_robots(bot, url)?;
        let allowed = bot
            .robots_cache
            .get(&origin(url)?)
            .map(|r| r.allowed(url, &bot.user_agent))
            .transpose()?
            .unwrap_or(true);
        if !allowed {
            return Ok(FetchResponse {
                status: 0,
                ok: false,
                url: url.to_string(),
                body: String::new(),
                headers: HashMap::new(),
                elapsed_ms: 0,
                robots_allowed: false,
            });
        }
        let delay = bot
            .robots_cache
            .get(&origin(url)?)
            .map(|r| r.crawl_delay_ms(&bot.user_agent))
            .unwrap_or(0)
            .max(bot.delay_ms);
        bot.limiter.delay_ms = delay;
    }

    let host = host_of(url).unwrap_or_default();
    bot.limiter.wait(&host);

    let mut session = session_from_bot(bot);
    let mut opts = extra.clone();
    if opts.timeout_ms.is_none() {
        opts.timeout_ms = Some(bot.timeout_ms);
    }
    if opts.retries.is_none() {
        opts.retries = Some(bot.retries);
    }
    if opts.backoff_ms.is_none() {
        opts.backoff_ms = Some(bot.backoff_ms);
    }
    if opts.max_redirects.is_none() {
        opts.max_redirects = Some(bot.max_redirects);
    }

    let resp =
        execute("GET", url, &mut session, &opts).map_err(|e| ScrapeError::new(e.to_string()))?;

    Ok(FetchResponse {
        status: resp.status,
        ok: resp.ok(),
        url: resp.url.clone(),
        body: resp.text(),
        headers: resp.headers,
        elapsed_ms: resp.elapsed_ms,
        robots_allowed: true,
    })
}

/// Fetch robots.txt body for a base/page URL.
pub fn fetch_robots_text(bot: &mut Bot, page_url: &str) -> ScrapeResult<String> {
    let ru = robots_url_for(page_url)?;
    let r = raw_get(&ru, bot)?;
    Ok(r.body)
}

fn session_from_bot(bot: &Bot) -> Session {
    let mut s = Session::new();
    s.user_agent = bot.user_agent.clone();
    s.timeout_ms = bot.timeout_ms;
    s.retries = bot.retries;
    s.backoff_ms = bot.backoff_ms;
    s.max_redirects = bot.max_redirects;
    s.headers = bot.headers.clone();
    if !s.headers.contains_key("User-Agent") && !s.headers.contains_key("user-agent") {
        s.headers
            .insert("User-Agent".into(), bot.user_agent.clone());
    }
    s
}

fn raw_get(url: &str, bot: &Bot) -> ScrapeResult<FetchResponse> {
    let mut session = session_from_bot(bot);
    let opts = RequestOpts {
        timeout_ms: Some(bot.timeout_ms),
        retries: Some(bot.retries.min(1)),
        backoff_ms: Some(bot.backoff_ms),
        max_redirects: Some(bot.max_redirects),
        ..Default::default()
    };
    let resp =
        execute("GET", url, &mut session, &opts).map_err(|e| ScrapeError::new(e.to_string()))?;
    Ok(FetchResponse {
        status: resp.status,
        ok: resp.ok(),
        url: resp.url.clone(),
        body: resp.text(),
        headers: resp.headers,
        elapsed_ms: resp.elapsed_ms,
        robots_allowed: true,
    })
}

/// One-shot GET without a persistent bot (uses defaults).
pub fn get_once(url: &str, opts: &RequestOpts) -> ScrapeResult<FetchResponse> {
    let mut bot = Bot::default();
    if let Some(ua) = opts
        .headers
        .get("User-Agent")
        .or_else(|| opts.headers.get("user-agent"))
        .cloned()
    {
        bot.user_agent = ua;
    }
    get(&mut bot, url, opts)
}
