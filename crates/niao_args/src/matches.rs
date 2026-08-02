use std::collections::HashMap;
use std::ffi::OsString;

use crate::value::ValueSource;

#[derive(Debug, Clone, Default)]
pub struct ArgMatches {
    pub(crate) name: String,
    pub(crate) values: HashMap<String, Vec<OsString>>,
    pub(crate) flags: HashMap<String, bool>,
    pub(crate) sources: HashMap<String, ValueSource>,
    pub(crate) subcommand: Option<Box<(String, ArgMatches)>>,
}

impl ArgMatches {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn contains_id(&self, id: &str) -> bool {
        self.values.contains_key(id) || self.flags.get(id).copied().unwrap_or(false)
    }

    pub fn get_flag(&self, id: &str) -> bool {
        self.flags.get(id).copied().unwrap_or(false)
    }

    pub fn get_one<T: FromArgString>(&self, id: &str) -> Option<T> {
        self.values
            .get(id)
            .and_then(|v| v.first())
            .and_then(|s| T::from_os(s))
    }

    pub fn get_many<T: FromArgString>(&self, id: &str) -> Option<impl Iterator<Item = T> + '_> {
        let vals = self.values.get(id)?;
        Some(vals.iter().filter_map(|s| T::from_os(s)))
    }

    pub fn get_count(&self, id: &str) -> u8 {
        self.values
            .get(id)
            .map(|v| v.len().min(u8::MAX as usize) as u8)
            .unwrap_or(0)
    }

    pub fn subcommand(&self) -> Option<(&str, &ArgMatches)> {
        self.subcommand.as_ref().map(|b| (b.0.as_str(), &b.1))
    }

    pub fn subcommand_name(&self) -> Option<&str> {
        self.subcommand.as_ref().map(|b| b.0.as_str())
    }

    pub fn remove_subcommand(&mut self) -> Option<(String, ArgMatches)> {
        self.subcommand.take().map(|b| (*b))
    }

    pub fn value_source(&self, id: &str) -> Option<ValueSource> {
        self.sources.get(id).copied()
    }

    pub(crate) fn set_flag(&mut self, id: impl Into<String>, value: bool, source: ValueSource) {
        let id = id.into();
        self.flags.insert(id.clone(), value);
        self.sources.insert(id, source);
    }

    pub(crate) fn push_value(
        &mut self,
        id: impl Into<String>,
        value: OsString,
        source: ValueSource,
    ) {
        let id = id.into();
        self.values.entry(id.clone()).or_default().push(value);
        self.sources.insert(id, source);
    }

    pub(crate) fn set_values<I>(&mut self, id: impl Into<String>, values: I, source: ValueSource)
    where
        I: IntoIterator<Item = OsString>,
    {
        let id = id.into();
        self.values.insert(id.clone(), values.into_iter().collect());
        self.sources.insert(id, source);
    }
}

pub trait FromArgString: Sized {
    fn from_os(s: &OsString) -> Option<Self>;
}

impl FromArgString for String {
    fn from_os(s: &OsString) -> Option<Self> {
        Some(s.to_string_lossy().into_owned())
    }
}

impl FromArgString for &str {
    fn from_os(s: &OsString) -> Option<Self> {
        // SAFETY: only used transiently in tests via get_one::<String>
        None
    }
}

macro_rules! impl_from_os_int {
    ($($ty:ty),+) => {
        $(
            impl FromArgString for $ty {
                fn from_os(s: &OsString) -> Option<Self> {
                    s.to_string_lossy().parse().ok()
                }
            }
        )+
    };
}

impl_from_os_int!(u16, u32, usize, i32, i64, u64);
