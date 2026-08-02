//! RESP2/RESP3 codec.

use std::io;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    SimpleString(String),
    Error(String),
    Integer(i64),
    BulkString(Option<Vec<u8>>),
    Array(Vec<Value>),
    Null,
}

pub fn encode_command(parts: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(format!("*{}\r\n", parts.len()).as_bytes());
    for p in parts {
        out.extend_from_slice(format!("${}\r\n", p.len()).as_bytes());
        out.extend_from_slice(p);
        out.extend_from_slice(b"\r\n");
    }
    out
}

pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn parse_one(&mut self) -> io::Result<Value> {
        if self.pos >= self.buf.len() {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "resp eof"));
        }
        let tag = self.buf[self.pos];
        self.pos += 1;
        match tag {
            b'+' => Ok(Value::SimpleString(self.read_line()?)),
            b'-' => Ok(Value::Error(self.read_line()?)),
            b':' => {
                let line = self.read_line()?;
                line.parse::<i64>()
                    .map(Value::Integer)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
            }
            b'$' => {
                let len: i64 = self
                    .read_line()?
                    .parse()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                if len < 0 {
                    return Ok(Value::BulkString(None));
                }
                let len = len as usize;
                if self.pos + len + 2 > self.buf.len() {
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "bulk short"));
                }
                let data = self.buf[self.pos..self.pos + len].to_vec();
                self.pos += len + 2;
                Ok(Value::BulkString(Some(data)))
            }
            b'*' => {
                let count: i64 = self
                    .read_line()?
                    .parse()
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                if count < 0 {
                    return Ok(Value::Null);
                }
                let mut items = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    items.push(self.parse_one()?);
                }
                Ok(Value::Array(items))
            }
            b'_' => {
                self.read_line()?;
                Ok(Value::Null)
            }
            other => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown resp tag {other}"),
            )),
        }
    }

    fn read_line(&mut self) -> io::Result<String> {
        let start = self.pos;
        while self.pos + 1 < self.buf.len() {
            if self.buf[self.pos] == b'\r' && self.buf[self.pos + 1] == b'\n' {
                let line = std::str::from_utf8(&self.buf[start..self.pos])
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
                    .to_string();
                self.pos += 2;
                return Ok(line);
            }
            self.pos += 1;
        }
        Err(io::Error::new(io::ErrorKind::UnexpectedEof, "line eof"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_and_bulk() {
        let mut r = Reader::new(b"+OK\r\n$-1\r\n");
        assert_eq!(r.parse_one().unwrap(), Value::SimpleString("OK".into()));
        assert_eq!(r.parse_one().unwrap(), Value::BulkString(None));
    }

    #[test]
    fn parse_array() {
        let mut r = Reader::new(b"*2\r\n:1\r\n:2\r\n");
        match r.parse_one().unwrap() {
            Value::Array(v) => assert_eq!(v.len(), 2),
            _ => panic!("expected array"),
        }
    }

    #[test]
    fn encode_get() {
        let enc = encode_command(&[b"GET", b"k"]);
        assert!(enc.starts_with(b"*2\r\n"));
    }
}
