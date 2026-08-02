//! SAX-style streaming XML parser.

use crate::error::{XmlError, MAX_BYTES};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::{HashMap, VecDeque};

/// Streaming parse options.
#[derive(Debug, Clone, Default)]
pub struct StreamOpts {
    pub trim_text: bool,
    pub expand_empty: bool,
}

/// One streaming event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEvent {
    Start {
        tag: String,
        attrs: HashMap<String, String>,
        line: u32,
        col: u32,
    },
    End {
        tag: String,
        line: u32,
        col: u32,
    },
    Text {
        text: String,
        line: u32,
        col: u32,
    },
    Comment {
        text: String,
        line: u32,
        col: u32,
    },
    Pi {
        target: String,
        data: String,
        line: u32,
        col: u32,
    },
    Decl {
        version: Option<String>,
        encoding: Option<String>,
        line: u32,
        col: u32,
    },
}

fn decl_attr(bytes: std::borrow::Cow<'_, [u8]>) -> String {
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Owned streaming reader (holds source string).
pub struct XmlStreamOwned {
    source: String,
    reader: Reader<std::io::Cursor<Vec<u8>>>,
    buf: Vec<u8>,
    finished: bool,
    opts: StreamOpts,
    pending: VecDeque<StreamEvent>,
}

impl XmlStreamOwned {
    pub fn new(input: impl Into<String>, opts: StreamOpts) -> Result<Self, XmlError> {
        let source = input.into();
        if source.len() > MAX_BYTES {
            return Err(XmlError::TooLarge(source.len()));
        }
        let reader = Reader::from_reader(std::io::Cursor::new(source.as_bytes().to_vec()));
        Ok(Self {
            source,
            reader,
            buf: Vec::new(),
            finished: false,
            opts,
            pending: VecDeque::new(),
        })
    }

    pub fn next_event(&mut self) -> Result<Option<StreamEvent>, XmlError> {
        if self.finished && self.pending.is_empty() {
            return Ok(None);
        }
        if let Some(ev) = self.pending.pop_front() {
            return Ok(Some(ev));
        }
        self.reader.config_mut().trim_text(self.opts.trim_text);
        self.reader.config_mut().expand_empty_elements = self.opts.expand_empty;

        loop {
            let pos = self.reader.buffer_position() as u32;
            match self.reader.read_event_into(&mut self.buf) {
                Ok(Event::Eof) => {
                    self.finished = true;
                    if let Some(ev) = self.pending.pop_front() {
                        return Ok(Some(ev));
                    }
                    return Ok(None);
                }
                Ok(Event::Start(e)) => {
                    let tag = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                    let mut attrs = HashMap::new();
                    for attr in e.attributes().with_checks(false).flatten() {
                        let k = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
                        let v = String::from_utf8_lossy(&attr.value).into_owned();
                        attrs.insert(k, v);
                    }
                    self.buf.clear();
                    return Ok(Some(StreamEvent::Start {
                        tag,
                        attrs,
                        line: pos,
                        col: 0,
                    }));
                }
                Ok(Event::Empty(e)) => {
                    let tag = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                    let mut attrs = HashMap::new();
                    for attr in e.attributes().with_checks(false).flatten() {
                        let k = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
                        let v = String::from_utf8_lossy(&attr.value).into_owned();
                        attrs.insert(k, v);
                    }
                    self.buf.clear();
                    self.pending.push_back(StreamEvent::End {
                        tag: tag.clone(),
                        line: pos,
                        col: 0,
                    });
                    return Ok(Some(StreamEvent::Start {
                        tag,
                        attrs,
                        line: pos,
                        col: 0,
                    }));
                }
                Ok(Event::End(e)) => {
                    let tag = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                    self.buf.clear();
                    return Ok(Some(StreamEvent::End {
                        tag,
                        line: pos,
                        col: 0,
                    }));
                }
                Ok(Event::Text(e)) => {
                    let text = e
                        .unescape()
                        .map(|c| c.into_owned())
                        .unwrap_or_else(|_| String::from_utf8_lossy(e.as_ref()).into_owned());
                    if text.is_empty() {
                        self.buf.clear();
                        continue;
                    }
                    self.buf.clear();
                    return Ok(Some(StreamEvent::Text {
                        text,
                        line: pos,
                        col: 0,
                    }));
                }
                Ok(Event::CData(e)) => {
                    let text = String::from_utf8_lossy(e.as_ref()).into_owned();
                    if text.is_empty() {
                        self.buf.clear();
                        continue;
                    }
                    self.buf.clear();
                    return Ok(Some(StreamEvent::Text {
                        text,
                        line: pos,
                        col: 0,
                    }));
                }
                Ok(Event::Comment(e)) => {
                    let text = e
                        .unescape()
                        .map(|c| c.into_owned())
                        .unwrap_or_else(|_| String::from_utf8_lossy(e.as_ref()).into_owned());
                    self.buf.clear();
                    return Ok(Some(StreamEvent::Comment {
                        text,
                        line: pos,
                        col: 0,
                    }));
                }
                Ok(Event::PI(e)) => {
                    let target = String::from_utf8_lossy(e.target()).into_owned();
                    let data = String::from_utf8_lossy(e.content()).trim().to_string();
                    self.buf.clear();
                    return Ok(Some(StreamEvent::Pi {
                        target,
                        data,
                        line: pos,
                        col: 0,
                    }));
                }
                Ok(Event::Decl(e)) => {
                    let version = e.version().ok().map(decl_attr);
                    let encoding = e.encoding().and_then(|r| r.ok()).map(decl_attr);
                    self.buf.clear();
                    return Ok(Some(StreamEvent::Decl {
                        version,
                        encoding,
                        line: pos,
                        col: 0,
                    }));
                }
                Ok(Event::DocType(_)) => {
                    self.buf.clear();
                    continue;
                }
                Err(e) => {
                    return Err(XmlError::parse(0, pos, e.to_string()));
                }
                _ => {
                    self.buf.clear();
                    continue;
                }
            }
        }
    }

    pub fn source(&self) -> &str {
        &self.source
    }
}

/// Collect all streaming events (for tests / small docs).
pub fn stream_collect(input: &str, opts: &StreamOpts) -> Result<Vec<StreamEvent>, XmlError> {
    let mut s = XmlStreamOwned::new(input, opts.clone())?;
    let mut out = Vec::new();
    while let Some(ev) = s.next_event()? {
        out.push(ev);
    }
    Ok(out)
}
