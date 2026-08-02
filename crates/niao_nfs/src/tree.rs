//! Directory walk, move, remove tree, disk usage.

use niao_parallel;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Disk usage for a path (~`shutil.disk_usage`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskUsage {
    pub total: u64,
    pub used: u64,
    pub free: u64,
}

/// Walk options.
#[derive(Debug, Clone)]
pub struct WalkOpts {
    pub topdown: bool,
    pub follow_symlinks: bool,
}

impl Default for WalkOpts {
    fn default() -> Self {
        Self {
            topdown: true,
            follow_symlinks: false,
        }
    }
}

/// One entry from a directory walk.
#[derive(Debug, Clone)]
pub struct WalkEntry {
    pub root: PathBuf,
    pub dirs: Vec<String>,
    pub files: Vec<String>,
}

/// Remove-tree options.
#[derive(Debug, Clone)]
pub struct RmTreeOpts {
    pub ignore_errors: bool,
    pub ignore_patterns: Vec<String>,
}

impl Default for RmTreeOpts {
    fn default() -> Self {
        Self {
            ignore_errors: false,
            ignore_patterns: Vec::new(),
        }
    }
}

/// Move/rename a file or directory (~`shutil.move`).
pub fn move_path(src: &Path, dst: &Path) -> io::Result<()> {
    match fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(e) if cross_device(&e) => {
            if src.is_dir() {
                crate::copy::copy_tree(src, dst, &crate::copy::copy_tree_opts_default())?;
                rmtree(src, &RmTreeOpts::default())?;
            } else {
                crate::copy::copyfile(src, dst)?;
                fs::remove_file(src)?;
            }
            Ok(())
        }
        Err(e) => Err(e),
    }
}

fn cross_device(e: &io::Error) -> bool {
    #[cfg(unix)]
    {
        e.raw_os_error() == Some(libc::EXDEV)
    }
    #[cfg(windows)]
    {
        e.raw_os_error() == Some(17) // ERROR_NOT_SAME_DEVICE
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = e;
        false
    }
}

/// Remove a directory tree (~`shutil.rmtree`).
pub fn rmtree(path: &Path, opts: &RmTreeOpts) -> io::Result<()> {
    if !path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("rmtree: {} does not exist", path.display()),
        ));
    }
    remove_recursive(path, opts)
}

fn remove_recursive(path: &Path, opts: &RmTreeOpts) -> io::Result<()> {
    if path.is_symlink() {
        return fs::remove_file(path);
    }
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = match entry {
                Ok(e) => e,
                Err(_e) if opts.ignore_errors => continue,
                Err(e) => return Err(e),
            };
            let name = entry.file_name().to_string_lossy().to_string();
            if opts.ignore_patterns.iter().any(|p| glob_match(p, &name)) {
                continue;
            }
            let child = entry.path();
            if let Err(e) = remove_recursive(&child, opts) {
                if opts.ignore_errors {
                    continue;
                }
                return Err(e);
            }
        }
        return fs::remove_dir(path);
    }
    fs::remove_file(path)
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

/// Total/free disk space for the volume containing `path`.
pub fn disk_usage(path: &Path) -> io::Result<DiskUsage> {
    let canonical = if path.exists() {
        fs::canonicalize(path)?
    } else if let Some(parent) = path.parent() {
        if parent.as_os_str().is_empty() {
            fs::canonicalize(".")?
        } else {
            fs::canonicalize(parent)?
        }
    } else {
        fs::canonicalize(".")?
    };
    platform_disk_usage(&canonical)
}

/// Sum of file sizes under `path` (parallel when large).
pub fn tree_size(path: &Path, threads: usize) -> io::Result<u64> {
    let mut files = Vec::new();
    collect_file_sizes(path, &mut files)?;
    if files.is_empty() {
        return Ok(0);
    }
    let nt = threads.max(1);
    if files.len() == 1 || nt == 1 {
        return Ok(files.iter().map(|(_, s)| *s).sum());
    }
    let sizes: Vec<u64> = files.iter().map(|(_, s)| *s).collect();
    Ok(niao_parallel::chunks_map_reduce(
        &sizes,
        nt,
        4096,
        0u64,
        |chunk| chunk.iter().sum(),
        |a, b| a + b,
    ))
}

