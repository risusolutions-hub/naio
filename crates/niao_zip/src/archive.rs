use crate::compression::{CompressionName, DEFAULT_LEVEL};
use crate::error::{ZipError, ZipResult};
use crate::info::EntryInfo;
use std::fs::{self, File};
use std::io::{copy, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use zip::read::ZipArchive;
use zip::write::SimpleFileOptions;
use zip::CompressionMethod;
use zip::ZipWriter;

/// Options when opening a ZIP for read.
#[derive(Debug, Clone, Default)]
pub struct OpenOptions {
    pub password: Option<Vec<u8>>,
}

/// Options when creating or appending a ZIP.
#[derive(Debug, Clone)]
pub struct WriteOptions {
    pub compression: CompressionName,
    pub level: i32,
    pub password: Option<String>,
    pub comment: Option<String>,
    pub large_file: bool,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            compression: CompressionName::Deflated,
            level: DEFAULT_LEVEL,
            password: None,
            comment: None,
            large_file: true,
        }
    }
}

/// Options for writing one entry.
#[derive(Debug, Clone, Default)]
pub struct EntryWriteOptions {
    pub arcname: Option<String>,
    pub compression: Option<CompressionName>,
    pub level: Option<i32>,
    pub comment: Option<String>,
    pub modified_unix: Option<i64>,
}

/// Options for extraction.
#[derive(Debug, Clone, Default)]
pub struct ExtractOptions {
    pub password: Option<Vec<u8>>,
    pub threads: Option<usize>,
    pub overwrite: bool,
}

pub enum ZipHandle {
    Read(ZipReader),
    Write(ZipWriterHandle),
}

pub struct ZipReader {
    path: PathBuf,
    archive: ZipArchive<File>,
    password: Option<Vec<u8>>,
    stream: Option<EntryStreamState>,
}

struct EntryStreamState {
    name: String,
    data: Option<Vec<u8>>,
    pos: usize,
}

impl ZipReader {
    pub fn open(path: impl AsRef<Path>, opts: &OpenOptions) -> ZipResult<Self> {
        let path = path.as_ref().to_path_buf();
        let file = File::open(&path)?;
        let archive = ZipArchive::new(file).map_err(ZipError::from)?;
        Ok(Self {
            path,
            archive,
            password: opts.password.clone(),
            stream: None,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn set_password(&mut self, password: Option<Vec<u8>>) {
        self.password = password;
    }

    pub fn comment(&self) -> Option<String> {
        let c = self.archive.comment();
        if c.is_empty() {
            None
        } else {
            Some(String::from_utf8_lossy(c).into_owned())
        }
    }

    pub fn namelist(&mut self) -> ZipResult<Vec<String>> {
        let mut names = Vec::with_capacity(self.archive.len());
        for i in 0..self.archive.len() {
            let file = self.archive.by_index(i)?;
            names.push(file.name().to_string());
        }
        Ok(names)
    }

    pub fn infolist(&mut self) -> ZipResult<Vec<EntryInfo>> {
        let mut out = Vec::with_capacity(self.archive.len());
        for i in 0..self.archive.len() {
            let file = self.archive.by_index(i)?;
            out.push(EntryInfo::from_file(file.name().to_string(), &file));
        }
        Ok(out)
    }

    pub fn getinfo(&mut self, name: &str) -> ZipResult<EntryInfo> {
        let file = self.open_by_name(name)?;
        Ok(EntryInfo::from_file(name.to_string(), &file))
    }

    pub(crate) fn open_by_name<'a>(&'a mut self, name: &str) -> ZipResult<zip::read::ZipFile<'a>> {
        if self.stream.is_some() {
            return Err(ZipError::EntryBusy);
        }
        let pwd = self.password.as_deref();
        if let Some(pwd) = pwd {
            self.archive
                .by_name_decrypt(name, pwd)
                .map_err(|e| map_password_err(name, e))
        } else {
            self.archive
                .by_name(name)
                .map_err(|_| ZipError::NotFound(name.to_string()))
        }
    }

    pub fn read(&mut self, name: &str) -> ZipResult<Vec<u8>> {
        let mut file = self.open_by_name(name)?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        Ok(buf)
    }

    pub fn open_entry(&mut self, name: &str) -> ZipResult<()> {
        if self.stream.is_some() {
            return Err(ZipError::EntryBusy);
        }
        // Validate entry exists.
        let _ = self.getinfo(name)?;
        self.stream = Some(EntryStreamState {
            name: name.to_string(),
            data: None,
            pos: 0,
        });
        Ok(())
    }

    pub fn entry_read(&mut self, max: usize) -> ZipResult<Vec<u8>> {
        let mut stream = self
            .stream
            .take()
            .ok_or_else(|| ZipError::Archive("no entry stream open".into()))?;
        if stream.data.is_none() {
            stream.data = Some(self.read(&stream.name)?);
        }
        let data = stream.data.as_ref().unwrap();
        let chunk = if stream.pos >= data.len() {
            Vec::new()
        } else {
            let end = (stream.pos + max).min(data.len());
            let out = data[stream.pos..end].to_vec();
            stream.pos = end;
            out
        };
        self.stream = Some(stream);
        Ok(chunk)
    }

    pub fn entry_close(&mut self) -> ZipResult<()> {
        self.stream = None;
        Ok(())
    }

    pub fn test(&mut self) -> ZipResult<()> {
        for i in 0..self.archive.len() {
            let name = {
                let f = self.archive.by_index(i)?;
                f.name().to_string()
            };
            let mut file = if let Some(pwd) = self.password.as_deref() {
                self.archive
                    .by_name_decrypt(&name, pwd)
                    .map_err(|e| map_password_err(&name, e))?
            } else {
                self.archive.by_index(i)?
            };
            let mut sink = std::io::sink();
            copy(&mut file, &mut sink)?;
        }
        Ok(())
    }
}

pub struct ZipWriterHandle {
    path: PathBuf,
    writer: ZipWriter<File>,
    default_opts: WriteOptions,
}

impl ZipWriterHandle {
    pub fn create(path: impl AsRef<Path>, opts: &WriteOptions) -> ZipResult<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let file = File::create(&path)?;
        let mut writer = ZipWriter::new(file);
        if let Some(comment) = &opts.comment {
            writer.set_comment(comment.clone());
        }
        Ok(Self {
            path,
            writer,
            default_opts: opts.clone(),
        })
    }

