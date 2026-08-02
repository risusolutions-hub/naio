mod error;
mod escape;
mod flags;
mod literal;
mod nfa;
mod parse;
mod vm;

pub use error::{Error, Result};
pub use escape::escape;

use nfa::{Compiler, Program};
use parse::{normalize_ast, parse, Ast};
use std::borrow::Cow;
use vm::{slots_to_ranges, VmResult};

/// Compiled regular expression (Thompson NFA + Pike VM).
#[derive(Debug, Clone)]
pub struct Regex {
    prog: Program,
    literal_prefix: Option<Vec<u32>>,
    num_groups: u32,
}

impl Regex {
    pub fn new(pattern: &str) -> Result<Self> {
        let (ast, flags) = parse(pattern)?;
        let ast = normalize_ast(ast);
        let num_groups = max_group_index(&ast);
        let literal_prefix = literal::extract_literal_prefix(&ast, flags);
        let prog = Compiler::new(flags, num_groups).compile(&ast);
        Ok(Self {
            prog,
            literal_prefix,
            num_groups,
        })
    }

    pub fn is_match(&self, text: &str) -> bool {
        self.find(text).is_some()
    }

    pub fn is_full_match(&self, text: &str) -> bool {
        self.find(text)
            .map(|m| m.start == 0 && m.end == text.len())
            .unwrap_or(false)
    }

    pub fn find<'h>(&self, text: &'h str) -> Option<Match<'h>> {
        self.find_at(text, 0)
    }

    pub fn find_at<'h>(&self, text: &'h str, from: usize) -> Option<Match<'h>> {
        if from == 0 {
            if let Some(ref prefix) = self.literal_prefix {
                if let Some(start) = literal::find_literal(text, prefix, self.prog.flags) {
                    if let Some(r) = crate::vm::search(&self.prog, text, start) {
                        return Some(match_from_result(text, r));
                    }
                }
                return None;
            }
            return crate::vm::find(&self.prog, text).map(|r| match_from_result(text, r));
        }
        if from >= text.len() {
            return crate::vm::search(&self.prog, text, from).map(|r| match_from_result(text, r));
        }
        crate::vm::find_from(&self.prog, text, from).map(|r| match_from_result(text, r))
    }

    pub fn find_iter<'h>(&'h self, text: &'h str) -> FindIter<'h> {
        FindIter {
            re: self,
            hay: text,
            at: 0,
        }
    }

    pub fn captures<'h>(&self, text: &'h str) -> Option<Captures<'h>> {
        self.find(text).map(|m| m.into_captures(text))
    }

    pub fn captures_iter<'h>(&'h self, text: &'h str) -> CapturesIter<'h> {
        CapturesIter {
            inner: self.find_iter(text),
            hay: text,
        }
    }

    pub fn capture_names(&self) -> CaptureNames {
        CaptureNames {
            remaining: self.num_groups as usize,
        }
    }

    pub fn replace_all<'h>(&self, hay: &'h str, rep: &str) -> Cow<'h, str> {
        let mut out = String::new();
        let mut last = 0;
        let mut any = false;
        for m in self.find_iter(hay) {
            any = true;
            out.push_str(&hay[last..m.start]);
            out.push_str(&expand_replacement(rep, hay, &m));
            last = m.end;
        }
        if !any {
            return Cow::Borrowed(hay);
        }
        out.push_str(&hay[last..]);
        Cow::Owned(out)
    }

    pub fn replacen<'h>(&self, hay: &'h str, n: usize, rep: &str) -> Cow<'h, str> {
        let mut out = String::new();
        let mut last = 0;
        let mut count = 0usize;
        for m in self.find_iter(hay) {
            if count >= n {
                break;
            }
            out.push_str(&hay[last..m.start]);
            out.push_str(&expand_replacement(rep, hay, &m));
            last = m.end;
            count += 1;
        }
        if count == 0 {
            return Cow::Borrowed(hay);
        }
        out.push_str(&hay[last..]);
        Cow::Owned(out)
    }

    pub fn split<'h>(&'h self, hay: &'h str) -> Split<'h> {
        Split {
            re: self,
            hay,
            at: 0,
            trailing_empty: true,
        }
    }
}

fn match_from_result<'h>(text: &'h str, r: VmResult) -> Match<'h> {
    let groups = slots_to_ranges(&r.slots);
    let (start, end) = groups.first().and_then(|g| *g).unwrap_or((0, 0));
    Match {
        text,
        start,
        end,
        groups,
    }
}

