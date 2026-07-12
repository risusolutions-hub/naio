//! Native nmarkdown standard library — lightweight Markdown to HTML,
//! plain-text stripping, and heading extraction. Std-only, line-based.
//!
//! Import with `import "nmarkdown"` (or `import "std/nmarkdown"`).

use crate::{NativeFn, NiaoResult, RuntimeError, Value, ValueRef};
use niao_ast::Span;
use std::collections::HashMap;
use std::rc::Rc;

// Wired into niao_errors::codes by central integration.
const E2860_NMARKDOWN_ARITY: u32 = 2860;
const E2862_NMARKDOWN_TYPE: u32 = 2862;

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E2860_NMARKDOWN_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn string_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(RuntimeError::at(
            span,
            E2862_NMARKDOWN_TYPE,
            format!(
                "{name}() expects a string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn str_val(s: String) -> NiaoResult<ValueRef> {
    Ok(Value::String(s).ref_cell())
}

// ---------------------------------------------------------------------------
// HTML escaping
// ---------------------------------------------------------------------------

fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn escape_attr(s: &str) -> String {
    escape_html(s)
}

// ---------------------------------------------------------------------------
// Inline formatting
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum InlineMode {
    Html,
    Strip,
}

fn parse_link(s: &str, start: usize) -> Option<(String, String, usize)> {
    let bytes = s.as_bytes();
    if bytes.get(start) != Some(&b'[') {
        return None;
    }
    let mut i = start + 1;
    let text_start = i;
    while i < bytes.len() && bytes[i] != b']' {
        i += 1;
    }
    if i >= bytes.len() || bytes.get(i) != Some(&b']') {
        return None;
    }
    let text = s[text_start..i].to_string();
    i += 1;
    if bytes.get(i) != Some(&b'(') {
        return None;
    }
    i += 1;
    let url_start = i;
    while i < bytes.len() && bytes[i] != b')' {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let url = s[url_start..i].to_string();
    Some((text, url, i + 1))
}

fn parse_delimited(s: &str, start: usize, marker: &str) -> Option<(String, usize)> {
    let end = s[start..].find(marker)?;
    let inner = s[start..start + end].to_string();
    Some((inner, start + end + marker.len()))
}

fn format_inline(s: &str, mode: InlineMode) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            if let Some((text, url, next)) = parse_link(s, i) {
                match mode {
                    InlineMode::Html => {
                        out.push_str("<a href=\"");
                        out.push_str(&escape_attr(&url));
                        out.push_str("\">");
                        out.push_str(&format_inline(&text, mode));
                        out.push_str("</a>");
                    }
                    InlineMode::Strip => out.push_str(&format_inline(&text, mode)),
                }
                i = next;
                continue;
            }
        }

        if i + 1 < bytes.len() && bytes[i] == b'*' && bytes[i + 1] == b'*' {
            if let Some((inner, next)) = parse_delimited(s, i + 2, "**") {
                match mode {
                    InlineMode::Html => {
                        out.push_str("<strong>");
                        out.push_str(&format_inline(&inner, mode));
                        out.push_str("</strong>");
                    }
                    InlineMode::Strip => out.push_str(&format_inline(&inner, mode)),
                }
                i = next;
                continue;
            }
        }

        if bytes[i] == b'*'
            && (i + 1 >= bytes.len() || bytes[i + 1] != b'*')
            && (i == 0 || bytes[i - 1] != b'*')
        {
            if let Some((inner, next)) = parse_delimited(s, i + 1, "*") {
                match mode {
                    InlineMode::Html => {
                        out.push_str("<em>");
                        out.push_str(&format_inline(&inner, mode));
                        out.push_str("</em>");
                    }
                    InlineMode::Strip => out.push_str(&format_inline(&inner, mode)),
                }
                i = next;
                continue;
            }
        }

        if bytes[i] == b'`' {
            if let Some((inner, next)) = parse_delimited(s, i + 1, "`") {
                match mode {
                    InlineMode::Html => {
                        out.push_str("<code>");
                        out.push_str(&escape_html(&inner));
                        out.push_str("</code>");
                    }
                    InlineMode::Strip => out.push_str(&inner),
                }
                i = next;
                continue;
            }
        }

        let ch = s[i..].chars().next().unwrap();
        match mode {
            InlineMode::Html => {
                let mut buf = [0u8; 4];
                let enc = ch.encode_utf8(&mut buf);
                out.push_str(&escape_html(enc));
            }
            InlineMode::Strip => out.push(ch),
        }
        i += ch.len_utf8();
    }
    out
}

// ---------------------------------------------------------------------------
// Block parsing helpers
// ---------------------------------------------------------------------------

