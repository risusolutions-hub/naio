//! YAML parse/emit errors.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum YamlError {
    Parse(String),
    Emit(String),
    TooLarge(usize),
    UnsafeTag(String),
    MultiDocSingle,
    EmptyInput,
    Io(String),
}

impl YamlError {
    pub fn message(&self) -> String {
        match self {
            Self::Parse(m) => m.clone(),
            Self::Emit(m) => m.clone(),
            Self::TooLarge(n) => format!("input size {n} exceeds limit {}", crate::MAX_BYTES),
            Self::UnsafeTag(t) => format!("unsafe YAML tag rejected in safe mode: {t}"),
            Self::MultiDocSingle => {
                "multiple YAML documents in input; use parse_all() or pass multi: true".into()
            }
            Self::EmptyInput => "empty YAML input".into(),
            Self::Io(m) => m.clone(),
        }
    }
}
