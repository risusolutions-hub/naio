use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use filetime::{set_file_mtime, FileTime};
use flate2::write::GzEncoder;
use flate2::Compression as GzLevel;
use tar::{Builder, Header};
use walkdir::WalkDir;

use crate::error::{Result, TarError};
use crate::format::Compression;
use crate::info::{safe_join, EntryKind};
use crate::io_util::copy_limited;
use crate::read::{ReadOpts, TarReader};

/// Per-member options when adding to an archive.
#[derive(Debug, Clone, Default)]
pub struct AddOpts {
    pub arcname: Option<String>,
    pub mode: Option<u32>,
    pub mtime: Option<i64>,
    pub recursive: bool,
}

/// Options for creating/writing tar archives.
#[derive(Debug, Clone)]
pub struct WriteOpts {
    pub compression: Option<Compression>,
    pub level: i32,
    pub mode: String,
}

impl Default for WriteOpts {
    fn default() -> Self {
        Self {
            compression: None,
            level: 6,
            mode: "w".into(),
        }
    }
}

/// Options for extracting archives.
#[derive(Debug, Clone, Default)]
pub struct ExtractOpts {
    pub members: Option<Vec<String>>,
    pub numeric_owner: bool,
    pub max_entry_bytes: usize,
    pub threads: usize,
}

pub struct TarWriter {
    output_path: PathBuf,
    staging_path: PathBuf,
    compression: Compression,
    level: i32,
    builder: Option<Builder<File>>,
    finished: bool,
    append_mode: bool,
}

impl TarWriter {
    pub fn create_path(path: impl AsRef<Path>, opts: &WriteOpts) -> Result<Self> {
        let output_path = path.as_ref().to_path_buf();
        let compression = opts
            .compression
            .unwrap_or_else(|| crate::format::detect_compression(&output_path));
        let staging_path = if compression == Compression::None {
            output_path.clone()
        } else {
            std::env::temp_dir().join(format!(
                "niao_tar_stage_{}_{}.tar",
                std::process::id(),
                output_path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("out")
            ))
        };
        if staging_path != output_path && staging_path.exists() {
            fs::remove_file(&staging_path)?;
        }
        let builder = Builder::new(File::create(&staging_path)?);
        Ok(Self {
            output_path,
            staging_path,
            compression,
            level: opts.level,
            builder: Some(builder),
            finished: false,
            append_mode: false,
        })
    }

    pub fn append_path(path: impl AsRef<Path>) -> Result<Self> {
        let output_path = path.as_ref().to_path_buf();
        if !matches!(
            crate::format::detect_compression(&output_path),
            Compression::None
        ) {
            return Err(TarError::InvalidMode(
                "append mode only supports uncompressed .tar files".into(),
            ));
        }
        if !output_path.exists() {
            return Err(TarError::Format(format!(
                "append mode requires existing tar file: {}",
                output_path.display()
            )));
        }
        let file = File::options().append(true).read(true).open(&output_path)?;
        let builder = Builder::new(file);
        Ok(Self {
            output_path: output_path.clone(),
            staging_path: output_path,
            compression: Compression::None,
            level: 6,
            builder: Some(builder),
            finished: false,
            append_mode: true,
        })
    }

    pub fn add_path(&mut self, path: impl AsRef<Path>, opts: &AddOpts) -> Result<()> {
        if self.finished {
            return Err(TarError::Closed);
        }
        let path = path.as_ref();
        if !path.exists() {
            return Err(TarError::Format(format!(
                "path not found: {}",
                path.display()
            )));
        }
        let meta = fs::symlink_metadata(path)?;
        if meta.is_dir() {
            if opts.recursive {
                return self.add_tree(path, opts);
            }
            return self.add_dir(path, opts);
        }
        if meta.is_symlink() {
            return self.add_symlink(path, opts);
        }
        let arcname = opts.arcname.clone().unwrap_or_else(|| path_to_name(path));
        let mut file = File::open(path)?;
        let mut header = Header::new_gnu();
        header.set_path(&arcname)?;
        header.set_size(meta.len());
        header.set_mode(opts.mode.unwrap_or(0o644));
        if let Some(mtime) = opts.mtime {
            header.set_mtime(mtime as u64);
        } else if let Ok(ft) = meta.modified() {
            header.set_mtime(
                ft.duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            );
        }
        header.set_cksum();
        self.builder_mut()?.append(&header, &mut file)?;
        Ok(())
    }

