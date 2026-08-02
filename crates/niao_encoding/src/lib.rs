//! Charset detection and transcoding for Niao (`nencoding`).
//!
//! Backed by Mozilla's `encoding_rs` (SIMD-accelerated transcoding) and
//! `chardetng` (charset detection). Supports UTF-8/16, Shift-JIS, GBK,
//! Latin-1 family, and BOM handling.

use chardetng::EncodingDetector;
use encoding_rs::{EncoderResult, Encoding, UTF_16BE, UTF_16LE, UTF_8};
use unicode_normalization::UnicodeNormalization;

/// Maximum input/output size (64 MiB guard).
pub const MAX_BYTES: usize = 64 * 1024 * 1024;

/// How to handle malformed byte sequences during decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeErrorMode {
    Strict,
    Replace,
    Ignore,
}

impl DecodeErrorMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "strict" => Some(Self::Strict),
            "replace" | "surrogateescape" => Some(Self::Replace),
            "ignore" => Some(Self::Ignore),
            _ => None,
        }
    }
}

/// Result of charset detection.
#[derive(Debug, Clone, PartialEq)]
pub struct DetectionResult {
    pub encoding: String,
    /// 0.0–1.0 confidence heuristic (1.0 for BOM / valid UTF-8).
    pub confidence: f64,
    pub bom_encoding: Option<String>,
    pub language: Option<String>,
}

/// Metadata for a supported encoding label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodingInfo {
    pub name: String,
    pub aliases: Vec<String>,
    pub has_bom: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    UnknownEncoding(String),
    InvalidInput(String),
    TooLarge(usize),
    MalformedSequence(String),
}

impl EncodeError {
    pub fn message(&self) -> String {
        match self {
            Self::UnknownEncoding(e) => format!("unknown encoding: {e}"),
            Self::InvalidInput(m) => m.clone(),
            Self::TooLarge(n) => format!("data size {n} exceeds limit {MAX_BYTES}"),
            Self::MalformedSequence(m) => m.clone(),
        }
    }
}

/// Canonical supported encodings exposed to Niao.
const SUPPORTED: &[(&str, &[&str])] = &[
    ("utf-8", &["utf8", "unicode"]),
    ("utf-8-sig", &["utf_8_sig"]),
    (
        "utf-16-le",
        &["utf16le", "utf-16", "utf16", "ucs-2le", "ucs2le"],
    ),
    ("utf-16-be", &["utf16be", "ucs-2be", "ucs2be"]),
    (
        "shift_jis",
        &["shift-jis", "sjis", "cp932", "ms932", "windows-31j"],
    ),
    ("euc-jp", &["euc_jp", "eucjp"]),
    ("iso-2022-jp", &["iso_2022_jp", "iso2022jp"]),
    ("gbk", &["cp936", "ms936", "windows-936"]),
    ("gb18030", &["cp54936"]),
    ("big5", &["big5hkscs", "cp950"]),
    ("euc-kr", &["euc_kr", "euckr", "cp949"]),
    (
        "iso-8859-1",
        &["latin-1", "latin1", "iso8859-1", "iso88591", "l1"],
    ),
    ("windows-1252", &["cp1252", "ansi"]),
    ("ascii", &["us-ascii", "646"]),
    ("koi8-r", &["koi8r"]),
    ("iso-8859-5", &["cyrillic", "iso8859-5"]),
    ("windows-1251", &["cp1251"]),
];

fn normalize_label(label: &str) -> String {
    label.trim().to_ascii_lowercase().replace('_', "-")
}

/// Resolve a user-facing label to an `encoding_rs` encoding.
pub fn resolve_encoding(label: &str) -> Option<&'static Encoding> {
    let norm = normalize_label(label);
    if norm == "utf-8-sig" {
        return Some(UTF_8);
    }
    if norm == "utf-16-le" || norm == "utf-16" {
        return Some(UTF_16LE);
    }
    if norm == "utf-16-be" {
        return Some(UTF_16BE);
    }
    if let Some(enc) = Encoding::for_label(norm.as_bytes()) {
        return Some(enc);
    }
    if let Some(enc) = Encoding::for_label(norm.replace('-', "_").as_bytes()) {
        return Some(enc);
    }
    for (canonical, aliases) in SUPPORTED {
        if norm == normalize_label(canonical) || aliases.iter().any(|a| norm == normalize_label(a))
        {
            return resolve_encoding(canonical);
        }
    }
    None
}

fn canonical_name(enc: &'static Encoding) -> String {
    let label = enc.name().to_ascii_lowercase();
    if label == "utf-8" {
        return "utf-8".into();
    }
    label.replace('_', "-")
}

