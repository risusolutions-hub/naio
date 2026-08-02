use crate::object::Object;
use crate::toml::error::{TomlError, TomlResult};
use crate::{Number, Value};

pub fn parse(s: &str) -> TomlResult<Value> {
    parse_to_value(s)
}

pub fn parse_to_value(s: &str) -> TomlResult<Value> {
    let mut p = TomlParser::new(s);
    p.parse_document()
}

struct TomlParser<'a> {
    src: &'a str,
    line: usize,
    col: usize,
    root: Object,
}

impl<'a> TomlParser<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            line: 1,
            col: 1,
            root: Object::new(),
        }
    }

    fn parse_document(mut self) -> TomlResult<Value> {
        let mut current: Vec<String> = Vec::new();
        let mut is_array_table = false;
        for raw_line in self.src.lines() {
            let line_no = self.line;
            self.line += 1;
            let line = strip_comment(raw_line).trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with("[[") && line.ends_with("]]") {
                let name = line[2..line.len() - 2].trim();
                current = dotted_segments(name)?;
                is_array_table = true;
                self.ensure_array_table(&current)?;
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                let name = line[1..line.len() - 1].trim();
                current = dotted_segments(name)?;
                is_array_table = false;
                self.ensure_table(&current)?;
                continue;
            }
            let (key, val) = split_key_value(line, line_no)?;
            let value = parse_scalar(val, line_no)?;
            if is_array_table {
                self.insert_array_table_value(&current, &key, value, line_no)?;
            } else {
                self.insert_dotted(&current, &key, value, line_no)?;
            }
        }
        Ok(Value::Object(self.root))
    }

    fn ensure_table(&mut self, path: &[String]) -> TomlResult<()> {
        let mut cur = &mut self.root;
        for seg in path {
            if cur.get(seg).is_none() {
                cur.insert(seg.clone(), Value::Object(Object::new()));
            }
            match cur.get_mut(seg) {
                Some(Value::Object(obj)) => cur = obj,
                _ => {
                    return Err(TomlError::new(
                        self.line,
                        1,
                        format!("key '{seg}' is not a table"),
                    ));
                }
            }
        }
        Ok(())
    }

    fn ensure_array_table(&mut self, path: &[String]) -> TomlResult<()> {
        if path.is_empty() {
            return Err(TomlError::new(self.line, 1, "empty table header"));
        }
        let mut cur = &mut self.root;
        for (i, seg) in path.iter().enumerate() {
            let last = i + 1 == path.len();
            if last {
                match cur.get_mut(seg) {
                    None => {
                        cur.insert(
                            seg.clone(),
                            Value::Array(vec![Value::Object(Object::new())]),
                        );
                    }
                    Some(Value::Array(items)) => {
                        items.push(Value::Object(Object::new()));
                    }
                    _ => {
                        return Err(TomlError::new(
                            self.line,
                            1,
                            format!("'{seg}' is not an array of tables"),
                        ));
                    }
                }
            } else {
                if cur.get(seg).is_none() {
                    cur.insert(seg.clone(), Value::Object(Object::new()));
                }
                cur = match cur.get_mut(seg) {
                    Some(Value::Object(o)) => o,
                    _ => {
                        return Err(TomlError::new(
                            self.line,
                            1,
                            format!("'{seg}' is not a table"),
                        ));
                    }
                };
            }
        }
        Ok(())
    }

    fn insert_array_table_value(
        &mut self,
        path: &[String],
        key: &str,
        value: Value,
        line_no: usize,
    ) -> TomlResult<()> {
        let mut cur = &mut self.root;
        for seg in path {
            cur = match cur.get_mut(seg) {
                Some(Value::Array(items)) => items
                    .last_mut()
                    .ok_or_else(|| TomlError::new(line_no, 1, "empty array-of-tables"))?
                    .as_object_mut()
                    .ok_or_else(|| TomlError::new(line_no, 1, "invalid array-of-tables row"))?,
                _ => {
                    return Err(TomlError::new(line_no, 1, "invalid array-of-tables path"));
                }
            };
        }
        insert_dotted_into(cur, key, value, line_no)
    }

    fn insert_dotted(
        &mut self,
        table_path: &[String],
        key: &str,
        value: Value,
        line_no: usize,
    ) -> TomlResult<()> {
        let mut cur = &mut self.root;
        for seg in table_path {
            cur = match cur.get_mut(seg) {
                Some(Value::Object(o)) => o,
                _ => {
                    return Err(TomlError::new(line_no, 1, format!("missing table '{seg}'")));
                }
            };
        }
        insert_dotted_into(cur, key, value, line_no)
    }
}

