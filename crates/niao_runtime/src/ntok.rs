//! Native ntok standard library — byte-level BPE tokenizer (GPT-2 style):
//! encode/decode/count, per-word cache, approximate counting, chunking, and
//! context-budget fitting.
//!
//! Import with `import "ntok"` (or `import "std/ntok"`).

use crate::{error_value, NativeFn, NiaoResult, RuntimeError, StringArray, Value, ValueRef};
use niao_ast::Span;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::rc::Rc;

// Wired in codes.rs by central integration.
const E2770_NTOK_ARITY: u32 = 2770;
const E2771_NTOK_ERROR: u32 = 2771;
const E2772_NTOK_TYPE: u32 = 2772;
const E2773_NTOK_INVALID_HANDLE: u32 = 2773;

// ---------------------------------------------------------------------------
// Byte-level BPE tokenizer
// ---------------------------------------------------------------------------

struct ByteEncoder {
    byte_to_unicode: [char; 256],
    unicode_to_byte: HashMap<char, u8>,
}

impl ByteEncoder {
    fn gpt2() -> Self {
        let mut bs: Vec<u32> = (b'!'..=b'~')
            .chain(b'\xA1'..=b'\xAC')
            .chain(b'\xAE'..=b'\xFF')
            .map(|b| b as u32)
            .collect();
        let mut cs = bs.clone();
        let mut n = 0u32;
        for b in 0u32..256 {
            if !bs.contains(&b) {
                bs.push(b);
                cs.push(256 + n);
                n += 1;
            }
        }
        let mut byte_to_unicode = [char::REPLACEMENT_CHARACTER; 256];
        let mut unicode_to_byte = HashMap::new();
        for (byte, &code) in bs.iter().zip(cs.iter()) {
            let ch = char::from_u32(code).unwrap_or(char::REPLACEMENT_CHARACTER);
            byte_to_unicode[*byte as usize] = ch;
            unicode_to_byte.insert(ch, *byte as u8);
        }
        Self {
            byte_to_unicode,
            unicode_to_byte,
        }
    }

    fn encode(&self, text: &str) -> String {
        text.bytes()
            .map(|b| self.byte_to_unicode[b as usize])
            .collect()
    }

    fn decode(&self, text: &str) -> Result<String, String> {
        let mut bytes = Vec::new();
        for ch in text.chars() {
            let b = self
                .unicode_to_byte
                .get(&ch)
                .ok_or_else(|| format!("invalid byte token character '{ch}'"))?;
            bytes.push(*b);
        }
        String::from_utf8(bytes).map_err(|e| format!("invalid utf-8 after decode: {e}"))
    }
}

struct BpeTokenizer {
    encoder: ByteEncoder,
    bpe_ranks: HashMap<(String, String), usize>,
    vocab: HashMap<String, i64>,
    decoder: HashMap<i64, String>,
    cache: RefCell<HashMap<String, Vec<String>>>,
}

impl BpeTokenizer {
    fn builtin() -> Self {
        let encoder = ByteEncoder::gpt2();
        let mut vocab = HashMap::new();
        let mut decoder = HashMap::new();
        let mut id = 0i64;
        for b in 0u8..=255 {
            let token = encoder.byte_to_unicode[b as usize].to_string();
            vocab.insert(token.clone(), id);
            decoder.insert(id, token);
            id += 1;
        }
        let merges = builtin_merges();
        let mut bpe_ranks = HashMap::new();
        for (rank, pair) in merges.iter().enumerate() {
            bpe_ranks.insert(pair.clone(), rank);
            let merged = format!("{}{}", pair.0, pair.1);
            if !vocab.contains_key(&merged) {
                vocab.insert(merged.clone(), id);
                decoder.insert(id, merged);
                id += 1;
            }
        }
        Self {
            encoder,
            bpe_ranks,
            vocab,
            decoder,
            cache: RefCell::new(HashMap::new()),
        }
    }

    fn from_files(vocab_path: &str, merges_path: Option<&str>) -> Result<Self, String> {
        let vocab_text = fs::read_to_string(vocab_path)
            .map_err(|e| format!("failed to read vocab file '{vocab_path}': {e}"))?;
        let vocab_json: HashMap<String, i64> = serde_json::from_str(&vocab_text)
            .map_err(|e| format!("invalid vocab.json at '{vocab_path}': {e}"))?;
        let mut decoder = HashMap::new();
        for (tok, id) in &vocab_json {
            decoder.insert(*id, tok.clone());
        }
        let encoder = ByteEncoder::gpt2();
        let mut bpe_ranks = HashMap::new();
        if let Some(path) = merges_path {
            let merges_text = fs::read_to_string(path)
                .map_err(|e| format!("failed to read merges file '{path}': {e}"))?;
            for (rank, line) in merges_text.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let mut parts = line.split_whitespace();
                let a = parts
                    .next()
                    .ok_or_else(|| format!("invalid merge line: '{line}'"))?
                    .to_string();
                let b = parts
                    .next()
                    .ok_or_else(|| format!("invalid merge line: '{line}'"))?
                    .to_string();
                if parts.next().is_some() {
                    return Err(format!("invalid merge line: '{line}'"));
                }
                bpe_ranks.insert((a, b), rank);
            }
        }
        Ok(Self {
            encoder,
            bpe_ranks,
            vocab: vocab_json,
            decoder,
            cache: RefCell::new(HashMap::new()),
        })
    }

    fn bpe(&self, token: &str) -> Vec<String> {
        if let Some(cached) = self.cache.borrow().get(token) {
            return cached.clone();
        }
        if token.len() <= 1 {
            return vec![token.to_string()];
        }
        let mut word: Vec<String> = token.chars().map(|c| c.to_string()).collect();
        let pairs = |w: &[String]| -> Vec<(String, String)> {
            w.windows(2).map(|p| (p[0].clone(), p[1].clone())).collect()
        };
        let mut current_pairs = pairs(&word);
        while !current_pairs.is_empty() {
            let mut best: Option<((String, String), usize)> = None;
            for pair in &current_pairs {
                if let Some(rank) = self.bpe_ranks.get(pair) {
                    if best.as_ref().map(|(_, r)| *rank < *r).unwrap_or(true) {
                        best = Some((pair.clone(), *rank));
                    }
                }
            }
            let Some(((first, second), _)) = best else {
                break;
            };
            let mut i = 0;
            let mut new_word = Vec::new();
            while i < word.len() {
                if i < word.len() - 1 && word[i] == first && word[i + 1] == second {
                    new_word.push(format!("{first}{second}"));
                    i += 2;
                } else {
                    new_word.push(word[i].clone());
                    i += 1;
                }
            }
            word = new_word;
            if word.len() == 1 {
                break;
            }
            current_pairs = pairs(&word);
        }
        self.cache
            .borrow_mut()
            .insert(token.to_string(), word.clone());
        word
    }

    fn tokenize_word(&self, piece: &str) -> Vec<i64> {
        let encoded = self.encoder.encode(piece);
        self.bpe(&encoded)
            .into_iter()
            .filter_map(|tok| self.vocab.get(&tok).copied())
            .collect()
    }

    fn encode_text(&self, text: &str) -> Vec<i64> {
        let mut ids = Vec::new();
        for piece in pretokenize(text) {
            ids.extend(self.tokenize_word(&piece));
        }
        ids
    }

    fn decode_ids(&self, ids: &[i64]) -> Result<String, String> {
        let mut pieces = String::new();
        for &id in ids {
            let piece = self
                .decoder
                .get(&id)
                .ok_or_else(|| format!("unknown token id {id}"))?;
            pieces.push_str(piece);
        }
        self.encoder.decode(&pieces)
    }
}

