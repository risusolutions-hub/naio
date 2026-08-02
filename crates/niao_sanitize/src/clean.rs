//! HTML cleaning via ammonia Builder with owned configuration.

use crate::error::SanitizeError;
use crate::policy::{default_protocols, default_tag_attributes, default_tags, RelativeUrlMode};
use ammonia::{Builder, UrlRelative};
use std::collections::{HashMap, HashSet};

/// Options for HTML sanitization (~bleach.clean).
#[derive(Debug, Clone)]
pub struct CleanOpts {
    pub tags: Option<HashSet<String>>,
    pub tag_attributes: Option<HashMap<String, HashSet<String>>>,
    pub generic_attributes: HashSet<String>,
    pub url_schemes: Option<HashSet<String>>,
    pub strip_comments: bool,
    pub link_rel: Option<String>,
    pub relative_urls: RelativeUrlMode,
    pub allowed_classes: HashMap<String, HashSet<String>>,
    pub clean_content_tags: HashSet<String>,
}

impl Default for CleanOpts {
    fn default() -> Self {
        Self {
            tags: None,
            tag_attributes: None,
            generic_attributes: HashSet::new(),
            url_schemes: None,
            strip_comments: true,
            link_rel: Some("noopener noreferrer".into()),
            relative_urls: RelativeUrlMode::PassThrough,
            allowed_classes: HashMap::new(),
            clean_content_tags: ["script", "style"].iter().map(|s| (*s).into()).collect(),
        }
    }
}

/// Reusable compiled sanitizer (owned policy).
#[derive(Debug, Clone)]
pub struct Sanitizer {
    opts: CleanOpts,
}

impl Sanitizer {
    pub fn new(opts: CleanOpts) -> Result<Self, SanitizeError> {
        validate_opts(&opts)?;
        Ok(Self { opts })
    }

    pub fn clean(&self, html: &str) -> String {
        run_clean(html, &self.opts)
    }
}

/// One-shot clean with defaults or custom opts.
pub fn clean(html: &str, opts: &CleanOpts) -> Result<String, SanitizeError> {
    validate_opts(opts)?;
    Ok(run_clean(html, opts))
}

/// True when input contains HTML markup.
pub fn is_html(s: &str) -> bool {
    ammonia::is_html(s)
}

/// Escape arbitrary text for safe HTML insertion (no tags).
pub fn clean_text(text: &str) -> String {
    ammonia::clean_text(text)
}

fn validate_opts(opts: &CleanOpts) -> Result<(), SanitizeError> {
    if let Some(tags) = &opts.tags {
        for t in &opts.clean_content_tags {
            if tags.contains(t) {
                return Err(SanitizeError::new(format!(
                    "tag '{t}' cannot be in both tags and clean_content_tags"
                )));
            }
        }
    }
    Ok(())
}

fn merged_tags(opts: &CleanOpts) -> HashSet<String> {
    opts.tags.clone().unwrap_or_else(default_tags)
}

fn merged_protocols(opts: &CleanOpts) -> HashSet<String> {
    opts.url_schemes.clone().unwrap_or_else(default_protocols)
}

fn merged_tag_attrs(opts: &CleanOpts) -> HashMap<String, HashSet<String>> {
    opts.tag_attributes
        .clone()
        .unwrap_or_else(default_tag_attributes)
}

pub(crate) fn run_clean(html: &str, opts: &CleanOpts) -> String {
    let tags_owned = merged_tags(opts);
    let schemes_owned = merged_protocols(opts);
    let tag_attrs_owned = merged_tag_attrs(opts);

    let tags: HashSet<&str> = tags_owned.iter().map(|s| s.as_str()).collect();
    let schemes: HashSet<&str> = schemes_owned.iter().map(|s| s.as_str()).collect();

    let mut tag_attributes: HashMap<&str, HashSet<&str>> = HashMap::new();
    for (tag, attrs) in &tag_attrs_owned {
        tag_attributes.insert(tag.as_str(), attrs.iter().map(|a| a.as_str()).collect());
    }

    let generic: HashSet<&str> = opts.generic_attributes.iter().map(|s| s.as_str()).collect();

    let mut allowed_classes: HashMap<&str, HashSet<&str>> = HashMap::new();
    for (tag, classes) in &opts.allowed_classes {
        allowed_classes.insert(tag.as_str(), classes.iter().map(|c| c.as_str()).collect());
    }

    let clean_content: HashSet<&str> = opts.clean_content_tags.iter().map(|s| s.as_str()).collect();

    let url_relative = match opts.relative_urls {
        RelativeUrlMode::PassThrough => UrlRelative::PassThrough,
        RelativeUrlMode::Drop => UrlRelative::Deny,
        RelativeUrlMode::Sanitize => UrlRelative::Deny,
    };

    let mut builder = Builder::default();
    builder
        .tags(tags)
        .tag_attributes(tag_attributes)
        .generic_attributes(generic)
        .url_schemes(schemes)
        .url_relative(url_relative)
        .strip_comments(opts.strip_comments)
        .clean_content_tags(clean_content);

    if !allowed_classes.is_empty() {
        builder.allowed_classes(allowed_classes);
    }

    if let Some(rel) = &opts.link_rel {
        if rel.is_empty() {
            builder.link_rel(None);
        } else {
            builder.link_rel(Some(rel.as_str()));
        }
    } else {
        builder.link_rel(None);
    }

    builder.clean(html).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_script() {
        let out = clean(
            "hello<script>alert(1)</script><b>world</b>",
            &CleanOpts::default(),
        )
        .unwrap();
        assert!(!out.contains("<script"));
        assert!(out.contains("<b>world</b>"));
    }

    #[test]
    fn blocks_onerror() {
        let out = clean(r#"<img src=x onerror=alert(1)>"#, &CleanOpts::default()).unwrap();
        assert!(!out.contains("onerror"));
    }

    #[test]
    fn blocks_javascript_href() {
        let out = clean(
            r#"<a href="javascript:alert(1)">x</a>"#,
            &CleanOpts::default(),
        )
        .unwrap();
        assert!(!out.contains("javascript:"));
    }
}
