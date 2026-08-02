//! Article / readability-style extraction (~newspaper / trafilatura subset).

use crate::error::{check_len, ScrapeResult};
use crate::urlutil::join;
use scraper::{Html, Selector};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ExtractOpts {
    pub min_text_length: usize,
}

impl Default for ExtractOpts {
    fn default() -> Self {
        Self {
            min_text_length: 25,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Article {
    pub title: String,
    pub text: String,
    pub html: String,
    pub byline: String,
    pub excerpt: String,
    pub site_name: String,
    pub lang: String,
    pub published: String,
    pub top_image: String,
    pub url: String,
    pub meta: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct LinkInfo {
    pub href: String,
    pub text: String,
}

/// Full article extraction (readability-ish scoring).
pub fn extract(html: &str, base_url: Option<&str>, opts: &ExtractOpts) -> ScrapeResult<Article> {
    check_len(html.len())?;
    let doc = Html::parse_document(html);
    let meta = extract_meta_map(&doc);
    let title = first_nonempty(&[
        meta.get("og:title").cloned().unwrap_or_default(),
        meta.get("twitter:title").cloned().unwrap_or_default(),
        meta.get("title").cloned().unwrap_or_default(),
        select_text(&doc, "title"),
        select_text(&doc, "h1"),
    ]);
    let byline = first_nonempty(&[
        meta.get("author").cloned().unwrap_or_default(),
        meta.get("article:author").cloned().unwrap_or_default(),
        meta.get("og:article:author").cloned().unwrap_or_default(),
        select_attr(&doc, "meta[name=\"author\"]", "content"),
        select_text(&doc, "[rel=\"author\"], .author, .byline"),
    ]);
    let site_name = first_nonempty(&[
        meta.get("og:site_name").cloned().unwrap_or_default(),
        meta.get("application-name").cloned().unwrap_or_default(),
    ]);
    let lang = first_nonempty(&[
        select_attr(&doc, "html", "lang"),
        meta.get("og:locale").cloned().unwrap_or_default(),
    ]);
    let published = first_nonempty(&[
        meta.get("article:published_time")
            .cloned()
            .unwrap_or_default(),
        meta.get("pubdate").cloned().unwrap_or_default(),
        meta.get("date").cloned().unwrap_or_default(),
        select_attr(&doc, "time[datetime]", "datetime"),
    ]);
    let top_image = first_nonempty(&[
        meta.get("og:image").cloned().unwrap_or_default(),
        meta.get("twitter:image").cloned().unwrap_or_default(),
    ]);
    let excerpt = first_nonempty(&[
        meta.get("description").cloned().unwrap_or_default(),
        meta.get("og:description").cloned().unwrap_or_default(),
        meta.get("twitter:description").cloned().unwrap_or_default(),
    ]);
    let url = first_nonempty(&[
        base_url.unwrap_or("").to_string(),
        meta.get("og:url").cloned().unwrap_or_default(),
        meta.get("canonical").cloned().unwrap_or_default(),
    ]);

    let (content_html, content_text) = pick_content(&doc, opts);
    let text = if content_text.len() >= opts.min_text_length {
        content_text
    } else {
        // Fallback: body text
        let body = select_text(&doc, "body");
        if body.len() > content_text.len() {
            clean_whitespace(&body)
        } else {
            content_text
        }
    };

    Ok(Article {
        title: clean_whitespace(&title),
        text,
        html: content_html,
        byline: clean_whitespace(&byline),
        excerpt: clean_whitespace(&excerpt),
        site_name: clean_whitespace(&site_name),
        lang: lang.trim().to_string(),
        published: published.trim().to_string(),
        top_image: top_image.trim().to_string(),
        url: url.trim().to_string(),
        meta,
    })
}

pub fn extract_text(html: &str) -> ScrapeResult<String> {
    let art = extract(html, None, &ExtractOpts::default())?;
    Ok(art.text)
}

pub fn extract_title(html: &str) -> ScrapeResult<String> {
    check_len(html.len())?;
    let doc = Html::parse_document(html);
    let meta = extract_meta_map(&doc);
    Ok(clean_whitespace(&first_nonempty(&[
        meta.get("og:title").cloned().unwrap_or_default(),
        select_text(&doc, "title"),
        select_text(&doc, "h1"),
    ])))
}

pub fn extract_meta(html: &str) -> ScrapeResult<HashMap<String, String>> {
    check_len(html.len())?;
    let doc = Html::parse_document(html);
    Ok(extract_meta_map(&doc))
}

pub fn extract_links(html: &str, base: Option<&str>) -> ScrapeResult<Vec<LinkInfo>> {
    check_len(html.len())?;
    let doc = Html::parse_document(html);
    let sel = Selector::parse("a[href]").unwrap();
    let mut out = Vec::new();
    for el in doc.select(&sel) {
        let href = el.value().attr("href").unwrap_or("").trim().to_string();
        if href.is_empty() || href.starts_with('#') || href.starts_with("javascript:") {
            continue;
        }
        let resolved = if let Some(b) = base {
            join(b, &href).unwrap_or(href.clone())
        } else {
            href.clone()
        };
        let text = clean_whitespace(&el.text().collect::<String>());
        out.push(LinkInfo {
            href: resolved,
            text,
        });
    }
    Ok(out)
}

/// Parallel batch extract.
pub fn parallel_extract(
    htmls: &[String],
    base_url: Option<&str>,
    opts: &ExtractOpts,
    threads: usize,
) -> ScrapeResult<Vec<Article>> {
    for (i, h) in htmls.iter().enumerate() {
        check_len(h.len()).map_err(|e| {
            crate::error::ScrapeError::new(format!("item {}: {}", i + 1, e.message()))
        })?;
    }
    let threads = threads.max(1);
    let opts = opts.clone();
    let base = base_url.map(|s| s.to_string());
    Ok(niao_parallel::map(htmls, threads, move |h| {
        extract(h, base.as_deref(), &opts).unwrap_or_default()
    }))
}

fn extract_meta_map(doc: &Html) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Ok(sel) = Selector::parse("meta") {
        for el in doc.select(&sel) {
            let content = el.value().attr("content").unwrap_or("").trim();
            if content.is_empty() {
                continue;
            }
            if let Some(name) = el
                .value()
                .attr("name")
                .or_else(|| el.value().attr("property"))
                .or_else(|| el.value().attr("itemprop"))
            {
                map.insert(name.trim().to_ascii_lowercase(), content.to_string());
            }
        }
    }
    if let Ok(sel) = Selector::parse("link[rel=\"canonical\"]") {
        if let Some(el) = doc.select(&sel).next() {
            if let Some(href) = el.value().attr("href") {
                map.insert("canonical".into(), href.trim().to_string());
            }
        }
    }
    if let Ok(sel) = Selector::parse("title") {
        if let Some(el) = doc.select(&sel).next() {
            map.insert(
                "title".into(),
                clean_whitespace(&el.text().collect::<String>()),
            );
        }
    }
    map
}

fn pick_content(doc: &Html, opts: &ExtractOpts) -> (String, String) {
    // Prefer semantic containers
    for sel_str in &[
        "article",
        "main",
        "[role=\"main\"]",
        "#content",
        ".post-content",
        ".article-body",
        ".entry-content",
    ] {
        if let Ok(sel) = Selector::parse(sel_str) {
            if let Some(el) = doc.select(&sel).next() {
                let text = clean_whitespace(&el.text().collect::<String>());
                if text.len() >= opts.min_text_length {
                    return (el.html(), text);
                }
            }
        }
    }

    // Score candidates among block elements
    let sel = match Selector::parse("p, div, section, article, td") {
        Ok(s) => s,
        Err(_) => return (String::new(), String::new()),
    };

    let mut best_score = 0i32;
    let mut best_html = String::new();
    let mut best_text = String::new();

    for el in doc.select(&sel) {
        let class_id = format!(
            "{} {}",
            el.value().attr("class").unwrap_or(""),
            el.value().attr("id").unwrap_or("")
        )
        .to_ascii_lowercase();
        if is_boilerplate(&class_id) {
            continue;
        }
        let text = clean_whitespace(&el.text().collect::<String>());
        if text.len() < opts.min_text_length {
            continue;
        }
        let mut score = text.len() as i32;
        // Density: punctuation / links penalty
        let commas = text.chars().filter(|c| *c == ',' || *c == '.').count() as i32;
        score += commas * 10;
        if class_id.contains("article")
            || class_id.contains("content")
            || class_id.contains("post")
            || class_id.contains("entry")
            || class_id.contains("story")
        {
            score += 50;
        }
        if class_id.contains("nav")
            || class_id.contains("footer")
            || class_id.contains("sidebar")
            || class_id.contains("comment")
            || class_id.contains("menu")
        {
            score -= 80;
        }
        // Link density penalty
        let link_text: usize = el
            .select(&Selector::parse("a").unwrap())
            .map(|a| a.text().collect::<String>().len())
            .sum();
        if text.len() > 0 {
            let density = link_text as f64 / text.len() as f64;
            if density > 0.4 {
                score -= 40;
            }
        }
        if score > best_score {
            best_score = score;
            best_html = el.html();
            best_text = text;
        }
    }
    (best_html, best_text)
}

fn is_boilerplate(class_id: &str) -> bool {
    for bad in &[
        "nav",
        "footer",
        "header",
        "sidebar",
        "menu",
        "cookie",
        "banner",
        "ads",
        "advert",
        "comment",
        "share",
        "social",
        "related",
        "recommend",
        "newsletter",
    ] {
        if class_id.contains(bad) {
            return true;
        }
    }
    false
}

fn select_text(doc: &Html, sel_str: &str) -> String {
    let Ok(sel) = Selector::parse(sel_str) else {
        return String::new();
    };
    doc.select(&sel)
        .next()
        .map(|el| el.text().collect::<String>())
        .unwrap_or_default()
}

fn select_attr(doc: &Html, sel_str: &str, attr: &str) -> String {
    let Ok(sel) = Selector::parse(sel_str) else {
        return String::new();
    };
    doc.select(&sel)
        .next()
        .and_then(|el| el.value().attr(attr).map(|s| s.to_string()))
        .unwrap_or_default()
}

fn first_nonempty(vals: &[String]) -> String {
    for v in vals {
        if !v.trim().is_empty() {
            return v.clone();
        }
    }
    String::new()
}

fn clean_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = true;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !prev_space && !out.is_empty() {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARTICLE: &str = r#"<!doctype html>
<html lang="en">
<head>
  <title>Hello World Article</title>
  <meta name="description" content="An excerpt about cats.">
  <meta property="og:title" content="Hello World Article">
  <meta property="og:site_name" content="Cat News">
  <meta name="author" content="Ada">
  <link rel="canonical" href="https://ex.com/hello">
</head>
<body>
  <nav class="nav"><a href="/">Home</a></nav>
  <article class="post-content">
    <h1>Hello World Article</h1>
    <p>Cats are wonderful companions. They purr, nap, and chase lasers with great enthusiasm every day.</p>
    <p>More detail about feline behavior appears in this second paragraph for density scoring.</p>
  </article>
  <footer class="footer">Copyright</footer>
</body>
</html>"#;

    #[test]
    fn extract_article() {
        let a = extract(
            ARTICLE,
            Some("https://ex.com/hello"),
            &ExtractOpts::default(),
        )
        .unwrap();
        assert!(a.title.contains("Hello"));
        assert!(a.text.contains("Cats are wonderful"));
        assert_eq!(a.byline, "Ada");
        assert_eq!(a.site_name, "Cat News");
        assert_eq!(a.lang, "en");
        assert!(a.excerpt.contains("cats"));
    }

    #[test]
    fn extract_links_resolves() {
        let links = extract_links(
            r##"<a href="/a">A</a><a href="https://x.com">X</a><a href="#skip">S</a>"##,
            Some("https://ex.com/"),
        )
        .unwrap();
        assert_eq!(links.len(), 2);
        assert!(links[0].href.contains("ex.com"));
    }

    #[test]
    fn title_only() {
        let t = extract_title("<html><head><title>T</title></head></html>").unwrap();
        assert_eq!(t, "T");
    }
}
