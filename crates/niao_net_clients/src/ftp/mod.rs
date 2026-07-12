//! RFC 959 FTP client — control + data channel, passive/active, LIST/RETR/STOR.

mod client;
mod control;
mod data;
pub mod mock;

pub use client::{connect, connect_with, FtpClient, FtpOptions, TransferMode};
