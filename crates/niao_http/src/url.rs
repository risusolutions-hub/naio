//! URL parse, build, percent-encoding.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub path: String,
    pub query: String,
    pub fragment: String,
    pub user: String,
    pub password: String,
}

impl Url {
    pub fn default_port(scheme: &str) -> u16 {
        match scheme {
            "http" | "ws" => 80,
            "https" | "wss" => 443,
            "ftp" => 21,
            _ => 0,
        }
    }

    pub fn to_string_full(&self) -> String {
        let mut s = String::new();
        s.push_str(&self.scheme);
        s.push_str("://");
        if !self.user.is_empty() {
            s.push_str(&percent_encode(self.user.as_bytes()));
            if !self.password.is_empty() {
                s.push(':');
                s.push_str(&percent_encode(self.password.as_bytes()));
            }
            s.push('@');
        }
        s.push_str(&self.host);
        if self.port != 0 && self.port != Self::default_port(&self.scheme) {
            s.push(':');
            s.push_str(&self.port.to_string());
        }
        if self.path.is_empty() {
            s.push('/');
        } else {
            s.push_str(&self.path);
        }
        if !self.query.is_empty() {
            s.push('?');
            s.push_str(&self.query);
        }
        if !self.fragment.is_empty() {
            s.push('#');
            s.push_str(&self.fragment);
        }
        s
    }

    pub fn authority(&self) -> String {
        if self.port != 0 && self.port != Self::default_port(&self.scheme) {
            format!("{}:{}", self.host, self.port)
        } else {
            self.host.clone()
        }
    }
}

pub fn parse_url(raw: &str) -> Result<Url, String> {
    let (raw, fragment) = split_once(raw, '#');
    let (raw, query) = split_once(raw, '?');
    let scheme_end = raw.find("://").ok_or("missing scheme")?;
    let scheme = raw[..scheme_end].to_ascii_lowercase();
    let rest = &raw[scheme_end + 3..];
    let (auth, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (user, password, hostport) = parse_authority(auth)?;
    let (host, port) = parse_hostport(hostport, &scheme)?;
    Ok(Url {
        scheme,
        host,
        port,
        path: path.to_string(),
        query: query.to_string(),
        fragment: fragment.to_string(),
        user,
        password,
    })
}

fn split_once(s: &str, ch: char) -> (&str, String) {
    match s.split_once(ch) {
        Some((a, b)) => (a, b.to_string()),
        None => (s, String::new()),
    }
}

fn parse_authority(auth: &str) -> Result<(String, String, &str), String> {
    if let Some(at) = auth.rfind('@') {
        let creds = &auth[..at];
        let hostport = &auth[at + 1..];
        let (user, pass) = match creds.split_once(':') {
            Some((u, p)) => (percent_decode(u)?, percent_decode(p)?),
            None => (percent_decode(creds)?, String::new()),
        };
        Ok((user, pass, hostport))
    } else {
        Ok((String::new(), String::new(), auth))
    }
}

fn parse_hostport(hostport: &str, scheme: &str) -> Result<(String, u16), String> {
    if hostport.starts_with('[') {
        let end = hostport.find(']').ok_or("bad ipv6 host")?;
        let host = hostport[1..end].to_string();
        let port = if hostport.len() > end + 1 {
            if hostport.as_bytes().get(end + 1).copied() != Some(b':') {
                return Err("bad ipv6 port".into());
            }
            hostport[end + 2..]
                .parse()
                .map_err(|_| "bad port".to_string())?
        } else {
            Url::default_port(scheme)
        };
        return Ok((host, port));
    }
    if let Some(colon) = hostport.rfind(':') {
        let host = hostport[..colon].to_string();
        let port: u16 = hostport[colon + 1..]
            .parse()
            .map_err(|_| "bad port".to_string())?;
        Ok((host, port))
    } else {
        Ok((hostport.to_string(), Url::default_port(scheme)))
    }
}

pub fn join(base: &Url, reference: &str) -> Result<Url, String> {
    if reference.contains("://") {
        return parse_url(reference);
    }
    let mut out = base.clone();
    if reference.starts_with('#') {
        out.fragment = reference[1..].to_string();
        return Ok(out);
    }
    if reference.starts_with('?') {
        out.query = reference[1..].to_string();
        return Ok(out);
    }
    if reference.starts_with('/') {
        out.path = reference.to_string();
    } else {
        let base_dir = base.path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        out.path = format!("{base_dir}/{reference}");
    }
    Ok(out)
}

pub fn percent_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push('%');
            out.push(hex(b >> 4));
            out.push(hex(b & 0xf));
        }
    }
    out
}

fn hex(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        _ => (b'A' + n - 10) as char,
    }
}

pub fn percent_decode(s: &str) -> Result<String, String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
        } else if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err("bad percent escape".into());
            }
            let hi = from_hex(bytes[i + 1])?;
            let lo = from_hex(bytes[i + 2])?;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "invalid utf8".into())
}

fn from_hex(b: u8) -> Result<u8, String> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err("bad hex".into()),
    }
}

pub fn form_urlencode(bytes: &[u8]) -> String {
    percent_encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_https() {
        let u = parse_url("https://example.com:8443/path?q=1#frag").unwrap();
        assert_eq!(u.scheme, "https");
        assert_eq!(u.host, "example.com");
        assert_eq!(u.port, 8443);
        assert_eq!(u.path, "/path");
        assert_eq!(u.query, "q=1");
        assert_eq!(u.fragment, "frag");
    }

    #[test]
    fn roundtrip_encode() {
        let s = "hello world";
        assert_eq!(percent_decode(&percent_encode(s.as_bytes())).unwrap(), s);
    }
}
