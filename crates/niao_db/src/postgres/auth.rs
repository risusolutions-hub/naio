use super::config::Config;
use super::error::Error;
use super::md5;
use std::io::Write;

pub fn handle_auth(
    stream: &mut std::net::TcpStream,
    body: &[u8],
    config: &Config,
) -> Result<(), Error> {
    if body.len() < 4 {
        return Err(Error::msg("short auth message"));
    }
    let kind = i32::from_be_bytes(body[0..4].try_into().unwrap());
    match kind {
        0 => Ok(()),
        3 => {
            let pw = config.get_password().unwrap_or("");
            write_password(stream, pw)
        }
        5 => {
            if body.len() < 8 {
                return Err(Error::msg("short md5 salt"));
            }
            let salt = &body[4..8];
            let user = config.get_user().unwrap_or("postgres");
            let pw = config.get_password().unwrap_or("");
            let hashed = md5::pg_md5_password(user, pw, salt);
            write_password(stream, &hashed)
        }
        10 => scram_auth(stream, body, config),
        other => Err(Error::msg(format!("unsupported auth type {other}"))),
    }
}

fn write_password(stream: &mut std::net::TcpStream, password: &str) -> Result<(), Error> {
    let mut payload = password.as_bytes().to_vec();
    payload.push(0);
    super::wire::write_message(stream, b'p', &payload).map_err(Error::io)
}

fn scram_auth(stream: &mut std::net::TcpStream, body: &[u8], config: &Config) -> Result<(), Error> {
    let mechs = std::str::from_utf8(&body[4..]).unwrap_or("");
    if !mechs.contains("SCRAM-SHA-256") {
        return Err(Error::msg("server does not offer SCRAM-SHA-256"));
    }
    let user = config.get_user().unwrap_or("postgres");
    let password = config.get_password().unwrap_or("");
    let client_nonce: String = (0..16)
        .map(|_| b"abcdefghijklmnopqrstuvwxyz0123456789"[fastrand() % 36] as char)
        .collect();
    let client_first = format!("n,,n={user},r={client_nonce}");
    super::wire::write_sasl_initial(stream, "SCRAM-SHA-256", &client_first).map_err(Error::io)?;
    let server_first = read_sasl_continue(stream)?;
    let (salt, iter, nonce, server_nonce) = parse_server_first(&server_first, &client_nonce)?;
    let salted = hi(password.as_bytes(), &salt, iter);
    let client_key = hmac(&salted, b"Client Key");
    let stored_key = niao_crypto::sha256(&client_key);
    let client_first_bare = format!("n={user},r={client_nonce}");
    let salt_b64_s = salt_b64(&salt);
    let server_first_bare = format!("r={server_nonce},s={salt_b64_s},i={iter}");
    let auth_message = format!("{client_first_bare},{server_first_bare},c=biws,r={server_nonce}");
    let client_sig = hmac(&stored_key, auth_message.as_bytes());
    let client_proof: Vec<u8> = client_key
        .iter()
        .zip(client_sig.iter())
        .map(|(a, b)| a ^ b)
        .collect();
    let proof_b64 = b64(&client_proof);
    let client_final = format!("c=biws,r={server_nonce},p={proof_b64}");
    super::wire::write_sasl_continue(stream, &client_final).map_err(Error::io)?;
    let server_final = read_sasl_final(stream)?;
    if !server_final.starts_with("v=") {
        return Err(Error::msg("SCRAM server final missing verifier"));
    }
    let server_key = hmac(&salted, b"Server Key");
    let server_sig = hmac(&server_key, auth_message.as_bytes());
    let expected = b64(&server_sig);
    if server_final != format!("v={expected}") {
        return Err(Error::msg("SCRAM verification failed"));
    }
    super::wire::write_sasl_final(stream, "").map_err(Error::io)?;
    Ok(())
}

fn fastrand() -> usize {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as usize)
        .unwrap_or(0)
}

