//! Linkify bare URLs and emails (~bleach.linkify subset).

use crate::clean::{clean, CleanOpts};
use crate::error::SanitizeError;
use crate::escape::{escape_attr, escape_html};
use linkify::{LinkFinder, LinkKind};
use std::collections::HashSet;

/// Options for linkification.
#[derive(Debug, Clone)]
pub struct LinkifyOpts {
    pub parse_email: bool,
    pub new_tab: bool,
    pub nofollow: bool,
    pub skip_tags: HashSet<String>,
    pub sanitize_after: bool,
    pub clean_opts: CleanOpts,
}

impl Default for LinkifyOpts {
    fn default() -> Self {
        Self {
            parse_email: true,
            new_tab: true,
            nofollow: false,
            skip_tags: HashSet::new(),
            sanitize_after: true,
            clean_opts: CleanOpts::default(),
        }
    }
}

/// Turn bare URLs (and optionally emails) into `<a>` tags.
pub fn linkify(text: &str, opts: &LinkifyOpts) -> Result<String, SanitizeError> {
    if text.is_empty() {
        return Ok(String::new());
    }

    let mut finder = LinkFinder::new();
    finder.kinds(if opts.parse_email {
        &[LinkKind::Url, LinkKind::Email]
    } else {
        &[LinkKind::Url]
    });

    let links: Vec<_> = finder.links(text).collect();
    if links.is_empty() {
        let out = text.to_string();
        return if opts.sanitize_after {
            clean(&out, &opts.clean_opts)
        } else {
            Ok(out)
        };
    }

    let mut out = String::with_capacity(text.len() + links.len() * 32);
    let mut last = 0usize;
    for link in links {
        let start = link.start();
        let end = link.end();
        out.push_str(&text[last..start]);
        let url = &text[start..end];
        let href = if link.kind() == &LinkKind::Email && !url.contains("://") {
            format!("mailto:{url}")
        } else {
            url.to_string()
        };
        let mut rel_parts = Vec::new();
        if opts.nofollow {
            rel_parts.push("nofollow");
        }
        if opts.new_tab {
            rel_parts.push("noopener");
            rel_parts.push("noreferrer");
        }
        let rel_attr = if rel_parts.is_empty() {
            String::new()
        } else {
            format!(r#" rel="{}""#, rel_parts.join(" "))
        };
        let target_attr = if opts.new_tab {
            r#" target="_blank""#
        } else {
            ""
        };
        let href_esc = escape_attr(&href);
        let url_esc = escape_html(url);
        out.push_str(&format!(
            r#"<a href="{href_esc}"{target_attr}{rel_attr}>{url_esc}</a>"#
        ));
        last = end;
    }
    out.push_str(&text[last..]);

    if opts.sanitize_after {
        clean(&out, &opts.clean_opts)
    } else {
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linkifies_url() {
        let out = linkify("see https://example.com ok", &LinkifyOpts::default()).unwrap();
        assert!(out.contains(r#"<a href="https://example.com""#));
        assert!(out.contains("example.com</a>"));
    }

    #[test]
    fn linkifies_email() {
        let out = linkify("mail me@example.com", &LinkifyOpts::default()).unwrap();
        assert!(out.contains("mailto:me@example.com"));
    }
}
