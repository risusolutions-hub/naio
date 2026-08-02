use crate::error::ExprError;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // literals
    Int(i64),
    Float(f64),
    String(String),
    True,
    False,
    Nil,
    // identifiers / keywords
    Ident(String),
    KwIf,
    KwElse,
    KwAnd,
    KwOr,
    KwNot,
    KwIn,
    // punctuation
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Colon,
    Dot,
    // operators
    Plus,
    Minus,
    Star,
    Slash,
    FloorDiv,
    Percent,
    Pow,
    Eq,
    NotEq,
    Lt,
    Gt,
    Le,
    Ge,
    Assign, // blocked at parse level for safety
    // logical symbols
    AmpAmp,
    PipePipe,
    Bang,
    // end
    Eof,
}

pub struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
        }
    }

    pub fn tokenize(mut self) -> Result<Vec<(usize, Token)>, ExprError> {
        let mut out = Vec::new();
        loop {
            let start = self.pos;
            let tok = self.next_token()?;
            let done = matches!(tok, Token::Eof);
            out.push((start, tok));
            if done {
                break;
            }
        }
        Ok(out)
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.pos += 1;
        Some(b)
    }

    fn skip_ws(&mut self) {
        while let Some(b) = self.peek() {
            if b.is_ascii_whitespace() {
                self.pos += 1;
            } else if b == b'#' {
                while self.peek().is_some_and(|c| c != b'\n') {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    fn next_token(&mut self) -> Result<Token, ExprError> {
        self.skip_ws();
        let start = self.pos;
        let Some(b) = self.peek() else {
            return Ok(Token::Eof);
        };

        match b {
            b'0'..=b'9' => self.number(),
            b'"' | b'\'' => self.string(b),
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.ident(),
            b'(' => {
                self.bump();
                Ok(Token::LParen)
            }
            b')' => {
                self.bump();
                Ok(Token::RParen)
            }
            b'[' => {
                self.bump();
                Ok(Token::LBracket)
            }
            b']' => {
                self.bump();
                Ok(Token::RBracket)
            }
            b'{' => {
                self.bump();
                Ok(Token::LBrace)
            }
            b'}' => {
                self.bump();
                Ok(Token::RBrace)
            }
            b',' => {
                self.bump();
                Ok(Token::Comma)
            }
            b':' => {
                self.bump();
                Ok(Token::Colon)
            }
            b'.' => {
                self.bump();
                Ok(Token::Dot)
            }
            b'+' => {
                self.bump();
                Ok(Token::Plus)
            }
            b'-' => {
                self.bump();
                Ok(Token::Minus)
            }
            b'*' => {
                self.bump();
                if self.peek() == Some(b'*') {
                    self.bump();
                    Ok(Token::Pow)
                } else {
                    Ok(Token::Star)
                }
            }
            b'/' => {
                self.bump();
                if self.peek() == Some(b'/') {
                    self.bump();
                    Ok(Token::FloorDiv)
                } else {
                    Ok(Token::Slash)
                }
            }
            b'%' => {
                self.bump();
                Ok(Token::Percent)
            }
            b'=' => {
                self.bump();
                if self.peek() == Some(b'=') {
                    self.bump();
                    Ok(Token::Eq)
                } else {
                    Err(ExprError::Lex {
                        pos: start,
                        message: "assignment is not allowed in sandboxed expressions".into(),
                    })
                }
            }
            b'!' => {
                self.bump();
                if self.peek() == Some(b'=') {
                    self.bump();
                    Ok(Token::NotEq)
                } else {
                    Ok(Token::Bang)
                }
            }
            b'<' => {
                self.bump();
                if self.peek() == Some(b'=') {
                    self.bump();
                    Ok(Token::Le)
                } else {
                    Ok(Token::Lt)
                }
            }
            b'>' => {
                self.bump();
                if self.peek() == Some(b'=') {
                    self.bump();
                    Ok(Token::Ge)
                } else {
                    Ok(Token::Gt)
                }
            }
            b'&' => {
                self.bump();
                if self.peek() == Some(b'&') {
                    self.bump();
                    Ok(Token::AmpAmp)
                } else {
                    Err(ExprError::Lex {
                        pos: start,
                        message: "unexpected '&' (use 'and')".into(),
                    })
                }
            }
            b'|' => {
                self.bump();
                if self.peek() == Some(b'|') {
                    self.bump();
                    Ok(Token::PipePipe)
                } else {
                    Err(ExprError::Lex {
                        pos: start,
                        message: "unexpected '|' (use 'or')".into(),
                    })
                }
            }
            _ => Err(ExprError::Lex {
                pos: start,
                message: format!(
                    "unexpected character {:?}",
                    self.src[start..].chars().next()
                ),
            }),
        }
    }

    fn number(&mut self) -> Result<Token, ExprError> {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        if self.peek() == Some(b'.')
            && self
                .bytes
                .get(self.pos + 1)
                .is_some_and(|c| c.is_ascii_digit())
        {
            self.pos += 1;
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.pos += 1;
            }
            let s = &self.src[start..self.pos];
            let f: f64 = s.parse().map_err(|_| ExprError::Lex {
                pos: start,
                message: format!("invalid float '{s}'"),
            })?;
            return Ok(Token::Float(f));
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            let save = self.pos;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            let mut any = false;
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                any = true;
                self.pos += 1;
            }
            if any {
                let s = &self.src[start..self.pos];
                let f: f64 = s.parse().map_err(|_| ExprError::Lex {
                    pos: start,
                    message: format!("invalid float '{s}'"),
                })?;
                return Ok(Token::Float(f));
            }
            self.pos = save;
        }
        let s = &self.src[start..self.pos];
        let n: i64 = s.parse().map_err(|_| ExprError::Lex {
            pos: start,
            message: format!("integer overflow or invalid int '{s}'"),
        })?;
        Ok(Token::Int(n))
    }

    fn string(&mut self, quote: u8) -> Result<Token, ExprError> {
        let start = self.pos;
        self.bump();
        let mut out = String::new();
        while let Some(b) = self.bump() {
            if b == quote {
                return Ok(Token::String(out));
            }
            if b == b'\\' {
                let esc = self.bump().ok_or_else(|| ExprError::Lex {
                    pos: self.pos,
                    message: "unterminated string escape".into(),
                })?;
                let ch = match esc {
                    b'n' => '\n',
                    b'r' => '\r',
                    b't' => '\t',
                    b'\\' => '\\',
                    b'"' => '"',
                    b'\'' => '\'',
                    _ => {
                        return Err(ExprError::Lex {
                            pos: self.pos,
                            message: format!("invalid escape \\{esc}"),
                        })
                    }
                };
                out.push(ch);
            } else if b.is_ascii() {
                out.push(b as char);
            } else {
                let ch = self.src[self.pos - 1..].chars().next().unwrap();
                out.push(ch);
            }
        }
        Err(ExprError::Lex {
            pos: start,
            message: "unterminated string literal".into(),
        })
    }

    fn ident(&mut self) -> Result<Token, ExprError> {
        let start = self.pos;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_')
        {
            self.pos += 1;
        }
        let word = &self.src[start..self.pos];
        Ok(match word {
            "true" | "True" => Token::True,
            "false" | "False" => Token::False,
            "nil" | "null" | "None" => Token::Nil,
            "if" => Token::KwIf,
            "else" => Token::KwElse,
            "and" => Token::KwAnd,
            "or" => Token::KwOr,
            "not" => Token::KwNot,
            "in" => Token::KwIn,
            _ => Token::Ident(word.to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lex_basic() {
        let toks: Vec<_> = Lexer::new("x + 2 * 3.5")
            .tokenize()
            .unwrap()
            .into_iter()
            .map(|(_, t)| t)
            .collect();
        assert_eq!(
            toks,
            vec![
                Token::Ident("x".into()),
                Token::Plus,
                Token::Int(2),
                Token::Star,
                Token::Float(3.5),
                Token::Eof
            ]
        );
    }
}
