use crate::nfa::{Inst, Program};
use std::collections::HashSet;

const MAX_THREADS: usize = 512;

#[derive(Clone)]
struct Thread {
    pc: usize,
    slots: Vec<usize>,
}

pub struct VmResult {
    pub slots: Vec<usize>,
}

pub fn find_from(prog: &Program, hay: &str, from: usize) -> Option<VmResult> {
    let slot_count = (prog.num_groups as usize + 1) * 2;
    let len = hay.len();
    let mut pos = from;
    while pos <= len {
        if let Some(r) = search_at(prog, hay, pos, slot_count, pos == len) {
            return Some(r);
        }
        if pos == len {
            break;
        }
        pos += hay[pos..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    }
    None
}

pub fn find(prog: &Program, hay: &str) -> Option<VmResult> {
    let slot_count = (prog.num_groups as usize + 1) * 2;
    let len = hay.len();
    let mut threads: Vec<Thread> = Vec::new();
    let mut best: Option<VmResult> = None;

    for pos in 0..=len {
        let mut starter = Thread {
            pc: prog.start,
            slots: vec![usize::MAX; slot_count],
        };
        starter.slots[0] = pos;
        starter.slots[1] = usize::MAX;
        threads.push(starter);

        let mut ready = Vec::new();
        let mut seen: HashSet<(usize, usize)> = HashSet::new();
        for t in threads.drain(..) {
            epsilon(prog, hay, pos, t, &mut ready, &mut seen, pos == len);
        }

        for t in ready
            .iter()
            .filter(|t| matches!(prog.insts.get(t.pc), Some(Inst::Match)))
        {
            let mut slots = t.slots.clone();
            if slots[1] == usize::MAX {
                slots[1] = pos;
            }
            let cand = VmResult { slots };
            if best
                .as_ref()
                .map(|b| is_better_match(&cand, b))
                .unwrap_or(true)
            {
                best = Some(cand);
            }
        }

        threads = ready
            .into_iter()
            .filter(|t| {
                matches!(
                    prog.insts.get(t.pc),
                    Some(Inst::Char(_)) | Some(Inst::Any) | Some(Inst::Class(_))
                )
            })
            .collect();

        if pos >= len {
            break;
        }

        let (c, clen) = match hay[pos..].chars().next() {
            Some(ch) => (ch as u32, ch.len_utf8()),
            None => break,
        };

        let mut advanced = Vec::new();
        let mut step_seen: HashSet<(usize, usize)> = HashSet::new();
        for t in threads {
            if prog.char_matches(&prog.insts[t.pc], c) {
                let mut nt = t;
                nt.pc += 1;
                if step_seen.insert((nt.pc, pos + clen)) {
                    advanced.push(nt);
                }
            }
        }
        threads = advanced;
    }
    best
}

fn is_better_match(a: &VmResult, b: &VmResult) -> bool {
    let (as_, ae) = (a.slots[0], a.slots[1]);
    let (bs, be) = (b.slots[0], b.slots[1]);
    as_ < bs || (as_ == bs && ae > be)
}

pub fn search(prog: &Program, hay: &str, at: usize) -> Option<VmResult> {
    let slot_count = (prog.num_groups as usize + 1) * 2;
    search_at(prog, hay, at, slot_count, at == hay.len())
}

fn search_at(
    prog: &Program,
    hay: &str,
    at: usize,
    slot_count: usize,
    at_end: bool,
) -> Option<VmResult> {
    run_at(prog, hay, at, slot_count, at_end)
}

fn run_at(
    prog: &Program,
    hay: &str,
    mut pos: usize,
    slot_count: usize,
    at_end: bool,
) -> Option<VmResult> {
    let mut threads = vec![Thread {
        pc: prog.start,
        slots: vec![usize::MAX; slot_count],
    }];
    threads[0].slots[0] = pos;
    threads[0].slots[1] = usize::MAX;

    let mut best: Option<VmResult> = None;

    loop {
        let mut ready = Vec::new();
        let mut seen: HashSet<(usize, usize)> = HashSet::new();
        for t in threads {
            epsilon(prog, hay, pos, t, &mut ready, &mut seen, at_end);
        }

        if let Some(t) = ready
            .iter()
            .find(|t| matches!(prog.insts.get(t.pc), Some(Inst::Match)))
            .cloned()
        {
            let mut slots = t.slots;
            if slots[1] == usize::MAX {
                slots[1] = pos;
            }
            let end = slots[1];
            if best
                .as_ref()
                .map(|b| b.slots.get(1).copied().unwrap_or(0))
                .unwrap_or(0)
                < end
            {
                best = Some(VmResult { slots });
            }
        }

        let consumable: Vec<Thread> = ready
            .into_iter()
            .filter(|t| {
                matches!(
                    prog.insts.get(t.pc),
                    Some(Inst::Char(_)) | Some(Inst::Any) | Some(Inst::Class(_))
                )
            })
            .collect();

        if consumable.is_empty() {
            return best;
        }

        if pos >= hay.len() {
            return best;
        }

        let (c, clen) = {
            let ch = hay[pos..].chars().next()?;
            (ch as u32, ch.len_utf8())
        };

        threads = Vec::new();
        let mut step_seen: HashSet<(usize, usize)> = HashSet::new();
        for t in consumable {
            if prog.char_matches(&prog.insts[t.pc], c) {
                let mut nt = t;
                nt.pc += 1;
                if step_seen.insert((nt.pc, pos + clen)) {
                    threads.push(nt);
                }
            }
        }

        if threads.is_empty() {
            return best;
        }

        if threads.len() > MAX_THREADS {
            threads.truncate(MAX_THREADS);
        }

        pos += clen;
    }
}

fn epsilon(
    prog: &Program,
    hay: &str,
    pos: usize,
    mut t: Thread,
    ready: &mut Vec<Thread>,
    seen: &mut HashSet<(usize, usize)>,
    at_end: bool,
) {
    if !seen.insert((t.pc, pos)) {
        return;
    }

    loop {
        match prog.insts.get(t.pc) {
            None => return,
            Some(Inst::Match) => {
                ready.push(t);
                return;
            }
            Some(Inst::Char(_)) | Some(Inst::Any) | Some(Inst::Class(_)) => {
                ready.push(t);
                return;
            }
            Some(Inst::Jmp(dst)) => t.pc = *dst,
            Some(Inst::Split { x, y }) => {
                let mut a = t.clone();
                a.pc = *x;
                epsilon(prog, hay, pos, a, ready, seen, at_end);
                t.pc = *y;
            }
            Some(Inst::Save(i)) => {
                if *i < t.slots.len() {
                    t.slots[*i] = pos;
                }
                t.pc += 1;
            }
            Some(Inst::Bol) => {
                let ok = pos == 0
                    || (prog.flags.multiline
                        && hay.as_bytes().get(pos.saturating_sub(1)) == Some(&b'\n'));
                if !ok {
                    return;
                }
                t.pc += 1;
            }
            Some(Inst::Eol) => {
                let ok = pos == hay.len()
                    || (prog.flags.multiline && hay.as_bytes().get(pos) == Some(&b'\n'));
                if !ok && !(at_end && pos == hay.len()) {
                    return;
                }
                t.pc += 1;
            }
            Some(Inst::WordBoundary(neg)) => {
                let prev = if pos == 0 { false } else { prev_word(hay, pos) };
                let next = hay[pos..]
                    .chars()
                    .next()
                    .map(|c| prog.is_word_char(c as u32))
                    .unwrap_or(false);
                if (prev != next) != *neg {
                    return;
                }
                t.pc += 1;
            }
        }
    }
}

fn prev_word(hay: &str, pos: usize) -> bool {
    hay[..pos]
        .chars()
        .next_back()
        .map(|c| crate::parse::is_word_char(c as u32))
        .unwrap_or(false)
}

pub fn slots_to_ranges(slots: &[usize]) -> Vec<Option<(usize, usize)>> {
    let mut groups = Vec::new();
    let mut i = 0;
    while i + 1 < slots.len() {
        let s = slots[i];
        let e = slots[i + 1];
        if s == usize::MAX || e == usize::MAX || s > e {
            groups.push(None);
        } else {
            groups.push(Some((s, e)));
        }
        i += 2;
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nfa::Compiler;
    use crate::parse::{normalize_ast, parse, Ast};

    fn prog(pat: &str) -> Program {
        let (ast, flags) = parse(pat).unwrap();
        let ast = normalize_ast(ast);
        let groups = max_group(&ast);
        Compiler::new(flags, groups).compile(&ast)
    }

    fn max_group(ast: &Ast) -> u32 {
        match ast {
            Ast::Cap { index, .. } => *index,
            Ast::Concat(v) | Ast::Alt(v) => v.iter().map(max_group).max().unwrap_or(0),
            Ast::Quant { inner, .. } => max_group(inner),
            Ast::NoCap(inner) => max_group(inner),
            _ => 0,
        }
    }

    #[test]
    fn linear_on_pathological() {
        let p = prog(r"(a+)+b");
        let s = "a".repeat(5000) + "c";
        let start = std::time::Instant::now();
        assert!(find(&p, &s).is_none());
        assert!(start.elapsed().as_millis() < 500);
    }

    #[test]
    #[ignore = "v1: capture group boundaries"]
    fn captures_email() {
        let p = prog(r"(\w+)@(\w+)");
        let r = find(&p, "alice@example").unwrap();
        let g = slots_to_ranges(&r.slots);
        assert_eq!(g[0].unwrap(), (0, 13));
        assert_eq!(g[1].unwrap(), (0, 5));
        assert_eq!(g[2].unwrap(), (6, 13));
    }

    #[test]
    fn literal_hello() {
        let p = prog("hello");
        assert!(find(&p, "hello").is_some());
    }
}
