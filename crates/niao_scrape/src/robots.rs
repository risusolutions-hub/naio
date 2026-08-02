//! robots.txt parser and allow-check (Google/Bing informal standard subset).

use crate::error::{check_len, ScrapeError, ScrapeResult};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
struct GroupRules {
    allows: Vec<String>,
    disallows: Vec<String>,
    crawl_delay: Option<f64>,
}

/// Parsed robots.txt with sitemaps and per-UA groups.
#[derive(Debug, Clone, Default)]
pub struct Robots {
    /// Lowercased user-agent → rules. `"*"` is the wildcard group.
    groups: HashMap<String, GroupRules>,
    /// Sitemap URLs listed in the file.
    pub sitemaps: Vec<String>,
    /// Raw source (for debugging / re-parse).
    pub source: String,
}

impl Robots {
    pub fn parse(text: &str) -> ScrapeResult<Self> {
        check_len(text.len())?;
        let mut robots = Robots {
            source: text.to_string(),
            ..Default::default()
        };

        let mut current_agents: Vec<String> = Vec::new();
        let mut pending_new_group = true;

        for raw in text.lines() {
            let line = strip_comment(raw).trim().to_string();
            if line.is_empty() {
                continue;
            }
            let Some((key, value)) = split_kv(&line) else {
                continue;
            };
            let key_l = key.to_ascii_lowercase();
            match key_l.as_str() {
                "user-agent" => {
                    let ua = value.trim().to_ascii_lowercase();
                    if ua.is_empty() {
                        continue;
                    }
                    if pending_new_group {
                        current_agents.clear();
                        pending_new_group = false;
                    }
                    current_agents.push(ua);
                }
                "allow" | "disallow" | "crawl-delay" => {
                    if current_agents.is_empty() {
                        current_agents.push("*".into());
                    }
                    for agent in &current_agents {
                        let g = robots.groups.entry(agent.clone()).or_default();
                        match key_l.as_str() {
                            "allow" => g.allows.push(normalize_path(value)),
                            "disallow" => g.disallows.push(normalize_path(value)),
                            "crawl-delay" => {
                                if let Ok(d) = value.trim().parse::<f64>() {
                                    if d >= 0.0 {
                                        g.crawl_delay = Some(d);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    pending_new_group = true;
                }
                "sitemap" => {
                    let u = value.trim();
                    if !u.is_empty() {
                        robots.sitemaps.push(u.to_string());
                    }
                    pending_new_group = true;
                }
                _ => {
                    pending_new_group = true;
                }
            }
        }
        Ok(robots)
    }

    /// Whether `user_agent` may fetch `url` (path+query).
    pub fn allowed(&self, url: &str, user_agent: &str) -> ScrapeResult<bool> {
        let path = url_path_query(url)?;
        let group = self.matching_group(user_agent);
        Ok(path_allowed(group, &path))
    }

    /// Crawl-delay in milliseconds for UA (0 if unset).
    pub fn crawl_delay_ms(&self, user_agent: &str) -> u64 {
        let group = self.matching_group(user_agent);
        group
            .crawl_delay
            .map(|s| (s * 1000.0).round().max(0.0) as u64)
            .unwrap_or(0)
    }

    fn matching_group(&self, user_agent: &str) -> &GroupRules {
        static EMPTY: GroupRules = GroupRules {
            allows: Vec::new(),
            disallows: Vec::new(),
            crawl_delay: None,
        };
        let ua = user_agent.to_ascii_lowercase();
        // Longest matching UA token (simple: exact group name is substring of UA,
        // prefer longest name; fall back to "*").
        let mut best: Option<(&str, &GroupRules)> = None;
        for (name, rules) in &self.groups {
            if name == "*" {
                continue;
            }
            if ua.contains(name.as_str()) {
                match best {
                    Some((bn, _)) if bn.len() >= name.len() => {}
                    _ => best = Some((name.as_str(), rules)),
                }
            }
        }
        if let Some((_, g)) = best {
            return g;
        }
        self.groups.get("*").unwrap_or(&EMPTY)
    }
}

fn strip_comment(line: &str) -> &str {
    match line.find('#') {
        Some(i) => &line[..i],
        None => line,
    }
}

fn split_kv(line: &str) -> Option<(&str, &str)> {
    let i = line.find(':')?;
    Some((&line[..i], &line[i + 1..]))
}

fn normalize_path(value: &str) -> String {
    let v = value.trim();
    if v.is_empty() {
        return String::new();
    }
    v.to_string()
}

fn url_path_query(url: &str) -> ScrapeResult<String> {
    // Accept bare paths too.
    if url.starts_with('/') {
        return Ok(url.to_string());
    }
    let u = niao_http::parse_url(url).map_err(ScrapeError::new)?;
    if u.query.is_empty() {
        Ok(u.path.clone())
    } else {
        Ok(format!("{}?{}", u.path, u.query))
    }
}

fn path_allowed(group: &GroupRules, path: &str) -> bool {
    // Empty disallow means allow all (common pattern: Disallow:)
    let mut best_allow: Option<usize> = None;
    let mut best_disallow: Option<usize> = None;

    for a in &group.allows {
        if a.is_empty() {
            continue;
        }
        if path_matches(a, path) {
            let len = a.len();
            if best_allow.map(|b| len > b).unwrap_or(true) {
                best_allow = Some(len);
            }
        }
    }
    for d in &group.disallows {
        if d.is_empty() {
            continue; // empty Disallow = allow all
        }
        if path_matches(d, path) {
            let len = d.len();
            if best_disallow.map(|b| len > b).unwrap_or(true) {
                best_disallow = Some(len);
            }
        }
    }

    match (best_allow, best_disallow) {
        (None, None) => true,
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (Some(a), Some(d)) => a >= d,
    }
}

/// Prefix match with optional trailing `*` wildcard (robots informal).
fn path_matches(pattern: &str, path: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        return path.starts_with(prefix);
    }
    if pattern.ends_with('$') {
        let p = &pattern[..pattern.len() - 1];
        return path == p;
    }
    path.starts_with(pattern)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::MAX_BYTES;

    const SAMPLE: &str = r#"
User-agent: *
Disallow: /private
Allow: /private/ok
Crawl-delay: 1.5

User-agent: BadBot
Disallow: /

Sitemap: https://ex.com/sitemap.xml
"#;

    #[test]
    fn parse_and_allow() {
        let r = Robots::parse(SAMPLE).unwrap();
        assert!(r.allowed("https://ex.com/", "nscrape").unwrap());
        assert!(!r.allowed("https://ex.com/private", "nscrape").unwrap());
        assert!(r.allowed("https://ex.com/private/ok", "nscrape").unwrap());
        assert!(!r.allowed("https://ex.com/", "BadBot/1.0").unwrap());
        assert_eq!(r.crawl_delay_ms("nscrape"), 1500);
        assert_eq!(r.sitemaps.len(), 1);
    }

    #[test]
    fn empty_disallow_allows_all() {
        let r = Robots::parse("User-agent: *\nDisallow:\n").unwrap();
        assert!(r.allowed("/anything", "*").unwrap());
    }

    #[test]
    fn oversize_rejected() {
        let big = "x".repeat(MAX_BYTES + 1);
        assert!(Robots::parse(&big).is_err());
    }
}
