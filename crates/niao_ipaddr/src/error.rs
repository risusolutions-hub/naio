//! Error types for nipaddr.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpError {
    Parse(String),
    VersionMismatch,
    NotNetwork,
    NotAddress,
    NotInterface,
    HostBitsSet,
    PrefixOutOfRange,
    EmptyRange,
    Overflow,
    TooManyHosts,
}

impl fmt::Display for IpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(m) => write!(f, "{m}"),
            Self::VersionMismatch => write!(f, "address version mismatch"),
            Self::NotNetwork => write!(f, "expected a network"),
            Self::NotAddress => write!(f, "expected an address"),
            Self::NotInterface => write!(f, "expected an interface"),
            Self::HostBitsSet => write!(f, "host bits set in strict network"),
            Self::PrefixOutOfRange => write!(f, "prefix length out of range"),
            Self::EmptyRange => write!(f, "empty address range"),
            Self::Overflow => write!(f, "address arithmetic overflow"),
            Self::TooManyHosts => write!(f, "host list exceeds limit"),
        }
    }
}

impl std::error::Error for IpError {}

pub type IpResult<T> = Result<T, IpError>;
