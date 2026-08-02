use niao_time::{now_unix_ms, Timezone};

/// Which direction to bias ambiguous dates (e.g. weekday without modifier).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreferDirection {
    #[default]
    Future,
    Past,
    Current,
}

/// Numeric date component order for slash/dash formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DateOrder {
    #[default]
    Mdy,
    Dmy,
    Ymd,
}

/// Whether parsed result must include date, time, or both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RequireParts {
    #[default]
    Any,
    Date,
    Time,
    Both,
}

/// Parser settings (~dateparser settings subset).
#[derive(Debug, Clone)]
pub struct ParseOptions {
    /// Reference instant (unix ms) for relative phrases.
    pub base_ms: i64,
    /// IANA timezone name for civil conversion.
    pub timezone: String,
    pub prefer: PreferDirection,
    pub date_order: DateOrder,
    /// Allow extra whitespace/punctuation; does not enable typo correction.
    pub fuzzy: bool,
    pub require: RequireParts,
    /// Language tags (English `"en"` is fully supported).
    pub languages: Vec<String>,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            base_ms: now_unix_ms(),
            timezone: "UTC".into(),
            prefer: PreferDirection::Future,
            date_order: DateOrder::Mdy,
            fuzzy: true,
            require: RequireParts::Any,
            languages: vec!["en".into()],
        }
    }
}

impl ParseOptions {
    /// >>> use niao_when::options::ParseOptions;
    /// >>> let o = ParseOptions::default();
    /// >>> o.languages[0] == "en"
    /// true
    pub fn with_base_ms(mut self, ms: i64) -> Self {
        self.base_ms = ms;
        self
    }

    /// >>> use niao_when::options::ParseOptions;
    /// >>> ParseOptions::default().with_timezone("America/New_York").timezone
    /// "America/New_York"
    pub fn with_timezone(mut self, tz: impl Into<String>) -> Self {
        self.timezone = tz.into();
        self
    }

    pub fn resolve_tz(&self) -> Result<Timezone, String> {
        Timezone::named(&self.timezone)
    }
}
