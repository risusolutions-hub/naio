use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum ExprError {
    Lex {
        pos: usize,
        message: String,
    },
    Parse {
        pos: usize,
        message: String,
    },
    Eval {
        message: String,
    },
    UndefinedVar(String),
    UndefinedFn(String),
    Arity {
        name: String,
        expected: String,
        got: usize,
    },
    Type {
        message: String,
    },
    DivByZero,
    Disabled {
        what: String,
    },
}

impl ExprError {
    pub fn message(&self) -> String {
        match self {
            ExprError::Lex { message, .. } => message.clone(),
            ExprError::Parse { message, .. } => message.clone(),
            ExprError::Eval { message } => message.clone(),
            ExprError::UndefinedVar(n) => format!("undefined variable '{n}'"),
            ExprError::UndefinedFn(n) => format!("undefined function '{n}'"),
            ExprError::Arity {
                name,
                expected,
                got,
            } => {
                format!("{name}() expects {expected}, got {got}")
            }
            ExprError::Type { message } => message.clone(),
            ExprError::DivByZero => "division by zero".into(),
            ExprError::Disabled { what } => format!("{what} is disabled in this evaluator"),
        }
    }
}

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for ExprError {}
