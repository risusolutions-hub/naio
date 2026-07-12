//! WHATWG/RFC 3986 URL parsing.

use super::encode::percent_encode;
use super::{Url, UrlComponents};

#[inline]
pub fn default_port(scheme: &str) -> u16 {
    match scheme {
        "http" | "ws" => 80,
        "https" | "wss" => 443,
        "ftp" => 21,
        _ => 0,
    }
}

#[inline]
fn trim_ascii_whitespace(s: &str) -> &str {
    s.trim_matches(|c: char| matches!(c, '\u{0009}' | '\u{000A}' | '\u{000D}' | ' '))
}

fn split_once(s: &str, ch: char) -> (&str, String) {
    match s.split_once(ch) {
        Some((a, b)) => (a, b.to_string()),
        None => (s, String::new()),
    }
}

fn parse_authority(auth: &str) -> Result<(String, String, &str), String> {
    if auth.is_empty() {
        return Err("missing authority".into());
    }
    if let Some(at) = auth.rfind('@') {
        let creds = &auth[..at];
        let hostport = &auth[at + 1..];
        if hostport.is_empty() {
            return Err("missing host".into());
        }
        let (user, pass) = match creds.split_once(':') {
            Some((u, p)) => (
                super::encode::percent_decode(u)?,
                super::encode::percent_decode(p)?,
            ),
            None => (super::encode::percent_decode(creds)?, String::new()),
        };
        Ok((user, pass, hostport))
    } else {
        Ok((String::new(), String::new(), auth))
    }
}

fn parse_hostport(hostport: &str, scheme: &str) -> Result<(String, u16), String> {
    if hostport.is_empty() {
        return Err("missing host".into());
    }
    if hostport.starts_with('[') {
        let end = hostport.find(']').ok_or("bad ipv6 host")?;
        let host = hostport[1..end].to_string();
        if host.is_empty() {
            return Err("bad ipv6 host".into());
        }
        let port = if hostport.len() > end + 1 {
            if hostport.as_bytes().get(end + 1).copied() != Some(b':') {
                return Err("bad ipv6 port".into());
            }
            let p: u16 = hostport[end + 2..]
                .parse()
                .map_err(|_| "bad port".to_string())?;
            if p == 0 {
                return Err("bad port".into());
            }
            p
        } else {
            default_port(scheme)
        };
        return Ok((host, port));
    }
    if let Some(colon) = hostport.rfind(':') {
        let host = &hostport[..colon];
        if host.is_empty() {
            return Err("missing host".into());
        }
        let port: u16 = hostport[colon + 1..]
            .parse()
            .map_err(|_| "bad port".to_string())?;
        if port == 0 {
            return Err("bad port".into());
        }
        Ok((host.to_string(), port))
    } else {
        Ok((hostport.to_string(), default_port(scheme)))
    }
}

fn validate_scheme(scheme: &str) -> Result<(), String> {
    if scheme.is_empty() {
        return Err("missing scheme".into());
    }
    let first = scheme.chars().next().unwrap();
    if !first.is_ascii_alphabetic() {
        return Err("invalid scheme".into());
    }
    if !scheme
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
    {
        return Err("invalid scheme".into());
    }
    Ok(())
}

/// Parse an absolute URL string into components.
pub fn parse_url(raw: &str) -> Result<Url, String> {
    let raw = trim_ascii_whitespace(raw);
    if raw.is_empty() {
        return Err("empty url".into());
    }

    let (raw, fragment) = split_once(raw, '#');
    let (raw, query) = split_once(raw, '?');

    let scheme_end = raw.find(':').ok_or("missing scheme")?;
    if scheme_end == 0 {
        return Err("missing scheme".into());
    }
    let scheme_raw = &raw[..scheme_end];
    validate_scheme(scheme_raw)?;
    let scheme = scheme_raw.to_ascii_lowercase();

    let rest = &raw[scheme_end + 1..];
    if !rest.starts_with("//") {
        return Err("missing authority".into());
    }
    let rest = &rest[2..];
    let (auth, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };

    let (user, password, hostport) = parse_authority(auth)?;
    let (host, port) = parse_hostport(hostport, &scheme)?;

    let path = if path.is_empty() {
        "/".to_string()
    } else {
        path.to_string()
    };

    Ok(Url {
        scheme,
        host,
        port,
        path,
        query,
        fragment,
        user,
        password,
    })
}