    pub fn append(path: impl AsRef<Path>, opts: &WriteOptions) -> ZipResult<Self> {
        let path = path.as_ref().to_path_buf();
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)?;
        let mut writer = ZipWriter::new_append(file)?;
        if let Some(comment) = &opts.comment {
            writer.set_comment(comment.clone());
        }
        Ok(Self {
            path,
            writer,
            default_opts: opts.clone(),
        })
    }

    pub fn set_comment(&mut self, comment: &str) -> ZipResult<()> {
        self.writer.set_comment(comment.to_string());
        Ok(())
    }

    fn file_options(&self, compression: CompressionName, level: i32) -> SimpleFileOptions {
        SimpleFileOptions::default()
            .compression_method(compression.to_method())
            .compression_level(Some(i64::from(level)))
            .large_file(self.default_opts.large_file)
    }

    pub fn write_file(&mut self, src: &Path, entry_opts: &EntryWriteOptions) -> ZipResult<u64> {
        let arcname = entry_opts
            .arcname
            .clone()
            .or_else(|| src.file_name().map(|s| s.to_string_lossy().into_owned()))
            .ok_or_else(|| ZipError::Archive("write_file: missing arcname".into()))?;
        let compression = entry_opts
            .compression
            .unwrap_or(self.default_opts.compression);
        let level = entry_opts.level.unwrap_or(self.default_opts.level);
        let pwd = self.default_opts.password.as_deref();
        let mut opts = self.file_options(compression, level);
        if let Some(pwd) = pwd {
            opts = opts.with_aes_encryption(zip::AesMode::Aes256, pwd);
        }
        let mut src_file = File::open(src)?;
        self.writer.start_file(arcname, opts)?;
        let n = copy(&mut src_file, &mut self.writer)?;
        Ok(n)
    }

    pub fn write_bytes(
        &mut self,
        arcname: &str,
        data: &[u8],
        entry_opts: &EntryWriteOptions,
    ) -> ZipResult<u64> {
        let compression = entry_opts
            .compression
            .unwrap_or(self.default_opts.compression);
        let level = entry_opts.level.unwrap_or(self.default_opts.level);
        let pwd = self.default_opts.password.as_deref();
        let mut opts = self.file_options(compression, level);
        if let Some(pwd) = pwd {
            opts = opts.with_aes_encryption(zip::AesMode::Aes256, pwd);
        }
        self.writer.start_file(arcname, opts)?;
        self.writer.write_all(data)?;
        Ok(data.len() as u64)
    }

    pub fn mkdir(&mut self, arcname: &str) -> ZipResult<()> {
        let mut name = arcname.to_string();
        if !name.ends_with('/') {
            name.push('/');
        }
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        self.writer.add_directory(name, opts)?;
        Ok(())
    }

    pub fn finish(self) -> ZipResult<PathBuf> {
        self.writer.finish()?;
        Ok(self.path)
    }
}