fn max_group_index(ast: &Ast) -> u32 {
    match ast {
        Ast::Cap { index, .. } => *index,
        Ast::Concat(v) | Ast::Alt(v) => v.iter().map(max_group_index).max().unwrap_or(0),
        Ast::Quant { inner, .. } => max_group_index(inner),
        Ast::NoCap(inner) => max_group_index(inner),
        _ => 0,
    }
}

fn expand_replacement(rep: &str, hay: &str, m: &Match<'_>) -> String {
    let mut out = String::with_capacity(rep.len());
    let bytes = rep.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            i += 1;
            if i >= bytes.len() {
                out.push('$');
                break;
            }
            if bytes[i] == b'$' {
                out.push('$');
                i += 1;
                continue;
            }
            if bytes[i] == b'&' || bytes[i] == b'0' {
                out.push_str(m.as_str());
                i += 1;
                continue;
            }
            if bytes[i].is_ascii_digit() {
                let mut n = (bytes[i] - b'0') as usize;
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    n = n * 10 + (bytes[i] - b'0') as usize;
                    i += 1;
                }
                if let Some(g) = m.groups.get(n).and_then(|x| *x) {
                    out.push_str(&hay[g.0..g.1]);
                }
                continue;
            }
            out.push('$');
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

#[derive(Debug, Clone)]
pub struct Match<'h> {
    text: &'h str,
    start: usize,
    end: usize,
    groups: Vec<Option<(usize, usize)>>,
}

impl<'h> Match<'h> {
    pub fn start(&self) -> usize {
        self.start
    }

    pub fn end(&self) -> usize {
        self.end
    }

    pub fn as_str(&self) -> &'h str {
        &self.text[self.start..self.end]
    }

    pub fn group_ranges(&self) -> &[Option<(usize, usize)>] {
        &self.groups
    }

    fn into_captures(self, text: &'h str) -> Captures<'h> {
        Captures {
            text,
            groups: self.groups,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Captures<'h> {
    text: &'h str,
    groups: Vec<Option<(usize, usize)>>,
}

impl<'h> Captures<'h> {
    pub fn get(&self, i: usize) -> Option<Match<'h>> {
        self.groups
            .get(i)
            .and_then(|g| *g)
            .map(|(start, end)| Match {
                text: self.text,
                start,
                end,
                groups: self.groups.clone(),
            })
    }

    pub fn iter(&self) -> impl Iterator<Item = Option<Match<'h>>> + '_ {
        (0..self.groups.len()).map(move |i| self.get(i))
    }
}

pub struct FindIter<'h> {
    re: &'h Regex,
    hay: &'h str,
    at: usize,
}

impl<'h> Iterator for FindIter<'h> {
    type Item = Match<'h>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.at > self.hay.len() {
            return None;
        }
        let slice = &self.hay[self.at..];
        let m = self.re.find_at(self.hay, self.at)?;
        let out = Match {
            text: self.hay,
            start: m.start,
            end: m.end,
            groups: m.group_ranges().to_vec(),
        };
        self.at = if m.start == m.end {
            if m.end >= self.hay.len() {
                return None;
            }
            m.end + 1
        } else {
            m.end
        };
        Some(out)
    }
}

pub struct CapturesIter<'h> {
    inner: FindIter<'h>,
    hay: &'h str,
}

impl<'h> Iterator for CapturesIter<'h> {
    type Item = Captures<'h>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|m| Captures {
            text: self.hay,
            groups: m.groups,
        })
    }
}

pub struct CaptureNames {
    remaining: usize,
}

impl Iterator for CaptureNames {
    type Item = Option<&'static str>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        Some(None)
    }
}

pub struct Split<'h> {
    re: &'h Regex,
    hay: &'h str,
    at: usize,
    trailing_empty: bool,
}

impl<'h> Iterator for Split<'h> {
    type Item = &'h str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.at > self.hay.len() {
            return None;
        }
        if let Some(m) = self.re.find_at(self.hay, self.at) {
            let start = m.start;
            let end = m.end;
            let piece = &self.hay[self.at..start];
            self.at = if end > self.at { end } else { self.at + 1 };
            Some(piece)
        } else if self.trailing_empty && self.at == self.hay.len() {
            self.trailing_empty = false;
            Some("")
        } else if self.at < self.hay.len() {
            let piece = &self.hay[self.at..];
            self.at = self.hay.len();
            self.trailing_empty = false;
            Some(piece)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests;
