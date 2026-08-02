use crate::number::Number;
use crate::Value;

pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    #[inline]
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    #[inline]
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            buf: Vec::with_capacity(cap),
        }
    }

    #[inline]
    pub fn into_string(self) -> String {
        // SAFETY: we only write valid UTF-8
        unsafe { String::from_utf8_unchecked(self.buf) }
    }

    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        &self.buf
    }

    #[inline]
    pub fn clear(&mut self) {
        self.buf.clear();
    }

    pub fn write_value(&mut self, v: &Value) {
        match v {
            Value::Null => self.buf.extend_from_slice(b"null"),
            Value::Bool(true) => self.buf.extend_from_slice(b"true"),
            Value::Bool(false) => self.buf.extend_from_slice(b"false"),
            Value::Number(n) => self.write_number(n),
            Value::String(s) => self.write_string(s),
            Value::Array(items) => {
                self.buf.push(b'[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        self.buf.push(b',');
                    }
                    self.write_value(item);
                }
                self.buf.push(b']');
            }
            Value::Object(obj) => {
                self.buf.push(b'{');
                let mut first = true;
                for (k, v) in obj.iter() {
                    if !first {
                        self.buf.push(b',');
                    }
                    first = false;
                    self.write_string(k);
                    self.buf.push(b':');
                    self.write_value(v);
                }
                self.buf.push(b'}');
            }
        }
    }

    fn write_number(&mut self, n: &Number) {
        match n {
            Number::I64(v) => {
                let mut tmp = itoa::Buffer::new();
                self.buf.extend_from_slice(tmp.format(*v).as_bytes());
            }
            Number::U64(v) => {
                let mut tmp = itoa::Buffer::new();
                self.buf.extend_from_slice(tmp.format_u64(*v).as_bytes());
            }
            Number::F64(f) => {
                if !f.is_finite() {
                    self.buf.extend_from_slice(b"null");
                } else if *f == 0.0 && f.is_sign_negative() {
                    self.buf.extend_from_slice(b"-0");
                } else {
                    let mut tmp = ryu::Buffer::new();
                    self.buf.extend_from_slice(tmp.format(*f).as_bytes());
                }
            }
        }
    }

    fn write_string(&mut self, s: &str) {
        self.buf.push(b'"');
        for ch in s.chars() {
            match ch {
                '"' => self.buf.extend_from_slice(b"\\\""),
                '\\' => self.buf.extend_from_slice(b"\\\\"),
                '\n' => self.buf.extend_from_slice(b"\\n"),
                '\r' => self.buf.extend_from_slice(b"\\r"),
                '\t' => self.buf.extend_from_slice(b"\\t"),
                c if c.is_control() => {
                    self.buf.extend_from_slice(b"\\u");
                    let code = c as u32;
                    self.buf.push(hex_digit((code >> 12) as u8));
                    self.buf.push(hex_digit((code >> 8) as u8));
                    self.buf.push(hex_digit((code >> 4) as u8));
                    self.buf.push(hex_digit(code as u8));
                }
                c => {
                    let mut buf = [0u8; 4];
                    let encoded = c.encode_utf8(&mut buf);
                    self.buf.extend_from_slice(encoded.as_bytes());
                }
            }
        }
        self.buf.push(b'"');
    }

    pub fn write_pretty(&mut self, v: &Value, indent: usize, depth: usize) {
        let pad = indent.saturating_mul(depth);
        match v {
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {
                self.write_value(v);
            }
            Value::Array(items) => {
                if items.is_empty() {
                    self.buf.extend_from_slice(b"[]");
                    return;
                }
                self.buf.push(b'[');
                self.buf.push(b'\n');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        self.buf.push(b',');
                        self.buf.push(b'\n');
                    }
                    self.write_indent(pad + indent);
                    self.write_pretty(item, indent, depth + 1);
                }
                self.buf.push(b'\n');
                self.write_indent(pad);
                self.buf.push(b']');
            }
            Value::Object(obj) => {
                if obj.is_empty() {
                    self.buf.extend_from_slice(b"{}");
                    return;
                }
                self.buf.push(b'{');
                self.buf.push(b'\n');
                let mut first = true;
                for (k, val) in obj.iter() {
                    if !first {
                        self.buf.push(b',');
                        self.buf.push(b'\n');
                    }
                    first = false;
                    self.write_indent(pad + indent);
                    self.write_string(k);
                    self.buf.extend_from_slice(b": ");
                    self.write_pretty(val, indent, depth + 1);
                }
                self.buf.push(b'\n');
                self.write_indent(pad);
                self.buf.push(b'}');
            }
        }
    }

    fn write_indent(&mut self, n: usize) {
        for _ in 0..n {
            self.buf.push(b' ');
        }
    }
}

#[inline]
fn hex_digit(v: u8) -> u8 {
    match v & 0xF {
        0..=9 => b'0' + (v & 0xF),
        _ => b'a' + (v & 0xF) - 10,
    }
}

mod itoa {
    pub struct Buffer {
        bytes: [u8; 20],
        len: usize,
    }
    impl Buffer {
        pub fn new() -> Self {
            Self {
                bytes: [0; 20],
                len: 0,
            }
        }
        pub fn format(&mut self, n: i64) -> &str {
            if n == 0 {
                self.bytes[0] = b'0';
                self.len = 1;
            } else if n > 0 {
                self.write_u64(n as u64);
            } else if n == i64::MIN {
                self.bytes.copy_from_slice(b"-9223372036854775808");
                self.len = 20;
            } else {
                self.bytes[0] = b'-';
                self.len = 1;
                self.write_u64((-n) as u64);
            }
            unsafe { std::str::from_utf8_unchecked(&self.bytes[..self.len]) }
        }
        pub fn format_u64(&mut self, n: u64) -> &str {
            self.write_u64(n);
            unsafe { std::str::from_utf8_unchecked(&self.bytes[..self.len]) }
        }
        fn write_u64(&mut self, mut n: u64) {
            let start = self.len;
            if n == 0 {
                self.bytes[start] = b'0';
                self.len = start + 1;
                return;
            }
            while n > 0 {
                self.bytes[self.len] = b'0' + (n % 10) as u8;
                n /= 10;
                self.len += 1;
            }
            self.bytes[start..self.len].reverse();
        }
    }
}

mod ryu {
    pub struct Buffer {
        bytes: [u8; 24],
    }
    impl Buffer {
        pub fn new() -> Self {
            Self { bytes: [0; 24] }
        }
        pub fn format(&mut self, f: f64) -> &str {
            let s = format!("{f}");
            let b = s.as_bytes();
            let len = b.len().min(24);
            self.bytes[..len].copy_from_slice(&b[..len]);
            unsafe { std::str::from_utf8_unchecked(&self.bytes[..len]) }
        }
    }
}

pub fn to_string(v: &Value) -> String {
    let mut w = Writer::with_capacity(64);
    w.write_value(v);
    w.into_string()
}

pub fn to_vec(v: &Value) -> Vec<u8> {
    let mut w = Writer::with_capacity(64);
    w.write_value(v);
    w.buf
}

pub fn to_string_pretty(v: &Value, indent: usize) -> String {
    let mut w = Writer::with_capacity(128);
    w.write_pretty(v, indent.max(1), 0);
    w.into_string()
}

pub fn write_value(v: &Value, buf: &mut Vec<u8>) {
    let mut w = Writer {
        buf: std::mem::take(buf),
    };
    w.write_value(v);
    *buf = w.buf;
}
