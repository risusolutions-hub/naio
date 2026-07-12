//! Zero-dependency CLI argument parser — clap v4-compatible runtime builder API.

mod arg;
mod command;
mod error;
mod help;
mod matches;
mod parse;
mod value;

pub use arg::{Arg, ArgAction, NumArgs, ValueHint};
pub use command::Command;
pub use error::{Error, ErrorKind};
pub use matches::ArgMatches;
pub use value::{FromArgValue, ValueSource};

/// Typed parser entry point (manual impls or future derive).
pub trait Parser: Sized {
    fn parse() -> Self {
        Self::parse_from(std::env::args())
    }

    fn parse_from<I, S>(iter: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<std::ffi::OsString> + Clone;
}

/// Subcommand enum marker (manual impls or future derive).
pub trait Subcommand: Sized {
    fn augment_subcommand(cmd: Command) -> Command;
    fn from_matches(matches: &ArgMatches) -> Result<Self, Error>;
}

/// Re-export for API parity with clap.
pub use Parser as ClapParser;
pub use Subcommand as ClapSubcommand;

#[cfg(test)]
mod tests;
