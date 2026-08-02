use unicode_normalization::UnicodeNormalization;

/// Unicode normalization form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NormalizationForm {
    Nfc,
    Nfd,
    Nfkc,
    Nfkd,
}

impl NormalizationForm {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            "NFC" => Some(Self::Nfc),
            "NFD" => Some(Self::Nfd),
            "NFKC" => Some(Self::Nfkc),
            "NFKD" => Some(Self::Nfkd),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Nfc => "NFC",
            Self::Nfd => "NFD",
            Self::Nfkc => "NFKC",
            Self::Nfkd => "NFKD",
        }
    }
}

#[inline]
pub fn normalize(s: &str, form: NormalizationForm) -> String {
    match form {
        NormalizationForm::Nfc => s.nfc().collect(),
        NormalizationForm::Nfd => s.nfd().collect(),
        NormalizationForm::Nfkc => s.nfkc().collect(),
        NormalizationForm::Nfkd => s.nfkd().collect(),
    }
}

#[inline]
pub fn nfc(s: &str) -> String {
    normalize(s, NormalizationForm::Nfc)
}

#[inline]
pub fn nfd(s: &str) -> String {
    normalize(s, NormalizationForm::Nfd)
}

#[inline]
pub fn nfkc(s: &str) -> String {
    normalize(s, NormalizationForm::Nfkc)
}

#[inline]
pub fn nfkd(s: &str) -> String {
    normalize(s, NormalizationForm::Nfkd)
}

/// True when `s` is already in the requested normalization form.
pub fn is_normalized(s: &str, form: NormalizationForm) -> bool {
    normalize(s, form) == s
}
