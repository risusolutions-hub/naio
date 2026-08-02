//! Polite scrape bot: robots + rate limit + retries policy.

use crate::rate::Limiter;
use crate::robots::Robots;
use std::collections::HashMap;

pub const DEFAULT_USER_AGENT: &str = "nscrape/0.1 (+https://niao.dev; polite; ~scrapy)";

#[derive(Debug, Clone)]
pub struct Bot {
    pub user_agent: String,
    pub delay_ms: u64,
    pub retries: u32,
    pub backoff_ms: u64,
    pub timeout_ms: u64,
    pub max_redirects: u8,
    pub respect_robots: bool,
    pub same_host_only: bool,
    pub max_pages: u64,
    pub headers: HashMap<String, String>,
    pub limiter: Limiter,
    /// Cached robots.txt per origin.
    pub robots_cache: HashMap<String, Robots>,
}

impl Default for Bot {
    fn default() -> Self {
        Self {
            user_agent: DEFAULT_USER_AGENT.into(),
            delay_ms: 500,
            retries: 2,
            backoff_ms: 200,
            timeout_ms: 30_000,
            max_redirects: 10,
            respect_robots: true,
            same_host_only: true,
            max_pages: 100,
            headers: HashMap::new(),
            limiter: Limiter::new(500),
            robots_cache: HashMap::new(),
        }
    }
}

impl Bot {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_delay(mut self, delay_ms: u64) -> Self {
        self.delay_ms = delay_ms;
        self.limiter.delay_ms = delay_ms;
        self
    }
}
