//! Typed errors for ndataset (codes 4120–4125).

use std::fmt;

pub const E4120_NDATASET_ARITY: u32 = 4120;
pub const E4121_NDATASET_ERROR: u32 = 4121;
pub const E4122_NDATASET_TYPE: u32 = 4122;
pub const E4123_NDATASET_INVALID_HANDLE: u32 = 4123;
pub const E4124_NDATASET_COLUMN: u32 = 4124;
pub const E4125_NDATASET_INDEX: u32 = 4125;

#[derive(Debug, Clone, PartialEq)]
pub enum DatasetError {
    Error(String),
    Column(String),
    Index(String),
    Param(String),
}

impl DatasetError {
    pub fn code(&self) -> u32 {
        match self {
            Self::Error(_) => E4121_NDATASET_ERROR,
            Self::Column(_) => E4124_NDATASET_COLUMN,
            Self::Index(_) => E4125_NDATASET_INDEX,
            Self::Param(_) => E4122_NDATASET_TYPE,
        }
    }
}

impl fmt::Display for DatasetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Error(msg) | Self::Column(msg) | Self::Index(msg) | Self::Param(msg) => {
                f.write_str(msg)
            }
        }
    }
}

impl std::error::Error for DatasetError {}

impl From<niao_frame::FrameError> for DatasetError {
    fn from(e: niao_frame::FrameError) -> Self {
        DatasetError::Error(e.to_string())
    }
}

pub type DatasetResult<T> = Result<T, DatasetError>;
