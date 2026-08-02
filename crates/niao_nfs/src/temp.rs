//! Temp file and directory helpers (wraps [`tempfile`] for secure creation).

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// Options shared by temp file/dir creation.
#[derive(Debug, Clone)]
pub struct TempOpts {
    pub dir: Option<PathBuf>,
    pub prefix: String,
    pub suffix: String,
}

impl Default for TempOpts {
    fn default() -> Self {
        Self {
            dir: None,
            prefix: ".tmp".to_string(),
            suffix: String::new(),
        }
    }
}

/// System temp directory path.
pub fn temp_dir_path() -> PathBuf {
    std::env::temp_dir()
}

/// Create a secure temp file (~`tempfile.mkstemp`).
pub fn mkstemp(opts: &TempOpts) -> io::Result<(File, PathBuf)> {
    let mut builder = tempfile::Builder::new();
    builder.prefix(&opts.prefix).suffix(&opts.suffix);
    if let Some(dir) = &opts.dir {
        builder.tempfile_in(dir).map(|f| {
            let path = f.path().to_path_buf();
            (f.into_file(), path)
        })
    } else {
        builder.tempfile().map(|f| {
            let path = f.path().to_path_buf();
            (f.into_file(), path)
        })
    }
}

/// Insecure temp path generation (~`tempfile.mktemp` — race-prone).
pub fn mktemp(opts: &TempOpts) -> io::Result<PathBuf> {
    let base = opts.dir.clone().unwrap_or_else(temp_dir_path);
    for _ in 0..100 {
        let name = format!("{}{}{}", opts.prefix, std::process::id(), opts.suffix);
        let candidate = base.join(format!("{name}_{}", nano_id()));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "mktemp: could not find unused name",
    ))
}

fn nano_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static CTR: AtomicU64 = AtomicU64::new(0);
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let c = CTR.fetch_add(1, Ordering::Relaxed);
    t ^ (c << 17) ^ ((std::process::id() as u64) << 33)
}

/// Owned temp file deleted on drop unless `keep` is set.
pub struct TempFileGuard {
    pub path: PathBuf,
    file: Option<File>,
    pub keep: bool,
}

impl TempFileGuard {
    pub fn new(opts: &TempOpts) -> io::Result<Self> {
        let (file, path) = mkstemp(opts)?;
        Ok(Self {
            path,
            file: Some(file),
            keep: false,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        match self.file.as_mut() {
            Some(f) => f.write(data),
            None => {
                let mut f = OpenOptions::new().write(true).open(&self.path)?;
                let n = f.write(data)?;
                self.file = Some(f);
                Ok(n)
            }
        }
    }

    pub fn read(&mut self, max: usize) -> io::Result<Vec<u8>> {
        let mut f = match self.file.take() {
            Some(f) => f,
            None => File::open(&self.path)?,
        };
        let mut buf = vec![0u8; max];
        let n = f.read(&mut buf)?;
        buf.truncate(n);
        self.file = Some(f);
        Ok(buf)
    }

    pub fn read_all(&mut self) -> io::Result<Vec<u8>> {
        let mut f = match self.file.take() {
            Some(f) => f,
            None => File::open(&self.path)?,
        };
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        self.file = Some(f);
        Ok(buf)
    }

    pub fn close(mut self) -> io::Result<PathBuf> {
        self.file.take();
        let path = self.path.clone();
        if !self.keep {
            let _ = fs::remove_file(&path);
        }
        self.keep = true; // prevent Drop delete
        Ok(path)
    }

    /// Adopt an existing temp path (e.g. from `mkstemp`).
    pub fn adopt(path: PathBuf, keep: bool) -> Self {
        Self {
            path,
            file: None,
            keep,
        }
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// Owned temp directory deleted on drop unless `keep` is set.
pub struct TempDirGuard {
    inner: Option<tempfile::TempDir>,
    pub keep: bool,
}

impl TempDirGuard {
    pub fn new(opts: &TempOpts) -> io::Result<Self> {
        let mut builder = tempfile::Builder::new();
        builder.prefix(&opts.prefix).suffix(&opts.suffix);
        let inner = if let Some(dir) = &opts.dir {
            builder.tempdir_in(dir)?
        } else {
            builder.tempdir()?
        };
        Ok(Self {
            inner: Some(inner),
            keep: false,
        })
    }

    pub fn path(&self) -> &Path {
        self.inner.as_ref().unwrap().path()
    }

    pub fn close(mut self) -> io::Result<PathBuf> {
        let path = self.path().to_path_buf();
        if self.keep {
            let _ = self.inner.take().unwrap().close();
        } else {
            self.inner.take();
        }
        self.keep = true;
        Ok(path)
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        if !self.keep {
            let _ = self.inner.take();
        }
    }
}