/// Detect BOM prefix; returns (encoding label, bom byte length).
pub fn detect_bom(bytes: &[u8]) -> Option<(String, usize)> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Some(("utf-8".into(), 3));
    }
    if bytes.starts_with(&[0xFF, 0xFE, 0x00, 0x00]) {
        return Some(("utf-32-le".into(), 4));
    }
    if bytes.starts_with(&[0x00, 0x00, 0xFE, 0xFF]) {
        return Some(("utf-32-be".into(), 4));
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return Some(("utf-16-le".into(), 2));
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return Some(("utf-16-be".into(), 2));
    }
    None
}

fn language_hint(enc: &'static Encoding) -> Option<String> {
    let name = enc.name();
    if name.contains("JP") || name == "SHIFT_JIS" || name == "EUC-JP" || name == "ISO-2022-JP" {
        return Some("Japanese".into());
    }
    if name.contains("GB") || name == "BIG5" {
        return Some("Chinese".into());
    }
    if name.contains("KR") {
        return Some("Korean".into());
    }
    if name.contains("RU") || name == "KOI8-R" || name == "windows-1251" {
        return Some("Russian".into());
    }
    None
}

fn is_probably_utf8(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok()
}

/// Detect the most likely charset for `bytes`.
pub fn detect(bytes: &[u8]) -> DetectionResult {
    if bytes.len() > MAX_BYTES {
        return DetectionResult {
            encoding: "utf-8".into(),
            confidence: 0.0,
            bom_encoding: None,
            language: None,
        };
    }

    if let Some((bom_enc, _)) = detect_bom(bytes) {
        let lang = language_hint(resolve_encoding(&bom_enc).unwrap_or(UTF_8));
        return DetectionResult {
            encoding: bom_enc.clone(),
            confidence: 1.0,
            bom_encoding: Some(bom_enc),
            language: lang,
        };
    }

    if is_probably_utf8(bytes) {
        return DetectionResult {
            encoding: "utf-8".into(),
            confidence: 1.0,
            bom_encoding: None,
            language: None,
        };
    }

    let mut detector = EncodingDetector::new();
    detector.feed(bytes, true);
    let (enc, certain) = detector.guess_assess(None, true);
    let confidence = if certain { 0.95 } else { 0.75 };
    let enc_name = canonical_name(enc);
    DetectionResult {
        language: language_hint(enc),
        bom_encoding: None,
        encoding: enc_name,
        confidence,
    }
}

/// Return up to `top` charset candidates sorted by confidence.
pub fn detect_all(bytes: &[u8], top: usize) -> Vec<DetectionResult> {
    let primary = detect(bytes);
    let mut out = vec![primary.clone()];

    if primary.confidence >= 1.0 {
        return out;
    }

    let mut detector = EncodingDetector::new();
    detector.feed(bytes, true);
    let (enc, _) = detector.guess_assess(None, true);
    let latin1 = Encoding::for_label(b"iso-8859-1").unwrap_or(encoding_rs::WINDOWS_1252);
    let candidates = [
        enc,
        UTF_8,
        UTF_16LE,
        UTF_16BE,
        encoding_rs::SHIFT_JIS,
        encoding_rs::GBK,
        latin1,
        encoding_rs::WINDOWS_1252,
    ];
    for c in candidates {
        let name = canonical_name(c);
        if out.iter().any(|r| r.encoding == name) {
            continue;
        }
        let valid = is_valid(bytes, &name);
        let conf = if valid { 0.5 } else { 0.2 };
        out.push(DetectionResult {
            encoding: name,
            confidence: conf,
            bom_encoding: None,
            language: language_hint(c),
        });
        if out.len() >= top.max(1) {
            break;
        }
    }
    out.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out.truncate(top.max(1));
    out
}

/// Strip a leading BOM, returning remaining bytes and detected BOM encoding.
pub fn strip_bom(bytes: &[u8]) -> (Vec<u8>, Option<String>) {
    if let Some((enc, len)) = detect_bom(bytes) {
        (bytes[len..].to_vec(), Some(enc))
    } else {
        (bytes.to_vec(), None)
    }
}

/// BOM bytes for an encoding (empty if none).
pub fn bom_for(label: &str) -> Result<Vec<u8>, EncodeError> {
    let norm = normalize_label(label);
    match norm.as_str() {
        "utf-8" | "utf-8-sig" => Ok(vec![0xEF, 0xBB, 0xBF]),
        "utf-16-le" | "utf-16" => Ok(vec![0xFF, 0xFE]),
        "utf-16-be" => Ok(vec![0xFE, 0xFF]),
        "utf-32-le" => Ok(vec![0xFF, 0xFE, 0x00, 0x00]),
        "utf-32-be" => Ok(vec![0x00, 0x00, 0xFE, 0xFF]),
        _ => {
            if resolve_encoding(label).is_some() {
                Ok(Vec::new())
            } else {
                Err(EncodeError::UnknownEncoding(label.to_string()))
            }
        }
    }
}

