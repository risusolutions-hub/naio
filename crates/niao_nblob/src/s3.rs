//! S3 backend via SigV4 + `niao_http` (~naws S3 subset).

use crate::error::{BlobError, BlobResult};
use crate::sigv4::{now_amz, sign, uri_encode, SignInput};
use crate::store::{BackendKind, Entry, ObjectStore, S3Opts};

#[derive(Debug, Clone)]
pub struct S3Store {
    pub opts: S3Opts,
    pub bucket: String,
}

impl S3Store {
    pub fn new(opts: S3Opts, bucket: impl Into<String>) -> Self {
        Self {
            opts,
            bucket: bucket.into(),
        }
    }

    fn host(&self) -> String {
        if let Some(ep) = &self.opts.endpoint {
            // endpoint may be host or full URL — strip scheme
            let ep = ep
                .trim()
                .trim_start_matches("https://")
                .trim_start_matches("http://")
                .trim_end_matches('/');
            if ep.contains('{') {
                return ep.replace("{bucket}", &self.bucket);
            }
            // path-style: endpoint/bucket
            return ep.to_string();
        }
        format!("{}.s3.{}.amazonaws.com", self.bucket, self.opts.region)
    }

    fn path_style(&self) -> bool {
        self.opts.endpoint.is_some()
    }

    fn object_path(&self, key: &str) -> String {
        let key = key.trim_start_matches('/');
        if self.path_style() {
            format!("/{}/{}", self.bucket, key)
        } else {
            format!("/{}", key)
        }
    }

    fn url(&self, path: &str, query: &str) -> String {
        let host = self.host();
        if query.is_empty() {
            format!("https://{host}{path}")
        } else {
            format!("https://{host}{path}?{query}")
        }
    }
}

impl ObjectStore for S3Store {
    fn kind(&self) -> BackendKind {
        BackendKind::S3
    }

