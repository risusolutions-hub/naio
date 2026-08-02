//! SequenceMatcher-style cached matcher (difflib subset).

use crate::error::TextDiffError;
use crate::opts::{DiffOpts, Granularity};
use crate::split::{check_pair, normalize_lines, splitlines};
use similar::{Algorithm, DiffTag, TextDiff};

#[derive(Debug, Clone)]
pub struct Matcher {
    old: Vec<String>,
    new: Vec<String>,
    norm_old: Vec<String>,
    norm_new: Vec<String>,
    opts: DiffOpts,
    granularity: Granularity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opcode {
    pub tag: String,
    pub i1: usize,
    pub i2: usize,
    pub j1: usize,
    pub j2: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchBlock {
    pub a: usize,
    pub b: usize,
    pub size: usize,
}

impl Matcher {
    pub fn new(
        old: &str,
        new: &str,
        opts: DiffOpts,
        granularity: Granularity,
    ) -> Result<Self, TextDiffError> {
        check_pair(old, new)?;
        let (old_lines, new_lines) = match granularity {
            Granularity::Line => (splitlines(old, false), splitlines(new, false)),
            Granularity::Word | Granularity::UnicodeWord | Granularity::Char => {
                (vec![old.to_string()], vec![new.to_string()])
            }
        };
        let norm_old = normalize_lines(&old_lines, &opts);
        let norm_new = normalize_lines(&new_lines, &opts);
        Ok(Self {
            old: old_lines,
            new: new_lines,
            norm_old,
            norm_new,
            opts,
            granularity,
        })
    }

    pub fn set_first(&mut self, old: &str) -> Result<(), TextDiffError> {
        check_pair(old, "")?;
        self.old = match self.granularity {
            Granularity::Line => splitlines(old, false),
            _ => vec![old.to_string()],
        };
        self.norm_old = normalize_lines(&self.old, &self.opts);
        Ok(())
    }

    pub fn set_second(&mut self, new: &str) -> Result<(), TextDiffError> {
        check_pair("", new)?;
        self.new = match self.granularity {
            Granularity::Line => splitlines(new, false),
            _ => vec![new.to_string()],
        };
        self.norm_new = normalize_lines(&self.new, &self.opts);
        Ok(())
    }

    fn line_refs(&self) -> (Vec<&str>, Vec<&str>) {
        let old: Vec<&str> = self.norm_old.iter().map(|s| s.as_str()).collect();
        let new: Vec<&str> = self.norm_new.iter().map(|s| s.as_str()).collect();
        (old, new)
    }

    fn text_diff(&self) -> TextDiff<'_, '_, str> {
        let (old, new) = self.line_refs();
        TextDiff::configure()
            .algorithm(self.opts.algorithm)
            .diff_slices(&old, &new)
    }

    pub fn ratio(&self) -> f64 {
        self.text_diff().ratio() as f64
    }

    pub fn quick_ratio(&self) -> f64 {
        let matches = self.matching_blocks();
        let m: usize = matches.iter().map(|b| b.size).sum();
        let total = self.norm_old.len() + self.norm_new.len();
        if total == 0 {
            return 1.0;
        }
        (2.0 * m as f64) / total as f64
    }

    pub fn real_quick_ratio(&self) -> f64 {
        let matches = self.matching_blocks();
        let m: usize = matches.iter().map(|b| b.size).sum();
        let la = self.norm_old.len();
        let lb = self.norm_new.len();
        if la + lb == 0 {
            return 1.0;
        }
        if la > lb {
            if m >= la {
                return 1.0;
            }
        } else if m >= lb {
            return 1.0;
        }
        (2.0 * m as f64) / (la + lb) as f64
    }

    pub fn opcodes(&self) -> Vec<Opcode> {
        let diff = self.text_diff();
        diff.ops()
            .iter()
            .map(|op| {
                let old = op.old_range();
                let new = op.new_range();
                Opcode {
                    tag: tag_name(op.tag()).into(),
                    i1: old.start,
                    i2: old.end,
                    j1: new.start,
                    j2: new.end,
                }
            })
            .collect()
    }

    pub fn matching_blocks(&self) -> Vec<MatchBlock> {
        let diff = self.text_diff();
        let mut blocks = Vec::new();
        for op in diff.ops() {
            if op.tag() == DiffTag::Equal {
                let old = op.old_range();
                blocks.push(MatchBlock {
                    a: old.start,
                    b: op.new_range().start,
                    size: old.end - old.start,
                });
            }
        }
        blocks.push(MatchBlock {
            a: self.norm_old.len(),
            b: self.norm_new.len(),
            size: 0,
        });
        blocks
    }

    pub fn old_lines(&self) -> &[String] {
        &self.old
    }

    pub fn new_lines(&self) -> &[String] {
        &self.new
    }

    pub fn opts(&self) -> &DiffOpts {
        &self.opts
    }

    pub fn granularity(&self) -> Granularity {
        self.granularity
    }
}

fn tag_name(tag: DiffTag) -> &'static str {
    match tag {
        DiffTag::Equal => "equal",
        DiffTag::Delete => "delete",
        DiffTag::Insert => "insert",
        DiffTag::Replace => "replace",
    }
}

pub fn ratio(
    a: &str,
    b: &str,
    opts: &DiffOpts,
    granularity: Granularity,
) -> Result<f64, TextDiffError> {
    let m = Matcher::new(a, b, opts.clone(), granularity)?;
    Ok(m.ratio())
}

pub fn quick_ratio(
    a: &str,
    b: &str,
    opts: &DiffOpts,
    granularity: Granularity,
) -> Result<f64, TextDiffError> {
    let m = Matcher::new(a, b, opts.clone(), granularity)?;
    Ok(m.quick_ratio())
}

pub fn real_quick_ratio(
    a: &str,
    b: &str,
    opts: &DiffOpts,
    granularity: Granularity,
) -> Result<f64, TextDiffError> {
    let m = Matcher::new(a, b, opts.clone(), granularity)?;
    Ok(m.real_quick_ratio())
}

pub fn opcodes(
    a: &str,
    b: &str,
    opts: &DiffOpts,
    granularity: Granularity,
) -> Result<Vec<Opcode>, TextDiffError> {
    let m = Matcher::new(a, b, opts.clone(), granularity)?;
    Ok(m.opcodes())
}

pub fn matching_blocks(
    a: &str,
    b: &str,
    opts: &DiffOpts,
    granularity: Granularity,
) -> Result<Vec<MatchBlock>, TextDiffError> {
    let m = Matcher::new(a, b, opts.clone(), granularity)?;
    Ok(m.matching_blocks())
}

pub fn parse_algorithm(name: &str) -> Option<Algorithm> {
    match name {
        "myers" => Some(Algorithm::Myers),
        "patience" => Some(Algorithm::Patience),
        _ => None,
    }
}
