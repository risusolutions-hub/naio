//! Fast buffered file copy and metadata helpers.

use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

/// Copy options (~`shutil.copy` / `copy2`).
#[derive(Debug, Clone)]
pub struct CopyOpts {
    /// Copy file contents only (no metadata) when `false`; when `true`, preserve
    /// mode, timestamps, and platform xattrs where supported.
    pub metadata: bool,
    /// Follow symlinks when copying a single file.
    pub follow_symlinks: bool,
}

impl Default for CopyOpts {
    fn default() -> Self {
        Self {
            metadata: false,
            follow_symlinks: true,
        }
    }
}

/// Options for recursive directory copy (~`shutil.copytree`).
#[derive(Debug, Clone)]
pub struct CopyTreeOpts {
    pub dirs_exist_ok: bool,
    pub symlinks: bool,
    pub ignore_patterns: Vec<String>,
    pub metadata: bool,
    pub threads: usize,
}

pub fn copy_tree_opts_default() -> CopyTreeOpts {
    CopyTreeOpts {
        dirs_exist_ok: false,
        symlinks: false,
        ignore_patterns: Vec::new(),
        metadata: true,
        threads: niao_parallel::available_threads(),
    }
}

const COPY_BUF: usize = 1024 * 1024;

/// Copy a single file with a 1 MiB buffered stream.
pub fn copy_file(src: &Path, dst: &Path, opts: &CopyOpts) -> io::Result<u64> {
    let src_meta = fs::symlink_metadata(src)?;
    if src_meta.is_symlink() && !opts.follow_symlinks {
        return copy_symlink(src, dst);
    }
    if src_meta.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::IsADirectory,
            "copy: source is a directory (use copytree)",
        ));
    }
    let real_src = if opts.follow_symlinks {
        fs::canonicalize(src).unwrap_or_else(|_| src.to_path_buf())
    } else {
        src.to_path_buf()
    };
    if let Some(parent) = dst.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let mut reader = fs::File::open(&real_src)?;
    let mut writer = fs::File::create(dst)?;
    let nbytes = copy_buffered(&mut reader, &mut writer)?;
    if opts.metadata {
        copy_stat(src, dst)?;
    }
    Ok(nbytes)
}

/// Copy with metadata (~`shutil.copy2`).
pub fn copy2(src: &Path, dst: &Path) -> io::Result<u64> {
    copy_file(
        src,
        dst,
        &CopyOpts {
            metadata: true,
            follow_symlinks: true,
        },
    )
}

/// Copy file contents only (~`shutil.copyfile`).
pub fn copyfile(src: &Path, dst: &Path) -> io::Result<u64> {
    copy_file(src, dst, &CopyOpts::default())
}

/// Copy permission bits (~`shutil.copymode`).
pub fn copy_mode(src: &Path, dst: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(src)?.permissions().mode();
        fs::set_permissions(dst, fs::Permissions::from_mode(mode))?;
        return Ok(());
    }
    #[cfg(not(unix))]
    {
        let readonly = fs::metadata(src)?.permissions().readonly();
        let mut perms = fs::metadata(dst)?.permissions();
        perms.set_readonly(readonly);
        fs::set_permissions(dst, perms)?;
        Ok(())
    }
}

/// Copy stat metadata (mode, atime, mtime).
pub fn copy_stat(src: &Path, dst: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(src)?;
    copy_mode(src, dst)?;
    let atime = meta.accessed().ok();
    let mtime = meta.modified().ok();
    if let (Some(a), Some(m)) = (atime, mtime) {
        filetime::set_file_times(
            dst,
            filetime::FileTime::from_system_time(a),
            filetime::FileTime::from_system_time(m),
        )?;
    }
    Ok(())
}

fn copy_symlink(src: &Path, dst: &Path) -> io::Result<u64> {
    let target = fs::read_link(src)?;
    if dst.exists() {
        fs::remove_file(dst)?;
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, dst)?;
    #[cfg(windows)]
    {
        if target.is_dir() {
            std::os::windows::fs::symlink_dir(&target, dst)?;
        } else {
            std::os::windows::fs::symlink_file(&target, dst)?;
        }
    }
    Ok(0)
}