fn insert_dotted_into(obj: &mut Object, key: &str, value: Value, line_no: usize) -> TomlResult<()> {
    let parts: Vec<&str> = key.split('.').collect();
    if parts.is_empty() {
        return Err(TomlError::new(line_no, 1, "empty key"));
    }
    let mut cur = obj;
    for part in &parts[..parts.len() - 1] {
        if cur.get(*part).is_none() {
            cur.insert(part.to_string(), Value::Object(Object::new()));
        }
        cur = match cur.get_mut(*part) {
            Some(Value::Object(o)) => o,
            _ => {
                return Err(TomlError::new(
                    line_no,
                    1,
                    format!("key '{part}' is not a table"),
                ));
            }
        };
    }
    cur.insert(parts.last().unwrap().to_string(), value);
    Ok(())
}

fn dotted_segments(name: &str) -> TomlResult<Vec<String>> {
    if name.is_empty() {
        return Err(TomlError::new(0, 0, "empty table name"));
    }
    Ok(name.split('.').map(|s| s.to_string()).collect())
}

fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut quote = '"';
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if in_string {
            if b == b'\\' {
                i += 2;
                continue;
            }
            if b as char == quote {
                in_string = false;
            }
        } else if b == b'"' || b == b'\'' {
            in_string = true;
            quote = b as char;
        } else if b == b'#' {
            return &line[..i];
        }
        i += 1;
    }
    line
}

fn split_key_value(line: &str, line_no: usize) -> TomlResult<(String, &str)> {
    let eq = line
        .find('=')
        .ok_or_else(|| TomlError::new(line_no, 1, "expected '=' in key/value"))?;
    let key = line[..eq].trim();
    if key.is_empty() {
        return Err(TomlError::new(line_no, 1, "empty key"));
    }
    Ok((key.to_string(), line[eq + 1..].trim()))
}

fn parse_scalar(s: &str, line_no: usize) -> TomlResult<Value> {
    if s.is_empty() {
        return Err(TomlError::new(line_no, 1, "empty value"));
    }
    if s.starts_with('"') || s.starts_with('\'') {
        return parse_string(s, line_no);
    }
    if s.starts_with('[') {
        return parse_array(s, line_no);
    }
    if s.starts_with('{') {
        return parse_inline_table(s, line_no);
    }
    if s == "true" {
        return Ok(Value::Bool(true));
    }
    if s == "false" {
        return Ok(Value::Bool(false));
    }
    if s.starts_with("0x") || s.starts_with("0o") || s.starts_with("0b") {
        return parse_prefixed_int(s, line_no);
    }
    if looks_float(s) {
        let f: f64 = s
            .replace('_', "")
            .parse()
            .map_err(|_| TomlError::new(line_no, 1, format!("invalid float '{s}'")))?;
        return Ok(Value::Number(Number::F64(f)));
    }
    if s.chars()
        .all(|c| c.is_ascii_digit() || c == '_' || c == '-')
        || (s.starts_with('-') && s.len() > 1)
    {
        let n: i64 = s
            .replace('_', "")
            .parse()
            .map_err(|_| TomlError::new(line_no, 1, format!("invalid integer '{s}'")))?;
        return Ok(Value::Number(Number::I64(n)));
    }
    Err(TomlError::new(
        line_no,
        1,
        format!("unsupported value '{s}'"),
    ))
}

fn looks_float(s: &str) -> bool {
    let t = s.replace('_', "");
    t.contains('.') || t.contains('e') || t.contains('E')
}