fn is_letter(c: char) -> bool {
    c.is_alphabetic()
}

fn is_number(c: char) -> bool {
    c.is_ascii_digit()
}

fn is_space(c: char) -> bool {
    c.is_whitespace()
}

/// GPT-2-ish pretokenization without external regex (niao_regex has no \\p classes).
fn pretokenize(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if is_space(c) {
            let start = i;
            i += 1;
            while i < chars.len() && is_space(chars[i]) {
                i += 1;
            }
            out.push(chars[start..i].iter().collect());
            continue;
        }
        if c == '\'' && i + 1 < chars.len() {
            let mut matched = false;
            for contr in ["'s", "'t", "'re", "'ve", "'m", "'ll", "'d"] {
                if text[i..].starts_with(contr) {
                    out.push(contr.to_string());
                    i += contr.chars().count();
                    matched = true;
                    break;
                }
            }
            if matched {
                continue;
            }
        }
        if is_letter(c) {
            let mut start = i;
            if start > 0 && is_space(chars[start - 1]) {
                start -= 1;
            }
            i += 1;
            while i < chars.len() && is_letter(chars[i]) {
                i += 1;
            }
            out.push(chars[start..i].iter().collect());
            continue;
        }
        if is_number(c) {
            let mut start = i;
            if start > 0 && is_space(chars[start - 1]) {
                start -= 1;
            }
            i += 1;
            while i < chars.len() && is_number(chars[i]) {
                i += 1;
            }
            out.push(chars[start..i].iter().collect());
            continue;
        }
        let mut start = i;
        if start > 0 && is_space(chars[start - 1]) {
            start -= 1;
        }
        i += 1;
        while i < chars.len()
            && !is_space(chars[i])
            && !is_letter(chars[i])
            && !is_number(chars[i])
            && chars[i] != '\''
        {
            i += 1;
        }
        out.push(chars[start..i].iter().collect());
    }
    out
}