fn heading_level(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_start();
    let leading = line.len() - trimmed.len();
    if leading > 3 || !trimmed.starts_with('#') {
        return None;
    }
    let level = trimmed.chars().take_while(|&c| c == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = &trimmed[level..];
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    Some((level, rest.trim()))
}

fn fence_open(line: &str) -> bool {
    line.trim_start().starts_with("```")
}

fn fence_close(line: &str) -> bool {
    line.trim_start().starts_with("```")
}

fn blockquote_text(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    trimmed.strip_prefix('>').map(|rest| rest.strip_prefix(' ').unwrap_or(rest).trim())
}

fn ul_item(line: &str) -> Option<&str> {
    line.trim_start().strip_prefix("- ")
}

fn ol_item(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let mut i = 0;
    let bytes = trimmed.as_bytes();
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 || i + 1 >= bytes.len() || bytes[i] != b'.' || bytes[i + 1] != b' ' {
        return None;
    }
    Some(trimmed[i + 2..].trim())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BlockKind {
    None,
    Paragraph,
    Ul,
    Ol,
    Blockquote,
}

struct HtmlWriter {
    out: String,
    block: BlockKind,
    in_code: bool,
}

impl HtmlWriter {
    fn new() -> Self {
        Self {
            out: String::new(),
            block: BlockKind::None,
            in_code: false,
        }
    }

    fn close_block(&mut self) {
        match self.block {
            BlockKind::Paragraph => self.out.push_str("</p>\n"),
            BlockKind::Ul => self.out.push_str("</ul>\n"),
            BlockKind::Ol => self.out.push_str("</ol>\n"),
            BlockKind::Blockquote => self.out.push_str("</p>\n</blockquote>\n"),
            BlockKind::None => {}
        }
        self.block = BlockKind::None;
    }

    fn open_paragraph(&mut self) {
        if self.block != BlockKind::Paragraph {
            self.close_block();
            self.out.push_str("<p>");
            self.block = BlockKind::Paragraph;
        } else {
            self.out.push_str("<br>\n");
        }
    }

    fn push_paragraph_line(&mut self, line: &str) {
        self.open_paragraph();
        self.out.push_str(&format_inline(line, InlineMode::Html));
    }

    fn push_heading(&mut self, level: usize, text: &str) {
        self.close_block();
        self.out.push_str(&format!("<h{level}>"));
        self.out.push_str(&format_inline(text, InlineMode::Html));
        self.out.push_str(&format!("</h{level}>\n"));
    }

    fn push_ul_item(&mut self, text: &str) {
        if self.block != BlockKind::Ul {
            self.close_block();
            self.out.push_str("<ul>\n");
            self.block = BlockKind::Ul;
        }
        self.out.push_str("<li>");
        self.out.push_str(&format_inline(text, InlineMode::Html));
        self.out.push_str("</li>\n");
    }

    fn push_ol_item(&mut self, text: &str) {
        if self.block != BlockKind::Ol {
            self.close_block();
            self.out.push_str("<ol>\n");
            self.block = BlockKind::Ol;
        }
        self.out.push_str("<li>");
        self.out.push_str(&format_inline(text, InlineMode::Html));
        self.out.push_str("</li>\n");
    }

    fn push_blockquote_line(&mut self, text: &str) {
        if self.block != BlockKind::Blockquote {
            self.close_block();
            self.out.push_str("<blockquote>\n<p>");
            self.block = BlockKind::Blockquote;
        } else {
            self.out.push_str("<br>\n");
        }
        self.out.push_str(&format_inline(text, InlineMode::Html));
    }

    fn close_blockquote(&mut self) {
        if self.block == BlockKind::Blockquote {
            self.close_block();
        }
    }

    fn open_code(&mut self) {
        self.close_block();
        self.out.push_str("<pre><code>");
        self.in_code = true;
    }

    fn close_code(&mut self) {
        if self.in_code {
            self.out.push_str("</code></pre>\n");
            self.in_code = false;
        }
    }

    fn push_code_line(&mut self, line: &str, first: bool) {
        if !first {
            self.out.push('\n');
        }
        self.out.push_str(&escape_html(line));
    }

    fn finish(mut self) -> String {
        self.close_block();
        self.close_code();
        self.out.trim_end().to_string()
    }
}

fn markdown_to_html_impl(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut w = HtmlWriter::new();
    let mut i = 0;
    let mut code_first = true;

    while i < lines.len() {
        let line = lines[i];

        if w.in_code {
            if fence_close(line) {
                w.close_code();
                code_first = true;
            } else {
                w.push_code_line(line, code_first);
                code_first = false;
            }
            i += 1;
            continue;
        }

        if fence_open(line) {
            w.open_code();
            i += 1;
            continue;
        }

        if line.trim().is_empty() {
            if w.block == BlockKind::Blockquote {
                w.close_blockquote();
            } else {
                w.close_block();
            }
            i += 1;
            continue;
        }

        if let Some((level, heading)) = heading_level(line) {
            w.push_heading(level, heading);
            i += 1;
            continue;
        }

        if let Some(text) = blockquote_text(line) {
            w.push_blockquote_line(text);
            i += 1;
            continue;
        }

        if let Some(item) = ul_item(line) {
            w.push_ul_item(item);
            i += 1;
            continue;
        }

        if let Some(item) = ol_item(line) {
            w.push_ol_item(item);
            i += 1;
            continue;
        }

        w.push_paragraph_line(line);
        i += 1;
    }

    w.finish()
}

fn markdown_strip_impl(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = String::new();
    let mut in_code = false;
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        if in_code {
            if fence_close(line) {
                in_code = false;
            } else {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(line);
            }
            i += 1;
            continue;
        }

        if fence_open(line) {
            in_code = true;
            i += 1;
            continue;
        }

        if line.trim().is_empty() {
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            i += 1;
            continue;
        }

        let plain = if let Some((_, heading)) = heading_level(line) {
            format_inline(heading, InlineMode::Strip)
        } else if let Some(text) = blockquote_text(line) {
            format_inline(text, InlineMode::Strip)
        } else if let Some(item) = ul_item(line) {
            format_inline(item, InlineMode::Strip)
        } else if let Some(item) = ol_item(line) {
            format_inline(item, InlineMode::Strip)
        } else {
            format_inline(line, InlineMode::Strip)
        };

        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&plain);
        i += 1;
    }

    out.trim_end().to_string()
}

