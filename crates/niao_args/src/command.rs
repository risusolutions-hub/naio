use std::collections::HashMap;
use std::ffi::OsString;

use crate::arg::Arg;
use crate::error::{Error, ErrorKind};
use crate::help;
use crate::matches::ArgMatches;
use crate::parse;

#[derive(Debug, Clone)]
pub struct Command {
    pub(crate) name: String,
    pub(crate) bin_name: Option<String>,
    pub(crate) version: Option<String>,
    pub(crate) about: Option<String>,
    pub(crate) long_about: Option<String>,
    pub(crate) arg_required_else_help: bool,
    pub(crate) disable_help_flag: bool,
    pub(crate) disable_version_flag: bool,
    pub(crate) args: Vec<Arg>,
    pub(crate) subcommands: Vec<Command>,
    pub(crate) aliases: HashMap<String, String>,
    pub(crate) visible_aliases: Vec<String>,
}

impl Command {
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            name: name.clone(),
            bin_name: None,
            version: None,
            about: None,
            long_about: None,
            arg_required_else_help: false,
            disable_help_flag: false,
            disable_version_flag: false,
            args: Vec::new(),
            subcommands: Vec::new(),
            aliases: HashMap::new(),
            visible_aliases: Vec::new(),
        }
    }

    pub fn bin_name(mut self, name: impl Into<String>) -> Self {
        self.bin_name = Some(name.into());
        self
    }

    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    pub fn about(mut self, about: impl Into<String>) -> Self {
        self.about = Some(about.into());
        self
    }

    pub fn long_about(mut self, about: impl Into<String>) -> Self {
        self.long_about = Some(about.into());
        self
    }

    pub fn arg_required_else_help(mut self, yes: bool) -> Self {
        self.arg_required_else_help = yes;
        self
    }

    pub fn disable_help_flag(mut self, yes: bool) -> Self {
        self.disable_help_flag = yes;
        self
    }

    pub fn disable_version_flag(mut self, yes: bool) -> Self {
        self.disable_version_flag = yes;
        self
    }

    pub fn arg(mut self, arg: Arg) -> Self {
        self.args.push(arg);
        self
    }

    pub fn args(mut self, args: impl IntoIterator<Item = Arg>) -> Self {
        self.args.extend(args);
        self
    }

    pub fn subcommand(mut self, sub: Command) -> Self {
        self.subcommands.push(sub);
        self
    }

    pub fn subcommands(mut self, subs: impl IntoIterator<Item = Command>) -> Self {
        self.subcommands.extend(subs);
        self
    }

    pub fn alias(mut self, alias: impl Into<String>) -> Self {
        let alias = alias.into();
        self.aliases.insert(alias.clone(), self.name.clone());
        self.visible_aliases.push(alias);
        self
    }

    pub fn aliases(mut self, aliases: impl IntoIterator<Item = impl Into<String>>) -> Self {
        for a in aliases {
            let a = a.into();
            self.aliases.insert(a.clone(), self.name.clone());
            self.visible_aliases.push(a);
        }
        self
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    pub fn render_help(&self) -> String {
        help::render_help(self)
    }

    pub fn render_usage(&self) -> String {
        help::render_usage(self)
    }

    pub fn get_matches(self) -> Result<ArgMatches, Error> {
        self.get_matches_from(std::env::args_os())
    }

    pub fn get_matches_from<I, T>(mut self, itr: I) -> Result<ArgMatches, Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        if !self.disable_help_flag {
            self = self.arg(
                Arg::long_flag("help", "help")
                    .short('h')
                    .help("Print help")
                    .hide(false),
            );
        }
        if !self.disable_version_flag {
            if let Some(ver) = self.version.clone() {
                self = self.arg(
                    Arg::long_flag("version", "version")
                        .help("Print version")
                        .default_value(ver),
                );
            }
        }
        parse::parse_command(&self, itr)
    }

    pub fn try_get_matches(self) -> Result<ArgMatches, Error> {
        self.get_matches()
    }

    pub fn try_get_matches_from<I, T>(self, itr: I) -> Result<ArgMatches, Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        self.get_matches_from(itr)
    }

    pub fn print_help(&self) -> Result<(), Error> {
        println!("{}", self.render_help());
        Ok(())
    }

    pub fn print_long_help(&self) -> Result<(), Error> {
        println!("{}", self.render_help());
        Ok(())
    }

    pub fn error(&self, kind: ErrorKind, message: impl Into<String>) -> Error {
        Error::new(kind, message)
    }
}