fn builtin_merges() -> Vec<(String, String)> {
    vec![
        ("Ġ".into(), "t".into()),
        ("Ġ".into(), "a".into()),
        ("h".into(), "e".into()),
        ("i".into(), "n".into()),
        ("r".into(), "e".into()),
        ("o".into(), "n".into()),
        ("Ġ".into(), "th".into()),
        ("e".into(), "r".into()),
        ("Ġ".into(), "the".into()),
        ("Ġ".into(), "o".into()),
        ("Ġ".into(), "s".into()),
        ("a".into(), "t".into()),
        ("i".into(), "s".into()),
        ("e".into(), "n".into()),
        ("o".into(), "r".into()),
        ("e".into(), "s".into()),
        ("Ġ".into(), "w".into()),
        ("Ġ".into(), "b".into()),
        ("a".into(), "n".into()),
        ("a".into(), "l".into()),
        ("i".into(), "t".into()),
        ("i".into(), "o".into()),
        ("o".into(), "u".into()),
        ("a".into(), "r".into()),
        ("Ġ".into(), "f".into()),
        ("Ġ".into(), "c".into()),
        ("Ġ".into(), "in".into()),
        ("Ġ".into(), "p".into()),
        ("Ġ".into(), "m".into()),
        ("Ġ".into(), "d".into()),
        ("Ġ".into(), "h".into()),
        ("Ġ".into(), "l".into()),
        ("Ġ".into(), "y".into()),
        ("Ġ".into(), "g".into()),
        ("Ġ".into(), "n".into()),
        ("Ġ".into(), "re".into()),
        ("Ġ".into(), "on".into()),
        ("Ġ".into(), "an".into()),
        ("Ġ".into(), "is".into()),
        ("Ġ".into(), "it".into()),
        ("Ġ".into(), "be".into()),
        ("Ġ".into(), "to".into()),
        ("Ġ".into(), "of".into()),
        ("Ġ".into(), "and".into()),
        ("Ġ".into(), "for".into()),
        ("Ġ".into(), "with".into()),
        ("Ġ".into(), "that".into()),
        ("Ġ".into(), "this".into()),
        ("Ġ".into(), "from".into()),
        ("Ġ".into(), "are".into()),
        ("Ġ".into(), "was".into()),
        ("Ġ".into(), "as".into()),
        ("Ġ".into(), "at".into()),
        ("Ġ".into(), "by".into()),
        ("Ġ".into(), "or".into()),
        ("Ġ".into(), "not".into()),
        ("Ġ".into(), "you".into()),
        ("Ġ".into(), "your".into()),
        ("Ġ".into(), "can".into()),
        ("Ġ".into(), "will".into()),
        ("Ġ".into(), "have".into()),
        ("Ġ".into(), "has".into()),
        ("Ġ".into(), "had".into()),
        ("Ġ".into(), "but".into()),
        ("Ġ".into(), "all".into()),
        ("Ġ".into(), "one".into()),
        ("Ġ".into(), "two".into()),
        ("Ġ".into(), "out".into()),
        ("Ġ".into(), "up".into()),
        ("Ġ".into(), "if".into()),
        ("Ġ".into(), "we".into()),
        ("Ġ".into(), "they".into()),
        ("Ġ".into(), "their".into()),
        ("Ġ".into(), "there".into()),
        ("Ġ".into(), "what".into()),
        ("Ġ".into(), "when".into()),
        ("Ġ".into(), "which".into()),
        ("Ġ".into(), "who".into()),
        ("Ġ".into(), "how".into()),
        ("Ġ".into(), "about".into()),
        ("Ġ".into(), "into".into()),
        ("Ġ".into(), "over".into()),
        ("Ġ".into(), "under".into()),
        ("Ġ".into(), "after".into()),
        ("Ġ".into(), "before".into()),
        ("Ġ".into(), "between".into()),
        ("Ġ".into(), "through".into()),
        ("Ġ".into(), "during".into()),
        ("Ġ".into(), "while".into()),
        ("Ġ".into(), "because".into()),
        ("Ġ".into(), "than".into()),
        ("Ġ".into(), "then".into()),
        ("Ġ".into(), "also".into()),
        ("Ġ".into(), "just".into()),
        ("Ġ".into(), "only".into()),
        ("Ġ".into(), "more".into()),
        ("Ġ".into(), "most".into()),
        ("Ġ".into(), "some".into()),
        ("Ġ".into(), "any".into()),
        ("Ġ".into(), "each".into()),
        ("Ġ".into(), "every".into()),
        ("Ġ".into(), "other".into()),
        ("Ġ".into(), "such".into()),
        ("Ġ".into(), "very".into()),
        ("Ġ".into(), "much".into()),
        ("Ġ".into(), "many".into()),
        ("Ġ".into(), "like".into()),
        ("Ġ".into(), "make".into()),
        ("Ġ".into(), "made".into()),
        ("Ġ".into(), "use".into()),
        ("Ġ".into(), "used".into()),
        ("Ġ".into(), "using".into()),
        ("Ġ".into(), "work".into()),
        ("Ġ".into(), "works".into()),
        ("Ġ".into(), "working".into()),
        ("Ġ".into(), "help".into()),
        ("Ġ".into(), "need".into()),
        ("Ġ".into(), "want".into()),
        ("Ġ".into(), "know".into()),
        ("Ġ".into(), "think".into()),
        ("Ġ".into(), "see".into()),
        ("Ġ".into(), "look".into()),
        ("Ġ".into(), "find".into()),
        ("Ġ".into(), "give".into()),
        ("Ġ".into(), "take".into()),
        ("Ġ".into(), "come".into()),
        ("Ġ".into(), "go".into()),
        ("Ġ".into(), "get".into()),
        ("Ġ".into(), "got".into()),
        ("Ġ".into(), "put".into()),
        ("Ġ".into(), "set".into()),
        ("Ġ".into(), "run".into()),
        ("Ġ".into(), "call".into()),
        ("Ġ".into(), "read".into()),
        ("Ġ".into(), "write".into()),
        ("Ġ".into(), "open".into()),
        ("Ġ".into(), "close".into()),
        ("Ġ".into(), "start".into()),
        ("Ġ".into(), "stop".into()),
        ("Ġ".into(), "end".into()),
        ("Ġ".into(), "new".into()),
        ("Ġ".into(), "old".into()),
        ("Ġ".into(), "first".into()),
        ("Ġ".into(), "last".into()),
        ("Ġ".into(), "next".into()),
        ("Ġ".into(), "back".into()),
        ("Ġ".into(), "down".into()),
        ("Ġ".into(), "off".into()),
        ("Ġ".into(), "on".into()),
        ("Ġ".into(), "no".into()),
        ("Ġ".into(), "yes".into()),
        ("Ġ".into(), "true".into()),
        ("Ġ".into(), "false".into()),
        ("Ġ".into(), "null".into()),
        ("Ġ".into(), "data".into()),
        ("Ġ".into(), "text".into()),
        ("Ġ".into(), "code".into()),
        ("Ġ".into(), "file".into()),
        ("Ġ".into(), "line".into()),
        ("Ġ".into(), "name".into()),
        ("Ġ".into(), "type".into()),
        ("Ġ".into(), "value".into()),
        ("Ġ".into(), "values".into()),
        ("Ġ".into(), "list".into()),
        ("Ġ".into(), "array".into()),
        ("Ġ".into(), "object".into()),
        ("Ġ".into(), "string".into()),
        ("Ġ".into(), "number".into()),
        ("Ġ".into(), "model".into()),
        ("Ġ".into(), "token".into()),
        ("Ġ".into(), "tokens".into()),
        ("Ġ".into(), "context".into()),
        ("Ġ".into(), "prompt".into()),
        ("Ġ".into(), "response".into()),
        ("Ġ".into(), "message".into()),
        ("Ġ".into(), "messages".into()),
        ("Ġ".into(), "user".into()),
        ("Ġ".into(), "system".into()),
        ("Ġ".into(), "assistant".into()),
        ("Ġ".into(), "input".into()),
        ("Ġ".into(), "output".into()),
        ("Ġ".into(), "result".into()),
        ("Ġ".into(), "error".into()),
        ("Ġ".into(), "errors".into()),
        ("Ġ".into(), "test".into()),
        ("Ġ".into(), "tests".into()),
        ("Ġ".into(), "example".into()),
        ("Ġ".into(), "examples".into()),
        ("Ġ".into(), "function".into()),
        ("Ġ".into(), "functions".into()),
        ("Ġ".into(), "class".into()),
        ("Ġ".into(), "method".into()),
        ("Ġ".into(), "methods".into()),
        ("Ġ".into(), "field".into()),
        ("Ġ".into(), "fields".into()),
        ("Ġ".into(), "key".into()),
        ("Ġ".into(), "keys".into()),
        ("Ġ".into(), "map".into()),
        ("Ġ".into(), "set".into()),
        ("Ġ".into(), "count".into()),
        ("Ġ".into(), "size".into()),
        ("Ġ".into(), "length".into()),
        ("Ġ".into(), "index".into()),
        ("Ġ".into(), "range".into()),
        ("Ġ".into(), "limit".into()),
        ("Ġ".into(), "max".into()),
        ("Ġ".into(), "min".into()),
        ("Ġ".into(), "sum".into()),
        ("Ġ".into(), "avg".into()),
        ("Ġ".into(), "mean".into()),
        ("Ġ".into(), "rate".into()),
        ("Ġ".into(), "time".into()),
        ("Ġ".into(), "date".into()),
        ("Ġ".into(), "day".into()),
        ("Ġ".into(), "year".into()),
        ("Ġ".into(), "month".into()),
        ("Ġ".into(), "hour".into()),
        ("Ġ".into(), "minute".into()),
        ("Ġ".into(), "second".into()),
        ("Ġ".into(), "ms".into()),
        ("Ġ".into(), "sec".into()),
        ("Ġ".into(), "version".into()),
        ("Ġ".into(), "state".into()),
        ("Ġ".into(), "status".into()),
        ("Ġ".into(), "config".into()),
        ("Ġ".into(), "option".into()),
        ("Ġ".into(), "options".into()),
        ("Ġ".into(), "default".into()),
        ("Ġ".into(), "custom".into()),
        ("Ġ".into(), "local".into()),
        ("Ġ".into(), "global".into()),
        ("Ġ".into(), "public".into()),
        ("Ġ".into(), "private".into()),
        ("Ġ".into(), "internal".into()),
        ("Ġ".into(), "external".into()),
        ("Ġ".into(), "server".into()),
        ("Ġ".into(), "client".into()),
        ("Ġ".into(), "host".into()),
        ("Ġ".into(), "port".into()),
        ("Ġ".into(), "path".into()),
        ("Ġ".into(), "url".into()),
        ("Ġ".into(), "http".into()),
        ("Ġ".into(), "https".into()),
        ("Ġ".into(), "api".into()),
        ("Ġ".into(), "json".into()),
        ("Ġ".into(), "xml".into()),
        ("Ġ".into(), "html".into()),
        ("Ġ".into(), "sql".into()),
        ("Ġ".into(), "db".into()),
        ("Ġ".into(), "database".into()),
        ("Ġ".into(), "table".into()),
        ("Ġ".into(), "row".into()),
        ("Ġ".into(), "rows".into()),
        ("Ġ".into(), "column".into()),
        ("Ġ".into(), "columns".into()),
        ("Ġ".into(), "query".into()),
        ("Ġ".into(), "search".into()),
        ("Ġ".into(), "filter".into()),
        ("Ġ".into(), "sort".into()),
        ("Ġ".into(), "order".into()),
        ("Ġ".into(), "group".into()),
        ("Ġ".into(), "join".into()),
        ("Ġ".into(), "left".into()),
        ("Ġ".into(), "right".into()),
        ("Ġ".into(), "top".into()),
        ("Ġ".into(), "bottom".into()),
        ("Ġ".into(), "high".into()),
        ("Ġ".into(), "low".into()),
        ("Ġ".into(), "fast".into()),
        ("Ġ".into(), "slow".into()),
        ("Ġ".into(), "good".into()),
        ("Ġ".into(), "bad".into()),
        ("Ġ".into(), "best".into()),
        ("Ġ".into(), "worst".into()),
        ("Ġ".into(), "better".into()),
        ("Ġ".into(), "worse".into()),
        ("Ġ".into(), "same".into()),
        ("Ġ".into(), "different".into()),
        ("Ġ".into(), "equal".into()),
        ("Ġ".into(), "match".into()),
        ("Ġ".into(), "matches".into()),
        ("Ġ".into(), "compare".into()),
        ("Ġ".into(), "diff".into()),
        ("Ġ".into(), "change".into()),
        ("Ġ".into(), "changed".into()),
        ("Ġ".into(), "update".into()),
        ("Ġ".into(), "updated".into()),
        ("Ġ".into(), "create".into()),
        ("Ġ".into(), "created".into()),
        ("Ġ".into(), "delete".into()),
        ("Ġ".into(), "deleted".into()),
        ("Ġ".into(), "remove".into()),
        ("Ġ".into(), "removed".into()),
        ("Ġ".into(), "add".into()),
        ("Ġ".into(), "added".into()),
        ("Ġ".into(), "insert".into()),
        ("Ġ".into(), "inserted".into()),
        ("Ġ".into(), "append".into()),
        ("Ġ".into(), "appended".into()),
        ("Ġ".into(), "replace".into()),
        ("Ġ".into(), "replaced".into()),
        ("Ġ".into(), "parse".into()),
        ("Ġ".into(), "parsed".into()),
        ("Ġ".into(), "format".into()),
        ("Ġ".into(), "formatted".into()),
        ("Ġ".into(), "encode".into()),
        ("Ġ".into(), "encoded".into()),
        ("Ġ".into(), "decode".into()),
        ("Ġ".into(), "decoded".into()),
        ("Ġ".into(), "load".into()),
        ("Ġ".into(), "loaded".into()),
        ("Ġ".into(), "save".into()),
        ("Ġ".into(), "saved".into()),
        ("Ġ".into(), "send".into()),
        ("Ġ".into(), "sent".into()),
        ("Ġ".into(), "receive".into()),
        ("Ġ".into(), "received".into()),
        ("Ġ".into(), "request".into()),
        ("Ġ".into(), "requests".into()),
        ("Ġ".into(), "response".into()),
        ("Ġ".into(), "responses".into()),
        ("Ġ".into(), "success".into()),
        ("Ġ".into(), "failure".into()),
        ("Ġ".into(), "fail".into()),
        ("Ġ".into(), "failed".into()),
        ("Ġ".into(), "pass".into()),
        ("Ġ".into(), "passed".into()),
        ("Ġ".into(), "skip".into()),
        ("Ġ".into(), "skipped".into()),
        ("Ġ".into(), "retry".into()),
        ("Ġ".into(), "retries".into()),
        ("Ġ".into(), "attempt".into()),
        ("Ġ".into(), "attempts".into()),
        ("Ġ".into(), "valid".into()),
        ("Ġ".into(), "invalid".into()),
        ("Ġ".into(), "empty".into()),
        ("Ġ".into(), "full".into()),
        ("Ġ".into(), "open".into()),
        ("Ġ".into(), "closed".into()),
        ("Ġ".into(), "ready".into()),
        ("Ġ".into(), "done".into()),
        ("Ġ".into(), "complete".into()),
        ("Ġ".into(), "completed".into()),
        ("Ġ".into(), "pending".into()),
        ("Ġ".into(), "active".into()),
        ("Ġ".into(), "inactive".into()),
        ("Ġ".into(), "enabled".into()),
        ("Ġ".into(), "disabled".into()),
        ("Ġ".into(), "available".into()),
        ("Ġ".into(), "unavailable".into()),
        ("Ġ".into(), "supported".into()),
        ("Ġ".into(), "unsupported".into()),
        ("Ġ".into(), "required".into()),
        ("Ġ".into(), "optional".into()),
        ("Ġ".into(), "missing".into()),
        ("Ġ".into(), "found".into()),
        ("Ġ".into(), "exists".into()),
        ("Ġ".into(), "exist".into()),
        ("Ġ".into(), "contains".into()),
        ("Ġ".into(), "include".into()),
        ("Ġ".into(), "includes".into()),
        ("Ġ".into(), "included".into()),
        ("Ġ".into(), "exclude".into()),
        ("Ġ".into(), "excludes".into()),
        ("Ġ".into(), "excluded".into()),
        ("Ġ".into(), "allow".into()),
        ("Ġ".into(), "allowed".into()),
        ("Ġ".into(), "deny".into()),
        ("Ġ".into(), "denied".into()),
        ("Ġ".into(), "block".into()),
        ("Ġ".into(), "blocked".into()),
        ("Ġ".into(), "permit".into()),
        ("Ġ".into(), "permitted".into()),
        ("Ġ".into(), "grant".into()),
        ("Ġ".into(), "granted".into()),
        ("Ġ".into(), "access".into()),
        ("Ġ".into(), "auth".into()),
        ("Ġ".into(), "login".into()),
        ("Ġ".into(), "logout".into()),
        ("Ġ".into(), "password".into()),
        ("Ġ".into(), "token".into()),
        ("Ġ".into(), "secret".into()),
        ("Ġ".into(), "key".into()),
        ("Ġ".into(), "hash".into()),
        ("Ġ".into(), "salt".into()),
        ("Ġ".into(), "encrypt".into()),
        ("Ġ".into(), "encrypted".into()),
        ("Ġ".into(), "decrypt".into()),
        ("Ġ".into(), "decrypted".into()),
        ("Ġ".into(), "sign".into()),
        ("Ġ".into(), "signed".into()),
        ("Ġ".into(), "verify".into()),
        ("Ġ".into(), "verified".into()),
        ("Ġ".into(), "validate".into()),
        ("Ġ".into(), "validated".into()),
        ("Ġ".into(), "sanitize".into()),
        ("Ġ".into(), "sanitized".into()),
        ("Ġ".into(), "clean".into()),
        ("Ġ".into(), "cleaned".into()),
        ("Ġ".into(), "normalize".into()),
        ("Ġ".into(), "normalized".into()),
        ("Ġ".into(), "transform".into()),
        ("Ġ".into(), "transformed".into()),
        ("Ġ".into(), "convert".into()),
        ("Ġ".into(), "converted".into()),
        ("Ġ".into(), "copy".into()),
        ("Ġ".into(), "copied".into()),
        ("Ġ".into(), "move".into()),
        ("Ġ".into(), "moved".into()),
        ("Ġ".into(), "swap".into()),
        ("Ġ".into(), "swapped".into()),
        ("Ġ".into(), "merge".into()),
        ("Ġ".into(), "merged".into()),
        ("Ġ".into(), "split".into()),
        ("Ġ".into(), "splitted".into()),
        ("Ġ".into(), "chunk".into()),
        ("Ġ".into(), "chunks".into()),
        ("Ġ".into(), "chunked".into()),
        ("Ġ".into(), "slice".into()),
        ("Ġ".into(), "sliced".into()),
        ("Ġ".into(), "trim".into()),
        ("Ġ".into(), "trimmed".into()),
        ("Ġ".into(), "pad".into()),
        ("Ġ".into(), "padded".into()),
        ("Ġ".into(), "wrap".into()),
        ("Ġ".into(), "wrapped".into()),
        ("Ġ".into(), "unwrap".into()),
        ("Ġ".into(), "unwrapped".into()),
        ("Ġ".into(), "fold".into()),
        ("Ġ".into(), "folded".into()),
        ("Ġ".into(), "reduce".into()),
        ("Ġ".into(), "reduced".into()),
        ("Ġ".into(), "map".into()),
        ("Ġ".into(), "mapped".into()),
        ("Ġ".into(), "flat".into()),
        ("Ġ".into(), "flatten".into()),
        ("Ġ".into(), "flattened".into()),
        ("Ġ".into(), "zip".into()),
        ("Ġ".into(), "zipped".into()),
        ("Ġ".into(), "pair".into()),
        ("Ġ".into(), "pairs".into()),
        ("Ġ".into(), "tuple".into()),
        ("Ġ".into(), "tuples".into()),
        ("Ġ".into(), "struct".into()),
        ("Ġ".into(), "structs".into()),
        ("Ġ".into(), "enum".into()),
        ("Ġ".into(), "enums".into()),
        ("Ġ".into(), "trait".into()),
        ("Ġ".into(), "traits".into()),
        ("Ġ".into(), "impl".into()),
        ("Ġ".into(), "module".into()),
        ("Ġ".into(), "modules".into()),
        ("Ġ".into(), "package".into()),
        ("Ġ".into(), "packages".into()),
        ("Ġ".into(), "import".into()),
        ("Ġ".into(), "imports".into()),
        ("Ġ".into(), "export".into()),
        ("Ġ".into(), "exports".into()),
        ("Ġ".into(), "return".into()),
        ("Ġ".into(), "returns".into()),
        ("Ġ".into(), "returned".into()),
        ("Ġ".into(), "break".into()),
        ("Ġ".into(), "continue".into()),
        ("Ġ".into(), "loop".into()),
        ("Ġ".into(), "loops".into()),
        ("Ġ".into(), "while".into()),
        ("Ġ".into(), "for".into()),
        ("Ġ".into(), "foreach".into()),
        ("Ġ".into(), "if".into()),
        ("Ġ".into(), "else".into()),
        ("Ġ".into(), "elif".into()),
        ("Ġ".into(), "switch".into()),
        ("Ġ".into(), "case".into()),
        ("Ġ".into(), "cases".into()),
        ("Ġ".into(), "match".into()),
        ("Ġ".into(), "matches".into()),
        ("Ġ".into(), "matched".into()),
        ("Ġ".into(), "try".into()),
        ("Ġ".into(), "catch".into()),
        ("Ġ".into(), "throw".into()),
        ("Ġ".into(), "throws".into()),
        ("Ġ".into(), "thrown".into()),
        ("Ġ".into(), "raise".into()),
        ("Ġ".into(), "raised".into()),
        ("Ġ".into(), "panic".into()),
        ("Ġ".into(), "panicked".into()),
        ("Ġ".into(), "recover".into()),
        ("Ġ".into(), "recovered".into()),
        ("Ġ".into(), "handle".into()),
        ("Ġ".into(), "handled".into()),
        ("Ġ".into(), "handler".into()),
        ("Ġ".into(), "handlers".into()),
        ("Ġ".into(), "callback".into()),
        ("Ġ".into(), "callbacks".into()),
        ("Ġ".into(), "event".into()),
        ("Ġ".into(), "events".into()),
        ("Ġ".into(), "listener".into()),
        ("Ġ".into(), "listeners".into()),
        ("Ġ".into(), "signal".into()),
        ("Ġ".into(), "signals".into()),
        ("Ġ".into(), "emit".into()),
        ("Ġ".into(), "emitted".into()),
        ("Ġ".into(), "trigger".into()),
        ("Ġ".into(), "triggered".into()),
        ("Ġ".into(), "notify".into()),
        ("Ġ".into(), "notified".into()),
        ("Ġ".into(), "publish".into()),
        ("Ġ".into(), "published".into()),
        ("Ġ".into(), "subscribe".into()),
        ("Ġ".into(), "subscribed".into()),
        ("Ġ".into(), "queue".into()),
        ("Ġ".into(), "queues".into()),
        ("Ġ".into(), "enqueue".into()),
        ("Ġ".into(), "enqueued".into()),
        ("Ġ".into(), "dequeue".into()),
        ("Ġ".into(), "dequeued".into()),
        ("Ġ".into(), "push".into()),
        ("Ġ".into(), "pushed".into()),
        ("Ġ".into(), "pop".into()),
        ("Ġ".into(), "popped".into()),
        ("Ġ".into(), "peek".into()),
        ("Ġ".into(), "peeked".into()),
        ("Ġ".into(), "shift".into()),
        ("Ġ".into(), "shifted".into()),
        ("Ġ".into(), "unshift".into()),
        ("Ġ".into(), "unshifted".into()),
        ("Ġ".into(), "stack".into()),
        ("Ġ".into(), "stacks".into()),
        ("Ġ".into(), "heap".into()),
        ("Ġ".into(), "heaps".into()),
        ("Ġ".into(), "tree".into()),
        ("Ġ".into(), "trees".into()),
        ("Ġ".into(), "node".into()),
        ("Ġ".into(), "nodes".into()),
        ("Ġ".into(), "edge".into()),
        ("Ġ".into(), "edges".into()),
        ("Ġ".into(), "graph".into()),
        ("Ġ".into(), "graphs".into()),
        ("Ġ".into(), "path".into()),
        ("Ġ".into(), "paths".into()),
        ("Ġ".into(), "route".into()),
        ("Ġ".into(), "routes".into()),
        ("Ġ".into(), "router".into()),
        ("Ġ".into(), "routers".into()),
        ("Ġ".into(), "link".into()),
        ("Ġ".into(), "links".into()),
        ("Ġ".into(), "connect".into()),
        ("Ġ".into(), "connected".into()),
        ("Ġ".into(), "disconnect".into()),
        ("Ġ".into(), "disconnected".into()),
        ("Ġ".into(), "bind".into()),
        ("Ġ".into(), "bound".into()),
        ("Ġ".into(), "listen".into()),
        ("Ġ".into(), "listening".into()),
        ("Ġ".into(), "accept".into()),
        ("Ġ".into(), "accepted".into()),
        ("Ġ".into(), "connect".into()),
        ("Ġ".into(), "connected".into()),
        ("Ġ".into(), "socket".into()),
        ("Ġ".into(), "sockets".into()),
        ("Ġ".into(), "stream".into()),
        ("Ġ".into(), "streams".into()),
        ("Ġ".into(), "buffer".into()),
        ("Ġ".into(), "buffers".into()),
        ("Ġ".into(), "cache".into()),
        ("Ġ".into(), "caches".into()),
        ("Ġ".into(), "cached".into()),
        ("Ġ".into(), "store".into()),
        ("Ġ".into(), "stores".into()),
        ("Ġ".into(), "stored".into()),
        ("Ġ".into(), "memory".into()),
        ("Ġ".into(), "disk".into()),
        ("Ġ".into(), "cpu".into()),
        ("Ġ".into(), "gpu".into()),
        ("Ġ".into(), "ram".into()),
        ("Ġ".into(), "thread".into()),
        ("Ġ".into(), "threads".into()),
        ("Ġ".into(), "process".into()),
        ("Ġ".into(), "processes".into()),
        ("Ġ".into(), "task".into()),
        ("Ġ".into(), "tasks".into()),
        ("Ġ".into(), "job".into()),
        ("Ġ".into(), "jobs".into()),
        ("Ġ".into(), "worker".into()),
        ("Ġ".into(), "workers".into()),
        ("Ġ".into(), "pool".into()),
        ("Ġ".into(), "pools".into()),
        ("Ġ".into(), "batch".into()),
        ("Ġ".into(), "batches".into()),
        ("Ġ".into(), "parallel".into()),
        ("Ġ".into(), "serial".into()),
        ("Ġ".into(), "sync".into()),
        ("Ġ".into(), "async".into()),
        ("Ġ".into(), "await".into()),
        ("Ġ".into(), "future".into()),
        ("Ġ".into(), "futures".into()),
        ("Ġ".into(), "promise".into()),
        ("Ġ".into(), "promises".into()),
        ("Ġ".into(), "channel".into()),
        ("Ġ".into(), "channels".into()),
        ("Ġ".into(), "mutex".into()),
        ("Ġ".into(), "lock".into()),
        ("Ġ".into(), "locks".into()),
        ("Ġ".into(), "locked".into()),
        ("Ġ".into(), "unlock".into()),
        ("Ġ".into(), "unlocked".into()),
        ("Ġ".into(), "atomic".into()),
        ("Ġ".into(), "atomics".into()),
        ("Ġ".into(), "race".into()),
        ("Ġ".into(), "races".into()),
        ("Ġ".into(), "deadlock".into()),
        ("Ġ".into(), "deadlocks".into()),
        ("Ġ".into(), "timeout".into()),
        ("Ġ".into(), "timeouts".into()),
        ("Ġ".into(), "interval".into()),
        ("Ġ".into(), "intervals".into()),
        ("Ġ".into(), "timer".into()),
        ("Ġ".into(), "timers".into()),
        ("Ġ".into(), "tick".into()),
        ("Ġ".into(), "ticks".into()),
        ("Ġ".into(), "sleep".into()),
        ("Ġ".into(), "slept".into()),
        ("Ġ".into(), "wake".into()),
        ("Ġ".into(), "woke".into()),
        ("Ġ".into(), "wakeup".into()),
        ("Ġ".into(), "wakeups".into()),
        ("Ġ".into(), "schedule".into()),
        ("Ġ".into(), "scheduled".into()),
        ("Ġ".into(), "delay".into()),
        ("Ġ".into(), "delayed".into()),
        ("Ġ".into(), "defer".into()),
        ("Ġ".into(), "deferred".into()),
        ("Ġ".into(), "cancel".into()),
        ("Ġ".into(), "cancelled".into()),
        ("Ġ".into(), "abort".into()),
        ("Ġ".into(), "aborted".into()),
        ("Ġ".into(), "resume".into()),
        ("Ġ".into(), "resumed".into()),
        ("Ġ".into(), "pause".into()),
        ("Ġ".into(), "paused".into()),
        ("Ġ".into(), "reset".into()),
        ("Ġ".into(), "reseted".into()),
        ("Ġ".into(), "clear".into()),
        ("Ġ".into(), "cleared".into()),
        ("Ġ".into(), "flush".into()),
        ("Ġ".into(), "flushed".into()),
        ("Ġ".into(), "drain".into()),
        ("Ġ".into(), "drained".into()),
        ("Ġ".into(), "fill".into()),
        ("Ġ".into(), "filled".into()),
        ("Ġ".into(), "grow".into()),
        ("Ġ".into(), "grew".into()),
        ("Ġ".into(), "shrink".into()),
        ("Ġ".into(), "shrank".into()),
        ("Ġ".into(), "expand".into()),
        ("Ġ".into(), "expanded".into()),
        ("Ġ".into(), "contract".into()),
        ("Ġ".into(), "contracted".into()),
        ("Ġ".into(), "scale".into()),
        ("Ġ".into(), "scaled".into()),
        ("Ġ".into(), "resize".into()),
        ("Ġ".into(), "resized".into()),
        ("Ġ".into(), "allocate".into()),
        ("Ġ".into(), "allocated".into()),
        ("Ġ".into(), "free".into()),
        ("Ġ".into(), "freed".into()),
        ("Ġ".into(), "gc".into()),
        ("Ġ".into(), "collect".into()),
        ("Ġ".into(), "collected".into()),
        ("Ġ".into(), "compact".into()),
        ("Ġ".into(), "compacted".into()),
        ("Ġ".into(), "defrag".into()),
        ("Ġ".into(), "defragged".into()),
        ("Ġ".into(), "optimize".into()),
        ("Ġ".into(), "optimized".into()),
        ("Ġ".into(), "profile".into()),
        ("Ġ".into(), "profiled".into()),
        ("Ġ".into(), "benchmark".into()),
        ("Ġ".into(), "benchmarked".into()),
        ("Ġ".into(), "measure".into()),
        ("Ġ".into(), "measured".into()),
        ("Ġ".into(), "monitor".into()),
        ("Ġ".into(), "monitored".into()),
        ("Ġ".into(), "trace".into()),
        ("Ġ".into(), "traced".into()),
        ("Ġ".into(), "log".into()),
        ("Ġ".into(), "logged".into()),
        ("Ġ".into(), "debug".into()),
        ("Ġ".into(), "debugged".into()),
        ("Ġ".into(), "info".into()),
        ("Ġ".into(), "warn".into()),
        ("Ġ".into(), "warning".into()),
        ("Ġ".into(), "warnings".into()),
        ("Ġ".into(), "alert".into()),
        ("Ġ".into(), "alerts".into()),
        ("Ġ".into(), "critical".into()),
        ("Ġ".into(), "fatal".into()),
        ("Ġ".into(), "crash".into()),
        ("Ġ".into(), "crashed".into()),
        ("Ġ".into(), "recover".into()),
        ("Ġ".into(), "recovered".into()),
        ("Ġ".into(), "backup".into()),
        ("Ġ".into(), "backups".into()),
        ("Ġ".into(), "restore".into()),
        ("Ġ".into(), "restored".into()),
        ("Ġ".into(), "snapshot".into()),
        ("Ġ".into(), "snapshots".into()),
        ("Ġ".into(), "checkpoint".into()),
        ("Ġ".into(), "checkpoints".into()),
        ("Ġ".into(), "commit".into()),
        ("Ġ".into(), "committed".into()),
        ("Ġ".into(), "rollback".into()),
        ("Ġ".into(), "rolled".into()),
        ("Ġ".into(), "transaction".into()),
        ("Ġ".into(), "transactions".into()),
        ("Ġ".into(), "atomic".into()),
        ("Ġ".into(), "consistency".into()),
        ("Ġ".into(), "isolation".into()),
        ("Ġ".into(), "durability".into()),
        ("Ġ".into(), "acid".into()),
        ("Ġ".into(), "base".into()),
        ("Ġ".into(), "cap".into()),
        ("Ġ".into(), "theorem".into()),
        ("Ġ".into(), "lemma".into()),
        ("Ġ".into(), "proof".into()),
        ("Ġ".into(), "proofs".into()),
        ("Ġ".into(), "axiom".into()),
        ("Ġ".into(), "axioms".into()),
        ("Ġ".into(), "hypothesis".into()),
        ("Ġ".into(), "hypotheses".into()),
        ("Ġ".into(), "theory".into()),
        ("Ġ".into(), "theories".into()),
        ("Ġ".into(), "law".into()),
        ("Ġ".into(), "laws".into()),
        ("Ġ".into(), "rule".into()),
        ("Ġ".into(), "rules".into()),
        ("Ġ".into(), "policy".into()),
        ("Ġ".into(), "policies".into()),
        ("Ġ".into(), "constraint".into()),
        ("Ġ".into(), "constraints".into()),
        ("Ġ".into(), "invariant".into()),
        ("Ġ".into(), "invariants".into()),
        ("Ġ".into(), "assert".into()),
        ("Ġ".into(), "asserted".into()),
        ("Ġ".into(), "assume".into()),
        ("Ġ".into(), "assumed".into()),
        ("Ġ".into(), "expect".into()),
        ("Ġ".into(), "expected".into()),
        ("Ġ".into(), "actual".into()),
        ("Ġ".into(), "delta".into()),
        ("Ġ".into(), "deltas".into()),
        ("Ġ".into(), "epsilon".into()),
        ("Ġ".into(), "tolerance".into()),
        ("Ġ".into(), "threshold".into()),
        ("Ġ".into(), "thresholds".into()),
        ("Ġ".into(), "bound".into()),
        ("Ġ".into(), "bounds".into()),
        ("Ġ".into(), "bounded".into()),
        ("Ġ".into(), "unbounded".into()),
        ("Ġ".into(), "finite".into()),
        ("Ġ".into(), "infinite".into()),
        ("Ġ".into(), "zero".into()),
        ("Ġ".into(), "one".into()),
        ("Ġ".into(), "two".into()),
        ("Ġ".into(), "three".into()),
        ("Ġ".into(), "four".into()),
        ("Ġ".into(), "five".into()),
        ("Ġ".into(), "six".into()),
        ("Ġ".into(), "seven".into()),
        ("Ġ".into(), "eight".into()),
        ("Ġ".into(), "nine".into()),
        ("Ġ".into(), "ten".into()),
    ]
}