fn decode_with_mode(
    bytes: &[u8],
    enc: &'static Encoding,
    mode: DecodeErrorMode,
) -> Result<String, EncodeError> {
    if bytes.len() > MAX_BYTES {
        return Err(EncodeError::TooLarge(bytes.len()));
    }
    match mode {
        DecodeErrorMode::Strict => {
            let (s, _, had) = enc.decode(bytes);
            if had {
                return Err(EncodeError::MalformedSequence(format!(
                    "malformed {} sequence",
                    canonical_name(enc)
                )));
            }
            Ok(s.into_owned())
        }
        DecodeErrorMode::Replace => {
            let (s, _, _) = enc.decode(bytes);
            Ok(s.into_owned())
        }
        DecodeErrorMode::Ignore => {
            let (s, had) = enc.decode_without_bom_handling(bytes);
            if had {
                Ok(s.chars().filter(|c| *c != '\u{FFFD}').collect())
            } else {
                Ok(s.into_owned())
            }
        }
    }
}

/// Decode bytes to UTF-8 string. Auto-detects when `encoding` is `None`.
pub fn decode(
    bytes: &[u8],
    encoding: Option<&str>,
    mode: DecodeErrorMode,
) -> Result<String, EncodeError> {
    if bytes.len() > MAX_BYTES {
        return Err(EncodeError::TooLarge(bytes.len()));
    }

    let enc_label = match encoding {
        Some(e) => e.to_string(),
        None => detect(bytes).encoding,
    };

    let with_bom = normalize_label(&enc_label) == "utf-8-sig";
    let enc = resolve_encoding(&enc_label)
        .ok_or_else(|| EncodeError::UnknownEncoding(enc_label.clone()))?;

    let payload = if with_bom {
        strip_bom(bytes).0
    } else {
        bytes.to_vec()
    };

    // UTF-16 decoders need the BOM prefix when present in the original buffer.
    let decode_bytes = if enc == UTF_16LE || enc == UTF_16BE {
        bytes
    } else {
        &payload
    };

    decode_with_mode(decode_bytes, enc, mode)
}

/// Encode a UTF-8 string to bytes in `encoding`.
pub fn encode(text: &str, encoding: &str, with_bom: bool) -> Result<Vec<u8>, EncodeError> {
    if text.len() > MAX_BYTES {
        return Err(EncodeError::TooLarge(text.len()));
    }
    let norm = normalize_label(encoding);
    let enc = resolve_encoding(encoding)
        .ok_or_else(|| EncodeError::UnknownEncoding(encoding.to_string()))?;

    let mut bytes = if enc == UTF_8 {
        let (cow, _, had_errors) = enc.encode(text);
        if had_errors {
            return Err(EncodeError::InvalidInput(
                "string contains characters not representable in target encoding".into(),
            ));
        }
        cow.into_owned()
    } else if enc == UTF_16LE {
        text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
    } else if enc == UTF_16BE {
        text.encode_utf16().flat_map(|u| u.to_be_bytes()).collect()
    } else {
        let mut encoder = enc.new_encoder();
        let mut out = Vec::new();
        let min_cap = text.chars().count().saturating_mul(4).max(4);
        out.reserve(
            encoder
                .max_buffer_length_from_utf8_without_replacement(text.len())
                .unwrap_or(min_cap)
                .max(min_cap),
        );
        let (res, _read) =
            encoder.encode_from_utf8_to_vec_without_replacement(text, &mut out, true);
        if !matches!(res, EncoderResult::InputEmpty) {
            return Err(EncodeError::InvalidInput(
                "string contains characters not representable in target encoding".into(),
            ));
        }
        out
    };

    if with_bom || norm == "utf-8-sig" {
        let bom = bom_for(if norm == "utf-8-sig" {
            "utf-8-sig"
        } else {
            encoding
        })?;
        let mut out = bom;
        out.extend_from_slice(&bytes);
        bytes = out;
    }

    if bytes.len() > MAX_BYTES {
        return Err(EncodeError::TooLarge(bytes.len()));
    }
    Ok(bytes)
}

/// Transcode bytes from `from` (or auto-detect) to `to` encoding.
pub fn transcode(
    bytes: &[u8],
    from: Option<&str>,
    to: &str,
    mode: DecodeErrorMode,
) -> Result<Vec<u8>, EncodeError> {
    let text = decode(bytes, from, mode)?;
    encode(&text, to, false)
}

/// Check whether `bytes` are valid in `encoding` (no replacement chars in strict decode).
pub fn is_valid(bytes: &[u8], encoding: &str) -> bool {
    let enc = match resolve_encoding(encoding) {
        Some(e) => e,
        None => return false,
    };
    if bytes.len() > MAX_BYTES {
        return false;
    }
    enc.decode_without_bom_handling_and_without_replacement(bytes)
        .is_some()
}

