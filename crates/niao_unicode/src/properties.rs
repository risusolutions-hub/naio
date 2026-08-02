use icu_locale_core::subtags::Script as LocaleScript;
use icu_properties::props::{
    BidiClass, BidiMirrored, CanonicalCombiningClass, EastAsianWidth, GeneralCategory, Script,
};
use icu_properties::{CodePointMapData, CodePointSetData};
use unicode_general_category::{get_general_category, GeneralCategory as Ugc};
use unicode_normalization::char::decompose_canonical;

#[inline]
fn gc_of(ch: char) -> GeneralCategory {
    CodePointMapData::<GeneralCategory>::new().get(ch)
}

#[inline]
fn script_of(ch: char) -> Script {
    CodePointMapData::<Script>::new().get(ch)
}

#[inline]
fn bidi_of(ch: char) -> BidiClass {
    CodePointMapData::<BidiClass>::new().get(ch)
}

#[inline]
fn ccc_of(ch: char) -> CanonicalCombiningClass {
    CodePointMapData::<CanonicalCombiningClass>::new().get(ch)
}

#[inline]
fn eaw_of(ch: char) -> EastAsianWidth {
    CodePointMapData::<EastAsianWidth>::new().get(ch)
}

fn category_short(gc: GeneralCategory) -> &'static str {
    match gc {
        GeneralCategory::UppercaseLetter => "Lu",
        GeneralCategory::LowercaseLetter => "Ll",
        GeneralCategory::TitlecaseLetter => "Lt",
        GeneralCategory::ModifierLetter => "Lm",
        GeneralCategory::OtherLetter => "Lo",
        GeneralCategory::NonspacingMark => "Mn",
        GeneralCategory::SpacingMark => "Mc",
        GeneralCategory::EnclosingMark => "Me",
        GeneralCategory::DecimalNumber => "Nd",
        GeneralCategory::LetterNumber => "Nl",
        GeneralCategory::OtherNumber => "No",
        GeneralCategory::SpaceSeparator => "Zs",
        GeneralCategory::LineSeparator => "Zl",
        GeneralCategory::ParagraphSeparator => "Zp",
        GeneralCategory::Control => "Cc",
        GeneralCategory::Format => "Cf",
        GeneralCategory::PrivateUse => "Co",
        GeneralCategory::Surrogate => "Cs",
        GeneralCategory::DashPunctuation => "Pd",
        GeneralCategory::OpenPunctuation => "Ps",
        GeneralCategory::ClosePunctuation => "Pe",
        GeneralCategory::InitialPunctuation => "Pi",
        GeneralCategory::FinalPunctuation => "Pf",
        GeneralCategory::ConnectorPunctuation => "Pc",
        GeneralCategory::OtherPunctuation => "Po",
        GeneralCategory::MathSymbol => "Sm",
        GeneralCategory::CurrencySymbol => "Sc",
        GeneralCategory::ModifierSymbol => "Sk",
        GeneralCategory::OtherSymbol => "So",
        GeneralCategory::Unassigned => "Cn",
    }
}

fn bidi_short(b: BidiClass) -> &'static str {
    match b {
        BidiClass::LeftToRight => "L",
        BidiClass::RightToLeft => "R",
        BidiClass::EuropeanNumber => "EN",
        BidiClass::EuropeanSeparator => "ES",
        BidiClass::EuropeanTerminator => "ET",
        BidiClass::ArabicNumber => "AN",
        BidiClass::CommonSeparator => "CS",
        BidiClass::ParagraphSeparator => "B",
        BidiClass::SegmentSeparator => "S",
        BidiClass::WhiteSpace => "WS",
        BidiClass::OtherNeutral => "ON",
        BidiClass::LeftToRightEmbedding => "LRE",
        BidiClass::LeftToRightOverride => "LRO",
        BidiClass::ArabicLetter => "AL",
        BidiClass::RightToLeftEmbedding => "RLE",
        BidiClass::RightToLeftOverride => "RLO",
        BidiClass::PopDirectionalFormat => "PDF",
        BidiClass::NonspacingMark => "NSM",
        BidiClass::BoundaryNeutral => "BN",
        BidiClass::FirstStrongIsolate => "FSI",
        BidiClass::LeftToRightIsolate => "LRI",
        BidiClass::RightToLeftIsolate => "RLI",
        BidiClass::PopDirectionalIsolate => "PDI",
        _ => "ON",
    }
}

fn eaw_short(w: EastAsianWidth) -> &'static str {
    match w {
        EastAsianWidth::Neutral => "N",
        EastAsianWidth::Ambiguous => "A",
        EastAsianWidth::Halfwidth => "H",
        EastAsianWidth::Fullwidth => "F",
        EastAsianWidth::Narrow => "Na",
        EastAsianWidth::Wide => "W",
        _ => "N",
    }
}

/// General category for a single scalar (e.g. `"Lu"`).
pub fn category(ch: char) -> Option<String> {
    Some(category_short(gc_of(ch)).to_string())
}

/// Per-scalar categories for every char in `s`.
pub fn categories(s: &str) -> Vec<String> {
    s.chars().filter_map(|c| category(c)).collect()
}

/// Unicode character name (U+0000..U+10FFFF); returns `None` for unnamed codepoints.
pub fn name(ch: char) -> Option<String> {
    unicode_names2::name(ch).map(|n| n.to_string())
}

/// Reverse name lookup (exact, case-insensitive); `None` when not found.
pub fn lookup(search: &str) -> Option<char> {
    unicode_names2::character(search)
}

