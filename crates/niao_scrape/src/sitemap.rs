//! Sitemap XML parse (urlset + sitemapindex).

use crate::error::{check_len, ScrapeError, ScrapeResult};
use quick_xml::events::Event;
use quick_xml::Reader;

#[derive(Debug, Clone, Default)]
pub struct SitemapUrl {
    pub loc: String,
    pub lastmod: Option<String>,
    pub changefreq: Option<String>,
    pub priority: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SitemapDoc {
    pub urls: Vec<SitemapUrl>,
    pub sitemaps: Vec<String>,
}

/// Parse a sitemap or sitemap index document.
pub fn parse_sitemap(xml: &str) -> ScrapeResult<SitemapDoc> {
    check_len(xml.len())?;
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut doc = SitemapDoc::default();
    let mut buf = Vec::new();
    let mut in_url = false;
    let mut in_sitemap = false;
    let mut cur = SitemapUrl::default();
    let mut cur_sitemap_loc = String::new();
    let mut current_tag = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_ascii_lowercase();
                match name.as_str() {
                    "url" => {
                        in_url = true;
                        cur = SitemapUrl::default();
                    }
                    "sitemap" => {
                        in_sitemap = true;
                        cur_sitemap_loc.clear();
                    }
                    "loc" | "lastmod" | "changefreq" | "priority" => {
                        current_tag = name;
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(t)) => {
                let text = t.unescape().unwrap_or_default().to_string();
                if text.is_empty() {
                    continue;
                }
                match current_tag.as_str() {
                    "loc" if in_url => cur.loc = text,
                    "loc" if in_sitemap => cur_sitemap_loc = text,
                    "lastmod" if in_url => cur.lastmod = Some(text),
                    "changefreq" if in_url => cur.changefreq = Some(text),
                    "priority" if in_url => cur.priority = Some(text),
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_ascii_lowercase();
                match name.as_str() {
                    "url" => {
                        if !cur.loc.is_empty() {
                            doc.urls.push(cur.clone());
                        }
                        in_url = false;
                    }
                    "sitemap" => {
                        if !cur_sitemap_loc.is_empty() {
                            doc.sitemaps.push(cur_sitemap_loc.clone());
                        }
                        in_sitemap = false;
                    }
                    "loc" | "lastmod" | "changefreq" | "priority" => {
                        current_tag.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(ScrapeError::new(format!("sitemap XML error: {e}")));
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(doc)
}

/// Convenience: all page URL locs from a sitemap (not nested index locs).
pub fn sitemap_urls(xml: &str) -> ScrapeResult<Vec<String>> {
    let doc = parse_sitemap(xml)?;
    Ok(doc.urls.into_iter().map(|u| u.loc).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_urlset() {
        let xml = r#"<?xml version="1.0"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>https://ex.com/a</loc><priority>0.8</priority></url>
  <url><loc>https://ex.com/b</loc><lastmod>2024-01-01</lastmod></url>
</urlset>"#;
        let doc = parse_sitemap(xml).unwrap();
        assert_eq!(doc.urls.len(), 2);
        assert_eq!(doc.urls[0].loc, "https://ex.com/a");
        assert_eq!(doc.urls[0].priority.as_deref(), Some("0.8"));
        assert_eq!(sitemap_urls(xml).unwrap().len(), 2);
    }

    #[test]
    fn parse_index() {
        let xml = r#"<?xml version="1.0"?>
<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <sitemap><loc>https://ex.com/sitemap-1.xml</loc></sitemap>
</sitemapindex>"#;
        let doc = parse_sitemap(xml).unwrap();
        assert_eq!(
            doc.sitemaps,
            vec!["https://ex.com/sitemap-1.xml".to_string()]
        );
        assert!(doc.urls.is_empty());
    }
}
