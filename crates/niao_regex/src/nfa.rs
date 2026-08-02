use crate::flags::Flags;
use crate::parse::{fold_case, is_word_char, Ast, Class};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inst {
    Char(u32),
    Any,
    Class(Class),
    Match,
    Jmp(usize),
    Split { x: usize, y: usize },
    Save(usize),
    Bol,
    Eol,
    WordBoundary(bool),
}

pub struct Compiler {
    insts: Vec<Inst>,
    flags: Flags,
    num_groups: u32,
}

struct Frag {
    start: usize,
    out: Vec<usize>,
}

impl Compiler {
    pub fn new(flags: Flags, num_groups: u32) -> Self {
        Self {
            insts: Vec::new(),
            flags,
            num_groups,
        }
    }

    pub fn compile(mut self, ast: &Ast) -> Program {
        let frag = self.compile_ast(ast);
        let match_pc = self.emit(Inst::Match);
        self.patch(frag.out, match_pc);
        Program {
            insts: self.insts,
            start: frag.start,
            flags: self.flags,
            num_groups: self.num_groups,
        }
    }

    fn compile_ast(&mut self, ast: &Ast) -> Frag {
        match ast {
            Ast::Empty => {
                let jmp = self.emit(Inst::Jmp(0));
                Frag {
                    start: jmp,
                    out: vec![jmp],
                }
            }
            Ast::Literal(c) => {
                let pc = self.emit(Inst::Char(*c));
                Frag {
                    start: pc,
                    out: vec![],
                }
            }
            Ast::Dot => {
                let pc = self.emit(Inst::Any);
                Frag {
                    start: pc,
                    out: vec![],
                }
            }
            Ast::Class(class) => {
                let pc = self.emit(Inst::Class(class.clone()));
                Frag {
                    start: pc,
                    out: vec![],
                }
            }
            Ast::AnchorStart => {
                let pc = self.emit(Inst::Bol);
                Frag {
                    start: pc,
                    out: vec![],
                }
            }
            Ast::AnchorEnd => {
                let pc = self.emit(Inst::Eol);
                Frag {
                    start: pc,
                    out: vec![],
                }
            }
            Ast::WordBoundary(neg) => {
                let pc = self.emit(Inst::WordBoundary(*neg));
                Frag {
                    start: pc,
                    out: vec![],
                }
            }
            Ast::Concat(parts) => {
                let mut it = parts.iter();
                let Some(first) = it.next() else {
                    return self.compile_ast(&Ast::Empty);
                };
                let mut frag = self.compile_ast(first);
                for p in it {
                    let next = self.compile_ast(p);
                    frag = self.concat(frag, next);
                }
                frag
            }
            Ast::Alt(alts) => {
                if alts.is_empty() {
                    return self.compile_ast(&Ast::Empty);
                }
                if alts.len() == 1 {
                    return self.compile_ast(&alts[0]);
                }
                let split = self.emit(Inst::Split { x: 0, y: 0 });
                let left = self.compile_ast(&alts[0]);
                let right = self.compile_alt_tail(&alts[1..]);
                self.insts[split] = Inst::Split {
                    x: left.start,
                    y: right.start,
                };
                let mut out = left.out;
                out.extend(right.out);
                Frag { start: split, out }
            }
            Ast::Quant {
                inner,
                min,
                max,
                greedy,
            } => self.compile_quant(inner, *min, *max, *greedy),
            Ast::Cap { inner, index } => {
                let open = self.emit(Inst::Save((index * 2) as usize));
                let body = self.compile_ast(inner);
                let close = self.emit(Inst::Save((index * 2 + 1) as usize));
                let inner_frag = self.concat(
                    Frag {
                        start: open,
                        out: vec![],
                    },
                    body,
                );
                self.concat(
                    inner_frag,
                    Frag {
                        start: close,
                        out: vec![],
                    },
                )
            }
            Ast::NoCap(inner) => self.compile_ast(inner),
        }
    }

    fn compile_alt_tail(&mut self, alts: &[Ast]) -> Frag {
        if alts.len() == 1 {
            return self.compile_ast(&alts[0]);
        }
        let split = self.emit(Inst::Split { x: 0, y: 0 });
        let left = self.compile_ast(&alts[0]);
        let right = self.compile_alt_tail(&alts[1..]);
        self.insts[split] = Inst::Split {
            x: left.start,
            y: right.start,
        };
        let mut out = left.out;
        out.extend(right.out);
        Frag { start: split, out }
    }

