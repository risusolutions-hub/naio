//! Mail and FTP clients for Niao (`lettre` / `suppaftp` replacements).

pub mod error;
pub mod ftp;

pub use error::{NetClientError, Result};
