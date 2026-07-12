//! Lightweight, zero-dependency structured logging for Niao.
//!
//! Disabled events perform one relaxed atomic load. Enabled events are written
//! synchronously to one or more text, JSON, or file layers. Spans are tracked
//! per thread; [`SpanContext`] provides explicit propagation across tasks.

use std::borrow::Cow;
use std::cell::RefCell;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Severity of an event. Higher numeric values are more verbose.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum Level {
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

impl Level {
    #[inline]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
            Self::Trace => "TRACE",
        }
    }
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Level {
    type Err = ParseLevelError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "error" => Ok(Self::Error),
            "warn" | "warning" => Ok(Self::Warn),
            "info" => Ok(Self::Info),
            "debug" => Ok(Self::Debug),
            "trace" => Ok(Self::Trace),
            _ => Err(ParseLevelError),
        }
    }
}

/// Maximum enabled verbosity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum LevelFilter {
    Off = 0,
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

impl LevelFilter {
    #[inline]
    pub const fn allows(self, level: Level) -> bool {
        level as u8 <= self as u8
    }

    #[inline]
    const fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Error,
            2 => Self::Warn,
            3 => Self::Info,
            4 => Self::Debug,
            5.. => Self::Trace,
            _ => Self::Off,
        }
    }
}

impl From<Level> for LevelFilter {
    fn from(level: Level) -> Self {
        Self::from_u8(level as u8)
    }
}

impl FromStr for LevelFilter {
    type Err = ParseLevelError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.trim().eq_ignore_ascii_case("off") {
            return Ok(Self::Off);
        }
        value.parse::<Level>().map(Into::into)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParseLevelError;

impl fmt::Display for ParseLevelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("expected off, error, warn, info, debug, or trace")
    }
}

impl std::error::Error for ParseLevelError {}

/// A structured field value. String references are not copied.
#[derive(Debug)]
pub enum FieldValue<'a> {
    Str(Cow<'a, str>),
    I64(i64),
    U64(u64),
    F64(f64),
    Bool(bool),
    Null,
}

macro_rules! signed_values {
    ($($ty:ty),* $(,)?) => {$(
        impl From<$ty> for FieldValue<'_> {
            #[inline]
            fn from(value: $ty) -> Self { Self::I64(value as i64) }
        }
    )*};
}

macro_rules! unsigned_values {
    ($($ty:ty),* $(,)?) => {$(
        impl From<$ty> for FieldValue<'_> {
            #[inline]
            fn from(value: $ty) -> Self { Self::U64(value as u64) }
        }
    )*};
}

signed_values!(i8, i16, i32, i64, isize);
unsigned_values!(u8, u16, u32, u64, usize);

impl From<f32> for FieldValue<'_> {
    fn from(value: f32) -> Self {
        Self::F64(value as f64)
    }
}

impl From<f64> for FieldValue<'_> {
    fn from(value: f64) -> Self {
        Self::F64(value)
    }
}

impl From<bool> for FieldValue<'_> {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl<'a> From<&'a str> for FieldValue<'a> {
    fn from(value: &'a str) -> Self {
        Self::Str(Cow::Borrowed(value))
    }
}

impl<'a> From<&'a String> for FieldValue<'a> {
    fn from(value: &'a String) -> Self {
        Self::Str(Cow::Borrowed(value.as_str()))
    }
}

impl From<String> for FieldValue<'_> {
    fn from(value: String) -> Self {
        Self::Str(Cow::Owned(value))
    }
}

impl From<()> for FieldValue<'_> {
    fn from(_: ()) -> Self {
        Self::Null
    }
}

/// A key/value field attached to an event or span.
#[derive(Debug)]
pub struct Field<'a> {
    pub name: Cow<'a, str>,
    pub value: FieldValue<'a>,
}

impl<'a> Field<'a> {
    #[inline]
    pub fn new(name: impl Into<Cow<'a, str>>, value: impl Into<FieldValue<'a>>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

/// Event metadata and fields passed to layers.
#[derive(Debug)]
pub struct Event<'a> {
    pub timestamp_us: u64,
    pub level: Level,
    pub target: &'a str,
    pub message: &'a str,
    pub fields: &'a [Field<'a>],
}

#[derive(Clone, Debug)]
pub struct SpanData {
    pub id: u64,
    pub name: String,
    pub fields: Vec<OwnedField>,
}

