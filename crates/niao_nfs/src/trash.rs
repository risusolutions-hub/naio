//! Send files to the system trash (~`send2trash`).

use std::io;
use std::path::Path;

/// Move `path` to the platform recycle bin / trash.
pub fn trash_path(path: &Path) -> io::Result<()> {
    trash::delete(path).map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))
}

/// Trash multiple paths; stops on first error.
pub fn trash_all(paths: &[&Path]) -> io::Result<()> {
    for p in paths {
        trash_path(p)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    #[ignore] // platform trash folder; manual verification
    fn trash_file() {
        let dir = TempDir::new().unwrap();
        let f = dir.path().join("discard.txt");
        fs::write(&f, b"x").unwrap();
        trash_path(&f).unwrap();
        assert!(!f.exists());
    }
}