    pub fn add_bytes(
        &mut self,
        arcname: impl AsRef<str>,
        data: &[u8],
        mode: Option<u32>,
    ) -> Result<()> {
        if self.finished {
            return Err(TarError::Closed);
        }
        let arcname = arcname.as_ref();
        let mut header = Header::new_gnu();
        header.set_path(arcname)?;
        header.set_size(data.len() as u64);
        header.set_mode(mode.unwrap_or(0o644));
        header.set_cksum();
        self.builder_mut()?.append(&header, data)?;
        Ok(())
    }

    pub fn add_dir(&mut self, path: impl AsRef<Path>, opts: &AddOpts) -> Result<()> {
        if self.finished {
            return Err(TarError::Closed);
        }
        let arcname = opts
            .arcname
            .clone()
            .unwrap_or_else(|| path_to_name(path.as_ref()));
        let mut header = Header::new_gnu();
        header.set_path(format!("{}/", arcname.trim_end_matches('/')))?;
        header.set_entry_type(tar::EntryType::Directory);
        header.set_mode(opts.mode.unwrap_or(0o755));
        header.set_size(0);
        header.set_cksum();
        self.builder_mut()?.append(&header, &mut std::io::empty())?;
        Ok(())
    }

    pub fn add_symlink(&mut self, path: impl AsRef<Path>, opts: &AddOpts) -> Result<()> {
        #[cfg(not(unix))]
        {
            let _ = (path, opts);
            return Err(TarError::Format(
                "symlinks are only supported on unix".into(),
            ));
        }
        #[cfg(unix)]
        {
            if self.finished {
                return Err(TarError::Closed);
            }
            let path = path.as_ref();
            let target = fs::read_link(path)?;
            let arcname = opts.arcname.clone().unwrap_or_else(|| path_to_name(path));
            let mut header = Header::new_gnu();
            header.set_path(&arcname)?;
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_link_name(&target)?;
            header.set_size(0);
            header.set_mode(opts.mode.unwrap_or(0o777));
            header.set_cksum();
            self.builder_mut()?.append(&header, &mut std::io::empty())?;
            Ok(())
        }
    }

    pub fn add_tree(&mut self, root: impl AsRef<Path>, opts: &AddOpts) -> Result<()> {
        let root = root.as_ref();
        if !root.is_dir() {
            return Err(TarError::Format(format!(
                "not a directory: {}",
                root.display()
            )));
        }
        let prefix = opts.arcname.clone().unwrap_or_else(|| path_to_name(root));
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());

        let mut paths: Vec<PathBuf> = WalkDir::new(&root)
            .into_iter()
            .filter_map(|e| e.ok())
            .map(|e| e.path().to_path_buf())
            .collect();
        paths.sort();

