//! Minimal TOML parser producing nested [`Value`] trees.

mod error;
mod parse;

pub use error::{TomlError, TomlResult};
pub use parse::{parse, parse_to_value};