    fn compile_quant(&mut self, inner: &Ast, min: u32, max: Option<u32>, greedy: bool) -> Frag {
        let max = max.unwrap_or(u32::MAX);
        if min == 0 && max == 0 {
            return self.compile_ast(&Ast::Empty);
        }
        if min == 1 && max == 1 {
            return self.compile_ast(inner);
        }

        if max != u32::MAX {
            let mut frag = if min == 0 {
                self.compile_ast(&Ast::Empty)
            } else {
                let mut f = self.compile_ast(inner);
                for _ in 1..min {
                    let next = self.compile_ast(inner);
                    f = self.concat(f, next);
                }
                f
            };
            for _ in min..max {
                let opt = self.optional_repeat(inner, greedy);
                frag = self.concat(frag, opt);
            }
            return frag;
        }

        self.optional_repeat_star(inner, min, greedy)
    }

    fn optional_repeat(&mut self, inner: &Ast, greedy: bool) -> Frag {
        let body = self.compile_ast(inner);
        let tail = self.emit(Inst::Jmp(0));
        let body = self.concat(
            body,
            Frag {
                start: tail,
                out: vec![tail],
            },
        );
        let split = self.emit(Inst::Split { x: 0, y: 0 });
        let exit = self.emit(Inst::Jmp(0));
        if greedy {
            self.insts[split] = Inst::Split {
                x: body.start,
                y: exit,
            };
        } else {
            self.insts[split] = Inst::Split {
                x: exit,
                y: body.start,
            };
        }
        self.patch(body.out, split);
        Frag {
            start: split,
            out: vec![exit],
        }
    }

    fn optional_repeat_star(&mut self, inner: &Ast, min: u32, greedy: bool) -> Frag {
        let body = self.compile_ast(inner);
        let tail = self.emit(Inst::Jmp(0));
        let body = self.concat(
            body,
            Frag {
                start: tail,
                out: vec![tail],
            },
        );
        let split = self.emit(Inst::Split { x: 0, y: 0 });
        let exit = self.emit(Inst::Jmp(0));
        if greedy {
            self.insts[split] = Inst::Split {
                x: body.start,
                y: exit,
            };
        } else {
            self.insts[split] = Inst::Split {
                x: exit,
                y: body.start,
            };
        }
        self.patch(body.out, split);

        if min == 0 {
            Frag {
                start: split,
                out: vec![exit],
            }
        } else {
            Frag {
                start: body.start,
                out: vec![exit],
            }
        }
    }

    fn concat(&mut self, a: Frag, b: Frag) -> Frag {
        self.patch(a.out, b.start);
        Frag {
            start: a.start,
            out: b.out,
        }
    }

    fn emit(&mut self, inst: Inst) -> usize {
        let pc = self.insts.len();
        self.insts.push(inst);
        pc
    }

    fn patch(&mut self, mut out: Vec<usize>, target: usize) {
        while let Some(pc) = out.pop() {
            match &mut self.insts[pc] {
                Inst::Jmp(ref mut d) => *d = target,
                Inst::Split { ref mut x, y: 0 } if *x == 0 => *x = target,
                Inst::Split { x: 0, ref mut y } => *y = target,
                Inst::Split { ref mut x, .. } if *x == 0 => *x = target,
                Inst::Split { x: _, ref mut y } => *y = target,
                _ => {}
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Program {
    pub insts: Vec<Inst>,
    pub start: usize,
    pub flags: Flags,
    pub num_groups: u32,
}

impl Program {
    pub fn char_matches(&self, inst: &Inst, c: u32) -> bool {
        match inst {
            Inst::Char(ch) => {
                if self.flags.case_insensitive {
                    fold_case(*ch) == fold_case(c)
                } else {
                    *ch == c
                }
            }
            Inst::Any => c != b'\n' as u32 || self.flags.dot_all,
            Inst::Class(class) => class.matches(c, self.flags),
            _ => false,
        }
    }

    pub fn is_word_char(&self, c: u32) -> bool {
        is_word_char(c)
    }
}
