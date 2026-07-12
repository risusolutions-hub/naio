use crate::error::ParseError;
use crate::number::Number;
use crate::object::Object;
use crate::Value;

pub const DEFAULT_MAX_DEPTH: usize = 512;

#[inline]
fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

#[inline]
fn skip_ws(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && is_ws(bytes[i]) {
        i += 1;
    }
    i
}

struct Parser<'a> {
    bytes: &'a [u8],
    i: usize,
    depth: usize,
    max_depth: usize,
}

impl<'a> Parser<'a> {
    fn new(bytes: &'a [u8], max_depth: usize) -> Self {
        Self {
            bytes,
            i: 0,
            depth: 0,
            max_depth,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.i).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.bytes.get(self.i).copied();
        if b.is_some() {
            self.i += 1;
        }
        b
    }

    fn parse_value(&mut self) -> Result<Value, ParseError> {
        self.i = skip_ws(self.bytes, self.i);
        let b = self.peek().ok_or(ParseError::UnexpectedEof)?;
        match b {
            b'n' => self.parse_literal("null", Value::Null),
            b't' => self.parse_literal("true", Value::Bool(true)),
            b'f' => self.parse_literal("false", Value::Bool(false)),
            b'"' => self.parse_string(),
            b'[' => self.parse_array(),
            b'{' => self.parse_object(),
            b'-' | b'0'..=b'9' => self.parse_number(),
            _ => Err(ParseError::Expected("value")),
        }
    }

    fn parse_literal(&mut self, lit: &str, val: Value) -> Result<Value, ParseError> {
        let end = self.i + lit.len();
        if end > self.bytes.len() || &self.bytes[self.i..end] != lit.as_bytes() {
            return Err(ParseError::Expected("literal"));
        }
        self.i = end;
        Ok(val)
    }

