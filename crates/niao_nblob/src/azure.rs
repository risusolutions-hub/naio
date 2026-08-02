//! Azure Blob backend via SharedKey / SAS / Bearer (~nazure blob subset).

use crate::error::{BlobError, BlobResult};
use crate::store::{AzureOpts, BackendKind, Entry, ObjectStore};
use niao_codec::base64;
use niao_crypto::hmac_sha256;
use std::time::{SystemTime, UNIX_EPOCH};

const BLOB_VERSION: &str = "2020-08-04";

const DOW: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn is_leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_to_ymd(mut d: u64) -> (u64, u64, u64) {
    let mut year = 1970u64;
    loop {
        let yd = if is_leap(year) { 366 } else { 365 };
        if d < yd {
            break;
        }
        d -= yd;
        year += 1;
    }
    let md_tab = [31u64, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 1u64;
    for (i, &base) in md_tab.iter().enumerate() {
        let days_in = if i == 1 && is_leap(year) { 29 } else { base };
        if d < days_in {
            month = i as u64 + 1;
            break;
        }
        d -= days_in;
    }
    (year, month, d + 1)
}

fn rfc1123_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let total_days = secs / 86400;
    let dow = (total_days % 7) as usize;
    let (year, month, day) = days_to_ymd(total_days);
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!(
        "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
        DOW[dow],
        day,
        MONTHS[month as usize - 1],
        year,
        h,
        m,
        s
    )
}

fn shared_key_blob(
    account: &str,
    key: &[u8],
    method: &str,
    content_length: &str,
    content_type: &str,
    _date: &str,
    ms_headers: &[(String, String)],
    canon_resource: &str,
) -> String {
    let mut ms = String::new();
    for (k, v) in ms_headers {
        ms.push_str(k);
        ms.push(':');
        ms.push_str(v);
        ms.push('\n');
    }
    // Canonicalized resource is "/{account}/{container}/..." (see Azure SharedKey docs).
    let string_to_sign = format!(
        "{method}\n\n\n{cl}\n\n{ct}\n\n\n\n\n\n\n{ms}{resource}",
        method = method,
        cl = content_length,
        ct = content_type,
        ms = ms,
        resource = canon_resource,
    );
    let mac = hmac_sha256(key, string_to_sign.as_bytes());
    let sig = base64::encode_standard(&mac);
    format!("SharedKey {account}:{sig}")
}

#[derive(Debug, Clone)]
pub struct AzureStore {
    pub opts: AzureOpts,
    pub container: String,
}

impl AzureStore {
    pub fn new(opts: AzureOpts, container: impl Into<String>) -> Self {
        Self {
            opts,
            container: container.into(),
        }
    }

    fn base_url(&self, blob: &str, query: Option<&str>) -> String {
        let blob = blob.trim_start_matches('/');
        let base = if blob.is_empty() {
            format!(
                "https://{}.blob.core.windows.net/{}",
                self.opts.account, self.container
            )
        } else {
            format!(
                "https://{}.blob.core.windows.net/{}/{}",
                self.opts.account, self.container, blob
            )
        };
        match query {
            Some(q) if !q.is_empty() => format!("{base}?{q}"),
            _ => base,
        }
    }

    fn auth(
        &self,
        method: &str,
        content_length: &str,
        content_type: &str,
        date: &str,
        ms_headers: &[(String, String)],
        canon_resource: &str,
    ) -> BlobResult<(Option<String>, String)> {
        if let Some(key) = &self.opts.key {
            let auth = shared_key_blob(
                &self.opts.account,
                key,
                method,
                content_length,
                content_type,
                date,
                ms_headers,
                canon_resource,
            );
            let _ = date;
            Ok((Some(auth), String::new()))
        } else if let Some(sas) = &self.opts.sas {
            Ok((None, sas.clone()))
        } else if let Some(bearer) = &self.opts.bearer {
            Ok((Some(format!("Bearer {bearer}")), String::new()))
        } else {
            Ok((None, String::new()))
        }
    }
}

impl ObjectStore for AzureStore {
    fn kind(&self) -> BackendKind {
        BackendKind::Azure
    }

    fn read(&self, key: &str) -> BlobResult<Vec<u8>> {
        let date = rfc1123_now();
        let canon = format!("/{}/{}/{}", self.opts.account, self.container, key);
        let mut ms: Vec<(String, String)> = vec![
            ("x-ms-date".into(), date.clone()),
            ("x-ms-version".into(), BLOB_VERSION.into()),
        ];
        ms.sort_by(|a, b| a.0.cmp(&b.0));
        let (auth_hdr, sas): (Option<String>, String) =
            self.auth("GET", "", "", &date, &ms, &canon)?;
        let url = self.base_url(key, if sas.is_empty() { None } else { Some(&sas) });
        let mut req = niao_http::get(&url)
            .set("x-ms-date", &date)
            .set("x-ms-version", BLOB_VERSION);
        if let Some(h) = auth_hdr {
            req = req.set("Authorization", h);
        }
        let resp = req.send()?;
        if resp.status == 404 {
            return Err(BlobError::not_found(key));
        }
        if !(200..300).contains(&resp.status) {
            let body = String::from_utf8_lossy(&resp.body);
            return Err(BlobError::http(resp.status, &body));
        }
        Ok(resp.body)
    }

