//! GraphQL lexer (executable documents + SDL).

use crate::error::{GqlError, GqlResult};

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Sof,
    Eof,
    Bang,
    Dollar,
    ParenL,
    ParenR,
    Spread,
    Colon,
    Equals,
    At,
    BracketL,
    BracketR,
    BraceL,
    BraceR,
    Pipe,
    Amp,
    Name(String),
    Int(String),
    Float(String),
    String(String),
    BlockString(String),
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    #[allow(dead_code)]
    pub start: usize,
    #[allow(dead_code)]
    pub end: usize,
    pub line: usize,
    pub column: usize,
}

pub struct Lexer<'a> {
    source: &'a str,
    bytes: &'a [u8],
    pos: usize,
    line: usize,
    line_start: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            bytes: source.as_bytes(),
            pos: 0,
            line: 1,
            line_start: 0,
        }
    }

    pub fn tokenize(mut self) -> GqlResult<Vec<Token>> {
        let mut tokens = Vec::new();
        tokens.push(Token {
            kind: TokenKind::Sof,
            start: 0,
            end: 0,
            line: 1,
            column: 1,
        });
        loop {
            let tok = self.next_token()?;
            let is_eof = matches!(tok.kind, TokenKind::Eof);
            tokens.push(tok);
            if is_eof {
                break;
            }
        }
        Ok(tokens)
    }

    fn column(&self) -> usize {
        self.pos - self.line_start + 1
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let c = self.peek()?;
        self.pos += 1;
        if c == b'\n' {
            self.line += 1;
            self.line_start = self.pos;
        }
        Some(c)
    }

    fn err(&self, msg: impl Into<String>) -> GqlError {
        GqlError::parse(msg, self.line, self.column())
    }

    fn skip_trivia(&mut self) -> GqlResult<()> {
        loop {
            match self.peek() {
                Some(b'#') => {
                    while let Some(c) = self.peek() {
                        if c == b'\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                Some(b' ' | b'\t' | b',' | b'\n' | b'\r') => {
                    self.bump();
                }
                _ => return Ok(()),
            }
        }
    }

    fn next_token(&mut self) -> GqlResult<Token> {
        self.skip_trivia()?;
        let start = self.pos;
        let line = self.line;
        let column = self.column();
        let Some(c) = self.peek() else {
            return Ok(Token {
                kind: TokenKind::Eof,
                start,
                end: start,
                line,
                column,
            });
        };

        let kind = match c {
            b'!' => {
                self.bump();
                TokenKind::Bang
            }
            b'$' => {
                self.bump();
                TokenKind::Dollar
            }
            b'(' => {
                self.bump();
                TokenKind::ParenL
            }
            b')' => {
                self.bump();
                TokenKind::ParenR
            }
            b':' => {
                self.bump();
                TokenKind::Colon
            }
            b'=' => {
                self.bump();
                TokenKind::Equals
            }
            b'@' => {
                self.bump();
                TokenKind::At
            }
            b'[' => {
                self.bump();
                TokenKind::BracketL
            }
            b']' => {
                self.bump();
                TokenKind::BracketR
            }
            b'{' => {
                self.bump();
                TokenKind::BraceL
            }
            b'}' => {
                self.bump();
                TokenKind::BraceR
            }
            b'|' => {
                self.bump();
                TokenKind::Pipe
            }
            b'&' => {
                self.bump();
                TokenKind::Amp
            }
            b'.' => self.read_spread()?,
            b'"' => self.read_string()?,
            b'0'..=b'9' | b'-' => self.read_number()?,
            b'A'..=b'Z' | b'a'..=b'z' | b'_' => self.read_name(),
            other => return Err(self.err(format!("unexpected character '{}'", other as char))),
        };

        Ok(Token {
            kind,
            start,
            end: self.pos,
            line,
            column,
        })
    }

    fn read_spread(&mut self) -> GqlResult<TokenKind> {
        if self.bytes.get(self.pos..self.pos + 3) == Some(b"...") {
            self.pos += 3;
            Ok(TokenKind::Spread)
        } else {
            Err(self.err("expected '...'"))
        }
    }

    fn read_name(&mut self) -> TokenKind {
        let start = self.pos;
        self.bump();
        while matches!(
            self.peek(),
            Some(b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_')
        ) {
            self.bump();
        }
        TokenKind::Name(self.source[start..self.pos].to_string())
    }

    fn read_number(&mut self) -> GqlResult<TokenKind> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.bump();
        }
        if self.peek() == Some(b'0') {
            self.bump();
            if matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.err("invalid number with leading zero"));
            }
        } else if matches!(self.peek(), Some(b'1'..=b'9')) {
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.bump();
            }
        } else {
            return Err(self.err("invalid number"));
        }

        let mut is_float = false;
        if self.peek() == Some(b'.') {
            is_float = true;
            self.bump();
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.err("expected digit after decimal point"));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.bump();
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            is_float = true;
            self.bump();
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.bump();
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(self.err("expected digit in exponent"));
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.bump();
            }
        }
        let text = self.source[start..self.pos].to_string();
        Ok(if is_float {
            TokenKind::Float(text)
        } else {
            TokenKind::Int(text)
        })
    }

    fn read_string(&mut self) -> GqlResult<TokenKind> {
        // Peek for block string """
        if self.bytes.get(self.pos..self.pos + 3) == Some(b"\"\"\"") {
            return self.read_block_string();
        }
        self.bump(); // opening "
        let mut out = String::new();
        loop {
            match self.bump() {
                None => return Err(self.err("unterminated string")),
                Some(b'"') => break,
                Some(b'\\') => {
                    let esc = self.bump().ok_or_else(|| self.err("unterminated escape"))?;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{0008}'),
                        b'f' => out.push('\u{000c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let mut hex = String::with_capacity(4);
                            for _ in 0..4 {
                                let h = self
                                    .bump()
                                    .ok_or_else(|| self.err("invalid unicode escape"))?;
                                if !h.is_ascii_hexdigit() {
                                    return Err(self.err("invalid unicode escape"));
                                }
                                hex.push(h as char);
                            }
                            let code = u32::from_str_radix(&hex, 16)
                                .map_err(|_| self.err("invalid unicode escape"))?;
                            let ch = char::from_u32(code)
                                .ok_or_else(|| self.err("invalid unicode code point"))?;
                            out.push(ch);
                        }
                        other => {
                            return Err(self.err(format!("invalid escape '\\{}'", other as char)))
                        }
                    }
                }
                Some(b'\n') => return Err(self.err("unterminated string")),
                Some(c) => out.push(c as char),
            }
        }
        Ok(TokenKind::String(out))
    }

    fn read_block_string(&mut self) -> GqlResult<TokenKind> {
        self.pos += 3; // """
        let start = self.pos;
        loop {
            if self.bytes.get(self.pos..self.pos + 3) == Some(b"\"\"\"") {
                let raw = &self.source[start..self.pos];
                self.pos += 3;
                return Ok(TokenKind::BlockString(dedent_block_string(raw)));
            }
            match self.bump() {
                None => return Err(self.err("unterminated block string")),
                Some(b'\\') => {
                    // Allow \"\"\" escape sequence for triple-quote inside block
                    if self.bytes.get(self.pos..self.pos + 3) == Some(b"\"\"\"") {
                        self.pos += 3;
                    }
                }
                Some(_) => {}
            }
        }
    }
}

