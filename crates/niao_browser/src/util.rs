//! Executable detection and small helpers (no CDP).

use crate::error::{BrowserError, BrowserResult};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Resolve a Chrome / Chromium / Edge executable path.
///
/// Order: `executable` arg → `NBROWSER_EXECUTABLE` / `CHROME` env → auto-detect.
pub fn resolve_executable(explicit: Option<&str>) -> BrowserResult<PathBuf> {
    if let Some(p) = explicit {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Ok(path);
        }
        return Err(BrowserError::ExecutableNotFound(format!(
            "executable not found: {p}"
        )));
    }
    if let Ok(p) = std::env::var("NBROWSER_EXECUTABLE") {
        let path = PathBuf::from(&p);
        if path.is_file() {
            return Ok(path);
        }
        return Err(BrowserError::ExecutableNotFound(format!(
            "NBROWSER_EXECUTABLE not found: {p}"
        )));
    }
    if let Ok(p) = std::env::var("CHROME") {
        let path = PathBuf::from(&p);
        if path.is_file() {
            return Ok(path);
        }
    }
    detect_executable().ok_or_else(|| {
        BrowserError::ExecutableNotFound("Could not auto detect a chrome/edge executable".into())
    })
}

/// Detect an executable without failing hard — returns `None` if missing.
pub fn executable_path() -> Option<String> {
    resolve_executable(None)
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

fn detect_executable() -> Option<PathBuf> {
    let candidates: &[&str] = if cfg!(windows) {
        &[
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
            r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
            r"C:\Program Files\Chromium\Application\chrome.exe",
        ]
    } else if cfg!(target_os = "macos") {
        &[
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
        ]
    } else {
        &[
            "/usr/bin/google-chrome-stable",
            "/usr/bin/google-chrome",
            "/usr/bin/chromium",
            "/usr/bin/chromium-browser",
            "/snap/bin/chromium",
            "/usr/bin/microsoft-edge",
            "/usr/bin/microsoft-edge-stable",
        ]
    };
    for c in candidates {
        let p = Path::new(c);
        if p.is_file() {
            return Some(p.to_path_buf());
        }
    }
    // PATH lookup
    for name in [
        "chrome",
        "google-chrome",
        "chromium",
        "chromium-browser",
        "msedge",
        "microsoft-edge",
    ] {
        if let Some(p) = which(name) {
            return Some(p);
        }
    }
    None
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let exe = dir.join(format!("{name}.exe"));
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    None
}

/// Reject empty CSS selectors early (before CDP).
pub fn require_selector(selector: &str) -> BrowserResult<&str> {
    let s = selector.trim();
    if s.is_empty() {
        return Err(BrowserError::msg("selector must not be empty"));
    }
    Ok(s)
}

/// Reject empty URLs.
pub fn require_url(url: &str) -> BrowserResult<&str> {
    let u = url.trim();
    if u.is_empty() {
        return Err(BrowserError::msg("url must not be empty"));
    }
    Ok(u)
}

/// Escape a string for embedding inside a single-quoted JS literal.
pub fn js_string_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            c => out.push(c),
        }
    }
    out.push('\'');
    out
}

/// Poll `f` until it returns `Ok(Some(T))` or timeout.
pub fn poll_until<T, F>(timeout: Duration, mut f: F) -> BrowserResult<T>
where
    F: FnMut() -> BrowserResult<Option<T>>,
{
    let deadline = Instant::now() + timeout;
    let mut last = BrowserError::Timeout("wait condition not met".into());
    loop {
        match f() {
            Ok(Some(v)) => return Ok(v),
            Ok(None) => {}
            Err(e) => {
                last = e;
            }
        }
        if Instant::now() >= deadline {
            return Err(last);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_selector_empty() {
        assert!(require_selector("").is_err());
        assert!(require_selector("   ").is_err());
        assert_eq!(require_selector(" #a ").unwrap(), "#a");
    }

    #[test]
    fn require_url_empty() {
        assert!(require_url("").is_err());
        assert_eq!(require_url(" about:blank ").unwrap(), "about:blank");
    }

    #[test]
    fn js_escape_quotes() {
        assert_eq!(js_string_literal("a'b\\c"), "'a\\'b\\\\c'");
    }
}
