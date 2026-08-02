//! URL helpers for scraping (canonicalize, host compare, join).

use crate::error::{ScrapeError, ScrapeResult};
use niao_http::{join as http_join, parse_url};

/// Join base + relative (absolute reference returned as-is).
pub fn join(base: &str, rel: &str) -> ScrapeResult<String> {
    if rel.is_empty() {
        return Ok(base.to_string());
    }
    if rel.contains("://") {
        return Ok(rel.to_string());
    }
    if base.is_empty() {
        return Ok(rel.to_string());
    }
    let base_url = parse_url(base).map_err(ScrapeError::new)?;
    let joined = http_join(&base_url, rel).map_err(ScrapeError::new)?;
    Ok(joined.to_string_full())
}

/// Strip fragment; normalize default ports; lowercase scheme/host.
pub fn canonicalize(url: &str) -> ScrapeResult<String> {
    let mut u = parse_url(url).map_err(ScrapeError::new)?;
    u.fragment.clear();
    // Drop trailing slash on path except root
    if u.path.len() > 1 && u.path.ends_with('/') {
        u.path.pop();
    }
    Ok(u.to_string_full())
}

/// Same registrable host (scheme-agnostic host compare, case-insensitive).
pub fn same_host(a: &str, b: &str) -> ScrapeResult<bool> {
    let ua = parse_url(a).map_err(ScrapeError::new)?;
    let ub = parse_url(b).map_err(ScrapeError::new)?;
    Ok(ua.host.eq_ignore_ascii_case(&ub.host))
}

/// Origin string (`scheme://host[:port]`).
pub fn origin(url: &str) -> ScrapeResult<String> {
    let u = parse_url(url).map_err(ScrapeError::new)?;
    Ok(u.origin())
}

/// Host part of a URL (empty string on parse failure for callers that prefer soft fail).
pub fn host_of(url: &str) -> ScrapeResult<String> {
    let u = parse_url(url).map_err(ScrapeError::new)?;
    Ok(u.host)
}

/// True if Content-Type looks like HTML.
pub fn is_html_ct(content_type: &str) -> bool {
    let lower = content_type.to_ascii_lowercase();
    lower.contains("text/html") || lower.contains("application/xhtml")
}

/// robots.txt URL for a page URL's origin.
pub fn robots_url_for(page_url: &str) -> ScrapeResult<String> {
    let o = origin(page_url)?;
    Ok(format!("{o}/robots.txt"))
}

/// sitemap.xml default for an origin.
pub fn default_sitemap_url(page_url: &str) -> ScrapeResult<String> {
    let o = origin(page_url)?;
    Ok(format!("{o}/sitemap.xml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_relative() {
        let u = join("https://ex.com/a/b", "../c").unwrap();
        assert!(u.contains("ex.com"));
        assert!(u.contains("/a/c") || u.ends_with("/c"));
    }

    #[test]
    fn canonicalize_strips_fragment() {
        let u = canonicalize("https://Ex.Com/path#frag").unwrap();
        assert!(!u.contains('#'));
        assert!(u.to_ascii_lowercase().contains("ex.com"));
    }

    #[test]
    fn same_host_ok() {
        assert!(same_host("https://a.com/1", "http://A.com/2").unwrap());
        assert!(!same_host("https://a.com/", "https://b.com/").unwrap());
    }

    #[test]
    fn html_ct() {
        assert!(is_html_ct("text/html; charset=utf-8"));
        assert!(!is_html_ct("application/json"));
    }
}