fn count_approx(text: &str) -> i64 {
    if text.is_empty() {
        return 0;
    }
    let chars = text.chars().count() as f64;
    let words = text.split_whitespace().count() as f64;
    let by_chars = (chars / 4.0).ceil();
    let by_words = (words * 1.3).ceil();
    by_chars.max(by_words).max(1.0) as i64
}

fn chunk_text(tok: &BpeTokenizer, text: &str, max_tokens: usize) -> Vec<String> {
    if max_tokens == 0 {
        return Vec::new();
    }
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut current_count = 0usize;
    for piece in pretokenize(text) {
        let n = tok.tokenize_word(&piece).len();
        if n > max_tokens {
            if !current.is_empty() {
                chunks.push(current);
                current = String::new();
                current_count = 0;
            }
            let mut piece_ids = tok.encode_text(&piece);
            while !piece_ids.is_empty() {
                let take = max_tokens.min(piece_ids.len());
                let part_ids = piece_ids.drain(..take).collect::<Vec<_>>();
                if let Ok(part) = tok.decode_ids(&part_ids) {
                    chunks.push(part);
                }
            }
            continue;
        }
        if current_count + n > max_tokens && !current.is_empty() {
            chunks.push(current);
            current = piece.to_string();
            current_count = n;
        } else {
            current.push_str(&piece);
            current_count += n;
        }
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn fit_text(tok: &BpeTokenizer, text: &str, max_tokens: usize) -> String {
    if max_tokens == 0 {
        return String::new();
    }
    let ids = tok.encode_text(text);
    if ids.len() <= max_tokens {
        return text.to_string();
    }
    tok.decode_ids(&ids[..max_tokens]).unwrap_or_else(|_| {
        let chunks = chunk_text(tok, text, max_tokens);
        chunks.into_iter().next().unwrap_or_default()
    })
}

// ---------------------------------------------------------------------------
// Handle registry
// ---------------------------------------------------------------------------

thread_local! {
    static TOKENIZERS: RefCell<HashMap<i64, BpeTokenizer>> = RefCell::new(HashMap::new());
    static NEXT_HANDLE: RefCell<i64> = const { RefCell::new(1) };
}

fn new_handle() -> i64 {
    NEXT_HANDLE.with(|h| {
        let mut h = h.borrow_mut();
        let id = *h;
        *h += 1;
        id
    })
}

fn with_tokenizer<T>(
    id: i64,
    span: Span,
    f: impl FnOnce(&BpeTokenizer) -> T,
) -> NiaoResult<Result<T, ValueRef>> {
    TOKENIZERS.with(|store| {
        let store = store.borrow();
        match store.get(&id) {
            Some(tok) => Ok(Ok(f(tok))),
            None => Ok(Err(error_value(
                E2773_NTOK_INVALID_HANDLE,
                "ntok_error",
                format!("invalid or closed tokenizer handle {id}"),
                span,
            ))),
        }
    })
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn arity(args: &[ValueRef], n: usize, name: &str, span: Span) -> NiaoResult<()> {
    if args.len() != n {
        return Err(RuntimeError::at(
            span,
            E2770_NTOK_ARITY,
            format!("{name}() expects {n} argument(s), got {}", args.len()),
        ));
    }
    Ok(())
}

fn arity_range(
    args: &[ValueRef],
    min: usize,
    max: usize,
    name: &str,
    span: Span,
) -> NiaoResult<()> {
    if args.len() < min || args.len() > max {
        return Err(RuntimeError::at(
            span,
            E2770_NTOK_ARITY,
            format!(
                "{name}() expects {min}..={max} argument(s), got {}",
                args.len()
            ),
        ));
    }
    Ok(())
}

fn type_err(span: Span, msg: impl Into<String>) -> RuntimeError {
    RuntimeError::at(span, E2772_NTOK_TYPE, msg.into())
}

fn string_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<String> {
    match &*args[idx].borrow() {
        Value::String(s) => Ok(s.clone()),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects a string as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn int_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<i64> {
    match &*args[idx].borrow() {
        Value::Int(n) => Ok(*n),
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an int as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn int_array_arg(args: &[ValueRef], idx: usize, name: &str, span: Span) -> NiaoResult<Vec<i64>> {
    match &*args[idx].borrow() {
        Value::IntArray(items) => Ok(items.clone()),
        Value::Array(items) => {
            let mut out = Vec::new();
            for (i, item) in items.iter().enumerate() {
                match &*item.borrow() {
                    Value::Int(n) => out.push(*n),
                    other => {
                        return Err(type_err(
                            span,
                            format!(
                                "{name}() expects int array at argument {}, element {} is {}",
                                idx + 1,
                                i + 1,
                                other.type_name()
                            ),
                        ));
                    }
                }
            }
            Ok(out)
        }
        other => Err(type_err(
            span,
            format!(
                "{name}() expects an int array as argument {}, got {}",
                idx + 1,
                other.type_name()
            ),
        )),
    }
}

fn ntok_err(span: Span, msg: impl Into<String>) -> ValueRef {
    error_value(E2771_NTOK_ERROR, "ntok_error", msg.into(), span)
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

fn ntok_builtin(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 0, "ntok_builtin", span)?;
    let id = new_handle();
    TOKENIZERS.with(|store| {
        store.borrow_mut().insert(id, BpeTokenizer::builtin());
    });
    Ok(Value::Int(id).ref_cell())
}

fn ntok_load(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity_range(args, 1, 2, "ntok_load", span)?;
    let vocab_path = string_arg(args, 0, "ntok_load", span)?;
    let merges_path = if args.len() > 1 {
        Some(string_arg(args, 1, "ntok_load", span)?)
    } else {
        let p = Path::new(&vocab_path);
        let sibling = p.with_file_name("merges.txt");
        if sibling.exists() {
            Some(sibling.to_string_lossy().to_string())
        } else {
            None
        }
    };
    match BpeTokenizer::from_files(&vocab_path, merges_path.as_deref()) {
        Ok(tok) => {
            let id = new_handle();
            TOKENIZERS.with(|store| store.borrow_mut().insert(id, tok));
            Ok(Value::Int(id).ref_cell())
        }
        Err(msg) => Ok(ntok_err(span, msg)),
    }
}

fn ntok_encode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ntok_encode", span)?;
    let id = int_arg(args, 0, "ntok_encode", span)?;
    let text = string_arg(args, 1, "ntok_encode", span)?;
    match with_tokenizer(id, span, |tok| tok.encode_text(&text))? {
        Ok(ids) => Ok(Value::IntArray(ids).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn ntok_decode(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ntok_decode", span)?;
    let id = int_arg(args, 0, "ntok_decode", span)?;
    let ids = int_array_arg(args, 1, "ntok_decode", span)?;
    match with_tokenizer(id, span, |tok| tok.decode_ids(&ids))? {
        Ok(Ok(s)) => Ok(Value::String(s).ref_cell()),
        Ok(Err(msg)) => Ok(ntok_err(span, msg)),
        Err(e) => Ok(e),
    }
}

fn ntok_count(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 2, "ntok_count", span)?;
    let id = int_arg(args, 0, "ntok_count", span)?;
    let text = string_arg(args, 1, "ntok_count", span)?;
    match with_tokenizer(id, span, |tok| tok.encode_text(&text).len() as i64)? {
        Ok(n) => Ok(Value::Int(n).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn ntok_count_approx(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntok_count_approx", span)?;
    let text = string_arg(args, 0, "ntok_count_approx", span)?;
    Ok(Value::Int(count_approx(&text)).ref_cell())
}

fn ntok_chunk(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "ntok_chunk", span)?;
    let id = int_arg(args, 0, "ntok_chunk", span)?;
    let text = string_arg(args, 1, "ntok_chunk", span)?;
    let max = int_arg(args, 2, "ntok_chunk", span)?;
    if max <= 0 {
        return Ok(ntok_err(span, "ntok_chunk() max_tokens must be > 0"));
    }
    match with_tokenizer(id, span, |tok| chunk_text(tok, &text, max as usize))? {
        Ok(chunks) => Ok(Value::StringArray(StringArray::dense(chunks)).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn ntok_fit(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 3, "ntok_fit", span)?;
    let id = int_arg(args, 0, "ntok_fit", span)?;
    let text = string_arg(args, 1, "ntok_fit", span)?;
    let max = int_arg(args, 2, "ntok_fit", span)?;
    if max < 0 {
        return Ok(ntok_err(span, "ntok_fit() max_tokens must be >= 0"));
    }
    match with_tokenizer(id, span, |tok| fit_text(tok, &text, max as usize))? {
        Ok(s) => Ok(Value::String(s).ref_cell()),
        Err(e) => Ok(e),
    }
}

fn ntok_close(args: &[ValueRef], span: Span) -> NiaoResult<ValueRef> {
    arity(args, 1, "ntok_close", span)?;
    let id = int_arg(args, 0, "ntok_close", span)?;
    let removed = TOKENIZERS.with(|store| store.borrow_mut().remove(&id).is_some());
    Ok(Value::Bool(removed).ref_cell())
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

macro_rules! ntok_fns {
    ($(($flat:literal, $short:literal, $f:ident)),+ $(,)?) => {
        fn all_pairs() -> Vec<(&'static str, &'static str, NativeFn)> {
            vec![$(($flat, $short, Rc::new($f) as NativeFn)),+]
        }
    };
}

ntok_fns![
    ("ntok_builtin", "builtin", ntok_builtin),
    ("ntok_load", "load", ntok_load),
    ("ntok_encode", "encode", ntok_encode),
    ("ntok_decode", "decode", ntok_decode),
    ("ntok_count", "count", ntok_count),
    ("ntok_count_approx", "count_approx", ntok_count_approx),
    ("ntok_chunk", "chunk", ntok_chunk),
    ("ntok_fit", "fit", ntok_fit),
    ("ntok_close", "close", ntok_close),
];

fn all_builtins() -> Vec<(&'static str, NativeFn)> {
    all_pairs()
        .into_iter()
        .map(|(flat, _, f)| (flat, f))
        .collect()
}

pub fn namespace() -> Value {
    let mut map = HashMap::new();
    for (_, short, f) in all_pairs() {
        map.insert(short.to_string(), Value::NativeFunction(f).ref_cell());
    }
    Value::Object(map)
}

pub const MODULE_NAME: &str = "ntok";
pub const MODULE_PATHS: &[&str] = &["ntok", "std/ntok"];

pub fn builtins() -> Vec<(&'static str, NativeFn)> {
    all_builtins()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> Span {
        Span::dummy()
    }

    fn i(v: i64) -> ValueRef {
        Value::Int(v).ref_cell()
    }

    fn s(v: &str) -> ValueRef {
        Value::String(v.to_string()).ref_cell()
    }

    fn handle(r: NiaoResult<ValueRef>) -> i64 {
        match &*r.unwrap().borrow() {
            Value::Int(n) => *n,
            other => panic!("expected handle, got {other:?}"),
        }
    }

    #[test]
    fn builtin_roundtrip() {
        let h = handle(ntok_builtin(&[], span()));
        let text = "Hello, world!";
        let ids_val = ntok_encode(&[i(h), s(text)], span())
            .unwrap()
            .borrow()
            .clone();
        match ids_val {
            Value::IntArray(v) => assert!(!v.is_empty()),
            other => panic!("expected int_array, got {other:?}"),
        }
        let count_val = ntok_count(&[i(h), s(text)], span())
            .unwrap()
            .borrow()
            .clone();
        match count_val {
            Value::Int(n) => assert!(n > 0),
            other => panic!("expected int, got {other:?}"),
        }
        let ids = ntok_encode(&[i(h), s(text)], span()).unwrap();
        let decoded_val = ntok_decode(&[i(h), ids], span()).unwrap().borrow().clone();
        match decoded_val {
            Value::String(s) => assert_eq!(s, text),
            other => panic!("expected string, got {other:?}"),
        }
        ntok_close(&[i(h)], span()).unwrap();
    }

    #[test]
    fn chunk_and_fit() {
        let h = handle(ntok_builtin(&[], span()));
        let text = "one two three four five six seven eight nine ten";
        let chunks_val = ntok_chunk(&[i(h), s(text), i(5)], span())
            .unwrap()
            .borrow()
            .clone();
        match chunks_val {
            Value::StringArray(parts) => assert!(parts.len() > 0),
            other => panic!("expected string_array, got {other:?}"),
        }
        let fitted_val = ntok_fit(&[i(h), s(text), i(8)], span())
            .unwrap()
            .borrow()
            .clone();
        match fitted_val {
            Value::String(s) => {
                let fitted_count_val =
                    ntok_count(&[i(h), Value::String(s.clone()).ref_cell()], span())
                        .unwrap()
                        .borrow()
                        .clone();
                match fitted_count_val {
                    Value::Int(n) => assert!(n <= 8),
                    other => panic!("expected int, got {other:?}"),
                }
            }
            other => panic!("expected string, got {other:?}"),
        }
        ntok_close(&[i(h)], span()).unwrap();
    }

    #[test]
    fn count_approx_positive() {
        let n_val = ntok_count_approx(&[s("hello world from ntok")], span())
            .unwrap()
            .borrow()
            .clone();
        match n_val {
            Value::Int(v) => assert!(v > 0),
            other => panic!("expected int, got {other:?}"),
        }
    }
}
