use crate::error::{Error, Result};
use crate::flags::Flags;

#[derive(Debug, Clone)]
pub enum Ast {
    Empty,
    Literal(u32),
    Dot,
    Class(Class),
    AnchorStart,
    AnchorEnd,
    WordBoundary(bool),
    Concat(Vec<Ast>),
    Alt(Vec<Ast>),
    Quant {
        inner: Box<Ast>,
        min: u32,
        max: Option<u32>,
        greedy: bool,
    },
    Cap {
        inner: Box<Ast>,
        index: u32,
    },
    NoCap(Box<Ast>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Class {
    pub negated: bool,
    pub ranges: Vec<(u32, u32)>,
}

impl Class {
    pub fn matches(&self, c: u32, flags: Flags) -> bool {
        let c = if flags.case_insensitive {
            fold_case(c)
        } else {
            c
        };
        let hit = self.ranges.iter().any(|&(lo, hi)| c >= lo && c <= hi);
        if self.negated {
            !hit
        } else {
            hit
        }
    }
}

#[inline]
pub fn fold_case(c: u32) -> u32 {
    if (c >= b'a' as u32 && c <= b'z' as u32) || (c >= 0xE0 && c <= 0xFF) {
        // ASCII lower + Latin-1 supplement rough fold for common letters
        char::from_u32(c)
            .map(|ch| ch.to_ascii_lowercase() as u32)
            .unwrap_or(c)
    } else if c >= b'A' as u32 && c <= b'Z' as u32 {
        c + 32
    } else {
        c
    }
}

pub fn parse(pattern: &str) -> Result<(Ast, Flags)> {
    let mut flags = Flags::default();
    let mut p = pattern;
    if p.starts_with("(?") {
        if let Some(end) = p.find(')') {
            let inner = &p[2..end];
            if !inner.contains(':') && !inner.contains('#') {
                let consumed = flags.apply_inline(inner);
                if consumed == inner.len() {
                    p = &p[end + 1..];
                }
            }
        }
    }
    let mut parser = Parser {
        input: p,
        pos: 0,
        flags,
        group_index: 1u32,
    };
    let ast = parser.parse_alt()?;
    parser.skip_ws();
    if !parser.at_end() {
        return Err(Error::new("unexpected trailing input", parser.pos));
    }
    Ok((ast, parser.flags))
}

struct Parser<'a> {
    input: &'a str,
    pos: usize,
    flags: Flags,
    group_index: u32,
}

impl<'a> Parser<'a> {
    fn at_end(&self) -> bool {
        self.pos >= self.input.len()
    }

    fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn skip_ws(&mut self) {}

    fn parse_alt(&mut self) -> Result<Ast> {
        let mut alts = vec![self.parse_concat()?];
        while self.peek() == Some('|') {
            self.bump();
            alts.push(self.parse_concat()?);
        }
        Ok(if alts.len() == 1 {
            alts.into_iter().next().unwrap()
        } else {
            Ast::Alt(alts)
        })
    }

    fn parse_concat(&mut self) -> Result<Ast> {
        let mut parts = Vec::new();
        loop {
            match self.peek() {
                None | Some('|') | Some(')') => break,
                _ => parts.push(self.parse_quant()?),
            }
        }
        Ok(if parts.is_empty() {
            Ast::Empty
        } else if parts.len() == 1 {
            parts.into_iter().next().unwrap()
        } else {
            Ast::Concat(parts)
        })
    }

    fn parse_quant(&mut self) -> Result<Ast> {
        let mut ast = self.parse_atom()?;
        loop {
            let greedy = self.peek() != Some('?');
            let (min, max) = match self.peek() {
                Some('*') => {
                    self.bump();
                    (0, None)
                }
                Some('+') => {
                    self.bump();
                    (1, None)
                }
                Some('?') => {
                    self.bump();
                    (0, Some(1))
                }
                Some('{') => {
                    self.bump();
                    let min = self.read_number()?;
                    let max = if self.peek() == Some(',') {
                        self.bump();
                        if self.peek() == Some('}') {
                            None
                        } else {
                            Some(self.read_number()?)
                        }
                    } else {
                        Some(min)
                    };
                    if self.peek() != Some('}') {
                        return Err(Error::new("expected '}'", self.pos));
                    }
                    self.bump();
                    (min, max)
                }
                _ => break,
            };
            if !greedy {
                self.bump(); // consume lazy '?'
            }
            ast = Ast::Quant {
                inner: Box::new(ast),
                min,
                max,
                greedy,
            };
        }
        Ok(ast)
    }

