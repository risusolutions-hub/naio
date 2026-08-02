use similar::Algorithm;

/// Shared diff options (difflib / diff-match-patch subset).
#[derive(Debug, Clone)]
pub struct DiffOpts {
    pub ignore_whitespace: bool,
    pub ignore_case: bool,
    pub algorithm: Algorithm,
    pub context: usize,
    pub fromfile: String,
    pub tofile: String,
    pub fromfiledate: String,
    pub tofiledate: String,
    pub lineterm: String,
    pub join: bool,
    pub autojunk: bool,
    pub fuzz: i32,
}

impl Default for DiffOpts {
    fn default() -> Self {
        Self {
            ignore_whitespace: false,
            ignore_case: false,
            algorithm: Algorithm::Myers,
            context: 3,
            fromfile: String::new(),
            tofile: String::new(),
            fromfiledate: String::new(),
            tofiledate: String::new(),
            lineterm: "\n".into(),
            join: false,
            autojunk: false,
            fuzz: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    Line,
    Word,
    Char,
    UnicodeWord,
}

impl Granularity {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "line" | "lines" => Some(Self::Line),
            "word" | "words" => Some(Self::Word),
            "char" | "chars" => Some(Self::Char),
            "unicode_word" | "unicode_words" => Some(Self::UnicodeWord),
            _ => None,
        }
    }
}
