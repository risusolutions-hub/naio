use std::ffi::OsString;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgAction {
    Set,
    SetTrue,
    Append,
    Count,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumArgs {
    ZeroOrOne,
    ExactlyOne,
    OneOrMore,
    ZeroOrMore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValueHint {
    #[default]
    Unknown,
    Other,
    FilePath,
    DirPath,
}

#[derive(Debug, Clone)]
pub struct Arg {
    pub(crate) id: String,
    pub(crate) long: Option<String>,
    pub(crate) short: Option<char>,
    pub(crate) action: ArgAction,
    pub(crate) num_args: NumArgs,
    pub(crate) required: bool,
    pub(crate) default_value: Option<OsString>,
    pub(crate) default_values: Vec<OsString>,
    pub(crate) env: Option<OsString>,
    pub(crate) help: Option<String>,
    pub(crate) value_name: Option<String>,
    pub(crate) value_delimiter: Option<char>,
    pub(crate) trailing_var_arg: bool,
    pub(crate) allow_hyphen_values: bool,
    pub(crate) hide: bool,
    pub(crate) global: bool,
    pub(crate) value_hint: ValueHint,
}

impl Arg {
    pub fn new(id: impl Into<String>) -> Self {
        let id = id.into();
        Self {
            id: id.clone(),
            long: Some(id),
            short: None,
            action: ArgAction::Set,
            num_args: NumArgs::ExactlyOne,
            required: false,
            default_value: None,
            default_values: Vec::new(),
            env: None,
            help: None,
            value_name: None,
            value_delimiter: None,
            trailing_var_arg: false,
            allow_hyphen_values: false,
            hide: false,
            global: false,
            value_hint: ValueHint::Unknown,
        }
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn long(mut self, long: impl Into<String>) -> Self {
        self.long = Some(long.into());
        self
    }

    pub fn short(mut self, short: char) -> Self {
        self.short = Some(short);
        self
    }

    pub fn action(mut self, action: ArgAction) -> Self {
        self.action = action;
        self
    }

    pub fn num_args(mut self, num: NumArgs) -> Self {
        self.num_args = num;
        self
    }

    pub fn required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    pub fn default_value(mut self, value: impl Into<OsString>) -> Self {
        self.default_value = Some(value.into());
        self
    }

    pub fn default_values(mut self, values: impl IntoIterator<Item = impl Into<OsString>>) -> Self {
        self.default_values = values.into_iter().map(Into::into).collect();
        self
    }

    pub fn env(mut self, var: impl Into<OsString>) -> Self {
        self.env = Some(var.into());
        self
    }

    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn value_name(mut self, name: impl Into<String>) -> Self {
        self.value_name = Some(name.into());
        self
    }

    pub fn value_delimiter(mut self, delimiter: char) -> Self {
        self.value_delimiter = Some(delimiter);
        self
    }

    pub fn trailing_var_arg(mut self, yes: bool) -> Self {
        self.trailing_var_arg = yes;
        self
    }

    pub fn allow_hyphen_values(mut self, yes: bool) -> Self {
        self.allow_hyphen_values = yes;
        self
    }

    pub fn hide(mut self, yes: bool) -> Self {
        self.hide = yes;
        self
    }

    pub fn global(mut self, yes: bool) -> Self {
        self.global = yes;
        self
    }

    pub fn value_hint(mut self, hint: ValueHint) -> Self {
        self.value_hint = hint;
        self
    }

    pub fn long_flag(id: impl Into<String>, long: impl Into<String>) -> Self {
        Arg::new(id)
            .long(long)
            .action(ArgAction::SetTrue)
            .num_args(NumArgs::ZeroOrOne)
    }

    pub fn positional(id: impl Into<String>) -> Self {
        let id = id.into();
        Arg::new(id.clone())
            .long("")
            .action(ArgAction::Set)
            .num_args(NumArgs::ExactlyOne)
    }
}