#[derive(Clone, Debug)]
pub struct OwnedField {
    pub name: String,
    pub value: OwnedValue,
}

#[derive(Clone, Debug)]
pub enum OwnedValue {
    Str(String),
    I64(i64),
    U64(u64),
    F64(f64),
    Bool(bool),
    Null,
}

impl<'a> From<&FieldValue<'a>> for OwnedValue {
    fn from(value: &FieldValue<'a>) -> Self {
        match value {
            FieldValue::Str(value) => Self::Str(value.to_string()),
            FieldValue::I64(value) => Self::I64(*value),
            FieldValue::U64(value) => Self::U64(*value),
            FieldValue::F64(value) => Self::F64(*value),
            FieldValue::Bool(value) => Self::Bool(*value),
            FieldValue::Null => Self::Null,
        }
    }
}

thread_local! {
    static SPAN_STACK: RefCell<Vec<SpanData>> = const { RefCell::new(Vec::new()) };
}

static NEXT_SPAN_ID: AtomicU64 = AtomicU64::new(1);

/// An entered span. Dropping it removes that span from the current thread.
#[derive(Debug)]
pub struct Span {
    id: u64,
}

impl Span {
    #[doc(hidden)]
    pub const fn disabled() -> Self {
        Self { id: 0 }
    }

    pub fn new(name: impl Into<String>, fields: &[Field<'_>]) -> Self {
        let id = NEXT_SPAN_ID.fetch_add(1, Ordering::Relaxed);
        let fields = fields
            .iter()
            .map(|field| OwnedField {
                name: field.name.to_string(),
                value: (&field.value).into(),
            })
            .collect();
        SPAN_STACK.with(|stack| {
            stack.borrow_mut().push(SpanData {
                id,
                name: name.into(),
                fields,
            });
        });
        Self { id }
    }

    /// Capture all currently entered spans for explicit task propagation.
    pub fn capture_context() -> SpanContext {
        SpanContext::capture()
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        if self.id != 0 {
            remove_span(self.id);
        }
    }
}

fn remove_span(id: u64) {
    SPAN_STACK.with(|stack| {
        let mut stack = stack.borrow_mut();
        if stack.last().is_some_and(|span| span.id == id) {
            stack.pop();
        } else if let Some(index) = stack.iter().position(|span| span.id == id) {
            stack.remove(index);
        }
    });
}

/// A clonable span stack for moving context into another task or thread.
#[derive(Clone, Debug, Default)]
pub struct SpanContext {
    spans: Vec<SpanData>,
}

impl SpanContext {
    pub fn capture() -> Self {
        Self {
            spans: SPAN_STACK.with(|stack| stack.borrow().clone()),
        }
    }

    /// Enter this context until the returned guard is dropped.
    pub fn enter(&self) -> ContextGuard {
        let mut entered = self.spans.clone();
        let ids = entered
            .iter_mut()
            .map(|span| {
                span.id = NEXT_SPAN_ID.fetch_add(1, Ordering::Relaxed);
                span.id
            })
            .collect::<Vec<_>>();
        SPAN_STACK.with(|stack| stack.borrow_mut().extend(entered));
        ContextGuard { ids }
    }
}

pub struct ContextGuard {
    ids: Vec<u64>,
}

impl Drop for ContextGuard {
    fn drop(&mut self) {
        for id in self.ids.iter().rev() {
            remove_span(*id);
        }
    }
}

/// Receives enabled events. Implementations must be thread-safe.
pub trait Layer: Send + Sync {
    fn max_level(&self) -> LevelFilter;
    fn on_event(&self, event: &Event<'_>, spans: &[SpanData]) -> io::Result<()>;
}

/// A collection of output layers.
pub struct Registry {
    layers: Vec<Box<dyn Layer>>,
    max_level: LevelFilter,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            layers: Vec::new(),
            max_level: LevelFilter::Off,
        }
    }

