//! URI parsing for object-store schemes (~fsspec / smart_open).

use crate::error::{BlobError, BlobResult};

/// Parsed object URI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobUri {
    /// Protocol: `file`, `memory`, `s3`, `gs`, `az`, `abfs`, or empty for bare paths.
    pub scheme: String,
    /// Authority / netloc (bucket for s3/gs, account for az/abfs, host for file).
    pub netloc: String,
    /// Path / key within the store (no leading slash for cloud keys).
    pub path: String,
    /// Original input (trimmed).
    pub raw: String,
}

impl BlobUri {
    /// Bucket / container name for cloud schemes (alias of `netloc`).
    pub fn bucket(&self) -> &str {
        &self.netloc
    }

    /// Object key (alias of `path`).
    pub fn key(&self) -> &str {
        &self.path
    }

    /// Reconstruct a canonical URI string.
    pub fn to_uri_string(&self) -> String {
        if self.scheme.is_empty() || self.scheme == "file" {
            if self.path.is_empty() {
                return self.raw.clone();
            }
            if self.scheme == "file" {
                if self.netloc.is_empty() {
                    return format!("file:///{}", self.path.trim_start_matches('/'));
                }
                return format!(
                    "file://{}/{}",
                    self.netloc,
                    self.path.trim_start_matches('/')
                );
            }
            return self.path.clone();
        }
        if self.path.is_empty() {
            format!("{}://{}", self.scheme, self.netloc)
        } else {
            format!(
                "{}://{}/{}",
                self.scheme,
                self.netloc,
                self.path.trim_start_matches('/')
            )
        }
    }

    pub fn is_local(&self) -> bool {
        self.scheme.is_empty() || self.scheme == "file"
    }

    pub fn is_memory(&self) -> bool {
        self.scheme == "memory"
    }
}

/// Parse `s3://bucket/key`, `gs://bucket/key`, `az://account/container/blob`,
/// `abfs://container@account.dfs.core.windows.net/path`, `file:///path`,
/// `memory://name/path`, or a bare local filesystem path.
pub fn parse(uri: &str) -> BlobResult<BlobUri> {
    let raw = uri.trim().to_string();
    if raw.is_empty() {
        return Err(BlobError::invalid_uri(uri));
    }

    // Bare Windows path: C:\... or \\server\share
    if looks_like_windows_path(&raw) {
        return Ok(BlobUri {
            scheme: String::new(),
            netloc: String::new(),
            path: raw.clone(),
            raw,
        });
    }

    if let Some(rest) = raw.strip_prefix("file://") {
        let rest = rest.trim_start_matches('/');
        // file:///C:/foo or file:///home/a
        #[cfg(windows)]
        {
            if rest.len() >= 2 && rest.as_bytes()[1] == b':' {
                return Ok(BlobUri {
                    scheme: "file".into(),
                    netloc: String::new(),
                    path: rest.replace('/', "\\"),
                    raw,
                });
            }
        }
        return Ok(BlobUri {
            scheme: "file".into(),
            netloc: String::new(),
            path: format!("/{rest}"),
            raw,
        });
    }

    if let Some((scheme, rest)) = split_scheme(&raw) {
        let scheme = scheme.to_ascii_lowercase();
        match scheme.as_str() {
            "s3" | "s3a" | "s3n" => parse_bucket_key("s3", rest, &raw),
            "gs" | "gcs" => parse_bucket_key("gs", rest, &raw),
            "az" | "azure" | "abfss" => parse_azure(&scheme, rest, &raw),
            "abfs" => parse_abfs(rest, &raw),
            "memory" | "mem" => parse_bucket_key("memory", rest, &raw),
            other => Err(BlobError::unsupported(other)),
        }
    } else {
        // Unix-style or relative local path
        Ok(BlobUri {
            scheme: String::new(),
            netloc: String::new(),
            path: raw.clone(),
            raw,
        })
    }
}

fn split_scheme(s: &str) -> Option<(&str, &str)> {
    let idx = s.find("://")?;
    if idx == 0 {
        return None;
    }
    // Avoid treating `C://` as a scheme on Windows-ish inputs
    if idx == 1 {
        return None;
    }
    Some((&s[..idx], &s[idx + 3..]))
}

fn looks_like_windows_path(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() >= 3 && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/') {
        return true;
    }
    b.len() >= 2 && b[0] == b'\\' && b[1] == b'\\'
}

fn parse_bucket_key(scheme: &str, rest: &str, raw: &str) -> BlobResult<BlobUri> {
    let rest = rest.trim_start_matches('/');
    if rest.is_empty() {
        return Err(BlobError::invalid_uri(raw));
    }
    let (netloc, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i + 1..].to_string()),
        None => (rest, String::new()),
    };
    if netloc.is_empty() {
        return Err(BlobError::invalid_uri(raw));
    }
    Ok(BlobUri {
        scheme: scheme.into(),
        netloc: netloc.into(),
        path,
        raw: raw.into(),
    })
}