fn map_password_err(name: &str, err: zip::result::ZipError) -> ZipError {
    let msg = err.to_string();
    if msg.contains("Invalid password") || msg.contains("password") {
        ZipError::BadPassword(name.to_string())
    } else if msg.contains("encrypted") {
        ZipError::PasswordRequired(name.to_string())
    } else {
        ZipError::from(err)
    }
}

/// Memory-backed reader for bytes and `is_zipfile` checks.
pub struct ZipReaderMem {
    archive: ZipArchive<Cursor<Vec<u8>>>,
    password: Option<Vec<u8>>,
}

impl ZipReaderMem {
    pub fn from_bytes(data: Vec<u8>, opts: &OpenOptions) -> ZipResult<Self> {
        let cursor = Cursor::new(data);
        let archive = ZipArchive::new(cursor).map_err(ZipError::from)?;
        Ok(Self {
            archive,
            password: opts.password.clone(),
        })
    }

    pub fn read(&mut self, name: &str) -> ZipResult<Vec<u8>> {
        let pwd = self.password.as_deref();
        let mut file = if let Some(pwd) = pwd {
            self.archive
                .by_name_decrypt(name, pwd)
                .map_err(|e| map_password_err(name, e))?
        } else {
            self.archive
                .by_name(name)
                .map_err(|_| ZipError::NotFound(name.to_string()))?
        };
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        Ok(buf)
    }

    pub fn namelist(&mut self) -> ZipResult<Vec<String>> {
        let mut names = Vec::with_capacity(self.archive.len());
        for i in 0..self.archive.len() {
            let file = self.archive.by_index(i)?;
            names.push(file.name().to_string());
        }
        Ok(names)
    }
}

pub fn is_zipfile_path(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let ok = zip::read::read_zipfile_from_stream(&mut file).is_ok();
    ok
}

pub fn is_zipfile_bytes(data: &[u8]) -> bool {
    let mut cursor = Cursor::new(data);
    let ok = zip::read::read_zipfile_from_stream(&mut cursor).is_ok();
    ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn temp_path(name: &str) -> PathBuf {
        let mut p = env::temp_dir();
        p.push(format!("niao_zip_test_{name}_{}", std::process::id()));
        p
    }

    #[test]
    fn roundtrip_write_read() {
        let path = temp_path("roundtrip.zip");
        let _ = fs::remove_file(&path);
        let write_opts = WriteOptions::default();
        let mut zw = ZipWriterHandle::create(&path, &write_opts).unwrap();
        zw.write_bytes("hello.txt", b"hello zip", &EntryWriteOptions::default())
            .unwrap();
        zw.mkdir("subdir").unwrap();
        zw.finish().unwrap();

        let mut zr = ZipReader::open(&path, &OpenOptions::default()).unwrap();
        let names = zr.namelist().unwrap();
        assert!(names.iter().any(|n| n == "hello.txt"));
        let data = zr.read("hello.txt").unwrap();
        assert_eq!(data, b"hello zip");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn is_zipfile_detects_archive() {
        let path = temp_path("detect.zip");
        let _ = fs::remove_file(&path);
        let mut zw = ZipWriterHandle::create(&path, &WriteOptions::default()).unwrap();
        zw.write_bytes("a.txt", b"a", &EntryWriteOptions::default())
            .unwrap();
        zw.finish().unwrap();
        assert!(is_zipfile_path(&path));
        let _ = fs::remove_file(path);
    }
}