pub fn components(url: &Url) -> UrlComponents {
    let def = default_port(&url.scheme);
    UrlComponents {
        scheme: url.scheme.clone(),
        username: url.user.clone(),
        password: url.password.clone(),
        host: url.host.clone(),
        port: if url.port != 0 && url.port != def {
            Some(url.port)
        } else {
            None
        },
        path: url.path.clone(),
        query: if url.query.is_empty() {
            None
        } else {
            Some(url.query.clone())
        },
        fragment: if url.fragment.is_empty() {
            None
        } else {
            Some(url.fragment.clone())
        },
    }
}

pub fn origin(url: &Url) -> String {
    let def = default_port(&url.scheme);
    if url.port != 0 && url.port != def {
        format!("{}://{}:{}", url.scheme, url.host, url.port)
    } else {
        format!("{}://{}", url.scheme, url.host)
    }
}

pub fn authority(url: &Url) -> String {
    let def = default_port(&url.scheme);
    if url.port != 0 && url.port != def {
        format!("{}:{}", url.host, url.port)
    } else {
        url.host.clone()
    }
}

pub fn to_string_full(url: &Url) -> String {
    let mut s = String::with_capacity(
        url.scheme.len()
            + url.host.len()
            + url.path.len()
            + url.query.len()
            + url.fragment.len()
            + 16,
    );
    s.push_str(&url.scheme);
    s.push_str("://");
    if !url.user.is_empty() {
        s.push_str(&percent_encode(url.user.as_bytes()));
        if !url.password.is_empty() {
            s.push(':');
            s.push_str(&percent_encode(url.password.as_bytes()));
        }
        s.push('@');
    }
    s.push_str(&url.host);
    let def = default_port(&url.scheme);
    if url.port != 0 && url.port != def {
        s.push(':');
        s.push_str(&url.port.to_string());
    }
    if url.path.is_empty() {
        s.push('/');
    } else {
        s.push_str(&url.path);
    }
    if !url.query.is_empty() {
        s.push('?');
        s.push_str(&url.query);
    }
    if !url.fragment.is_empty() {
        s.push('#');
        s.push_str(&url.fragment);
    }
    s
}

pub struct QueryPairs<'a> {
    rest: &'a str,
}

pub fn query_pairs(url: &Url) -> QueryPairs<'_> {
    QueryPairs {
        rest: url.query.as_str(),
    }
}

impl<'a> Iterator for QueryPairs<'a> {
    type Item = (&'a str, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        if self.rest.is_empty() {
            return None;
        }
        let (pair, remaining) = match self.rest.find('&') {
            Some(i) => (&self.rest[..i], &self.rest[i + 1..]),
            None => (self.rest, ""),
        };
        self.rest = remaining;
        if let Some(eq) = pair.find('=') {
            Some((&pair[..eq], &pair[eq + 1..]))
        } else {
            Some((pair, ""))
        }
    }
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
    fn default_port_omitted_in_components() {
        let u = parse_url("http://example.com/path").unwrap();
        let c = components(&u);
        assert_eq!(c.port, None);
        assert_eq!(c.path, "/path");
    }

    #[test]
    fn ipv6_host() {
        let u = parse_url("http://[::1]:8080/").unwrap();
        assert_eq!(u.host, "::1");
        assert_eq!(u.port, 8080);
    }

    #[test]
    fn credentials() {
        let u = parse_url("https://user:pass@host/path").unwrap();
        assert_eq!(u.user, "user");
        assert_eq!(u.password, "pass");
    }

    #[test]
    fn query_pairs_iter() {
        let u = parse_url("http://h/?a=1&b=2&c").unwrap();
        let pairs: Vec<_> = query_pairs(&u).collect();
        assert_eq!(pairs, vec![("a", "1"), ("b", "2"), ("c", "")]);
    }
}