/// `az://account/container/blob` or `azure://account/container/blob`
fn parse_azure(scheme_in: &str, rest: &str, raw: &str) -> BlobResult<BlobUri> {
    let scheme = if scheme_in == "abfss" { "abfs" } else { "az" };
    let rest = rest.trim_start_matches('/');
    let parts: Vec<&str> = rest.splitn(3, '/').collect();
    if parts.is_empty() || parts[0].is_empty() {
        return Err(BlobError::invalid_uri(raw));
    }
    // netloc = account/container  (or account alone)
    let (netloc, path) = if parts.len() == 1 {
        (parts[0].to_string(), String::new())
    } else if parts.len() == 2 {
        (format!("{}/{}", parts[0], parts[1]), String::new())
    } else {
        (format!("{}/{}", parts[0], parts[1]), parts[2].to_string())
    };
    Ok(BlobUri {
        scheme: scheme.into(),
        netloc,
        path,
        raw: raw.into(),
    })
}

/// `abfs://container@account.dfs.core.windows.net/path`
fn parse_abfs(rest: &str, raw: &str) -> BlobResult<BlobUri> {
    let rest = rest.trim_start_matches('/');
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], rest[i + 1..].to_string()),
        None => (rest, String::new()),
    };
    let (container, account) = match authority.split_once('@') {
        Some((c, host)) => {
            let account = host.split('.').next().unwrap_or(host);
            (c, account)
        }
        None => return Err(BlobError::invalid_uri(raw)),
    };
    if container.is_empty() || account.is_empty() {
        return Err(BlobError::invalid_uri(raw));
    }
    Ok(BlobUri {
        scheme: "az".into(),
        netloc: format!("{account}/{container}"),
        path,
        raw: raw.into(),
    })
}

/// Join a base URI/path with a relative child (does not normalize `..`).
pub fn join(base: &str, child: &str) -> BlobResult<String> {
    let child = child.trim_start_matches('/');
    if child.is_empty() {
        return Ok(base.to_string());
    }
    let parsed = parse(base)?;
    if parsed.scheme.is_empty() || parsed.scheme == "file" {
        let mut p = parsed.path;
        if p.ends_with('/') || p.ends_with('\\') {
            p.push_str(child);
        } else if p.is_empty() {
            p = child.to_string();
        } else {
            #[cfg(windows)]
            {
                p.push('\\');
            }
            #[cfg(not(windows))]
            {
                p.push('/');
            }
            p.push_str(child);
        }
        if parsed.scheme == "file" {
            return Ok(format!("file:///{}", p.trim_start_matches(['/', '\\'])));
        }
        return Ok(p);
    }
    let mut path = parsed.path;
    if path.is_empty() {
        path = child.to_string();
    } else {
        path.push('/');
        path.push_str(child);
    }
    Ok(BlobUri {
        scheme: parsed.scheme,
        netloc: parsed.netloc,
        path,
        raw: String::new(),
    }
    .to_uri_string())
}

/// Return the scheme string (`""` for bare local paths, else lowercase scheme).
pub fn scheme_of(uri: &str) -> BlobResult<String> {
    Ok(parse(uri)?.scheme)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_s3() {
        let u = parse("s3://my-bucket/a/b.txt").unwrap();
        assert_eq!(u.scheme, "s3");
        assert_eq!(u.netloc, "my-bucket");
        assert_eq!(u.path, "a/b.txt");
    }

    #[test]
    fn parse_gs() {
        let u = parse("gs://bkt/obj").unwrap();
        assert_eq!(u.scheme, "gs");
        assert_eq!(u.bucket(), "bkt");
        assert_eq!(u.key(), "obj");
    }

    #[test]
    fn parse_az() {
        let u = parse("az://acct/cont/folder/x.bin").unwrap();
        assert_eq!(u.scheme, "az");
        assert_eq!(u.netloc, "acct/cont");
        assert_eq!(u.path, "folder/x.bin");
    }

    #[test]
    fn parse_abfs() {
        let u = parse("abfs://cont@acct.dfs.core.windows.net/p/q").unwrap();
        assert_eq!(u.scheme, "az");
        assert_eq!(u.netloc, "acct/cont");
        assert_eq!(u.path, "p/q");
    }

    #[test]
    fn parse_local() {
        let u = parse("/tmp/foo").unwrap();
        assert!(u.is_local());
        assert_eq!(u.path, "/tmp/foo");
    }

    #[test]
    fn parse_memory() {
        let u = parse("memory://root/a").unwrap();
        assert!(u.is_memory());
        assert_eq!(u.netloc, "root");
        assert_eq!(u.path, "a");
    }

    #[test]
    fn join_s3() {
        assert_eq!(
            join("s3://b/prefix", "x.txt").unwrap(),
            "s3://b/prefix/x.txt"
        );
    }

    #[test]
    fn empty_uri_err() {
        assert!(parse("").is_err());
        assert!(parse("   ").is_err());
    }
}
