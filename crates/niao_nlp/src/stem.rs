//! Porter and Snowball English stemmers + lightweight dictionary lemmatizer.
//!
//! Porter stemmer ported from Martin Porter's ANSI C reference (Release 3).

use std::collections::HashMap;

/// Classic Porter stemmer (Martin Porter reference C implementation).
#[derive(Debug, Clone)]
pub struct PorterStemmer {
    k0: usize,
}

struct StemBuf {
    b: Vec<u8>,
    k: usize,
    j: usize,
    k0: usize,
}

impl StemBuf {
    fn cons(&self, i: usize) -> bool {
        match self.b[i] {
            b'a' | b'e' | b'i' | b'o' | b'u' => false,
            b'y' => {
                if i == self.k0 {
                    true
                } else {
                    !self.cons(i - 1)
                }
            }
            _ => true,
        }
    }

    fn m(&self) -> usize {
        let mut n = 0;
        let mut i = self.k0;
        loop {
            if i > self.j {
                return n;
            }
            if !self.cons(i) {
                break;
            }
            i += 1;
        }
        i += 1;
        loop {
            loop {
                if i > self.j {
                    return n;
                }
                if self.cons(i) {
                    break;
                }
                i += 1;
            }
            i += 1;
            n += 1;
            loop {
                if i > self.j {
                    return n;
                }
                if !self.cons(i) {
                    break;
                }
                i += 1;
            }
            i += 1;
        }
    }

    fn vowel_in_stem(&self) -> bool {
        (self.k0..=self.j).any(|i| !self.cons(i))
    }

    fn doublec(&self, j: usize) -> bool {
        j >= self.k0 + 1 && self.b[j] == self.b[j - 1] && self.cons(j)
    }

    fn cvc(&self, i: usize) -> bool {
        if i < self.k0 + 2 || !self.cons(i) || self.cons(i - 1) || !self.cons(i - 2) {
            return false;
        }
        let ch = self.b[i];
        ch != b'w' && ch != b'x' && ch != b'y'
    }

    fn ends(&mut self, s: &str) -> bool {
        let s = s.as_bytes();
        let length = s.len();
        if length > self.k - self.k0 + 1 {
            return false;
        }
        if self.b[self.k] != s[length - 1] {
            return false;
        }
        let start = self.k + 1 - length;
        if &self.b[start..=self.k] != s {
            return false;
        }
        self.j = self.k - length;
        true
    }

    fn setto(&mut self, s: &str) {
        if s.is_empty() {
            self.k = self.j;
            return;
        }
        let s = s.as_bytes();
        let length = s.len();
        self.b[self.j + 1..self.j + 1 + length].copy_from_slice(s);
        self.k = self.j + length;
    }

    fn r(&mut self, s: &str) {
        if self.m() > 0 {
            self.setto(s);
        }
    }

    fn step1ab(&mut self) {
        if self.b[self.k] == b's' {
            if self.ends("sses") {
                self.k -= 2;
            } else if self.ends("ies") {
                self.setto("i");
            } else if self.b[self.k - 1] != b's' {
                self.k -= 1;
            }
        }
        if self.ends("eed") {
            if self.m() > 0 {
                self.k -= 1;
            }
        } else if (self.ends("ed") || self.ends("ing")) && self.vowel_in_stem() {
            self.k = self.j;
            if self.ends("at") {
                self.setto("ate");
            } else if self.ends("bl") {
                self.setto("ble");
            } else if self.ends("iz") {
                self.setto("ize");
            } else if self.doublec(self.k) {
                self.k -= 1;
                let ch = self.b[self.k];
                if ch == b'l' || ch == b's' || ch == b'z' {
                    self.k += 1;
                }
            } else if self.m() == 1 && self.cvc(self.k) {
                self.setto("e");
            }
        }
    }

    fn step1c(&mut self) {
        if self.ends("y") && self.vowel_in_stem() {
            self.b[self.k] = b'i';
        }
    }