    fn read_number(&mut self) -> Result<u32> {
        let start = self.pos;
        let mut n: u32 = 0;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                n = n
                    .checked_mul(10)
                    .and_then(|x| x.checked_add(c as u32 - b'0' as u32))
                    .ok_or_else(|| Error::new("quantifier too large", self.pos))?;
                self.bump();
            } else {
                break;
            }
        }
        if self.pos == start {
            return Err(Error::new("expected number", self.pos));
        }
        Ok(n)
    }

    fn parse_atom(&mut self) -> Result<Ast> {
        match self.peek() {
            None => Err(Error::new("unexpected end of pattern", self.pos)),
            Some('(') => self.parse_group(),
            Some('[') => self.parse_class(),
            Some('.') => {
                self.bump();
                Ok(Ast::Dot)
            }
            Some('^') => {
                self.bump();
                Ok(Ast::AnchorStart)
            }
            Some('$') => {
                self.bump();
                Ok(Ast::AnchorEnd)
            }
            Some('\\') => self.parse_escape(),
            Some(c) if is_special(c) => Err(Error::new(format!("unexpected '{c}'"), self.pos)),
            Some(c) => {
                self.bump();
                Ok(Ast::Literal(c as u32))
            }
        }
    }

    fn parse_group(&mut self) -> Result<Ast> {
        self.bump(); // (
        if self.peek() == Some('?') {
            self.bump();
            match self.peek() {
                Some(':') => {
                    self.bump();
                    let inner = self.parse_alt()?;
                    if self.peek() != Some(')') {
                        return Err(Error::new("expected ')'", self.pos));
                    }
                    self.bump();
                    return Ok(Ast::NoCap(Box::new(inner)));
                }
                Some('i') | Some('m') | Some('s') | Some('u') | Some('U') | Some('-') => {
                    let saved = self.flags;
                    let inner_str = &self.input[self.pos..];
                    let consumed = self.flags.apply_inline(inner_str);
                    self.pos += consumed;
                    if self.peek() == Some(':') {
                        self.bump();
                        let inner = self.parse_alt()?;
                        if self.peek() != Some(')') {
                            return Err(Error::new("expected ')'", self.pos));
                        }
                        self.bump();
                        self.flags = saved;
                        return Ok(inner);
                    }
                    if self.peek() != Some(')') {
                        return Err(Error::new("expected ')' after inline flags", self.pos));
                    }
                    self.bump();
                    let inner = self.parse_alt()?;
                    if self.peek() != Some(')') {
                        return Err(Error::new("expected ')'", self.pos));
                    }
                    self.bump();
                    let out = inner;
                    self.flags = saved;
                    return Ok(out);
                }
                Some('#') => {
                    while self.peek().is_some() && self.peek() != Some(')') {
                        self.bump();
                    }
                    if self.peek() != Some(')') {
                        return Err(Error::new("unclosed comment group", self.pos));
                    }
                    self.bump();
                    return Ok(Ast::Empty);
                }
                _ => return Err(Error::new("invalid group", self.pos)),
            }
        }
        let idx = self.group_index;
        self.group_index += 1;
        let inner = self.parse_alt()?;
        if self.peek() != Some(')') {
            return Err(Error::new("expected ')'", self.pos));
        }
        self.bump();
        Ok(Ast::Cap {
            inner: Box::new(inner),
            index: idx,
        })
    }

    fn parse_class(&mut self) -> Result<Ast> {
        self.bump(); // [
        let mut class = Class {
            negated: false,
            ranges: Vec::new(),
        };
        if self.peek() == Some('^') {
            class.negated = true;
            self.bump();
        }
        if self.peek() == Some(']') {
            class.ranges.push((']' as u32, ']' as u32));
            self.bump();
        }
        while self.peek() != Some(']') {
            if self.peek().is_none() {
                return Err(Error::new("unclosed character class", self.pos));
            }
            let start = self.class_atom()?;
            if self.peek() == Some('-') && self.input[self.pos + 1..].chars().next() != Some(']') {
                self.bump();
                let end = self.class_atom()?;
                if start > end {
                    return Err(Error::new("invalid character range", self.pos));
                }
                class.ranges.push((start, end));
            } else {
                class.ranges.push((start, start));
            }
        }
        self.bump(); // ]
        Ok(Ast::Class(class))
    }

    fn class_atom(&mut self) -> Result<u32> {
        if self.peek() == Some('\\') {
            self.bump();
            return self.class_escape_atom();
        }
        let c = self
            .bump()
            .ok_or_else(|| Error::new("unexpected end in class", self.pos))?;
        Ok(c as u32)
    }

    fn class_escape_atom(&mut self) -> Result<u32> {
        match self.peek() {
            Some('d') => {
                self.bump();
                Ok(u32::MAX - 1)
            }
            Some('D') => {
                self.bump();
                Ok(u32::MAX - 2)
            }
            Some('w') => {
                self.bump();
                Ok(u32::MAX - 3)
            }
            Some('W') => {
                self.bump();
                Ok(u32::MAX - 4)
            }
            Some('s') => {
                self.bump();
                Ok(u32::MAX - 5)
            }
            Some('S') => {
                self.bump();
                Ok(u32::MAX - 6)
            }
            Some(c) => {
                self.bump();
                Ok(c as u32)
            }
            None => Err(Error::new("unexpected end after \\", self.pos)),
        }
    }

    fn parse_escape(&mut self) -> Result<Ast> {
        self.bump();
        match self.peek() {
            Some('d') => {
                self.bump();
                Ok(Ast::Class(Class {
                    negated: false,
                    ranges: digit_ranges(),
                }))
            }
            Some('D') => {
                self.bump();
                Ok(Ast::Class(Class {
                    negated: true,
                    ranges: digit_ranges(),
                }))
            }
            Some('w') => {
                self.bump();
                Ok(Ast::Class(Class {
                    negated: false,
                    ranges: word_ranges(),
                }))
            }
            Some('W') => {
                self.bump();
                Ok(Ast::Class(Class {
                    negated: true,
                    ranges: word_ranges(),
                }))
            }
            Some('s') => {
                self.bump();
                Ok(Ast::Class(Class {
                    negated: false,
                    ranges: space_ranges(),
                }))
            }
            Some('S') => {
                self.bump();
                Ok(Ast::Class(Class {
                    negated: true,
                    ranges: space_ranges(),
                }))
            }
            Some('b') => {
                self.bump();
                Ok(Ast::WordBoundary(true))
            }
            Some('B') => {
                self.bump();
                Ok(Ast::WordBoundary(false))
            }
            Some('n') => {
                self.bump();
                Ok(Ast::Literal(b'\n' as u32))
            }
            Some('t') => {
                self.bump();
                Ok(Ast::Literal(b'\t' as u32))
            }
            Some('r') => {
                self.bump();
                Ok(Ast::Literal(b'\r' as u32))
            }
            Some('f') => {
                self.bump();
                Ok(Ast::Literal(0x0C))
            }
            Some('0') => {
                self.bump();
                Ok(Ast::Literal(0))
            }
            Some('x') => {
                self.bump();
                let hi = self.hex_digit()?;
                let lo = self.hex_digit()?;
                Ok(Ast::Literal((hi * 16 + lo) as u32))
            }
            Some('u') => {
                self.bump();
                if self.peek() != Some('{') {
                    return Err(Error::new("expected '{'' after \\u", self.pos));
                }
                self.bump();
                let mut val: u32 = 0;
                while let Some(c) = self.peek() {
                    if c == '}' {
                        self.bump();
                        return Ok(Ast::Literal(val));
                    }
                    let d = c
                        .to_digit(16)
                        .ok_or_else(|| Error::new("invalid hex in \\u{}", self.pos))?;
                    val = val * 16 + d;
                    self.bump();
                }
                Err(Error::new("unclosed \\u{}", self.pos))
            }
            Some(c) => {
                self.bump();
                Ok(Ast::Literal(c as u32))
            }
            None => Err(Error::new("unexpected end after \\", self.pos)),
        }
    }

    fn hex_digit(&mut self) -> Result<u8> {
        let c = self
            .bump()
            .ok_or_else(|| Error::new("expected hex digit", self.pos))?;
        c.to_digit(16)
            .map(|d| d as u8)
            .ok_or_else(|| Error::new("expected hex digit", self.pos))
    }
}

