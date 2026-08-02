use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    DisplayHelp,
    DisplayHelpOnMissing,
    DisplayVersion,
    UnknownArgument,
    MissingRequiredArgument,
    MissingSubcommand,
    InvalidValue { arg: String, value: String },
    TooManyValues { arg: String },
    Usage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: String,
}

impl Error {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn display_help(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::DisplayHelp, message)
    }

    pub fn exit_code(&self) -> i32 {
        match self.kind {
            ErrorKind::DisplayHelp
            | ErrorKind::DisplayHelpOnMissing
            | ErrorKind::DisplayVersion => 0,
            _ => 2,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for Error {}
