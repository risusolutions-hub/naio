//! Google Cloud Storage JSON API backend (Bearer token; ~ngcp subset).

use crate::error::{BlobError, BlobResult};
use crate::store::{BackendKind, Entry, GcsOpts, ObjectStore};

#[derive(Debug, Clone)]
pub struct GcsStore {
    pub opts: GcsOpts,
    pub bucket: String,
}

impl GcsStore {
    pub fn new(opts: GcsOpts, bucket: impl Into<String>) -> Self {
        Self {
            opts,
            bucket: bucket.into(),
        }
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.opts.access_token)
    }
}

impl ObjectStore for GcsStore {
    fn kind(&self) -> BackendKind {
        BackendKind::Gcs
    }

    fn read(&self, key: &str) -> BlobResult<Vec<u8>> {
        let enc = percent_encode(key);
        let url = format!(
            "https://storage.googleapis.com/storage/v1/b/{}/o/{}?alt=media",
            self.bucket, enc
        );
        let resp = niao_http::get(&url)
            .set("Authorization", self.auth_header())
            .send()?;
        if resp.status == 404 {
            return Err(BlobError::not_found(key));
        }
        if resp.status >= 400 {
            let body = String::from_utf8_lossy(&resp.body);
            return Err(BlobError::http(resp.status, &body));
        }
        Ok(resp.body)
    }

    fn write(&self, key: &str, data: &[u8], content_type: Option<&str>) -> BlobResult<u64> {
        let ct = content_type.unwrap_or("application/octet-stream");
        let enc = percent_encode(key);
        let url = format!(
            "https://storage.googleapis.com/upload/storage/v1/b/{}/o?uploadType=media&name={}",
            self.bucket, enc
        );
        let resp = niao_http::post(&url)
            .set("Authorization", self.auth_header())
            .set("Content-Type", ct)
            .send_bytes(data)?;
        if resp.status >= 400 {
            let body = String::from_utf8_lossy(&resp.body);
            return Err(BlobError::http(resp.status, &body));
        }
        Ok(data.len() as u64)
    }

    fn exists(&self, key: &str) -> BlobResult<bool> {
        match self.info(key) {
            Ok(_) => Ok(true),
            Err(e) if e.message.starts_with("not found") => Ok(false),
            Err(e) => Err(e),
        }
    }

    fn info(&self, key: &str) -> BlobResult<Entry> {
        let enc = percent_encode(key);
        let url = format!(
            "https://storage.googleapis.com/storage/v1/b/{}/o/{}",
            self.bucket, enc
        );
        let resp = niao_http::get(&url)
            .set("Authorization", self.auth_header())
            .send()?;
        if resp.status == 404 {
            return Err(BlobError::not_found(key));
        }
        if resp.status >= 400 {
            let body = String::from_utf8_lossy(&resp.body);
            return Err(BlobError::http(resp.status, &body));
        }
        let body = String::from_utf8_lossy(&resp.body);
        let size = json_number_field(&body, "size").unwrap_or(0);
        let name = json_string_field(&body, "name").unwrap_or_else(|| key.to_string());
        Ok(Entry {
            name,
            kind: "file".into(),
            size,
            mtime: None,
        })
    }

    fn list(&self, prefix: &str, _detail: bool) -> BlobResult<Vec<Entry>> {
        let mut url = format!(
            "https://storage.googleapis.com/storage/v1/b/{}/o?delimiter=/",
            self.bucket
        );
        if !prefix.is_empty() {
            url.push_str("&prefix=");
            url.push_str(&percent_encode(prefix));
        }
        let resp = niao_http::get(&url)
            .set("Authorization", self.auth_header())
            .send()?;
        if resp.status >= 400 {
            let body = String::from_utf8_lossy(&resp.body);
            return Err(BlobError::http(resp.status, &body));
        }
        let body = String::from_utf8_lossy(&resp.body);
        Ok(parse_gcs_list_json(&body))
    }

    fn remove(&self, key: &str) -> BlobResult<()> {
        let enc = percent_encode(key);
        let url = format!(
            "https://storage.googleapis.com/storage/v1/b/{}/o/{}",
            self.bucket, enc
        );
        let resp = niao_http::delete(&url)
            .set("Authorization", self.auth_header())
            .send()?;
        if resp.status >= 400 && resp.status != 404 {
            let body = String::from_utf8_lossy(&resp.body);
            return Err(BlobError::http(resp.status, &body));
        }
        Ok(())
    }

    fn mkdir(&self, _key: &str) -> BlobResult<()> {
        Ok(())
    }
}

fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b'/' => out.push_str("%2F"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn json_string_field(json: &str, field: &str) -> Option<String> {
    let needle = format!("\"{field}\"");
    let idx = json.find(&needle)?;
    let after = &json[idx + needle.len()..];
    let after = after.trim_start().trim_start_matches(':').trim_start();
    if !after.starts_with('"') {
        return None;
    }
    let mut out = String::new();
    let mut chars = after[1..].chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            }
            '"' => break,
            c => out.push(c),
        }
    }
    Some(out)
}

fn json_number_field(json: &str, field: &str) -> Option<u64> {
    // GCS returns size as a JSON string: "size": "123"
    if let Some(s) = json_string_field(json, field) {
        return s.parse().ok();
    }
    let needle = format!("\"{field}\"");
    let idx = json.find(&needle)?;
    let after = &json[idx + needle.len()..];
    let after = after.trim_start().trim_start_matches(':').trim_start();
    let end = after
        .find(|c: char| !(c.is_ascii_digit()))
        .unwrap_or(after.len());
    after[..end].parse().ok()
}

fn parse_gcs_list_json(json: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    // items[].name / items[].size
    let mut rest = json;
    while let Some(idx) = rest.find("\"name\"") {
        let after = &rest[idx..];
        if let Some(name) = json_string_field(after, "name") {
            if name.ends_with('/') {
                out.push(Entry {
                    name: name.trim_end_matches('/').to_string(),
                    kind: "dir".into(),
                    size: 0,
                    mtime: None,
                });
            } else {
                let size = json_number_field(after, "size").unwrap_or(0);
                out.push(Entry {
                    name,
                    kind: "file".into(),
                    size,
                    mtime: None,
                });
            }
        }
        rest = &rest[idx + 6..];
    }
    // prefixes
    rest = json;
    if let Some(pidx) = rest.find("\"prefixes\"") {
        let slice = &rest[pidx..];
        let mut s = slice;
        while let Some(q) = s.find('"') {
            let after = &s[q + 1..];
            if let Some(end) = after.find('"') {
                let p = &after[..end];
                if p.ends_with('/') {
                    out.push(Entry {
                        name: p.trim_end_matches('/').to_string(),
                        kind: "dir".into(),
                        size: 0,
                        mtime: None,
                    });
                }
                s = &after[end + 1..];
            } else {
                break;
            }
            if s.contains(']') && s.find(']').unwrap() < s.find('"').unwrap_or(usize::MAX) {
                break;
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.name == b.name);
    out
}