    fn read(&self, key: &str) -> BlobResult<Vec<u8>> {
        let host = self.host();
        let path = self.object_path(key);
        let (amz_dt, amz_d) = now_amz();
        let inp = SignInput {
            method: "GET",
            host: &host,
            path: &path,
            query: "",
            region: &self.opts.region,
            service: "s3",
            access_key: &self.opts.access_key,
            secret_key: &self.opts.secret_key,
            session_token: self.opts.session_token.as_deref(),
            body: b"",
            amz_datetime: &amz_dt,
            amz_date: &amz_d,
            extra_headers: &[],
        };
        let signed = sign(&inp);
        let url = self.url(&path, "");
        let mut builder = niao_http::get(&url);
        for (k, v) in &signed.headers {
            builder = builder.set(k.clone(), v.clone());
        }
        let resp = builder.send()?;
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
        let host = self.host();
        let path = self.object_path(key);
        let (amz_dt, amz_d) = now_amz();
        let extra = [("content-type", ct)];
        let inp = SignInput {
            method: "PUT",
            host: &host,
            path: &path,
            query: "",
            region: &self.opts.region,
            service: "s3",
            access_key: &self.opts.access_key,
            secret_key: &self.opts.secret_key,
            session_token: self.opts.session_token.as_deref(),
            body: data,
            amz_datetime: &amz_dt,
            amz_date: &amz_d,
            extra_headers: &extra,
        };
        let signed = sign(&inp);
        let url = self.url(&path, "");
        let mut builder = niao_http::put(&url).set("Content-Type", ct);
        for (k, v) in &signed.headers {
            builder = builder.set(k.clone(), v.clone());
        }
        let resp = builder.send_bytes(data)?;
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
        let host = self.host();
        let path = self.object_path(key);
        let (amz_dt, amz_d) = now_amz();
        let inp = SignInput {
            method: "HEAD",
            host: &host,
            path: &path,
            query: "",
            region: &self.opts.region,
            service: "s3",
            access_key: &self.opts.access_key,
            secret_key: &self.opts.secret_key,
            session_token: self.opts.session_token.as_deref(),
            body: b"",
            amz_datetime: &amz_dt,
            amz_date: &amz_d,
            extra_headers: &[],
        };
        let signed = sign(&inp);
        let url = self.url(&path, "");
        let mut builder = niao_http::head(&url);
        for (k, v) in &signed.headers {
            builder = builder.set(k.clone(), v.clone());
        }
        let resp = builder.send()?;
        if resp.status == 404 {
            return Err(BlobError::not_found(key));
        }
        if resp.status >= 400 {
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
        let host = self.host();
        let path = if self.path_style() {
            format!("/{}", self.bucket)
        } else {
            "/".into()
        };
        let mut query = String::from("list-type=2");
        if !prefix.is_empty() {
            query.push_str("&prefix=");
            query.push_str(&uri_encode(prefix, true));
        }
        query.push_str("&delimiter=");
        query.push_str(&uri_encode("/", true));
        let (amz_dt, amz_d) = now_amz();
        let inp = SignInput {
            method: "GET",
            host: &host,
            path: &path,
            query: &query,
            region: &self.opts.region,
            service: "s3",
            access_key: &self.opts.access_key,
            secret_key: &self.opts.secret_key,
            session_token: self.opts.session_token.as_deref(),
            body: b"",
            amz_datetime: &amz_dt,
            amz_date: &amz_d,
            extra_headers: &[],
        };
        let signed = sign(&inp);
        let url = self.url(&path, &query);
        let mut builder = niao_http::get(&url);
        for (k, v) in &signed.headers {
            builder = builder.set(k.clone(), v.clone());
        }
        let resp = builder.send()?;
        if resp.status >= 400 {
            let body = String::from_utf8_lossy(&resp.body);
            return Err(BlobError::http(resp.status, &body));
        }
        let xml = String::from_utf8_lossy(&resp.body);
        Ok(parse_s3_list_xml(&xml))
    }

    fn remove(&self, key: &str) -> BlobResult<()> {
        let host = self.host();
        let path = self.object_path(key);
        let (amz_dt, amz_d) = now_amz();
        let inp = SignInput {
            method: "DELETE",
            host: &host,
            path: &path,
            query: "",
            region: &self.opts.region,
            service: "s3",
            access_key: &self.opts.access_key,
            secret_key: &self.opts.secret_key,
            session_token: self.opts.session_token.as_deref(),
            body: b"",
            amz_datetime: &amz_dt,
            amz_date: &amz_d,
            extra_headers: &[],
        };
        let signed = sign(&inp);
        let url = self.url(&path, "");
        let mut builder = niao_http::delete(&url);
        for (k, v) in &signed.headers {
            builder = builder.set(k.clone(), v.clone());
        }
        let resp = builder.send()?;
        if resp.status >= 400 && resp.status != 404 {
            let body = String::from_utf8_lossy(&resp.body);
            return Err(BlobError::http(resp.status, &body));
        }
        Ok(())
    }

    fn mkdir(&self, _key: &str) -> BlobResult<()> {
        // S3 has no real directories; prefixes are virtual.
        Ok(())
    }
}

fn parse_s3_list_xml(xml: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    // Keys
    let mut rest = xml;
    while let Some(start) = rest.find("<Key>") {
        let after = &rest[start + 5..];
        if let Some(end) = after.find("</Key>") {
            let key = after[..end].to_string();
            let size = extract_nearby_size(rest, start).unwrap_or(0);
            out.push(Entry {
                name: key,
                kind: "file".into(),
                size,
                mtime: None,
            });
            rest = &after[end + 6..];
        } else {
            break;
        }
    }
    // CommonPrefixes
    rest = xml;
    while let Some(start) = rest.find("<Prefix>") {
        // skip the request Prefix at top — CommonPrefixes contain nested Prefix
        let after = &rest[start + 8..];
        if let Some(end) = after.find("</Prefix>") {
            let p = after[..end].to_string();
            if p.ends_with('/') {
                out.push(Entry {
                    name: p.trim_end_matches('/').to_string(),
                    kind: "dir".into(),
                    size: 0,
                    mtime: None,
                });
            }
            rest = &after[end + 9..];
        } else {
            break;
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.name == b.name);
    out
}

fn extract_nearby_size(xml: &str, key_pos: usize) -> Option<u64> {
    let window = &xml[key_pos..key_pos.saturating_add(400).min(xml.len())];
    let start = window.find("<Size>")?;
    let after = &window[start + 6..];
    let end = after.find("</Size>")?;
    after[..end].parse().ok()
}