fn collect_file_sizes(path: &Path, out: &mut Vec<(PathBuf, u64)>) -> io::Result<()> {
    if path.is_symlink() {
        return Ok(());
    }
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            collect_file_sizes(&entry?.path(), out)?;
        }
        return Ok(());
    }
    let len = fs::metadata(path)?.len();
    out.push((path.to_path_buf(), len));
    Ok(())
}

/// Directory walk returning one record per visited directory.
pub fn walk(root: &Path, opts: &WalkOpts) -> io::Result<Vec<WalkEntry>> {
    let mut out = Vec::new();
    walk_inner(root, root, opts, &mut out)?;
    if !opts.topdown {
        out.reverse();
    }
    Ok(out)
}

fn walk_inner(
    root: &Path,
    current: &Path,
    opts: &WalkOpts,
    out: &mut Vec<WalkEntry>,
) -> io::Result<()> {
    let meta = fs::symlink_metadata(current)?;
    if meta.is_symlink() && !opts.follow_symlinks {
        return Ok(());
    }
    let read = if meta.is_dir() {
        fs::read_dir(current)?
    } else {
        return Ok(());
    };
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    let mut child_dirs = Vec::new();
    for entry in read {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let child_meta = entry.metadata()?;
        if child_meta.is_dir() {
            dirs.push(name.clone());
            child_dirs.push(entry.path());
        } else {
            files.push(name);
        }
    }
    dirs.sort_unstable();
    files.sort_unstable();
    if opts.topdown {
        out.push(WalkEntry {
            root: current.to_path_buf(),
            dirs: dirs.clone(),
            files: files.clone(),
        });
    }
    for child in child_dirs {
        walk_inner(root, &child, opts, out)?;
    }
    if !opts.topdown {
        out.push(WalkEntry {
            root: current.to_path_buf(),
            dirs,
            files,
        });
    }
    Ok(())
}

/// Longest common path prefix for a list of paths (~`os.path.commonprefix` style).
pub fn common_prefix(paths: &[PathBuf]) -> PathBuf {
    if paths.is_empty() {
        return PathBuf::new();
    }
    let mut components: Vec<_> = paths[0].components().collect();
    for path in &paths[1..] {
        let comps: Vec<_> = path.components().collect();
        let mut n = 0;
        while n < components.len() && n < comps.len() && components[n] == comps[n] {
            n += 1;
        }
        components.truncate(n);
        if components.is_empty() {
            break;
        }
    }
    components.iter().collect()
}

#[cfg(unix)]
fn platform_disk_usage(path: &Path) -> io::Result<DiskUsage> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let cpath = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path"))?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::statvfs(cpath.as_ptr(), &mut stat) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    let block = stat.f_frsize as u64;
    let total = stat.f_blocks as u64 * block;
    let free = stat.f_bfree as u64 * block;
    let avail = stat.f_bavail as u64 * block;
    let used = total.saturating_sub(avail);
    Ok(DiskUsage { total, used, free })
}

#[cfg(windows)]
fn platform_disk_usage(path: &Path) -> io::Result<DiskUsage> {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut free_avail = 0u64;
    let mut total = 0u64;
    let mut free = 0u64;
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free_avail as *mut _,
            &mut total as *mut _,
            &mut free as *mut _,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    let used = total.saturating_sub(free_avail);
    Ok(DiskUsage {
        total,
        used,
        free: free_avail,
    })
}

#[cfg(not(any(unix, windows)))]
fn platform_disk_usage(_path: &Path) -> io::Result<DiskUsage> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "disk_usage not supported on this platform",
    ))
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GetDiskFreeSpaceExW(
        path: *const u16,
        free_avail: *mut u64,
        total: *mut u64,
        free: *mut u64,
    ) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn rmtree_removes_all() {
        let dir = TempDir::new().unwrap();
        let sub = dir.path().join("a/b");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("f"), b"x").unwrap();
        rmtree(dir.path(), &RmTreeOpts::default()).unwrap();
        assert!(!dir.path().exists());
    }

    #[test]
    fn walk_lists_files() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.txt"), b"").unwrap();
        let entries = walk(dir.path(), &WalkOpts::default()).unwrap();
        assert!(!entries.is_empty());
        assert!(entries[0].files.contains(&"a.txt".to_string()));
    }
}