fn parse_prefixed_int(s: &str, line_no: usize) -> TomlResult<Value> {
    let t = s.replace('_', "");
    if let Some(hex) = t.strip_prefix("0x") {
        let n = i64::from_str_radix(hex, 16)
            .map_err(|_| TomlError::new(line_no, 1, format!("invalid hex '{s}'")))?;
        return Ok(Value::Number(Number::I64(n)));
    }
    if let Some(oct) = t.strip_prefix("0o") {
        let n = i64::from_str_radix(oct, 8)
            .map_err(|_| TomlError::new(line_no, 1, format!("invalid oct '{s}'")))?;
        return Ok(Value::Number(Number::I64(n)));
    }
    if let Some(bin) = t.strip_prefix("0b") {
        let n = i64::from_str_radix(bin, 2)
            .map_err(|_| TomlError::new(line_no, 1, format!("invalid bin '{s}'")))?;
        return Ok(Value::Number(Number::I64(n)));
    }
    Err(TomlError::new(line_no, 1, format!("invalid int '{s}'")))
}

fn parse_string(s: &str, line_no: usize) -> TomlResult<Value> {
    let quote = s.as_bytes()[0];
    if s.starts_with("\"\"\"") || s.starts_with("'''") {
        let q = &s[..3];
        let end = s
            .rfind(q)
            .filter(|&i| i > 0)
            .ok_or_else(|| TomlError::new(line_no, 1, "unterminated multiline string"))?;
        return Ok(Value::String(s[3..end].to_string()));
    }
    if s.as_bytes()[s.len() - 1] != quote {
        return Err(TomlError::new(line_no, 1, "unterminated string"));
    }
    let inner = &s[1..s.len() - 1];
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            let e = chars
                .next()
                .ok_or_else(|| TomlError::new(line_no, 1, "invalid string escape"))?;
            match e {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                '\\' => out.push('\\'),
                '"' => out.push('"'),
                _ => out.push(e),
            }
        } else {
            out.push(c);
        }
    }
    Ok(Value::String(out))
}

fn parse_array(s: &str, line_no: usize) -> TomlResult<Value> {
    if !s.starts_with('[') || !s.ends_with(']') {
        return Err(TomlError::new(line_no, 1, "invalid array"));
    }
    let inner = s[1..s.len() - 1].trim();
    if inner.is_empty() {
        return Ok(Value::Array(Vec::new()));
    }
    let mut items = Vec::new();
    for part in split_top_level(inner, ',') {
        items.push(parse_scalar(part.trim(), line_no)?);
    }
    Ok(Value::Array(items))
}

fn parse_inline_table(s: &str, line_no: usize) -> TomlResult<Value> {
    if !s.starts_with('{') || !s.ends_with('}') {
        return Err(TomlError::new(line_no, 1, "invalid inline table"));
    }
    let inner = s[1..s.len() - 1].trim();
    let mut obj = Object::new();
    if inner.is_empty() {
        return Ok(Value::Object(obj));
    }
    for part in split_top_level(inner, ',') {
        let (k, v) = split_key_value(part.trim(), line_no)?;
        insert_dotted_into(&mut obj, &k, parse_scalar(v, line_no)?, line_no)?;
    }
    Ok(Value::Object(obj))
}

fn split_top_level(s: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut quote = '"';
    for (i, c) in s.char_indices() {
        if in_str {
            if c == '\\' {
                continue;
            }
            if c == quote {
                in_str = false;
            }
            continue;
        }
        if c == '"' || c == '\'' {
            in_str = true;
            quote = c;
            continue;
        }
        if c == '{' || c == '[' {
            depth += 1;
        } else if c == '}' || c == ']' {
            depth -= 1;
        } else if c == sep && depth == 0 {
            out.push(s[start..i].trim());
            start = i + 1;
        }
    }
    out.push(s[start..].trim());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::toml::parse;

    #[test]
    fn ahiru_sample() {
        let src = include_str!("../../../../examples/ahiru.config.toml");
        let v = parse(src).expect("parse");
        assert_eq!(
            v.get("server")
                .and_then(|s| s.get("port"))
                .and_then(|p| p.as_i64()),
            Some(3001)
        );
    }

    #[test]
    fn niao_manifest() {
        let v = parse("name = \"niao-demo\"\nversion = \"0.1.0\"\nentry = \"src/main.niao\"\n")
            .unwrap();
        assert_eq!(v.get("name").and_then(|v| v.as_str()), Some("niao-demo"));
    }

    #[test]
    fn array_of_tables() {
        let src = "[[items]]\nname = \"a\"\n[[items]]\nname = \"b\"\n";
        let v = parse(src).unwrap();
        let arr = v.get("items").unwrap().as_array().unwrap();
        assert_eq!(arr.len(), 2);
    }
}