        for path in paths {
            let rel = path
                .strip_prefix(&root)
                .map_err(|e| TarError::Format(e.to_string()))?;
            let arcname = if rel.as_os_str().is_empty() {
                prefix.clone()
            } else {
                format!("{}/{}", prefix.trim_end_matches('/'), rel.to_string_lossy())
            };
            let local = AddOpts {
                arcname: Some(arcname),
                mode: opts.mode,
                mtime: opts.mtime,
                recursive: false,
            };
            let meta = fs::symlink_metadata(&path)?;
            if meta.is_dir() {
                self.add_dir(&path, &local)?;
            } else if meta.is_symlink() {
                self.add_symlink(&path, &local)?;
            } else if meta.is_file() {
                self.add_path(&path, &local)?;
            }
        }
        Ok(())
    }

    fn builder_mut(&mut self) -> Result<&mut Builder<File>> {
        if self.finished {
            return Err(TarError::Closed);
        }
        self.builder.as_mut().ok_or(TarError::Closed)
    }

    pub fn finish(&mut self) -> Result<()> {
        if self.finished {
            return Err(TarError::Closed);
        }
        let builder = self.builder.take().ok_or(TarError::Closed)?;
        let mut file = builder.into_inner()?;
        file.flush()?;

        if self.compression != Compression::None && !self.append_mode {
            compress_staging(
                &self.staging_path,
                &self.output_path,
                self.compression,
                self.level,
            )?;
            if self.staging_path != self.output_path {
                let _ = fs::remove_file(&self.staging_path);
            }
        }
        self.finished = true;
        Ok(())
    }

    pub fn output_path(&self) -> &Path {
        &self.output_path
    }

    pub fn compression(&self) -> Compression {
        self.compression
    }
}

fn compress_staging(
    staging: &Path,
    output: &Path,
    compression: Compression,
    level: i32,
) -> Result<()> {
    let mut inp = File::open(staging)?;
    let out = File::create(output)?;
    match compression {
        Compression::None => {
            if staging != output {
                fs::copy(staging, output)?;
            }
        }
        Compression::Gz => {
            let mut enc = GzEncoder::new(out, gz_level(level));
            std::io::copy(&mut inp, &mut enc)?;
            enc.finish()?;
        }
        Compression::Zst => {
            let mut enc = zstd::stream::write::Encoder::new(out, zstd_level(level))
                .map_err(|e| TarError::Format(format!("zstd encoder: {e}")))?;
            std::io::copy(&mut inp, &mut enc)?;
            enc.finish()?;
        }
    }
    Ok(())
}

fn path_to_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

fn gz_level(level: i32) -> GzLevel {
    match level.clamp(0, 9) {
        0 => GzLevel::none(),
        1 => GzLevel::fast(),
        9 => GzLevel::best(),
        n => GzLevel::new(n as u32),
    }
}

fn zstd_level(level: i32) -> i32 {
    if level <= 0 {
        zstd::DEFAULT_COMPRESSION_LEVEL
    } else {
        level.clamp(1, 22)
    }
}

pub fn extract_all(
    reader: &TarReader,
    dest: impl AsRef<Path>,
    opts: &ExtractOpts,
) -> Result<Vec<String>> {
    let dest = dest.as_ref();
    fs::create_dir_all(dest)?;
    let data = reader.raw_data()?;
    let cursor = std::io::Cursor::new(data);
    let mut archive = tar::Archive::new(cursor);
    let filter = opts
        .members
        .as_ref()
        .map(|v| v.iter().cloned().collect::<std::collections::HashSet<_>>());
    let mut extracted = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry
            .path()
            .map_err(|e| TarError::Format(e.to_string()))?
            .into_owned();
        let name = path.to_string_lossy().into_owned();
        if let Some(ref allow) = filter {
            if !allow.contains(&name) {
                continue;
            }
        }
        let out = safe_join(dest, &name)?;
        let kind = crate::info::entry_kind(entry.header());
        match kind {
            EntryKind::Directory => {
                fs::create_dir_all(&out)?;
            }
            EntryKind::Symlink => {
                #[cfg(unix)]
                {
                    let target = entry.link_name()?.ok_or_else(|| {
                        TarError::Format(format!("symlink without target: {name}"))
                    })?;
                    if out.exists() {
                        fs::remove_file(&out)?;
                    }
                    std::os::unix::fs::symlink(&target, &out)?;
                }
                #[cfg(not(unix))]
                {
                    return Err(TarError::Format(
                        "cannot extract symlinks on this platform".into(),
                    ));
                }
            }
            EntryKind::HardLink => {
                let target = entry
                    .link_name()?
                    .ok_or_else(|| TarError::Format(format!("hard link without target: {name}")))?;
                let link_src = safe_join(dest, &target.to_string_lossy())?;
                if let Some(parent) = out.parent() {
                    fs::create_dir_all(parent)?;
                }
                #[cfg(unix)]
                {
                    fs::hard_link(&link_src, &out)?;
                }
                #[cfg(not(unix))]
                {
                    fs::copy(&link_src, &out)?;
                }
            }
            _ => {
                if let Some(parent) = out.parent() {
                    fs::create_dir_all(parent)?;
                }
                let size = entry.header().size().unwrap_or(0);
                let mut file = File::create(&out)?;
                copy_limited(&mut entry, &mut file, size.max(opts.max_entry_bytes as u64))?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(mode) = entry.header().mode() {
                        let _ = fs::set_permissions(&out, fs::Permissions::from_mode(mode));
                    }
                }
                if let Ok(mtime) = entry.header().mtime() {
                    let ft = FileTime::from_unix_time(mtime as i64, 0);
                    let _ = set_file_mtime(&out, ft);
                }
            }
        }
        extracted.push(name);
    }
    let _ = opts.numeric_owner;
    Ok(extracted)
}