/// ISO 15924 script abbreviation (e.g. `"Latn"`).
pub fn script(ch: char) -> Option<String> {
    let sc = script_of(ch);
    if sc == Script::Unknown {
        return None;
    }
    let loc: LocaleScript = sc.into();
    Some(loc.to_string())
}

/// Bidirectional class short name (e.g. `"L"`).
pub fn bidi(ch: char) -> String {
    bidi_short(bidi_of(ch)).to_string()
}

/// Canonical combining class (0..=254).
pub fn combining(ch: char) -> u8 {
    ccc_of(ch).to_icu4c_value()
}

/// East Asian width property (`"N"`, `"W"`, `"Na"`, …).
pub fn east_asian_width(ch: char) -> String {
    eaw_short(eaw_of(ch)).to_string()
}

/// Decimal digit value 0..9, or `None`.
pub fn decimal(ch: char) -> Option<i64> {
    ch.to_digit(10).map(|d| d as i64)
}

/// Digit value for numeric chars, or `None`.
pub fn digit(ch: char, base: u32) -> Option<i64> {
    ch.to_digit(base).map(|d| d as i64)
}

/// Numeric value as float (Roman numerals, fractions, superscripts, …).
pub fn numeric(ch: char) -> Option<f64> {
    if let Some(d) = ch.to_digit(10) {
        return Some(d as f64);
    }
    match ch {
        '⅐' => Some(1.0 / 7.0),
        '⅑' => Some(1.0 / 9.0),
        '⅒' => Some(1.0 / 10.0),
        '⅓' => Some(1.0 / 3.0),
        '⅔' => Some(2.0 / 3.0),
        '⅕' => Some(1.0 / 5.0),
        '⅖' => Some(2.0 / 5.0),
        '⅗' => Some(3.0 / 5.0),
        '⅘' => Some(4.0 / 5.0),
        '⅙' => Some(1.0 / 6.0),
        '⅚' => Some(5.0 / 6.0),
        '⅛' => Some(1.0 / 8.0),
        '⅜' => Some(3.0 / 8.0),
        '⅝' => Some(5.0 / 8.0),
        '⅞' => Some(7.0 / 8.0),
        '⅟' => Some(1.0 / 160.0),
        '〇' | '零' => Some(0.0),
        '一' | '壹' => Some(1.0),
        '二' | '貳' | '两' => Some(2.0),
        '三' | '叁' => Some(3.0),
        '四' | '肆' => Some(4.0),
        '五' | '伍' => Some(5.0),
        '六' | '陸' => Some(6.0),
        '七' | '柒' => Some(7.0),
        '八' | '捌' => Some(8.0),
        '九' | '玖' => Some(9.0),
        '十' | '拾' => Some(10.0),
        '百' | '佰' => Some(100.0),
        '千' | '仟' => Some(1000.0),
        '万' => Some(10_000.0),
        '億' => Some(100_000_000.0),
        '兆' => Some(1_000_000_000_000.0),
        'Ⅰ' => Some(1.0),
        'Ⅱ' => Some(2.0),
        'Ⅲ' => Some(3.0),
        'Ⅳ' => Some(4.0),
        'Ⅴ' => Some(5.0),
        'Ⅵ' => Some(6.0),
        'Ⅶ' => Some(7.0),
        'Ⅷ' => Some(8.0),
        'Ⅸ' => Some(9.0),
        'Ⅹ' => Some(10.0),
        'Ⅺ' => Some(11.0),
        'Ⅻ' => Some(12.0),
        'ⅰ' => Some(1.0),
        'ⅱ' => Some(2.0),
        'ⅲ' => Some(3.0),
        'ⅳ' => Some(4.0),
        'ⅴ' => Some(5.0),
        'ⅵ' => Some(6.0),
        'ⅶ' => Some(7.0),
        'ⅷ' => Some(8.0),
        'ⅸ' => Some(9.0),
        'ⅹ' => Some(10.0),
        'ⅺ' => Some(11.0),
        'ⅻ' => Some(12.0),
        '½' => Some(1.0 / 2.0),
        '¼' => Some(1.0 / 4.0),
        '¾' => Some(3.0 / 4.0),
        _ => None,
    }
}

/// True when the character has a mirrored glyph in bidirectional text.
pub fn mirrored(ch: char) -> bool {
    CodePointSetData::new::<BidiMirrored>().contains(ch)
}

/// Canonical decomposition as uppercase hex codepoints separated by spaces.
pub fn decomposition(ch: char) -> String {
    let mut parts = Vec::new();
    decompose_canonical(ch, |c| parts.push(c));
    if parts.len() <= 1 {
        return String::new();
    }
    parts
        .iter()
        .map(|c| format!("{:04X}", *c as u32))
        .collect::<Vec<_>>()
        .join(" ")
}

#[inline]
pub fn is_alphabetic(ch: char) -> bool {
    matches!(
        get_general_category(ch),
        Ugc::UppercaseLetter
            | Ugc::LowercaseLetter
            | Ugc::TitlecaseLetter
            | Ugc::ModifierLetter
            | Ugc::OtherLetter
    )
}

#[inline]
pub fn is_numeric(ch: char) -> bool {
    matches!(
        get_general_category(ch),
        Ugc::DecimalNumber | Ugc::LetterNumber | Ugc::OtherNumber
    )
}

#[inline]
pub fn is_whitespace(ch: char) -> bool {
    ch.is_whitespace()
}

#[inline]
pub fn is_control(ch: char) -> bool {
    get_general_category(ch) == Ugc::Control
}