#[derive(Clone)]
struct Heading {
    level: i64,
    text: String,
}

fn extract_headings(text: &str) -> Vec<Heading> {
    let mut headings = Vec::new();
    let mut in_code = false;
    for line in text.lines() {
        if in_code {
            if fence_close(line) {
                in_code = false;
            }
            continue;
        }
        if fence_open(line) {
            in_code = true;
            continue;
        }
        if let Some((level, raw)) = heading_level(line) {
            headings.push(Heading {
                level: level as i64,
                text: format_inline(raw, InlineMode::Strip),
            });
        }
    }
    headings
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn nmarkdown_to_html(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmarkdown_to_html", span)?;
    let text = string_arg(args, 0, "nmarkdown_to_html", span)?;
    str_val(markdown_to_html_impl(&text))
}

fn nmarkdown_strip(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmarkdown_strip", span)?;
    let text = string_arg(args, 0, "nmarkdown_strip", span)?;
    str_val(markdown_strip_impl(&text))
}

fn nmarkdown_headings(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "nmarkdown_headings", span)?;
    let text = string_arg(args, 0, "nmarkdown_headings", span)?;
    let items: Vec<ValueRef> = extract_headings(&text)
        .into_iter()
        .map(|h| {
            let mut obj = HashMap::new();
            obj.insert("level".to_string(), Value::Int(h.level).ref_cell());
            obj.insert("text".to_string(), Value::String(h.text).ref_cell());
            Value::Object(obj).ref_cell()
        })
        .collect();
    Ok(Value::Array(items).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! nmarkdown_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

nmarkdown_fns![
    ("nmarkdown_to_html", "to_html", nmarkdown_to_html),
    ("nmarkdown_strip", "strip", nmarkdown_strip),
    ("nmarkdown_headings", "headings", nmarkdown_headings),
];

fn all_builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs()
        .into_iter()
        .map(|(flat, _, f)| (flat, f))
        .collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "nmarkdown";
pub const MODULE_PATHS: &[&str] = &["nmarkdown", "std/nmarkdown"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_html_and_extract() {
        let md = "# Title\n\n## Sub **bold**\n\nNot a heading";
        let html = markdown_to_html_impl(md);
        assert!(html.contains("<h1>Title</h1>"));
        assert!(html.contains("<h2>Sub <strong>bold</strong></h2>"));

        let hs = extract_headings(md);
        assert_eq!(hs.len(), 2);
        assert_eq!(hs[0].level, 1);
        assert_eq!(hs[0].text, "Title");
        assert_eq!(hs[1].level, 2);
        assert_eq!(hs[1].text, "Sub bold");
    }

    #[test]
    fn bold_and_italic_html() {
        let html = markdown_to_html_impl("**bold** and *italic*");
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<em>italic</em>"));
    }

    #[test]
    fn links_html() {
        let html = markdown_to_html_impl("see [docs](https://niao.dev) now");
        assert!(html.contains("<a href=\"https://niao.dev\">docs</a>"));
        let stripped = markdown_strip_impl("[docs](https://niao.dev)");
        assert_eq!(stripped, "docs");
    }

    #[test]
    fn escapes_html_in_text() {
        let html = markdown_to_html_impl("<script>alert(1)</script>");
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>"));
    }

    #[test]
    fn fenced_code_block() {
        let md = "before\n\n```\n<tag>\n```\n\nafter";
        let html = markdown_to_html_impl(md);
        assert!(html.contains("<pre><code>&lt;tag&gt;</code></pre>"));
        assert!(markdown_strip_impl(md).contains("<tag>"));
    }

    #[test]
    fn lists_and_blockquote() {
        let md = "> quote\n- one\n1. first";
        let html = markdown_to_html_impl(md);
        assert!(html.contains("<blockquote>"));
        assert!(html.contains("<ul>"));
        assert!(html.contains("<ol>"));
    }
}