/// List supported encodings.
pub fn list_encodings() -> Vec<EncodingInfo> {
    SUPPORTED
        .iter()
        .map(|(name, aliases)| {
            let has_bom = matches!(
                normalize_label(name).as_str(),
                "utf-8" | "utf-8-sig" | "utf-16-le" | "utf-16-be" | "utf-16"
            );
            EncodingInfo {
                name: name.to_string(),
                aliases: aliases.iter().map(|s| s.to_string()).collect(),
                has_bom,
            }
        })
        .collect()
}

/// Look up encoding metadata by label or alias.
pub fn lookup_encoding(label: &str) -> Option<EncodingInfo> {
    let norm = normalize_label(label);
    for (name, aliases) in SUPPORTED {
        if norm == normalize_label(name) || aliases.iter().any(|a| norm == normalize_label(a)) {
            return list_encodings().into_iter().find(|i| i.name == *name);
        }
    }
    resolve_encoding(label).map(|enc| EncodingInfo {
        name: canonical_name(enc),
        aliases: vec![],
        has_bom: false,
    })
}

/// Unicode normalization (NFC, NFD, NFKC, NFKD).
pub fn normalize(text: &str, form: &str) -> Result<String, EncodeError> {
    if text.len() > MAX_BYTES {
        return Err(EncodeError::TooLarge(text.len()));
    }
    let out = match form.to_ascii_uppercase().as_str() {
        "NFC" => text.nfc().collect::<String>(),
        "NFD" => text.nfd().collect::<String>(),
        "NFKC" => text.nfkc().collect::<String>(),
        "NFKD" => text.nfkd().collect::<String>(),
        _ => {
            return Err(EncodeError::InvalidInput(format!(
                "unknown normalization form '{form}' (use NFC, NFD, NFKC, NFKD)"
            )));
        }
    };
    Ok(out)
}

/// Estimate decoded UTF-8 length upper bound without allocating the string.
pub fn decoded_len_upper_bound(bytes: &[u8], encoding: &str) -> Result<usize, EncodeError> {
    let enc = resolve_encoding(encoding)
        .ok_or_else(|| EncodeError::UnknownEncoding(encoding.to_string()))?;
    Ok(enc
        .decode_without_bom_handling_and_without_replacement(bytes)
        .map(|cow| cow.len())
        .unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_roundtrip() {
        let s = "Hello 世界";
        let bytes = encode(s, "utf-8", false).unwrap();
        assert_eq!(
            decode(&bytes, Some("utf-8"), DecodeErrorMode::Strict).unwrap(),
            s
        );
    }

    #[test]
    fn utf8_bom_strip() {
        let bytes = [0xEF, 0xBB, 0xBF, b'h', b'i'];
        let (rest, enc) = strip_bom(&bytes);
        assert_eq!(enc.as_deref(), Some("utf-8"));
        assert_eq!(rest, b"hi");
    }

    #[test]
    fn shift_jis_roundtrip() {
        let text = "日本語";
        let bytes = encode(text, "shift_jis", false).unwrap();
        let out = decode(&bytes, Some("shift_jis"), DecodeErrorMode::Strict).unwrap();
        assert_eq!(out, text);
    }

    #[test]
    fn gbk_roundtrip() {
        let text = "中文";
        let bytes = encode(text, "gbk", false).unwrap();
        assert_eq!(
            decode(&bytes, Some("gbk"), DecodeErrorMode::Strict).unwrap(),
            text
        );
    }

    #[test]
    fn utf16_le_bom() {
        let bytes = encode("A", "utf-16-le", true).unwrap();
        assert!(bytes.starts_with(&[0xFF, 0xFE]));
        assert_eq!(bytes.len(), 4);
        assert_eq!(
            decode(&bytes, Some("utf-16-le"), DecodeErrorMode::Strict).unwrap(),
            "A"
        );
    }

    #[test]
    fn detect_utf8() {
        let r = detect(b"plain ascii");
        assert_eq!(r.encoding, "utf-8");
        assert!(r.confidence >= 1.0);
    }

    #[test]
    fn strict_errors_on_bad_utf8() {
        let bad = &[0xFF, 0xFE, 0x01];
        assert!(decode(bad, Some("utf-8"), DecodeErrorMode::Strict).is_err());
        assert!(decode(bad, Some("utf-8"), DecodeErrorMode::Replace).is_ok());
    }

    #[test]
    fn latin1_roundtrip() {
        let bytes = vec![0xE9]; // é in latin-1
        let s = decode(&bytes, Some("latin-1"), DecodeErrorMode::Strict).unwrap();
        assert_eq!(s, "é");
    }

    #[test]
    fn normalize_nfc() {
        let s = "e\u{0301}";
        assert_eq!(normalize(s, "NFC").unwrap(), "é");
    }
}
