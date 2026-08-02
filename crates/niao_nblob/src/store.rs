//! Object-store trait and shared entry metadata.

use crate::error::BlobResult;
use std::sync::Arc;

/// Directory / object listing entry.
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    /// `"file"` or `"dir"`
    pub kind: String,
    pub size: u64,
    /// Optional Unix epoch seconds.
    pub mtime: Option<i64>,
}

/// Open mode for buffered file handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenMode {
    Read,
    Write,
    Append,
}

impl OpenMode {
    pub fn parse(s: &str) -> BlobResult<Self> {
        match s {
            "r" | "rb" | "rt" => Ok(Self::Read),
            "w" | "wb" | "wt" => Ok(Self::Write),
            "a" | "ab" | "at" => Ok(Self::Append),
            other => Err(crate::error::BlobError::new(format!(
                "invalid open mode: {other}"
            ))),
        }
    }

    pub fn is_write(self) -> bool {
        matches!(self, Self::Write | Self::Append)
    }
}

/// Credential / endpoint options for cloud backends.
#[derive(Debug, Clone, Default)]
pub struct S3Opts {
    pub region: String,
    pub access_key: String,
    pub secret_key: String,
    pub session_token: Option<String>,
    pub endpoint: Option<String>,
    pub default_bucket: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AzureOpts {
    pub account: String,
    pub key: Option<Vec<u8>>,
    pub sas: Option<String>,
    pub bearer: Option<String>,
    pub default_container: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct GcsOpts {
    pub access_token: String,
    pub project: Option<String>,
    pub default_bucket: Option<String>,
}

/// Backend kind discriminator for handle metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Local,
    Memory,
    S3,
    Azure,
    Gcs,
}

impl BackendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Memory => "memory",
            Self::S3 => "s3",
            Self::Azure => "azure",
            Self::Gcs => "gcs",
        }
    }
}

/// Unified object-store operations.
pub trait ObjectStore: Send + Sync {
    fn kind(&self) -> BackendKind;

    fn read(&self, key: &str) -> BlobResult<Vec<u8>>;

    fn write(&self, key: &str, data: &[u8], content_type: Option<&str>) -> BlobResult<u64>;

    fn exists(&self, key: &str) -> BlobResult<bool>;

    fn info(&self, key: &str) -> BlobResult<Entry>;

    fn list(&self, prefix: &str, detail: bool) -> BlobResult<Vec<Entry>>;

    fn remove(&self, key: &str) -> BlobResult<()>;

    fn mkdir(&self, key: &str) -> BlobResult<()>;

    /// Optional copy within the same store; default = read+write.
    fn copy(&self, src: &str, dst: &str) -> BlobResult<()> {
        let data = self.read(src)?;
        self.write(dst, &data, None)?;
        Ok(())
    }

    fn rename(&self, src: &str, dst: &str) -> BlobResult<()> {
        self.copy(src, dst)?;
        self.remove(src)?;
        Ok(())
    }
}

pub type StoreArc = Arc<dyn ObjectStore>;
