use std::io::{self, Write};

pub struct Message {
    pub tag: u8,
    pub body: Vec<u8>,
}

pub struct ParsedMessage {
    pub message: Message,
    pub consumed: usize,
}

pub fn try_parse_message(buf: &[u8]) -> io::Result<Option<ParsedMessage>> {
    if buf.len() < 5 {
        return Ok(None);
    }
    let tag = buf[0];
    let len = i32::from_be_bytes(buf[1..5].try_into().unwrap()) as usize;
    if len < 4 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "bad msg len"));
    }
    let total = len + 1;
    if buf.len() < total {
        return Ok(None);
    }
    let body = buf[5..total].to_vec();
    Ok(Some(ParsedMessage {
        message: Message { tag, body },
        consumed: total,
    }))
}

pub fn write_message(stream: &mut impl Write, tag: u8, body: &[u8]) -> io::Result<()> {
    let len = (body.len() as i32 + 4).to_be_bytes();
    stream.write_all(&[tag])?;
    stream.write_all(&len)?;
    stream.write_all(body)?;
    Ok(())
}

pub fn write_startup(stream: &mut impl Write, params: &[(&str, String)]) -> io::Result<()> {
    let mut body = Vec::new();
    body.extend_from_slice(&196608i32.to_be_bytes());
    for (k, v) in params {
        body.extend_from_slice(k.as_bytes());
        body.push(0);
        body.extend_from_slice(v.as_bytes());
        body.push(0);
    }
    body.push(0);
    let len = body.len() as i32;
    stream.write_all(&len.to_be_bytes())?;
    stream.write_all(&body)?;
    Ok(())
}

pub fn write_query(stream: &mut impl Write, sql: &str) -> io::Result<()> {
    let mut body = sql.as_bytes().to_vec();
    body.push(0);
    write_message(stream, b'Q', &body)
}

pub fn write_parse(stream: &mut impl Write, name: &str, sql: &str) -> io::Result<()> {
    let mut body = Vec::new();
    body.extend_from_slice(name.as_bytes());
    body.push(0);
    body.extend_from_slice(sql.as_bytes());
    body.push(0);
    body.extend_from_slice(&0i16.to_be_bytes());
    write_message(stream, b'P', &body)
}

pub fn write_bind(
    stream: &mut impl Write,
    portal: &str,
    statement: &str,
    params: &[Option<String>],
) -> io::Result<()> {
    let mut body = Vec::new();
    body.extend_from_slice(portal.as_bytes());
    body.push(0);
    body.extend_from_slice(statement.as_bytes());
    body.push(0);
    body.extend_from_slice(&(params.len() as i16).to_be_bytes());
    for p in params {
        match p {
            None => body.extend_from_slice(&(-1i32).to_be_bytes()),
            Some(s) => {
                body.extend_from_slice(&(0i32).to_be_bytes());
                body.extend_from_slice(&(s.len() as i32).to_be_bytes());
                body.extend_from_slice(s.as_bytes());
            }
        }
    }
    body.extend_from_slice(&0i16.to_be_bytes());
    write_message(stream, b'B', &body)
}

pub fn write_execute(stream: &mut impl Write, portal: &str, max_rows: i32) -> io::Result<()> {
    let mut body = Vec::new();
    body.extend_from_slice(portal.as_bytes());
    body.push(0);
    body.extend_from_slice(&max_rows.to_be_bytes());
    write_message(stream, b'E', &body)
}

pub fn write_sync(stream: &mut impl Write) -> io::Result<()> {
    write_message(stream, b'S', &[])
}

pub fn write_sasl_initial(stream: &mut impl Write, mech: &str, data: &str) -> io::Result<()> {
    let mut body = Vec::new();
    body.extend_from_slice(mech.as_bytes());
    body.push(0);
    body.extend_from_slice(&(data.len() as i32).to_be_bytes());
    body.extend_from_slice(data.as_bytes());
    write_message(stream, b'p', &body)
}

pub fn write_sasl_continue(stream: &mut impl Write, data: &str) -> io::Result<()> {
    let mut body = data.as_bytes().to_vec();
    body.push(0);
    write_message(stream, b'p', &body)
}

pub fn write_sasl_final(stream: &mut impl Write, data: &str) -> io::Result<()> {
    let mut body = data.as_bytes().to_vec();
    body.push(0);
    write_message(stream, b'p', &body)
}

pub fn parse_command_complete(body: &[u8]) -> u64 {
    let s = std::str::from_utf8(body).unwrap_or("");
    s.split_whitespace().nth(1).and_then(|n| n.parse().ok()).unwrap_or(0)
}

pub fn parse_row_description(body: &[u8]) -> Vec<String> {
    if body.len() < 2 {
        return Vec::new();
    }
    let count = i16::from_be_bytes(body[0..2].try_into().unwrap()) as usize;
    let mut cols = Vec::with_capacity(count);
    let mut pos = 2;
    for _ in 0..count {
        let start = pos;
        while pos < body.len() && body[pos] != 0 {
            pos += 1;
        }
        cols.push(
            std::str::from_utf8(&body[start..pos])
                .unwrap_or("")
                .to_string(),
        );
        pos += 19;
    }
    cols
}

pub fn write_copy_data(stream: &mut impl Write, data: &[u8]) -> io::Result<()> {
    write_message(stream, b'd', data)
}

pub fn write_copy_done(stream: &mut impl Write) -> io::Result<()> {
    write_message(stream, b'c', &[])
}

mod tests {
    use super::*;

    #[test]
    fn startup_roundtrip_len() {
        let mut v = Vec::new();
        write_startup(&mut v, &[("user", "u".into())]).unwrap();
        assert!(v.len() > 8);
    }
}
