//! Atomic write via temp file + rename.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Atomic write options.
#[derive(Debug, Clone)]
pub struct AtomicWriteOpts {
    pub dir: Option<PathBuf>,
    pub mode: Option<u32>,
    pub fsync: bool,
}

impl Default for AtomicWriteOpts {
    fn default() -> Self {
        Self {
            dir: None,
            mode: None,
            fsync: true,
        }
    }
}

/// Write UTF-8 text atomically.
pub fn write_atomic(path: &Path, text: &str, opts: &AtomicWriteOpts) -> io::Result<()> {
    write_bytes_atomic(path, text.as_bytes(), opts)
}

/// Write bytes atomically (temp in target directory, then rename).
pub fn write_bytes_atomic(path: &Path, data: &[u8], opts: &AtomicWriteOpts) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .or_else(|| opts.dir.clone())
        .unwrap_or_else(|| PathBuf::from("."));

    fs::create_dir_all(&parent)?;

    let tmp_dir = opts.dir.clone().unwrap_or_else(|| parent.clone());
    let mut tmp = tempfile::Builder::new()
        .prefix(&format!(".{}.", path_file_name(path)))
        .suffix(".tmp")
        .tempfile_in(&tmp_dir)?;
    tmp.write_all(data)?;
    if opts.fsync {
        tmp.as_file().sync_all()?;
    }
    tmp.persist(path).map_err(|e| e.error)?;

    #[cfg(unix)]
    if let Some(mode) = opts.mode {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }

    Ok(())
}

fn path_file_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn atomic_write_survives() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("out.txt");
        write_atomic(&path, "hello", &AtomicWriteOpts::default()).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "hello");
    }
}
