use crate::archive::{ExtractOptions, ZipReader};
use crate::error::{ZipError, ZipResult};
use std::fs;
use std::io::copy;
use std::path::{Component, Path, PathBuf};

/// Extract one entry to `dest_dir` / entry relative path.
pub fn extract_one(
    archive_path: &Path,
    name: &str,
    dest_dir: &Path,
    password: Option<&[u8]>,
    overwrite: bool,
) -> ZipResult<PathBuf> {
    let mut reader = ZipReader::open(
        archive_path,
        &crate::archive::OpenOptions {
            password: password.map(|p| p.to_vec()),
        },
    )?;
    let info = reader.getinfo(name)?;
    let out_path = safe_join(dest_dir, name)?;
    if info.is_dir {
        fs::create_dir_all(&out_path)?;
        return Ok(out_path);
    }
    if out_path.exists() && !overwrite {
        return Err(ZipError::Archive(format!(
            "extract: destination exists: {}",
            out_path.display()
        )));
    }
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut src = reader.open_by_name(name)?;
    let mut dst = fs::File::create(&out_path)?;
    copy(&mut src, &mut dst)?;
    Ok(out_path)
}

/// Extract every entry under `dest_dir`.
pub fn extract_all(
    archive_path: &Path,
    dest_dir: &Path,
    opts: &ExtractOptions,
) -> ZipResult<Vec<PathBuf>> {
    fs::create_dir_all(dest_dir)?;
    let mut reader = ZipReader::open(
        archive_path,
        &crate::archive::OpenOptions {
            password: opts.password.clone(),
        },
    )?;
    let names = reader.namelist()?;
    let threads = opts
        .threads
        .unwrap_or_else(niao_parallel::available_threads);
    let overwrite = opts.overwrite;
    let pwd = opts.password.clone();

    if threads <= 1 || names.len() < 4 {
        let mut out = Vec::with_capacity(names.len());
        for name in names {
            out.push(extract_one(
                archive_path,
                &name,
                dest_dir,
                pwd.as_deref(),
                overwrite,
            )?);
        }
        return Ok(out);
    }

    let path = archive_path.to_path_buf();
    let dest = dest_dir.to_path_buf();
    let pwd_copy = pwd.clone();
    let results = niao_parallel::map(&names, threads, |name| {
        extract_one(&path, name, &dest, pwd_copy.as_deref(), overwrite)
    });
    let mut out = Vec::with_capacity(results.len());
    for r in results {
        out.push(r?);
    }
    Ok(out)
}

/// Reject path traversal (`..`, absolute paths).
pub fn safe_join(base: &Path, entry: &str) -> ZipResult<PathBuf> {
    let rel = Path::new(entry);
    if rel.is_absolute() {
        return Err(ZipError::Archive(format!(
            "zip slip: absolute path {entry}"
        )));
    }
    for comp in rel.components() {
        if matches!(comp, Component::ParentDir) {
            return Err(ZipError::Archive(format!(
                "zip slip: parent segment in {entry}"
            )));
        }
    }
    Ok(base.join(rel))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_join_rejects_dotdot() {
        assert!(safe_join(Path::new("/tmp"), "../etc/passwd").is_err());
    }

    #[test]
    fn safe_join_ok() {
        let p = safe_join(Path::new("/tmp"), "a/b.txt").unwrap();
        assert_eq!(p, Path::new("/tmp/a/b.txt"));
    }
}