    pub fn with<L: Layer + 'static>(mut self, layer: L) -> Self {
        self.max_level = self.max_level.max(layer.max_level());
        self.layers.push(Box::new(layer));
        self
    }

    #[inline]
    pub fn enabled(&self, level: Level) -> bool {
        self.max_level.allows(level)
    }

    pub fn emit(&self, event: &Event<'_>) -> io::Result<()> {
        SPAN_STACK.with(|stack| {
            let stack = stack.borrow();
            for layer in &self.layers {
                if layer.max_level().allows(event.level) {
                    layer.on_event(event, &stack)?;
                }
            }
            Ok(())
        })
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

struct Output {
    writer: Mutex<Box<dyn Write + Send>>,
    max_level: LevelFilter,
    format: OutputFormat,
}

#[derive(Clone, Copy)]
enum OutputFormat {
    Text,
    Json,
}

impl Output {
    fn new(
        writer: impl Write + Send + 'static,
        max_level: LevelFilter,
        format: OutputFormat,
    ) -> Self {
        Self {
            writer: Mutex::new(Box::new(writer)),
            max_level,
            format,
        }
    }

    fn write(&self, event: &Event<'_>, spans: &[SpanData]) -> io::Result<()> {
        let mut writer = self
            .writer
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        match self.format {
            OutputFormat::Text => write_text(&mut *writer, event, spans),
            OutputFormat::Json => write_json(&mut *writer, event, spans),
        }
    }
}

/// Human-readable output layer.
pub struct FmtLayer(Output);

impl FmtLayer {
    pub fn new(writer: impl Write + Send + 'static, max_level: LevelFilter) -> Self {
        Self(Output::new(writer, max_level, OutputFormat::Text))
    }

    pub fn stderr(max_level: LevelFilter) -> Self {
        Self::new(io::stderr(), max_level)
    }

    pub fn stdout(max_level: LevelFilter) -> Self {
        Self::new(io::stdout(), max_level)
    }
}

impl Layer for FmtLayer {
    fn max_level(&self) -> LevelFilter {
        self.0.max_level
    }

    fn on_event(&self, event: &Event<'_>, spans: &[SpanData]) -> io::Result<()> {
        self.0.write(event, spans)
    }
}

/// Newline-delimited JSON output layer.
pub struct JsonLayer(Output);

impl JsonLayer {
    pub fn new(writer: impl Write + Send + 'static, max_level: LevelFilter) -> Self {
        Self(Output::new(writer, max_level, OutputFormat::Json))
    }
}

impl Layer for JsonLayer {
    fn max_level(&self) -> LevelFilter {
        self.0.max_level
    }

    fn on_event(&self, event: &Event<'_>, spans: &[SpanData]) -> io::Result<()> {
        self.0.write(event, spans)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileFormat {
    Text,
    Json,
}

/// Append-only file output layer.
pub struct FileLayer(Output);

impl FileLayer {
    pub fn new(
        path: impl AsRef<Path>,
        format: FileFormat,
        max_level: LevelFilter,
    ) -> io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        let format = match format {
            FileFormat::Text => OutputFormat::Text,
            FileFormat::Json => OutputFormat::Json,
        };
        Ok(Self(Output::new(file, max_level, format)))
    }

    pub fn from_file(file: File, format: FileFormat, max_level: LevelFilter) -> Self {
        let format = match format {
            FileFormat::Text => OutputFormat::Text,
            FileFormat::Json => OutputFormat::Json,
        };
        Self(Output::new(file, max_level, format))
    }
}

impl Layer for FileLayer {
    fn max_level(&self) -> LevelFilter {
        self.0.max_level
    }

    fn on_event(&self, event: &Event<'_>, spans: &[SpanData]) -> io::Result<()> {
        self.0.write(event, spans)
    }
}

fn write_text(writer: &mut dyn Write, event: &Event<'_>, spans: &[SpanData]) -> io::Result<()> {
    write!(
        writer,
        "{} {} {}",
        event.timestamp_us,
        event.level.as_str(),
        event.target
    )?;
    for span in spans {
        write!(writer, " span={}", span.name)?;
        for field in &span.fields {
            write!(writer, " {}.", span.name)?;
            write!(writer, "{}=", field.name)?;
            write_owned_text(writer, &field.value)?;
        }
    }
    write!(writer, " {}", event.message)?;
    for field in event.fields {
        write!(writer, " {}=", field.name)?;
        write_value_text(writer, &field.value)?;
    }
    writer.write_all(b"\n")
}

fn write_value_text(writer: &mut dyn Write, value: &FieldValue<'_>) -> io::Result<()> {
    match value {
        FieldValue::Str(value) => writer.write_all(value.as_bytes()),
        FieldValue::I64(value) => write!(writer, "{value}"),
        FieldValue::U64(value) => write!(writer, "{value}"),
        FieldValue::F64(value) => write!(writer, "{value}"),
        FieldValue::Bool(value) => write!(writer, "{value}"),
        FieldValue::Null => writer.write_all(b"null"),
    }
}

fn write_owned_text(writer: &mut dyn Write, value: &OwnedValue) -> io::Result<()> {
    match value {
        OwnedValue::Str(value) => writer.write_all(value.as_bytes()),
        OwnedValue::I64(value) => write!(writer, "{value}"),
        OwnedValue::U64(value) => write!(writer, "{value}"),
        OwnedValue::F64(value) => write!(writer, "{value}"),
        OwnedValue::Bool(value) => write!(writer, "{value}"),
        OwnedValue::Null => writer.write_all(b"null"),
    }
}

fn write_json(writer: &mut dyn Write, event: &Event<'_>, spans: &[SpanData]) -> io::Result<()> {
    write!(
        writer,
        "{{\"timestamp_us\":{},\"level\":\"",
        event.timestamp_us
    )?;
    writer.write_all(event.level.as_str().as_bytes())?;
    writer.write_all(b"\",\"target\":\"")?;
    write_json_string(writer, event.target)?;
    writer.write_all(b"\",\"message\":\"")?;
    write_json_string(writer, event.message)?;
    writer.write_all(b"\",\"fields\":{")?;
    for (index, field) in event.fields.iter().enumerate() {
        if index != 0 {
            writer.write_all(b",")?;
        }
        writer.write_all(b"\"")?;
        write_json_string(writer, &field.name)?;
        writer.write_all(b"\":")?;
        write_value_json(writer, &field.value)?;
    }
    writer.write_all(b"},\"spans\":[")?;
    for (span_index, span) in spans.iter().enumerate() {
        if span_index != 0 {
            writer.write_all(b",")?;
        }
        writer.write_all(b"{\"name\":\"")?;
        write_json_string(writer, &span.name)?;
        writer.write_all(b"\",\"fields\":{")?;
        for (field_index, field) in span.fields.iter().enumerate() {
            if field_index != 0 {
                writer.write_all(b",")?;
            }
            writer.write_all(b"\"")?;
            write_json_string(writer, &field.name)?;
            writer.write_all(b"\":")?;
            write_owned_json(writer, &field.value)?;
        }
        writer.write_all(b"}}")?;
    }
    writer.write_all(b"]}\n")
}

fn write_json_string(writer: &mut dyn Write, value: &str) -> io::Result<()> {
    let bytes = value.as_bytes();
    let mut start = 0;
    for (index, byte) in bytes.iter().copied().enumerate() {
        let escape = match byte {
            b'"' => Some(br#"\""#.as_slice()),
            b'\\' => Some(br#"\\"#.as_slice()),
            b'\n' => Some(br#"\n"#.as_slice()),
            b'\r' => Some(br#"\r"#.as_slice()),
            b'\t' => Some(br#"\t"#.as_slice()),
            0x00..=0x1f => None,
            _ => continue,
        };
        writer.write_all(&bytes[start..index])?;
        if let Some(escape) = escape {
            writer.write_all(escape)?;
        } else {
            write!(writer, "\\u{:04x}", byte)?;
        }
        start = index + 1;
    }
    writer.write_all(&bytes[start..])
}

fn write_value_json(writer: &mut dyn Write, value: &FieldValue<'_>) -> io::Result<()> {
    match value {
        FieldValue::Str(value) => {
            writer.write_all(b"\"")?;
            write_json_string(writer, value)?;
            writer.write_all(b"\"")
        }
        FieldValue::I64(value) => write!(writer, "{value}"),
        FieldValue::U64(value) => write!(writer, "{value}"),
        FieldValue::F64(value) if value.is_finite() => write!(writer, "{value}"),
        FieldValue::F64(_) => writer.write_all(b"null"),
        FieldValue::Bool(value) => write!(writer, "{value}"),
        FieldValue::Null => writer.write_all(b"null"),
    }
}

fn write_owned_json(writer: &mut dyn Write, value: &OwnedValue) -> io::Result<()> {
    match value {
        OwnedValue::Str(value) => {
            writer.write_all(b"\"")?;
            write_json_string(writer, value)?;
            writer.write_all(b"\"")
        }
        OwnedValue::I64(value) => write!(writer, "{value}"),
        OwnedValue::U64(value) => write!(writer, "{value}"),
        OwnedValue::F64(value) if value.is_finite() => write!(writer, "{value}"),
        OwnedValue::F64(_) => writer.write_all(b"null"),
        OwnedValue::Bool(value) => write!(writer, "{value}"),
        OwnedValue::Null => writer.write_all(b"null"),
    }
}

static GLOBAL: OnceLock<Registry> = OnceLock::new();
static GLOBAL_LEVEL: AtomicU8 = AtomicU8::new(LevelFilter::Off as u8);

/// Returns whether the global subscriber enables `level`.
#[inline(always)]
pub fn enabled(level: Level) -> bool {
    level as u8 <= GLOBAL_LEVEL.load(Ordering::Relaxed)
}

/// Installs the process-wide subscriber.
pub fn set_global_default(registry: Registry) -> Result<(), SetGlobalError> {
    let max_level = registry.max_level;
    GLOBAL.set(registry).map_err(|_| SetGlobalError)?;
    GLOBAL_LEVEL.store(max_level as u8, Ordering::Release);
    Ok(())
}

/// Initializes a text subscriber using `NIAO_LOG`, then `RUST_LOG`, then `info`.
pub fn init() -> Result<(), SetGlobalError> {
    SubscriberBuilder::new().with_env_filter().try_init()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SetGlobalError;

impl fmt::Display for SetGlobalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a global niao_log subscriber is already installed")
    }
}

impl std::error::Error for SetGlobalError {}

/// Builder for the common text/JSON/file subscriber setup.
pub struct SubscriberBuilder {
    max_level: LevelFilter,
    registry: Registry,
    has_layer: bool,
}

impl SubscriberBuilder {
    pub fn new() -> Self {
        Self {
            max_level: LevelFilter::Info,
            registry: Registry::new(),
            has_layer: false,
        }
    }

    pub fn with_max_level(mut self, max_level: LevelFilter) -> Self {
        self.max_level = max_level;
        self
    }

    pub fn with_env_filter(mut self) -> Self {
        self.max_level = env_level();
        self
    }

    pub fn with_fmt(mut self) -> Self {
        self.registry = self.registry.with(FmtLayer::stderr(self.max_level));
        self.has_layer = true;
        self
    }

    pub fn with_json(mut self, writer: impl Write + Send + 'static) -> Self {
        self.registry = self.registry.with(JsonLayer::new(writer, self.max_level));
        self.has_layer = true;
        self
    }

    pub fn with_file(mut self, layer: FileLayer) -> Self {
        self.registry = self.registry.with(layer);
        self.has_layer = true;
        self
    }

    pub fn try_init(mut self) -> Result<(), SetGlobalError> {
        if !self.has_layer {
            self.registry = self.registry.with(FmtLayer::stderr(self.max_level));
        }
        set_global_default(self.registry)
    }
}

impl Default for SubscriberBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolve a maximum level from `NIAO_LOG` or `RUST_LOG`.
///
/// Comma-separated target directives are accepted; the most verbose directive
/// is used because target checks would add overhead to every disabled event.
pub fn env_level() -> LevelFilter {
    std::env::var("NIAO_LOG")
        .or_else(|_| std::env::var("RUST_LOG"))
        .ok()
        .and_then(|value| parse_filter(&value))
        .unwrap_or(LevelFilter::Info)
}

fn parse_filter(value: &str) -> Option<LevelFilter> {
    value
        .split(',')
        .filter_map(|directive| {
            directive
                .rsplit_once('=')
                .map_or(directive, |(_, level)| level)
                .parse::<LevelFilter>()
                .ok()
        })
        .max()
}

/// Emit an event through the global subscriber.
#[inline]
pub fn event(level: Level, target: &str, message: &str, fields: &[Field<'_>]) {
    if !enabled(level) {
        return;
    }
    if let Some(registry) = GLOBAL.get() {
        let event = Event {
            timestamp_us: timestamp_us(),
            level,
            target,
            message,
            fields,
        };
        let _ = registry.emit(&event);
    }
}

#[inline]
fn timestamp_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_micros() as u64)
}

#[macro_export]
macro_rules! event {
    ($level:expr, $message:expr $(,)?) => {
        $crate::event($level, module_path!(), $message, &[])
    };
    ($level:expr, $message:expr, $($name:expr => $value:expr),+ $(,)?) => {{
        let level = $level;
        if $crate::enabled(level) {
            let fields = [$($crate::Field::new($name, $value)),+];
            $crate::event(level, module_path!(), $message, &fields)
        }
    }};
    ($level:expr, $format:literal, $($argument:expr),+ $(,)?) => {{
        let level = $level;
        if $crate::enabled(level) {
            let message = format!($format, $($argument),+);
            $crate::event(level, module_path!(), &message, &[])
        }
    }};
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => { $crate::event!($crate::Level::Error, $($arg)*) };
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => { $crate::event!($crate::Level::Warn, $($arg)*) };
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => { $crate::event!($crate::Level::Info, $($arg)*) };
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => { $crate::event!($crate::Level::Debug, $($arg)*) };
}

#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => { $crate::event!($crate::Level::Trace, $($arg)*) };
}

#[macro_export]
macro_rules! span {
    ($level:expr, $name:expr $(,)?) => {{
        let level = $level;
        if $crate::enabled(level) {
            $crate::Span::new($name, &[])
        } else {
            $crate::Span::disabled()
        }
    }};
    ($level:expr, $name:expr, $($field:expr => $value:expr),+ $(,)?) => {{
        let level = $level;
        if $crate::enabled(level) {
            let fields = [$($crate::Field::new($field, $value)),+];
            $crate::Span::new($name, &fields)
        } else {
            $crate::Span::disabled()
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[derive(Clone, Default)]
    struct SharedWriter(Arc<Mutex<Vec<u8>>>);

    impl Write for SharedWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl SharedWriter {
        fn text(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    #[test]
    fn level_filtering_skips_verbose_events() {
        let output = SharedWriter::default();
        let registry = Registry::new().with(FmtLayer::new(output.clone(), LevelFilter::Info));
        let debug = Event {
            timestamp_us: 1,
            level: Level::Debug,
            target: "test",
            message: "hidden",
            fields: &[],
        };
        let info = Event {
            timestamp_us: 2,
            level: Level::Info,
            target: "test",
            message: "shown",
            fields: &[],
        };
        assert!(!registry.enabled(Level::Debug));
        assert!(registry.enabled(Level::Info));
        registry.emit(&debug).unwrap();
        registry.emit(&info).unwrap();
        assert!(!output.text().contains("hidden"));
        assert!(output.text().contains("shown"));
    }

    #[test]
    fn json_output_has_structured_fields_and_spans() {
        let output = SharedWriter::default();
        let registry = Registry::new().with(JsonLayer::new(output.clone(), LevelFilter::Trace));
        let _span = Span::new("request", &[Field::new("request_id", 42_u64)]);
        let fields = [
            Field::new("ok", true),
            Field::new("user", "a\"b"),
            Field::new("missing", ()),
        ];
        let event = Event {
            timestamp_us: 123,
            level: Level::Info,
            target: "tests",
            message: "accepted",
            fields: &fields,
        };
        registry.emit(&event).unwrap();
        assert_eq!(
            output.text(),
            "{\"timestamp_us\":123,\"level\":\"INFO\",\"target\":\"tests\",\"message\":\"accepted\",\"fields\":{\"ok\":true,\"user\":\"a\\\"b\",\"missing\":null},\"spans\":[{\"name\":\"request\",\"fields\":{\"request_id\":42}}]}\n"
        );
    }

    #[test]
    fn context_can_cross_threads() {
        let span = Span::new("parent", &[]);
        let context = SpanContext::capture();
        drop(span);
        let names = std::thread::spawn(move || {
            let _guard = context.enter();
            SPAN_STACK.with(|stack| {
                stack
                    .borrow()
                    .iter()
                    .map(|span| span.name.clone())
                    .collect::<Vec<_>>()
            })
        })
        .join()
        .unwrap();
        assert_eq!(names, ["parent"]);
    }

    #[test]
    fn parses_env_style_filter_directives() {
        assert_eq!(parse_filter("warn"), Some(LevelFilter::Warn));
        assert_eq!(
            parse_filter("niao=debug,other=trace"),
            Some(LevelFilter::Trace)
        );
        assert_eq!(parse_filter("garbage"), None);
    }

    #[test]
    fn disabled_macros_do_not_evaluate_fields() {
        let mut evaluated = false;
        info!("hidden", "expensive" => {
            evaluated = true;
            42
        });
        let _span = span!(Level::Debug, "hidden span", "expensive" => {
            evaluated = true;
            42
        });
        assert!(!evaluated);
    }
}
