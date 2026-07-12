//! Literal prefix scanner for fast-path skipping before Pike VM.

use crate::flags::Flags;
use crate::parse::{fold_case, Ast};

pub fn extract_literal_prefix(ast: &Ast, flags: Flags) -> Option<Vec<u32>> {
    let mut out = Vec::new();
    if !collect(ast, flags, &mut out) {
        return None;
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn collect(ast: &Ast, flags: Flags, out: &mut Vec<u32>) -> bool {
    match ast {
        Ast::Literal(c) => {
            out.push(if flags.case_insensitive {
                fold_case(*c)
            } else {
                *c
            });
            true
        }
        Ast::Concat(parts) => {
            for p in parts {
                if !collect(p, flags, out) {
                    return false;
                }
            }
            true
        }
        _ => false,
    }
}

pub fn find_literal(hay: &str, prefix: &[u32], flags: Flags) -> Option<usize> {
    if prefix.is_empty() {
        return Some(0);
    }
    let first = prefix[0];
    let mut pos = 0;
    let bytes = hay.as_bytes();
    while pos <= bytes.len() {
        if let Some((i, ch)) = hay[pos..].char_indices().next() {
            let abs = pos + i;
            let c = if flags.case_insensitive {
                fold_case(ch as u32)
            } else {
                ch as u32
            };
            if c == first && matches_at(hay, abs, prefix, flags) {
                return Some(abs);
            }
            pos = abs + ch.len_utf8();
        } else {
            break;
        }
    }
    None
}

fn matches_at(hay: &str, start: usize, prefix: &[u32], flags: Flags) -> bool {
    let mut pos = start;
    for &want in prefix {
        let Some(ch) = hay[pos..].chars().next() else {
            return false;
        };
        let c = if flags.case_insensitive {
            fold_case(ch as u32)
        } else {
            ch as u32
        };
        if c != want {
            return false;
        }
        pos += ch.len_utf8();
    }
    true
}

#[inline]
pub fn scan_first_char(hay: &str, ch: u32, flags: Flags) -> Option<usize> {
    let mut pos = 0;
    while pos <= hay.len() {
        if let Some((i, c)) = hay[pos..].char_indices().next() {
            let got = if flags.case_insensitive {
                fold_case(c as u32)
            } else {
                c as u32
            };
            if got == ch {
                return Some(pos + i);
            }
            pos += i + c.len_utf8();
        } else {
            break;
        }
    }
    None
}
