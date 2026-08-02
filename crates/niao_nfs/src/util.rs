//! Path utilities.

use std::io;
use std::path::{Path, PathBuf};

/// Return `true` if both paths refer to the same file (~`os.path.samefile`).
pub fn samefile(a: &Path, b: &Path) -> io::Result<bool> {
    let ma = std::fs::canonicalize(a)?;
    let mb = std::fs::canonicalize(b)?;
    Ok(ma == mb)
}

/// Locate an executable on `PATH` (~`shutil.which`).
pub fn which(cmd: &str) -> Option<PathBuf> {
    if cmd.is_empty() {
        return None;
    }
    let path_var = std::env::var_os("PATH")?;
    let paths = std::env::split_paths(&path_var);
    #[cfg(windows)]
    let exts: Vec<String> = std::env::var_os("PATHEXT")
        .map(|v| {
            std::env::split_paths(&v)
                .map(|p| p.to_string_lossy().to_ascii_lowercase())
                .collect()
        })
        .unwrap_or_else(|| vec![".exe".into(), ".cmd".into(), ".bat".into(), ".com".into()]);
    for dir in paths {
        let candidate = dir.join(cmd);
        #[cfg(not(windows))]
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            if candidate.is_file() {
                return Some(candidate);
            }
            for ext in &exts {
                let with = dir.join(format!("{cmd}{ext}"));
                if with.is_file() {
                    return Some(with);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn which_path() {
        let got = which(if cfg!(windows) { "cmd" } else { "sh" });
        assert!(got.is_some());
    }
}