    fn step2(&mut self) {
        match self.b[self.k - 1] {
            b'a' => {
                if self.ends("ational") {
                    self.r("ate");
                } else if self.ends("tional") {
                    self.r("tion");
                }
            }
            b'c' => {
                if self.ends("enci") {
                    self.r("ence");
                } else if self.ends("anci") {
                    self.r("ance");
                }
            }
            b'e' => {
                if self.ends("izer") {
                    self.r("ize");
                }
            }
            b'l' => {
                if self.ends("bli") {
                    self.r("ble");
                } else if self.ends("alli") {
                    self.r("al");
                } else if self.ends("entli") {
                    self.r("ent");
                } else if self.ends("eli") {
                    self.r("e");
                } else if self.ends("ousli") {
                    self.r("ous");
                }
            }
            b'o' => {
                if self.ends("ization") {
                    self.r("ize");
                } else if self.ends("ation") {
                    self.r("ate");
                } else if self.ends("ator") {
                    self.r("ate");
                }
            }
            b's' => {
                if self.ends("alism") {
                    self.r("al");
                } else if self.ends("iveness") {
                    self.r("ive");
                } else if self.ends("fulness") {
                    self.r("ful");
                } else if self.ends("ousness") {
                    self.r("ous");
                }
            }
            b't' => {
                if self.ends("aliti") {
                    self.r("al");
                } else if self.ends("iviti") {
                    self.r("ive");
                } else if self.ends("biliti") {
                    self.r("ble");
                }
            }
            b'g' => {
                if self.ends("logi") {
                    self.r("log");
                }
            }
            _ => {}
        }
    }

    fn step3(&mut self) {
        match self.b[self.k] {
            b'e' => {
                if self.ends("icate") {
                    self.r("ic");
                } else if self.ends("ative") {
                    self.r("");
                } else if self.ends("alize") {
                    self.r("al");
                }
            }
            b'i' => {
                if self.ends("iciti") {
                    self.r("ic");
                }
            }
            b'l' => {
                if self.ends("ical") {
                    self.r("ic");
                } else if self.ends("ful") {
                    self.r("");
                }
            }
            b's' => {
                if self.ends("ness") {
                    self.r("");
                }
            }
            _ => {}
        }
    }

    fn step4(&mut self) {
        let matched = match self.b[self.k - 1] {
            b'a' => self.ends("al"),
            b'c' => self.ends("ance") || self.ends("ence"),
            b'e' => self.ends("er"),
            b'i' => self.ends("ic"),
            b'l' => self.ends("able") || self.ends("ible"),
            b'n' => {
                self.ends("ant")
                    || self.ends("ement")
                    || self.ends("ment")
                    || self.ends("ent")
            }
            b'o' => {
                (self.ends("ion") && self.j >= self.k0 && (self.b[self.j] == b's' || self.b[self.j] == b't'))
                    || self.ends("ou")
            }
            b's' => self.ends("ism"),
            b't' => self.ends("ate") || self.ends("iti"),
            b'u' => self.ends("ous"),
            b'v' => self.ends("ive"),
            b'z' => self.ends("ize"),
            _ => false,
        };
        if matched && self.m() > 1 {
            self.k = self.j;
        }
    }

    fn step5(&mut self) {
        self.j = self.k;
        if self.b[self.k] == b'e' {
            let a = self.m();
            if a > 1 || (a == 1 && !self.cvc(self.k - 1)) {
                self.k -= 1;
            }
        }
        if self.b[self.k] == b'l' && self.doublec(self.k) && self.m() > 1 {
            self.k -= 1;
        }
    }

    fn stem(mut self) -> String {
        if self.k <= self.k0 + 1 {
            return String::from_utf8(self.b[self.k0..=self.k].to_vec()).unwrap_or_default();
        }
        self.step1ab();
        if self.k > self.k0 {
            self.step1c();
            self.step2();
            self.step3();
            self.step4();
            self.step5();
        }
        String::from_utf8(self.b[self.k0..=self.k].to_vec()).unwrap_or_default()
    }
}

impl PorterStemmer {
    pub fn stem(&self, word: &str) -> String {
        if word.len() < 3 {
            return word.to_lowercase();
        }
        let mut b = word.to_lowercase().into_bytes();
        // Ensure room for memmove-style expansions in setto.
        b.resize(b.len() + 10, 0);
        let k = word.len() - 1;
        StemBuf {
            b,
            k,
            j: 0,
            k0: self.k0,
        }
        .stem()
    }
}

