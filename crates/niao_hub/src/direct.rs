//! Resumable direct-URL downloads (Range requests + retries).

use crate::checksum::{verify_file, HashAlgo};
use crate::error::{HubError, HubResult};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use ureq::tls::{TlsConfig, TlsProvider};
use ureq::Agent;

const USER_AGENT: &str = "nhub/0.1.0 (Niao; +https://github.com/niao-lang/niao)";

#[derive(Debug, Clone)]
pub struct DirectOpts {
    pub timeout_ms: u64,
    pub retries: usize,
    pub resume: bool,
    pub expected_sha256: Option<String>,
    pub headers: Vec<(String, String)>,
}

impl Default for DirectOpts {
    fn default() -> Self {
        Self {
            timeout_ms: 60_000,
            retries: 3,
            resume: true,
            expected_sha256: None,
            headers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DirectResult {
    pub path: PathBuf,
    pub bytes: u64,
    pub resumed: bool,
}

fn build_agent(timeout_ms: u64) -> HubResult<Agent> {
    let config = Agent::config_builder()
        .timeout_global(Some(Duration::from_millis(timeout_ms.max(1))))
        .tls_config(TlsConfig::builder().provider(TlsProvider::Rustls).build())
        .max_redirects(5)
        .build();
    Ok(config.into())
}

fn parse_content_length(headers: &ureq::http::HeaderMap) -> Option<u64> {
    headers
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
}

fn parse_total_from_range(headers: &ureq::http::HeaderMap) -> Option<u64> {
    let cr = headers.get("content-range")?.to_str().ok()?;
    cr.split('/').next_back()?.parse().ok()
}

fn jitter_ms() -> u64 {
    (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_millis()
        % 500) as u64
}

fn backoff_ms(attempt: usize) -> u64 {
    (300 + (attempt as u64).pow(2) * 100 + jitter_ms()).min(10_000)
}

fn fetch_chunk(
    agent: &Agent,
    url: &str,
    start: u64,
    dest: &Path,
    append: bool,
    headers: &[(String, String)],
) -> HubResult<(u64, Option<u64>, bool)> {
    let mut req = agent.get(url).header("User-Agent", USER_AGENT);
    for (k, v) in headers {
        req = req.header(k.as_str(), v.as_str());
    }
    if start > 0 {
        req = req.header("Range", format!("bytes={start}-"));
    }

    let response = req.call().map_err(|e| HubError::Network(e.to_string()))?;
    let status = response.status();
    let is_partial = status.as_u16() == 206;
    if !(status.is_success() || is_partial) {
        return Err(HubError::Network(format!("HTTP {status}")));
    }

    let hdrs = response.headers().clone();
    let total = parse_total_from_range(&hdrs).or_else(|| parse_content_length(&hdrs));

    let mut file = if append && start > 0 && is_partial {
        std::fs::OpenOptions::new().append(true).open(dest)?
    } else {
        std::fs::File::create(dest)?
    };

    let mut reader = response.into_body().into_reader();
    let mut buf = [0u8; 65536];
    let mut written = if append && start > 0 && is_partial {
        start
    } else {
        0
    };
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| HubError::Network(e.to_string()))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        written += n as u64;
    }
    file.flush()?;
    Ok((written, total, is_partial))
}

/// Download `url` to `dest` with optional resume and checksum verification.
pub fn download_url(url: &str, dest: &Path, opts: &DirectOpts) -> HubResult<DirectResult> {
    if url.is_empty() {
        return Err(HubError::InvalidArg("url must not be empty".into()));
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let agent = build_agent(opts.timeout_ms)?;
    let mut start: u64 = 0;
    let mut resumed = false;

    if opts.resume && dest.exists() {
        start = std::fs::metadata(dest)?.len();
        resumed = start > 0;
    } else if dest.exists() {
        std::fs::remove_file(dest)?;
    }

    let mut total_size: Option<u64> = None;
    let mut attempt = 0usize;

    loop {
        let append = opts.resume && start > 0;
        match fetch_chunk(&agent, url, start, dest, append, &opts.headers) {
            Ok((written, total, partial)) => {
                start = written;
                if !partial && written > 0 && opts.resume {
                    resumed = false;
                }
                if total_size.is_none() {
                    total_size = total;
                }
                if let Some(total) = total_size {
                    if start < total {
                        if attempt >= opts.retries {
                            return Err(HubError::Network(format!(
                                "incomplete download: {start}/{total} bytes"
                            )));
                        }
                        attempt += 1;
                        std::thread::sleep(Duration::from_millis(backoff_ms(attempt)));
                        continue;
                    }
                }
                break;
            }
            Err(e) => {
                if attempt >= opts.retries {
                    return Err(e);
                }
                attempt += 1;
                std::thread::sleep(Duration::from_millis(backoff_ms(attempt)));
            }
        }
    }

    let bytes = std::fs::metadata(dest)?.len();
    if let Some(ref expected) = opts.expected_sha256 {
        verify_file(dest, expected, HashAlgo::Sha256)?;
    }

    Ok(DirectResult {
        path: dest.to_path_buf(),
        bytes,
        resumed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use niao_http::{OutgoingResponse, Server};
    use std::thread;

    #[test]
    fn direct_download_full() {
        let server = Server::http("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let body = b"hello nhub direct download";
        let body_vec = body.to_vec();
        let handle = thread::spawn(move || {
            let req = server.recv().unwrap();
            assert_eq!(req.method(), "GET");
            req.respond(
                OutgoingResponse::from_data(body_vec.clone())
                    .with_status(200)
                    .header("Content-Length", &body_vec.len().to_string()),
            )
            .unwrap();
        });

        let dir = std::env::temp_dir().join(format!("nhub_dl_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let dest = dir.join("file.bin");
        let url = format!("http://{addr}/data");
        let r = download_url(&url, &dest, &DirectOpts::default()).unwrap();
        assert_eq!(r.bytes, body.len() as u64);
        assert!(!r.resumed);
        assert_eq!(std::fs::read(&dest).unwrap(), body);
        let _ = std::fs::remove_dir_all(&dir);
        handle.join().unwrap();
    }

    #[test]
    fn direct_download_resume() {
        let server = Server::http("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let full = b"0123456789abcdef";
        let handle = thread::spawn(move || {
            let req = server.recv().unwrap();
            let range = req.headers().get("range").unwrap_or("");
            assert!(range.starts_with("bytes=8-"));
            req.respond(
                OutgoingResponse::from_data(full[8..].to_vec())
                    .with_status(206)
                    .header("Content-Range", "bytes 8-15/16"),
            )
            .unwrap();
        });

        let dir = std::env::temp_dir().join(format!("nhub_resume_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let dest = dir.join("part.bin");
        std::fs::write(&dest, &full[..8]).unwrap();
        let url = format!("http://{addr}/big");
        let opts = DirectOpts {
            resume: true,
            retries: 1,
            ..Default::default()
        };
        let r = download_url(&url, &dest, &opts).unwrap();
        assert!(r.resumed);
        assert_eq!(std::fs::read(&dest).unwrap(), full);
        let _ = std::fs::remove_dir_all(&dir);
        handle.join().unwrap();
    }
}
