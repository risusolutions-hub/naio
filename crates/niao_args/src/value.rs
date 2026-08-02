use std::borrow::Cow;
use std::path::PathBuf;

use crate::error::{Error, ErrorKind};
use crate::matches::ArgMatches;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueSource {
    CommandLine,
    DefaultValue,
    EnvVariable,
}

/// Parse a value from CLI / default / env.
pub trait FromArgValue: Sized {
    fn from_matches(matches: &ArgMatches, id: &str) -> Result<Self, Error>;
}

impl FromArgValue for String {
    fn from_matches(matches: &ArgMatches, id: &str) -> Result<Self, Error> {
        matches
            .get_one::<String>(id)
            .map(|s| s.to_string())
            .ok_or_else(|| missing(id))
    }
}

impl FromArgValue for PathBuf {
    fn from_matches(matches: &ArgMatches, id: &str) -> Result<Self, Error> {
        let raw = matches.get_one::<String>(id).ok_or_else(|| missing(id))?;
        Ok(PathBuf::from(raw.as_str()))
    }
}

impl FromArgValue for bool {
    fn from_matches(matches: &ArgMatches, id: &str) -> Result<Self, Error> {
        Ok(matches.get_flag(id))
    }
}

macro_rules! impl_from_int {
    ($($ty:ty),+) => {
        $(
            impl FromArgValue for $ty {
                fn from_matches(matches: &ArgMatches, id: &str) -> Result<Self, Error> {
                    let raw = matches
                        .get_one::<String>(id)
                        .ok_or_else(|| missing(id))?;
                    raw.parse::<$ty>().map_err(|_| invalid(id, raw.as_str()))
                }
            }
        )+
    };
}

impl_from_int!(u16, u32, usize, i32, i64, u64);

impl<T: FromArgValue> FromArgValue for Option<T> {
    fn from_matches(matches: &ArgMatches, id: &str) -> Result<Self, Error> {
        if matches.contains_id(id) {
            Ok(Some(T::from_matches(matches, id)?))
        } else {
            Ok(None)
        }
    }
}

impl FromArgValue for Vec<String> {
    fn from_matches(matches: &ArgMatches, id: &str) -> Result<Self, Error> {
        Ok(matches
            .get_many::<String>(id)
            .map(|vals| vals.map(|s| s.to_string()).collect())
            .unwrap_or_default())
    }
}

pub(crate) fn parse_value<T: std::str::FromStr>(arg: &str, value: &str) -> Result<T, Error> {
    value.parse::<T>().map_err(|_| invalid(arg, value))
}

pub(crate) fn missing(arg: &str) -> Error {
    Error::new(
        ErrorKind::MissingRequiredArgument,
        format!("the following required arguments were not provided: <{arg}>"),
    )
}

pub(crate) fn invalid(arg: &str, value: &str) -> Error {
    Error::new(
        ErrorKind::InvalidValue {
            arg: arg.to_string(),
            value: value.to_string(),
        },
        format!("invalid value '{value}' for '{arg}'"),
    )
}

pub(crate) fn cow_str(s: &str) -> Cow<'_, str> {
    Cow::Borrowed(s)
}
