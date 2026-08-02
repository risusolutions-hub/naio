//! multipart/form-data body builder.

use crate::error::{ReqError, ReqResult};
use niao_rand::{thread_rng, Rng};
use std::sync::atomic::{AtomicU64, Ordering};

static BOUNDARY_SEQ: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub struct MultipartPart {
    pub name: String,
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub data: Vec<u8>,
}

impl MultipartPart {
    pub fn field(name: impl Into<String>, value: impl AsRef<[u8]>) -> Self {
        Self {
            name: name.into(),
            filename: None,
            content_type: None,
            data: value.as_ref().to_vec(),
        }
    }

    pub fn file(
        name: impl Into<String>,
        filename: impl Into<String>,
        data: impl AsRef<[u8]>,
        content_type: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            filename: Some(filename.into()),
            content_type,
            data: data.as_ref().to_vec(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MultipartBody {
    pub boundary: String,
    pub body: Vec<u8>,
}

impl MultipartBody {
    pub fn content_type(&self) -> String {
        format!("multipart/form-data; boundary={}", self.boundary)
    }
}

/// Generate a unique multipart boundary.
pub fn random_boundary() -> String {
    let seq = BOUNDARY_SEQ.fetch_add(1, Ordering::Relaxed);
    let mut rng = thread_rng();
    let mut s = String::from("----nreq");
    s.push_str(&format!("{seq:x}"));
    for _ in 0..12 {
        s.push_str(&format!("{:02x}", (rng.next_u64() & 0xff) as u8));
    }
    s
}

/// Encode multipart parts into a body + boundary.
pub fn build_multipart(
    parts: &[MultipartPart],
    boundary: Option<&str>,
) -> ReqResult<MultipartBody> {
    if parts.is_empty() {
        return Err(ReqError::Config(
            "multipart requires at least one part".into(),
        ));
    }
    let boundary = boundary.map(str::to_string).unwrap_or_else(random_boundary);
    if boundary.is_empty() || boundary.contains('\r') || boundary.contains('\n') {
        return Err(ReqError::Config("invalid multipart boundary".into()));
    }
    let mut body = Vec::with_capacity(parts.iter().map(|p| p.data.len() + 128).sum());
    for part in parts {
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"\r\n");
        if let Some(fname) = &part.filename {
            body.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
                    escape_qd(&part.name),
                    escape_qd(fname)
                )
                .as_bytes(),
            );
            let ct = part
                .content_type
                .as_deref()
                .unwrap_or("application/octet-stream");
            body.extend_from_slice(format!("Content-Type: {ct}\r\n\r\n").as_bytes());
        } else {
            body.extend_from_slice(
                format!(
                    "Content-Disposition: form-data; name=\"{}\"\r\n\r\n",
                    escape_qd(&part.name)
                )
                .as_bytes(),
            );
        }
        body.extend_from_slice(&part.data);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"--\r\n");
    Ok(MultipartBody { boundary, body })
}

fn escape_qd(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_field_and_file() {
        let parts = vec![
            MultipartPart::field("title", b"hello"),
            MultipartPart::file("file", "a.txt", b"content", Some("text/plain".into())),
        ];
        let mp = build_multipart(&parts, Some("BOUND")).unwrap();
        let s = String::from_utf8_lossy(&mp.body);
        assert!(s.contains("--BOUND"));
        assert!(s.contains("name=\"title\""));
        assert!(s.contains("filename=\"a.txt\""));
        assert!(s.contains("content"));
        assert!(s.ends_with("--BOUND--\r\n"));
    }

    #[test]
    fn empty_parts_error() {
        assert!(build_multipart(&[], None).is_err());
    }
}
