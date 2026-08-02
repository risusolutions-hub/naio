//! Text normalization utilities.

/// Lowercase ASCII letters; other bytes unchanged (sklearn-compatible default).
#[inline]
pub fn lowercase(s: &str) -> String {
    s.chars().flat_map(|c| c.to_lowercase()).collect()
}

/// Strip combining marks (basic accent removal) via NFKD + drop nonspacing marks.
pub fn strip_accents(s: &str) -> String {
    let nfkd: String = s.nfd().collect();
    nfkd.chars()
        .filter(|c| !unicode_general_category::get_general_category(*c).is_mark())
        .collect()
}

/// Collapse runs of whitespace to a single space and trim ends.
pub fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = true;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

/// Remove punctuation (keep word chars and whitespace).
pub fn remove_punctuation(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect()
}

static CONTRACTIONS: &[(&str, &str)] = &[
    ("won't", "will not"),
    ("can't", "cannot"),
    ("n't", " not"),
    ("'re", " are"),
    ("'ve", " have"),
    ("'ll", " will"),
    ("'d", " would"),
    ("'m", " am"),
    ("it's", "it is"),
    ("that's", "that is"),
    ("what's", "what is"),
    ("who's", "who is"),
];

/// Expand common English contractions (case-insensitive input expected).
pub fn expand_contractions(s: &str) -> String {
    let lower = s.to_lowercase();
    let mut out = lower;
    for (pat, rep) in CONTRACTIONS {
        out = out.replace(pat, rep);
    }
    out
}

/// Mask URLs, emails, and numbers with placeholders.
pub fn mask_patterns(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if let Some(rest) = s[i..]
            .strip_prefix("http://")
            .or_else(|| s[i..].strip_prefix("https://"))
        {
            out.push_str("<URL>");
            i += s.len() - rest.len();
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            continue;
        }
        if bytes[i] == b'@'
            && i > 0
            && s[..i]
                .rfind(|c: char| c.is_whitespace())
                .map(|start| {
                    s[start + 1..i]
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '.' || c == '_' || c == '-')
                })
                .unwrap_or(false)
        {
            out.push_str("<EMAIL>");
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            continue;
        }
        if bytes[i].is_ascii_digit() {
            out.push_str("<NUM>");
            while i < bytes.len()
                && (bytes[i].is_ascii_digit() || bytes[i] == b'.' || bytes[i] == b',')
            {
                i += 1;
            }
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

#[derive(Debug, Clone, Default)]
pub struct NormalizeOptions {
    pub lowercase: bool,
    pub strip_accents: bool,
    pub remove_punct: bool,
    pub collapse_ws: bool,
    pub expand_contractions: bool,
    pub mask: bool,
}

impl NormalizeOptions {
    pub fn sklearn_default() -> Self {
        Self {
            lowercase: true,
            strip_accents: false,
            remove_punct: false,
            collapse_ws: true,
            expand_contractions: false,
            mask: false,
        }
    }
}

/// Apply normalization pipeline.
pub fn normalize(text: &str, opts: &NormalizeOptions) -> String {
    let mut s = text.to_string();
    if opts.expand_contractions {
        s = expand_contractions(&s);
    }
    if opts.lowercase {
        s = lowercase(&s);
    }
    if opts.strip_accents {
        s = strip_accents(&s);
    }
    if opts.mask {
        s = mask_patterns(&s);
    }
    if opts.remove_punct {
        s = remove_punctuation(&s);
    }
    if opts.collapse_ws {
        s = collapse_whitespace(&s);
    }
    s
}

// Minimal NFD without external unicode crate — hand-rolled for common Latin accents.
mod unicode_general_category {
    pub fn get_general_category(c: char) -> Category {
        if c.is_ascii() {
            return Category::Other;
        }
        // Combining marks in Latin-1 supplement / Latin Extended-A ranges used by strip_accents.
        match c {
            '\u{0300}'..='\u{036F}'
            | '\u{1AB0}'..='\u{1AFF}'
            | '\u{1DC0}'..='\u{1DFF}'
            | '\u{20D0}'..='\u{20FF}'
            | '\u{FE20}'..='\u{FE2F}' => Category::Mark,
            _ => Category::Other,
        }
    }

    #[derive(Copy, Clone, PartialEq, Eq)]
    pub enum Category {
        Mark,
        Other,
    }

    impl Category {
        pub fn is_mark(self) -> bool {
            matches!(self, Category::Mark)
        }
    }
}

trait Nfd {
    fn nfd(&self) -> NfdIter<'_>;
}

struct NfdIter<'a> {
    inner: std::str::Chars<'a>,
    pending: Option<char>,
}

impl<'a> NfdIter<'a> {
    fn new(s: &'a str) -> Self {
        Self {
            inner: s.chars(),
            pending: None,
        }
    }
}

impl Iterator for NfdIter<'_> {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(c) = self.pending.take() {
            return Some(c);
        }
        let c = self.inner.next()?;
        if let Some(decomposed) = decompose(c) {
            self.pending = Some(decomposed.1);
            Some(decomposed.0)
        } else {
            Some(c)
        }
    }
}

impl Nfd for str {
    fn nfd(&self) -> NfdIter<'_> {
        NfdIter::new(self)
    }
}

fn decompose(c: char) -> Option<(char, char)> {
    match c {
        'à' => Some(('a', '\u{0300}')),
        'á' => Some(('a', '\u{0301}')),
        'â' => Some(('a', '\u{0302}')),
        'ã' => Some(('a', '\u{0303}')),
        'ä' => Some(('a', '\u{0308}')),
        'å' => Some(('a', '\u{030A}')),
        'ç' => Some(('c', '\u{0327}')),
        'è' => Some(('e', '\u{0300}')),
        'é' => Some(('e', '\u{0301}')),
        'ê' => Some(('e', '\u{0302}')),
        'ë' => Some(('e', '\u{0308}')),
        'ì' => Some(('i', '\u{0300}')),
        'í' => Some(('i', '\u{0301}')),
        'î' => Some(('i', '\u{0302}')),
        'ï' => Some(('i', '\u{0308}')),
        'ñ' => Some(('n', '\u{0303}')),
        'ò' => Some(('o', '\u{0300}')),
        'ó' => Some(('o', '\u{0301}')),
        'ô' => Some(('o', '\u{0302}')),
        'õ' => Some(('o', '\u{0303}')),
        'ö' => Some(('o', '\u{0308}')),
        'ù' => Some(('u', '\u{0300}')),
        'ú' => Some(('u', '\u{0301}')),
        'û' => Some(('u', '\u{0302}')),
        'ü' => Some(('u', '\u{0308}')),
        'ý' => Some(('y', '\u{0301}')),
        'ÿ' => Some(('y', '\u{0308}')),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accent_strip() {
        assert_eq!(strip_accents("café"), "cafe");
        assert_eq!(strip_accents("naïve"), "naive");
    }

    #[test]
    fn mask_url_email_num() {
        let s = mask_patterns("see https://x.com and user@host.com plus 42 items");
        assert!(s.contains("<URL>"));
        assert!(s.contains("<EMAIL>"));
        assert!(s.contains("<NUM>"));
    }
}