    fn write(&self, key: &str, data: &[u8], content_type: Option<&str>) -> BlobResult<u64> {
        let ct = content_type.unwrap_or("application/octet-stream");
        let date = rfc1123_now();
        let cl = data.len().to_string();
        let canon = format!("/{}/{}/{}", self.opts.account, self.container, key);
        let mut ms: Vec<(String, String)> = vec![
            ("x-ms-blob-type".into(), "BlockBlob".into()),
            ("x-ms-date".into(), date.clone()),
            ("x-ms-version".into(), BLOB_VERSION.into()),
        ];
        ms.sort_by(|a, b| a.0.cmp(&b.0));
        let (auth_hdr, sas): (Option<String>, String) =
            self.auth("PUT", &cl, ct, &date, &ms, &canon)?;
        let url = self.base_url(key, if sas.is_empty() { None } else { Some(&sas) });
        let mut req = niao_http::put(&url)
            .set("x-ms-blob-type", "BlockBlob")
            .set("x-ms-date", &date)
            .set("x-ms-version", BLOB_VERSION)
            .set("Content-Type", ct)
            .set("Content-Length", &cl);
        if let Some(h) = auth_hdr {
            req = req.set("Authorization", h);
        }
        let resp = req.send_bytes(data)?;
        if !(200..300).contains(&resp.status) {
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
        let date = rfc1123_now();
        let canon = format!("/{}/{}/{}", self.opts.account, self.container, key);
        let mut ms: Vec<(String, String)> = vec![
            ("x-ms-date".into(), date.clone()),
            ("x-ms-version".into(), BLOB_VERSION.into()),
        ];
        ms.sort_by(|a, b| a.0.cmp(&b.0));
        let (auth_hdr, sas): (Option<String>, String) =
            self.auth("HEAD", "", "", &date, &ms, &canon)?;
        let url = self.base_url(key, if sas.is_empty() { None } else { Some(&sas) });
        let mut req = niao_http::head(&url)
            .set("x-ms-date", &date)
            .set("x-ms-version", BLOB_VERSION);
        if let Some(h) = auth_hdr {
            req = req.set("Authorization", h);
        }
        let resp = req.send()?;
        if resp.status == 404 {
            return Err(BlobError::not_found(key));
        }
        if !(200..300).contains(&resp.status) {
            let body = String::from_utf8_lossy(&resp.body);
            return Err(BlobError::http(resp.status, &body));
        }
        let size = resp
            .header("content-length")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        Ok(Entry {
            name: key.to_string(),
            kind: "file".into(),
            size,
            mtime: None,
        })
    }

    fn list(&self, prefix: &str, _detail: bool) -> BlobResult<Vec<Entry>> {
        let date = rfc1123_now();
        let mut query = format!("restype=container&comp=list");
        if !prefix.is_empty() {
            query.push_str("&prefix=");
            query.push_str(&urlencoding_simple(prefix));
        }
        query.push_str("&delimiter=/");
        let canon = format!(
            "/{}/{}\\ncomp:list\\ndelimiter:/\\nprefix:{}\\nrestype:container",
            self.opts.account, self.container, prefix
        );
        // For list, SharedKey uses newline-separated params; SAS/bearer simpler.
        let mut ms: Vec<(String, String)> = vec![
            ("x-ms-date".into(), date.clone()),
            ("x-ms-version".into(), BLOB_VERSION.into()),
        ];
        ms.sort_by(|a, b| a.0.cmp(&b.0));
        let canon_res = format!("/{}/{}", self.opts.account, self.container);
        let _ = canon;
        let (auth_hdr, sas): (Option<String>, String) =
            self.auth("GET", "", "", &date, &ms, &canon_res)?;
        let qs = if sas.is_empty() {
            query
        } else {
            format!("{query}&{sas}")
        };
        let url = self.base_url("", Some(&qs));
        let mut req = niao_http::get(&url)
            .set("x-ms-date", &date)
            .set("x-ms-version", BLOB_VERSION);
        if let Some(h) = auth_hdr {
            req = req.set("Authorization", h);
        }
        let resp = req.send()?;
        if !(200..300).contains(&resp.status) {
            let body = String::from_utf8_lossy(&resp.body);
            return Err(BlobError::http(resp.status, &body));
        }
        let xml = String::from_utf8_lossy(&resp.body);
        Ok(parse_azure_list_xml(&xml))
    }

    fn remove(&self, key: &str) -> BlobResult<()> {
        let date = rfc1123_now();
        let canon = format!("/{}/{}/{}", self.opts.account, self.container, key);
        let mut ms: Vec<(String, String)> = vec![
            ("x-ms-date".into(), date.clone()),
            ("x-ms-version".into(), BLOB_VERSION.into()),
        ];
        ms.sort_by(|a, b| a.0.cmp(&b.0));
        let (auth_hdr, sas): (Option<String>, String) =
            self.auth("DELETE", "", "", &date, &ms, &canon)?;
        let url = self.base_url(key, if sas.is_empty() { None } else { Some(&sas) });
        let mut req = niao_http::delete(&url)
            .set("x-ms-date", &date)
            .set("x-ms-version", BLOB_VERSION);
        if let Some(h) = auth_hdr {
            req = req.set("Authorization", h);
        }
        let resp = req.send()?;
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

fn urlencoding_simple(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn parse_azure_list_xml(xml: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<Name>") {
        let after = &rest[start + 6..];
        if let Some(end) = after.find("</Name>") {
            let name = after[..end].to_string();
            if name.ends_with('/') {
                out.push(Entry {
                    name: name.trim_end_matches('/').to_string(),
                    kind: "dir".into(),
                    size: 0,
                    mtime: None,
                });
            } else {
                out.push(Entry {
                    name,
                    kind: "file".into(),
                    size: 0,
                    mtime: None,
                });
            }
            rest = &after[end + 7..];
        } else {
            break;
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.name == b.name);
    out
}
