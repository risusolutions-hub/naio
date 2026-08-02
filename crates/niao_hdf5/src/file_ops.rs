//! File open/create and HDF5 file utilities.

use crate::error::{Hdf5Error, Hdf5Result};
use hdf5_metno::file::File;
use hdf5_metno::Group;
use hdf5_metno::OpenMode;
use std::path::Path;

const HDF5_MAGIC: &[u8; 8] = b"\x89HDF\r\n\x1a\n";

/// Open mode string: `r`, `r+`, `w`, `w-`, `a`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Read,
    ReadWrite,
    Write,
    WriteExclusive,
    Append,
}

impl Mode {
    pub fn parse(s: &str) -> Hdf5Result<Self> {
        match s {
            "r" => Ok(Mode::Read),
            "r+" => Ok(Mode::ReadWrite),
            "w" => Ok(Mode::Write),
            "w-" => Ok(Mode::WriteExclusive),
            "a" => Ok(Mode::Append),
            other => Err(Hdf5Error::Io(format!(
                "invalid mode '{other}'; use r, r+, w, w-, a"
            ))),
        }
    }

    fn to_open_mode(self) -> OpenMode {
        match self {
            Mode::Read => OpenMode::Read,
            Mode::ReadWrite => OpenMode::ReadWrite,
            Mode::Write => OpenMode::Create,
            Mode::WriteExclusive => OpenMode::CreateExcl,
            Mode::Append => OpenMode::Append,
        }
    }
}

/// Open an existing HDF5 file.
pub fn open_file(path: &str, mode: Mode) -> Hdf5Result<File> {
    let p = Path::new(path);
    match mode {
        Mode::Read => Ok(File::open(p)?),
        Mode::ReadWrite => Ok(File::open_rw(p)?),
        other => Ok(File::open_as(p, other.to_open_mode())?),
    }
}

/// Create (truncate) or exclusively create an HDF5 file.
pub fn create_file(path: &str, mode: Mode) -> Hdf5Result<File> {
    let p = Path::new(path);
    match mode {
        Mode::Write => Ok(File::create(p)?),
        Mode::WriteExclusive => Ok(File::create_excl(p)?),
        Mode::Append => Ok(File::append(p)?),
        other => Ok(File::open_as(p, other.to_open_mode())?),
    }
}

/// Returns true when `path` points at an HDF5 file (magic bytes).
pub fn is_hdf5(path: &str) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    bytes.len() >= 8 && bytes[..8] == *HDF5_MAGIC
}

/// Runtime HDF5 C library version string.
pub fn library_version() -> String {
    let v = hdf5_metno::library_version();
    format!("{}.{}.{}", v.0, v.1, v.2)
}

/// Flush file metadata and data to storage.
pub fn flush_file(file: &File) -> Hdf5Result<()> {
    file.flush()?;
    Ok(())
}

/// Close file (consumes handle in Rust; runtime drops separately).
pub fn close_file(file: File) -> Hdf5Result<()> {
    file.close()?;
    Ok(())
}

/// Copy entire file contents.
pub fn copy_file(src: &str, dst: &str) -> Hdf5Result<()> {
    let src_f = File::open(src)?;
    let dst_f = File::create(dst)?;
    copy_group_members(&src_f, &dst_f)?;
    dst_f.flush()?;
    Ok(())
}

fn copy_group_members(src: &File, dst: &File) -> Hdf5Result<()> {
    for name in src.member_names()? {
        let info = src.loc_info_by_name(&name)?;
        use hdf5_metno::LocationType;
        match info.loc_type {
            LocationType::Group => {
                let sg = src.group(&name)?;
                let dg = dst.create_group(&name)?;
                copy_subgroup(&sg, &dg)?;
            }
            LocationType::Dataset => {
                let ds = src.dataset(&name)?;
                ds.copy_to(dst, &name)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn copy_subgroup(src: &Group, dst: &Group) -> Hdf5Result<()> {
    for name in src.member_names()? {
        let info = src.loc_info_by_name(&name)?;
        use hdf5_metno::LocationType;
        match info.loc_type {
            LocationType::Group => {
                let sg = src.group(&name)?;
                let dg = dst.create_group(&name)?;
                copy_subgroup(&sg, &dg)?;
            }
            LocationType::Dataset => {
                let ds = src.dataset(&name)?;
                ds.copy_to(dst, &name)?;
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn magic_detection() {
        let dir = std::env::temp_dir().join("niao_hdf5_magic");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("x.bin");
        fs::write(&path, b"not hdf5").unwrap();
        assert!(!is_hdf5(path.to_str().unwrap()));
        let h5 = dir.join("t.h5");
        let f = File::create(&h5).unwrap();
        drop(f);
        assert!(is_hdf5(h5.to_str().unwrap()));
        let _ = fs::remove_dir_all(&dir);
    }
}