fn copy_buffered<R: Read, W: Write>(reader: &mut R, writer: &mut W) -> io::Result<u64> {
    let mut buf = vec![0u8; COPY_BUF];
    let mut total = 0u64;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        writer.write_all(&buf[..n])?;
        total += n as u64;
    }
    Ok(total)
}

/// Recursive directory copy with optional parallel file copies.
pub fn copy_tree(src: &Path, dst: &Path, opts: &CopyTreeOpts) -> io::Result<()> {
    if !src.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            "copytree: source is not a directory",
        ));
    }
    if dst.exists() {
        if !opts.dirs_exist_ok {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "copytree: destination already exists",
            ));
        }
    } else {
        fs::create_dir_all(dst)?;
    }

    let mut plan: Vec<(std::path::PathBuf, std::path::PathBuf, TreeKind)> = Vec::new();
    collect_tree_plan(src, dst, src, opts, &mut plan)?;

    for (_s, d, kind) in &plan {
        if let TreeKind::Dir = kind {
            fs::create_dir_all(d)?;
        }
    }

    let files: Vec<_> = plan
        .iter()
        .filter(|(_, _, k)| matches!(k, TreeKind::File))
        .map(|(s, d, _)| (s.clone(), d.clone()))
        .collect();

    let copy_opts = CopyOpts {
        metadata: opts.metadata,
        follow_symlinks: true,
    };
    let threads = opts.threads.max(1);
    if files.len() <= 1 || threads == 1 {
        for (s, d) in &files {
            copy_file(s, d, &copy_opts)?;
        }
    } else {
        niao_parallel::try_map(&files, threads, |(s, d)| copy_file(s, d, &copy_opts))?;
    }

    for (s, d, kind) in &plan {
        if let TreeKind::Symlink = kind {
            copy_symlink(s, d)?;
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum TreeKind {
    Dir,
    File,
    Symlink,
}

fn should_ignore(name: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|p| {
        if p.contains('*') {
            glob_match(p, name)
        } else {
            name == p
        }
    })
}

fn glob_match(pattern: &str, name: &str) -> bool {
    if let Some(rest) = pattern.strip_prefix('*') {
        if rest.is_empty() {
            return true;
        }
        return name.ends_with(rest);
    }
    if let Some(rest) = pattern.strip_suffix('*') {
        return name.starts_with(rest);
    }
    pattern == name
}

fn collect_tree_plan(
    root_src: &Path,
    root_dst: &Path,
    current: &Path,
    opts: &CopyTreeOpts,
    out: &mut Vec<(std::path::PathBuf, std::path::PathBuf, TreeKind)>,
) -> io::Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if should_ignore(&name_str, &opts.ignore_patterns) {
            continue;
        }
        let src_path = entry.path();
        let rel = src_path.strip_prefix(root_src).unwrap_or(&src_path);
        let dst_path = root_dst.join(rel);
        let meta = entry.metadata()?;
        if meta.is_dir() {
            out.push((src_path.clone(), dst_path.clone(), TreeKind::Dir));
            collect_tree_plan(root_src, root_dst, &src_path, opts, out)?;
        } else if meta.is_symlink() && opts.symlinks {
            out.push((src_path, dst_path, TreeKind::Symlink));
        } else if meta.is_file() || (meta.is_symlink() && !opts.symlinks) {
            out.push((src_path, dst_path, TreeKind::File));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn copy_file_roundtrip() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("a.txt");
        let dst = dir.path().join("b.txt");
        fs::write(&src, b"payload").unwrap();
        let n = copyfile(&src, &dst).unwrap();
        assert_eq!(n, 7);
        assert_eq!(fs::read(&dst).unwrap(), b"payload");
    }

    #[test]
    fn copytree_basic() {
        let src_root = TempDir::new().unwrap();
        let dst_root = TempDir::new().unwrap();
        let a = src_root.path().join("sub");
        fs::create_dir(&a).unwrap();
        fs::write(a.join("f.txt"), b"x").unwrap();
        let dst = dst_root.path().join("tree");
        copy_tree(src_root.path(), &dst, &copy_tree_opts_default()).unwrap();
        assert_eq!(fs::read(dst.join("sub").join("f.txt")).unwrap(), b"x");
    }
}