fn is_special(c: char) -> bool {
    matches!(
        c,
        '*' | '+' | '?' | '{' | '}' | '|' | '(' | ')' | '[' | ']' | '\\'
    )
}

fn expand_class_into(class: &mut Class, token: u32) {
    match token {
        t if t == u32::MAX - 1 => class.ranges.extend(digit_ranges()),
        t if t == u32::MAX - 2 => {
            // \D inside class: add all non-digit single chars is expensive; use negated sub-match
            class.ranges.extend(digit_ranges());
            class.negated = !class.negated;
        }
        t if t == u32::MAX - 3 => class.ranges.extend(word_ranges()),
        t if t == u32::MAX - 4 => {
            class.ranges.extend(word_ranges());
            class.negated = !class.negated;
        }
        t if t == u32::MAX - 5 => class.ranges.extend(space_ranges()),
        t if t == u32::MAX - 6 => {
            class.ranges.extend(space_ranges());
            class.negated = !class.negated;
        }
        c => class.ranges.push((c, c)),
    }
}

fn digit_ranges() -> Vec<(u32, u32)> {
    vec![(b'0' as u32, b'9' as u32)]
}

fn word_ranges() -> Vec<(u32, u32)> {
    vec![
        (b'0' as u32, b'9' as u32),
        (b'A' as u32, b'Z' as u32),
        (b'a' as u32, b'z' as u32),
        (b'_' as u32, b'_' as u32),
    ]
}

