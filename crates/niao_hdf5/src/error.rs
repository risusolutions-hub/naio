//! HDF5 error types for Niao.

use std::fmt;

/// Unified error surfaced by nhdf5 operations.
#[derive(Debug)]
pub enum Hdf5Error {
    Io(String),
    H5(String),
    NotFound(String),
    TypeMismatch(String),
    InvalidDtype(String),
    InvalidShape(String),
    ReadOnly(String),
}

impl Hdf5Error {
    pub fn message(&self) -> String {
        match self {
            Hdf5Error::Io(m) => m.clone(),
            Hdf5Error::H5(m) => m.clone(),
            Hdf5Error::NotFound(m) => m.clone(),
            Hdf5Error::TypeMismatch(m) => m.clone(),
            Hdf5Error::InvalidDtype(m) => m.clone(),
            Hdf5Error::InvalidShape(m) => m.clone(),
            Hdf5Error::ReadOnly(m) => m.clone(),
        }
    }
}

impl fmt::Display for Hdf5Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for Hdf5Error {}

impl From<hdf5_metno::Error> for Hdf5Error {
    fn from(e: hdf5_metno::Error) -> Self {
        Hdf5Error::H5(e.to_string())
    }
}

impl From<std::io::Error> for Hdf5Error {
    fn from(e: std::io::Error) -> Self {
        Hdf5Error::Io(e.to_string())
    }
}

pub type Hdf5Result<T> = Result<T, Hdf5Error>;