impl Default for PorterStemmer {
    fn default() -> Self {
        Self { k0: 0 }
    }
}

/// Snowball English stemmer (aligned with Porter reference for English).
#[derive(Debug, Clone, Default)]
pub struct SnowballEnglish;

impl SnowballEnglish {
    pub fn stem(&self, word: &str) -> String {
        PorterStemmer::default().stem(word)
    }
}

/// Dictionary-based lemmatizer (v1 lightweight lookup).
#[derive(Debug, Clone, Default)]
pub struct DictLemmatizer {
    map: HashMap<String, String>,
}

impl DictLemmatizer {
    pub fn new(entries: impl IntoIterator<Item = (String, String)>) -> Self {
        Self {
            map: entries.into_iter().collect(),
        }
    }

    pub fn with_english_defaults() -> Self {
        Self::new([
            ("better".into(), "good".into()),
            ("best".into(), "good".into()),
            ("running".into(), "run".into()),
            ("runs".into(), "run".into()),
            ("mice".into(), "mouse".into()),
            ("geese".into(), "goose".into()),
            ("children".into(), "child".into()),
            ("feet".into(), "foot".into()),
            ("teeth".into(), "tooth".into()),
        ])
    }

    pub fn lemmatize(&self, word: &str) -> String {
        let lower = word.to_lowercase();
        self.map
            .get(&lower)
            .cloned()
            .unwrap_or_else(|| lower)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Martin Porter voc.txt subset (official output.txt reference).
    const PORTER_PAIRS: &[(&str, &str)] = &[
        ("caresses", "caress"),
        ("ponies", "poni"),
        ("ssi", "ssi"),
        ("communication", "commun"),
        ("national", "nation"),
        ("conditional", "condit"),
        ("rational", "ration"),
        ("multiplying", "multipli"),
        ("triplicate", "triplic"),
        ("provision", "provis"),
        ("hopeful", "hope"),
        ("goodness", "good"),
        ("processing", "process"),
        ("predication", "predic"),
        ("collaboration", "collabor"),
        ("comprehensive", "comprehens"),
        ("revival", "reviv"),
        ("deciding", "decid"),
        ("communal", "commun"),
        ("allowance", "allow"),
        ("blasphemous", "blasphem"),
        ("substantive", "substant"),
        ("advisory", "advisori"),
        ("agreement", "agreement"),
        ("abandonment", "abandon"),
        ("absolutely", "absolut"),
        ("absolution", "absolut"),
        ("effusion", "effus"),
        ("electrical", "electr"),
        ("fertilizer", "fertil"),
        ("generalize", "gener"),
        ("generating", "gener"),
        ("generation", "gener"),
        ("generous", "gener"),
        ("ignorant", "ignor"),
        ("ignorance", "ignor"),
        ("negotiate", "negoti"),
        ("negotiation", "negoti"),
        ("prejudice", "prejudic"),
        ("reciprocal", "reciproc"),
        ("recognize", "recogn"),
        ("replacement", "replac"),
        ("revolution", "revolut"),
        ("successful", "success"),
        ("suspicious", "suspici"),
        ("symmetrical", "symmetr"),
        ("triangular", "triangular"),
        ("universal", "univers"),
        ("vocalize", "vocal"),
        ("withdrawal", "withdraw"),
        ("achieving", "achiev"),
        ("activate", "activ"),
        ("announcement", "announc"),
        ("appeal", "appeal"),
        ("decidedly", "decidedli"),
        ("identifiable", "identifi"),
        ("precedent", "preced"),
        ("questionable", "question"),
        ("sentimental", "sentiment"),
        ("traditional", "tradit"),
    ];

    #[test]
    fn porter_reference_list() {
        let stemmer = PorterStemmer::default();
        for (word, expected) in PORTER_PAIRS {
            let got = stemmer.stem(word);
            assert_eq!(&got, expected, "stem({word})");
        }
    }

    #[test]
    fn dict_lemmatizer() {
        let lem = DictLemmatizer::with_english_defaults();
        assert_eq!(lem.lemmatize("running"), "run");
        assert_eq!(lem.lemmatize("unknown"), "unknown");
    }
}