pub fn extract_member(
    reader: &TarReader,
    member: &str,
    dest: impl AsRef<Path>,
    opts: &ExtractOpts,
) -> Result<()> {
    let mut members = opts.clone();
    members.members = Some(vec![member.to_string()]);
    extract_all(reader, dest, &members)?;
    Ok(())
}

pub fn pack_tree(
    src: impl AsRef<Path>,
    archive_path: impl AsRef<Path>,
    arcname: Option<&str>,
    opts: &WriteOpts,
) -> Result<()> {
    let mut w = TarWriter::create_path(archive_path, opts)?;
    let add = AddOpts {
        arcname: arcname.map(str::to_string),
        ..Default::default()
    };
    w.add_tree(src, &add)?;
    w.finish()?;
    Ok(())
}

pub fn unpack(
    archive_path: impl AsRef<Path>,
    dest: impl AsRef<Path>,
    opts: &ExtractOpts,
) -> Result<Vec<String>> {
    let reader = TarReader::open_path(
        archive_path,
        &ReadOpts {
            max_entry_bytes: opts.max_entry_bytes,
            ..Default::default()
        },
    )?;
    extract_all(&reader, dest, opts)
}

pub fn create_archive(
    paths: &[PathBuf],
    archive_path: impl AsRef<Path>,
    opts: &WriteOpts,
) -> Result<()> {
    let mut w = TarWriter::create_path(archive_path, opts)?;
    for path in paths {
        let meta = fs::symlink_metadata(path)?;
        let add = AddOpts::default();
        if meta.is_dir() {
            w.add_tree(path, &add)?;
        } else {
            w.add_path(path, &add)?;
        }
    }
    w.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn write_tree_extract() {
        let src = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();
        let arc = TempDir::new().unwrap();
        fs::create_dir_all(src.path().join("sub")).unwrap();
        fs::write(src.path().join("sub/a.txt"), b"data").unwrap();

        let arc_path = arc.path().join("pkg.tar.gz");
        let opts = WriteOpts {
            compression: Some(Compression::Gz),
            ..Default::default()
        };
        pack_tree(src.path(), &arc_path, Some("root"), &opts).unwrap();
        let out = unpack(&arc_path, dst.path(), &ExtractOpts::default()).unwrap();
        assert!(out.iter().any(|p| p.contains("a.txt")));
        let text = fs::read_to_string(dst.path().join("root/sub/a.txt")).unwrap();
        assert_eq!(text, "data");
    }
}