    fn parse_string(&mut self) -> Result<Value, ParseError> {
        self.bump(); // "
        let mut out = String::new();
        loop {
            let b = self.bump().ok_or(ParseError::UnexpectedEof)?;
            match b {
                b'"' => return Ok(Value::String(out)),
                b'\\' => {
                    let esc = self.bump().ok_or(ParseError::UnexpectedEof)?;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\x08'),
                        b'f' => out.push('\x0C'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let ch = self.parse_unicode_escape()?;
                            out.push(ch);
                        }
                        _ => return Err(ParseError::InvalidEscape),
                    }
                }
                b if b < 0x20 => return Err(ParseError::InvalidEscape),
                b => out.push(b as char),
            }
        }
    }

    fn parse_unicode_escape(&mut self) -> Result<char, ParseError> {
        if self.i + 4 > self.bytes.len() {
            return Err(ParseError::InvalidUnicode);
        }
        let hex = &self.bytes[self.i..self.i + 4];
        self.i += 4;
        let code = parse_hex4(hex)?;
        if (0xD800..=0xDBFF).contains(&code) {
            if self.peek() != Some(b'\\') {
                return Err(ParseError::InvalidUnicode);
            }
            self.bump();
            if self.bump() != Some(b'u') {
                return Err(ParseError::InvalidUnicode);
            }
            if self.i + 4 > self.bytes.len() {
                return Err(ParseError::InvalidUnicode);
            }
            let low_hex = &self.bytes[self.i..self.i + 4];
            self.i += 4;
            let low = parse_hex4(low_hex)?;
            if !(0xDC00..=0xDFFF).contains(&low) {
                return Err(ParseError::InvalidUnicode);
            }
            let combined = 0x10000 + (((code - 0xD800) as u32) << 10) + ((low - 0xDC00) as u32);
            char::from_u32(combined).ok_or(ParseError::InvalidUnicode)
        } else {
            char::from_u32(code as u32).ok_or(ParseError::InvalidUnicode)
        }
    }

    fn parse_array(&mut self) -> Result<Value, ParseError> {
        if self.depth >= self.max_depth {
            return Err(ParseError::DepthLimit);
        }
        self.bump(); // [
        self.depth += 1;
        self.i = skip_ws(self.bytes, self.i);
        let mut items = Vec::new();
        if self.peek() == Some(b']') {
            self.bump();
            self.depth -= 1;
            return Ok(Value::Array(items));
        }
        loop {
            items.push(self.parse_value()?);
            self.i = skip_ws(self.bytes, self.i);
            match self.bump() {
                Some(b']') => break,
                Some(b',') => {
                    self.i = skip_ws(self.bytes, self.i);
                }
                _ => return Err(ParseError::Expected("] or ,")),
            }
        }
        self.depth -= 1;
        Ok(Value::Array(items))
    }

    fn parse_object(&mut self) -> Result<Value, ParseError> {
        if self.depth >= self.max_depth {
            return Err(ParseError::DepthLimit);
        }
        self.bump(); // {
        self.depth += 1;
        self.i = skip_ws(self.bytes, self.i);
        let mut obj = Object::new();
        if self.peek() == Some(b'}') {
            self.bump();
            self.depth -= 1;
            return Ok(Value::Object(obj));
        }
        loop {
            self.i = skip_ws(self.bytes, self.i);
            if self.peek() != Some(b'"') {
                return Err(ParseError::Expected("string key"));
            }
            let key_val = self.parse_string()?;
            let key = match key_val {
                Value::String(s) => s,
                _ => return Err(ParseError::Expected("string key")),
            };
            self.i = skip_ws(self.bytes, self.i);
            if self.bump() != Some(b':') {
                return Err(ParseError::Expected(":"));
            }
            let val = self.parse_value()?;
            obj.insert(key, val);
            self.i = skip_ws(self.bytes, self.i);
            match self.bump() {
                Some(b'}') => break,
                Some(b',') => {}
                _ => return Err(ParseError::Expected("} or ,")),
            }
        }
        self.depth -= 1;
        Ok(Value::Object(obj))
    }

    fn parse_number(&mut self) -> Result<Value, ParseError> {
        let start = self.i;
        let mut is_float = false;
        if self.peek() == Some(b'-') {
            self.bump();
        }
        match self.peek() {
            Some(b'0') => {
                self.bump();
            }
            Some(b'1'..=b'9') => {
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.bump();
                }
            }
            _ => return Err(ParseError::InvalidNumber),
        }
        if self.peek() == Some(b'.') {
            is_float = true;
            self.bump();
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(ParseError::InvalidNumber);
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
                return Err(ParseError::InvalidNumber);
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.bump();
            }
        }
        let slice = &self.bytes[start..self.i];
        if is_float {
            let s = std::str::from_utf8(slice).map_err(|_| ParseError::InvalidNumber)?;
            let f: f64 = s.parse().map_err(|_| ParseError::InvalidNumber)?;
            Ok(Value::Number(Number::F64(f)))
        } else {
            let s = std::str::from_utf8(slice).map_err(|_| ParseError::InvalidNumber)?;
            if s.starts_with('-') {
                let n: i64 = s.parse().map_err(|_| ParseError::InvalidNumber)?;
                Ok(Value::Number(Number::I64(n)))
            } else {
                let n: u64 = s.parse().map_err(|_| ParseError::InvalidNumber)?;
                if n <= i64::MAX as u64 {
                    Ok(Value::Number(Number::I64(n as i64)))
                } else {
                    Ok(Value::Number(Number::U64(n)))
                }
            }
        }
    }
}

fn parse_hex4(hex: &[u8]) -> Result<u16, ParseError> {
    let mut v = 0u16;
    for &b in hex {
        v = (v << 4)
            | match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                b'A'..=b'F' => b - b'A' + 10,
                _ => return Err(ParseError::InvalidUnicode),
            } as u16;
    }
    Ok(v)
}

pub fn parse_bytes(bytes: &[u8]) -> Result<Value, ParseError> {
    parse_bytes_with_depth(bytes, DEFAULT_MAX_DEPTH)
}

pub fn parse_bytes_with_depth(bytes: &[u8], max_depth: usize) -> Result<Value, ParseError> {
    let mut p = Parser::new(bytes, max_depth);
    let val = p.parse_value()?;
    p.i = skip_ws(p.bytes, p.i);
    if p.i < p.bytes.len() {
        return Err(ParseError::TrailingData);
    }
    Ok(val)
}

pub fn parse(s: &str) -> Result<Value, ParseError> {
    parse_bytes(s.as_bytes())
}

pub fn is_valid(s: &str) -> bool {
    parse(s).is_ok()
}

pub fn is_valid_bytes(bytes: &[u8]) -> bool {
    parse_bytes(bytes).is_ok()
}