fn space_ranges() -> Vec<(u32, u32)> {
    vec![
        (0x09, 0x0D),
        (0x20, 0x20),
        (0x85, 0x85),
        (0xA0, 0xA0),
        (0x1680, 0x1680),
        (0x2000, 0x200A),
        (0x2028, 0x2029),
        (0x202F, 0x202F),
        (0x205F, 0x205F),
        (0x3000, 0x3000),
    ]
}

pub fn normalize_ast(ast: Ast) -> Ast {
    match ast {
        Ast::Class(mut c) => {
            let mut flat = Class {
                negated: c.negated,
                ranges: Vec::new(),
            };
            for &(lo, hi) in &c.ranges {
                if lo >= u32::MAX - 6 && lo == hi {
                    expand_class_into(&mut flat, lo);
                } else {
                    for ch in lo..=hi {
                        flat.ranges.push((ch, ch));
                    }
                }
            }
            Ast::Class(flat)
        }
        Ast::Concat(v) => Ast::Concat(v.into_iter().map(normalize_ast).collect()),
        Ast::Alt(v) => Ast::Alt(v.into_iter().map(normalize_ast).collect()),
        Ast::Quant {
            inner,
            min,
            max,
            greedy,
        } => Ast::Quant {
            inner: Box::new(normalize_ast(*inner)),
            min,
            max,
            greedy,
        },
        Ast::Cap { inner, index } => Ast::Cap {
            inner: Box::new(normalize_ast(*inner)),
            index,
        },
        Ast::NoCap(inner) => Ast::NoCap(Box::new(normalize_ast(*inner))),
        other => other,
    }
}

pub fn is_word_char(c: u32) -> bool {
    char::from_u32(c)
        .map(|ch| ch.is_alphanumeric() || ch == '_')
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple() {
        let (ast, _) = parse(r"\d+").unwrap();
        assert!(matches!(ast, Ast::Quant { .. }));
    }

    #[test]
    fn parse_groups() {
        let (ast, _) = parse(r"(a|b)+").unwrap();
        assert!(matches!(ast, Ast::Quant { .. }));
    }
}
