//! BFS crawl and recursive sitemap URL collection.

use crate::bot::Bot;
use crate::error::ScrapeResult;
use crate::extract::{extract, ExtractOpts};
use crate::fetch::{get, FetchResponse};
use crate::sitemap::parse_sitemap;
use crate::urlutil::{canonicalize, same_host};
use std::collections::{HashSet, VecDeque};

#[derive(Debug, Clone)]
pub struct Page {
    pub url: String,
    pub status: u16,
    pub title: String,
    pub text: String,
    pub html: String,
    pub links: Vec<String>,
    pub depth: u32,
    pub robots_allowed: bool,
    pub elapsed_ms: u64,
}

#[derive(Debug)]
pub struct Crawl {
    pub bot: Bot,
    queue: VecDeque<(String, u32)>,
    visited: HashSet<String>,
    pub results: Vec<Page>,
    pub max_depth: u32,
    pub max_pages: u64,
    pub seed: String,
    pub done: bool,
}

impl Crawl {
    pub fn start(bot: Bot, start_url: &str, max_depth: u32) -> ScrapeResult<Self> {
        let seed = canonicalize(start_url).unwrap_or_else(|_| start_url.to_string());
        let max_pages = bot.max_pages;
        let mut queue = VecDeque::new();
        queue.push_back((seed.clone(), 0));
        Ok(Self {
            bot,
            queue,
            visited: HashSet::new(),
            results: Vec::new(),
            max_depth,
            max_pages,
            seed,
            done: false,
        })
    }

    /// Fetch next page; returns None when finished.
    pub fn next(&mut self) -> ScrapeResult<Option<Page>> {
        while let Some((url, depth)) = self.queue.pop_front() {
            let key = canonicalize(&url).unwrap_or_else(|_| url.clone());
            if self.visited.contains(&key) {
                continue;
            }
            if self.results.len() as u64 >= self.max_pages {
                self.done = true;
                return Ok(None);
            }
            self.visited.insert(key);

            let resp: FetchResponse = get(&mut self.bot, &url, &Default::default())?;
            if !resp.robots_allowed {
                let page = Page {
                    url: url.clone(),
                    status: 0,
                    title: String::new(),
                    text: String::new(),
                    html: String::new(),
                    links: Vec::new(),
                    depth,
                    robots_allowed: false,
                    elapsed_ms: 0,
                };
                self.results.push(page.clone());
                return Ok(Some(page));
            }

            let art =
                extract(&resp.body, Some(&resp.url), &ExtractOpts::default()).unwrap_or_default();
            let links = if depth < self.max_depth {
                page_links(&resp.body, &resp.url)
            } else {
                Vec::new()
            };

            if depth < self.max_depth {
                for link in &links {
                    if self.bot.same_host_only && !same_host(&self.seed, link).unwrap_or(false) {
                        continue;
                    }
                    let ck = canonicalize(link).unwrap_or_else(|_| link.clone());
                    if !self.visited.contains(&ck) {
                        self.queue.push_back((link.clone(), depth + 1));
                    }
                }
            }

            let page = Page {
                url: resp.url,
                status: resp.status,
                title: art.title,
                text: art.text,
                html: art.html,
                links,
                depth,
                robots_allowed: true,
                elapsed_ms: resp.elapsed_ms,
            };
            self.results.push(page.clone());
            return Ok(Some(page));
        }
        self.done = true;
        Ok(None)
    }

    pub fn pending(&self) -> usize {
        self.queue.len()
    }

    pub fn visited_count(&self) -> usize {
        self.visited.len()
    }
}

fn page_links(html: &str, base: &str) -> Vec<String> {
    crate::extract::extract_links(html, Some(base))
        .unwrap_or_default()
        .into_iter()
        .map(|l| l.href)
        .collect()
}

/// Recursively collect URLs from a sitemap (follows nested sitemap indexes).
pub fn crawl_sitemap(bot: &mut Bot, start: &str, max_sitemaps: usize) -> ScrapeResult<Vec<String>> {
    let mut queue = VecDeque::new();
    queue.push_back(start.to_string());
    let mut seen_sm = HashSet::new();
    let mut urls = Vec::new();
    let mut fetched_sm = 0usize;

    while let Some(sm_url) = queue.pop_front() {
        if !seen_sm.insert(sm_url.clone()) {
            continue;
        }
        if fetched_sm >= max_sitemaps {
            break;
        }
        fetched_sm += 1;
        let resp = get(bot, &sm_url, &Default::default())?;
        if !resp.ok {
            continue;
        }
        let doc = parse_sitemap(&resp.body)?;
        for u in doc.urls {
            if !u.loc.is_empty() {
                urls.push(u.loc);
            }
        }
        for nested in doc.sitemaps {
            if !seen_sm.contains(&nested) {
                queue.push_back(nested);
            }
        }
    }
    Ok(urls)
}