fn read_sasl_continue(stream: &mut std::net::TcpStream) -> Result<String, Error> {
    let mut buf = Vec::new();
    loop {
        let msg = read_one(stream, &mut buf)?;
        if msg.tag == b'E' {
            return Err(Error::from_error_response(&msg.body));
        }
        if msg.tag == b'R' && msg.body.len() >= 4 {
            let k = i32::from_be_bytes(msg.body[0..4].try_into().unwrap());
            if k == 11 {
                return Ok(std::str::from_utf8(&msg.body[4..])
                    .unwrap_or("")
                    .trim_end_matches('\0')
                    .to_string());
            }
        }
    }
}

fn read_sasl_final(stream: &mut std::net::TcpStream) -> Result<String, Error> {
    let mut buf = Vec::new();
    loop {
        let msg = read_one(stream, &mut buf)?;
        if msg.tag == b'E' {
            return Err(Error::from_error_response(&msg.body));
        }
        if msg.tag == b'R' && msg.body.len() >= 4 {
            let k = i32::from_be_bytes(msg.body[0..4].try_into().unwrap());
            if k == 12 {
                return Ok(std::str::from_utf8(&msg.body[4..])
                    .unwrap_or("")
                    .trim_end_matches('\0')
                    .to_string());
            }
        }
    }
}

fn read_one(
    stream: &mut std::net::TcpStream,
    buf: &mut Vec<u8>,
) -> Result<super::wire::Message, Error> {
    use std::io::Read;
    loop {
        if let Ok(parsed) = super::wire::try_parse_message(buf) {
            if let Some(msg) = parsed {
                buf.drain(..msg.consumed);
                return Ok(msg.message);
            }
        }
        let mut tmp = [0u8; 4096];
        let n = stream.read(&mut tmp).map_err(Error::io)?;
        if n == 0 {
            return Err(Error::msg("connection closed during auth"));
        }
        buf.extend_from_slice(&tmp[..n]);
    }
}

fn parse_server_first(
    s: &str,
    client_nonce: &str,
) -> Result<(Vec<u8>, u32, String, String), Error> {
    let mut salt_b64 = None;
    let mut iter = 4096u32;
    let mut nonce = None;
    for part in s.split(',') {
        if let Some(v) = part.strip_prefix("s=") {
            salt_b64 = Some(v.to_string());
        } else if let Some(v) = part.strip_prefix("i=") {
            iter = v.parse().unwrap_or(4096);
        } else if let Some(v) = part.strip_prefix("r=") {
            nonce = Some(v.to_string());
        }
    }
    let server_nonce = nonce.ok_or_else(|| Error::msg("SCRAM missing nonce"))?;
    if !server_nonce.starts_with(client_nonce) {
        return Err(Error::msg("SCRAM nonce mismatch"));
    }
    let salt = b64dec(
        salt_b64
            .ok_or_else(|| Error::msg("SCRAM missing salt"))?
            .as_bytes(),
    )?;
    Ok((salt, iter, client_nonce.to_string(), server_nonce))
}

fn hi(password: &[u8], salt: &[u8], iter: u32) -> Vec<u8> {
    let mut ui = hmac(password, salt);
    let mut u = ui.clone();
    for _ in 1..iter {
        u = hmac(password, &u);
        for (a, b) in ui.iter_mut().zip(u.iter()) {
            *a ^= b;
        }
    }
    ui
}

fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    niao_crypto::hmac_sha256(key, data).to_vec()
}

fn b64(data: &[u8]) -> String {
    niao_codec::base64::encode_standard(data)
}

fn b64dec(data: &[u8]) -> Result<Vec<u8>, Error> {
    niao_codec::base64::decode_standard(std::str::from_utf8(data).unwrap_or(""))
        .map_err(|e| Error::msg(e.to_string()))
}

fn salt_b64(s: &[u8]) -> String {
    b64(s)
}
