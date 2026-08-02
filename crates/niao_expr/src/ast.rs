use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    FloorDiv,
    Mod,
    Pow,
    Eq,
    NotEq,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
    In,
}

#[derive(Debug, Clone)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(Arc<str>),
    Array(Vec<Expr>),
    Object(Vec<(Arc<str>, Expr)>),
    Name(Arc<str>),
    Unary(UnaryOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    Attr(Box<Expr>, Arc<str>),
    Index(Box<Expr>, Box<Expr>),
}

impl Expr {
    pub fn collect_names(&self, out: &mut Vec<Arc<str>>) {
        match self {
            Expr::Name(n) => {
                if !out.iter().any(|x| x.as_ref() == n.as_ref()) {
                    out.push(Arc::clone(n));
                }
            }
            Expr::Unary(_, e) => e.collect_names(out),
            Expr::Binary(_, a, b) => {
                a.collect_names(out);
                b.collect_names(out);
            }
            Expr::Ternary(c, t, f) => {
                c.collect_names(out);
                t.collect_names(out);
                f.collect_names(out);
            }
            Expr::Call(c, args) => {
                c.collect_names(out);
                args.iter().for_each(|a| a.collect_names(out));
            }
            Expr::Attr(e, _) => e.collect_names(out),
            Expr::Index(a, b) => {
                a.collect_names(out);
                b.collect_names(out);
            }
            Expr::Array(es) => es.iter().for_each(|e| e.collect_names(out)),
            Expr::Object(pairs) => pairs.iter().for_each(|(_, e)| e.collect_names(out)),
            _ => {}
        }
    }
}

#[derive(Debug, Clone)]
pub struct Compiled {
    pub source: Arc<str>,
    pub expr: Expr,
    pub names: Vec<Arc<str>>,
}