/// GraphQL block string value dedent (spec §StringValue).
fn dedent_block_string(raw: &str) -> String {
    let lines: Vec<&str> = raw.split('\n').collect();
    let mut common = usize::MAX;
    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            continue;
        }
        let indent = line.chars().take_while(|c| *c == ' ' || *c == '\t').count();
        if indent < line.len() {
            common = common.min(indent);
        }
    }
    if common == usize::MAX {
        common = 0;
    }
    let mut formatted = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            formatted.push((*line).to_string());
        } else if line.len() >= common {
            formatted.push(line[common..].to_string());
        } else {
            formatted.push(String::new());
        }
    }
    // Trim leading/trailing blank lines
    while formatted
        .first()
        .map(|s| s.trim().is_empty())
        .unwrap_or(false)
    {
        formatted.remove(0);
    }
    while formatted
        .last()
        .map(|s| s.trim().is_empty())
        .unwrap_or(false)
    {
        formatted.pop();
    }
    formatted.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_query_punctuators() {
        let tokens = Lexer::new("{ hero { name } }").tokenize().unwrap();
        assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::BraceL)));
        assert!(tokens
            .iter()
            .any(|t| matches!(t.kind, TokenKind::Name(ref n) if n == "hero")));
    }

    #[test]
    fn lexes_string_escapes() {
        let tokens = Lexer::new(r#""hi\n\"there\"""#).tokenize().unwrap();
        let s = tokens
            .iter()
            .find_map(|t| match &t.kind {
                TokenKind::String(s) => Some(s.as_str()),
                _ => None,
            })
            .unwrap();
        assert_eq!(s, "hi\n\"there\"");
    }
}
