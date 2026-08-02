use crate::ast::{BinOp, Compiled, Expr, UnaryOp};
use crate::error::ExprError;
use crate::lex::{Lexer, Token};
use crate::value::str_key;
use std::sync::Arc;

pub fn parse(source: &str) -> Result<Compiled, ExprError> {
    let tokens = Lexer::new(source).tokenize()?;
    let mut p = Parser { tokens, idx: 0 };
    let expr = p.parse_expr()?;
    p.expect(Token::Eof)?;
    let mut names = Vec::new();
    expr.collect_names(&mut names);
    Ok(Compiled {
        source: Arc::from(source),
        expr,
        names,
    })
}

pub fn valid(source: &str) -> bool {
    parse(source).is_ok()
}

struct Parser {
    tokens: Vec<(usize, Token)>,
    idx: usize,
}

impl Parser {
    fn peek(&self) -> &(usize, Token) {
        &self.tokens[self.idx]
    }

    fn bump(&mut self) -> (usize, Token) {
        let t = self.tokens[self.idx].clone();
        if !matches!(t.1, Token::Eof) {
            self.idx += 1;
        }
        t
    }

    fn at(&self, tok: &Token) -> bool {
        std::mem::discriminant(&self.peek().1) == std::mem::discriminant(tok)
    }

    fn expect(&mut self, tok: Token) -> Result<(), ExprError> {
        let (pos, got) = self.bump();
        if std::mem::discriminant(&got) != std::mem::discriminant(&tok) {
            return Err(ExprError::Parse {
                pos,
                message: format!("expected {:?}, got {:?}", tok, got),
            });
        }
        Ok(())
    }

    fn parse_expr(&mut self) -> Result<Expr, ExprError> {
        self.parse_ternary()
    }

    fn parse_ternary(&mut self) -> Result<Expr, ExprError> {
        let then_e = self.parse_or()?;
        if matches!(self.peek().1, Token::KwIf) {
            self.bump();
            let cond = self.parse_or()?;
            self.expect(Token::KwElse)?;
            let else_e = self.parse_expr()?;
            return Ok(Expr::Ternary(
                Box::new(then_e),
                Box::new(cond),
                Box::new(else_e),
            ));
        }
        Ok(then_e)
    }

    fn parse_or(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.parse_and()?;
        loop {
            match self.peek().1 {
                Token::KwOr | Token::PipePipe => {
                    self.bump();
                    let right = self.parse_and()?;
                    left = Expr::Binary(BinOp::Or, Box::new(left), Box::new(right));
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.parse_not()?;
        loop {
            match self.peek().1 {
                Token::KwAnd | Token::AmpAmp => {
                    self.bump();
                    let right = self.parse_not()?;
                    left = Expr::Binary(BinOp::And, Box::new(left), Box::new(right));
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_not(&mut self) -> Result<Expr, ExprError> {
        match self.peek().1 {
            Token::KwNot | Token::Bang => {
                self.bump();
                let inner = self.parse_not()?;
                Ok(Expr::Unary(UnaryOp::Not, Box::new(inner)))
            }
            _ => self.parse_compare(),
        }
    }

    fn parse_compare(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.parse_add()?;
        loop {
            let op = match self.peek().1 {
                Token::Eq => BinOp::Eq,
                Token::NotEq => BinOp::NotEq,
                Token::Lt => BinOp::Lt,
                Token::Gt => BinOp::Gt,
                Token::Le => BinOp::Le,
                Token::Ge => BinOp::Ge,
                Token::KwIn => BinOp::In,
                _ => break,
            };
            self.bump();
            let right = self.parse_add()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_add(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.parse_mul()?;
        loop {
            let op = match self.peek().1 {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.bump();
            let right = self.parse_mul()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_mul(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek().1 {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
                Token::FloorDiv => BinOp::FloorDiv,
                Token::Percent => BinOp::Mod,
                _ => break,
            };
            self.bump();
            let right = self.parse_unary()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, ExprError> {
        if matches!(self.peek().1, Token::Minus) {
            self.bump();
            let inner = self.parse_unary()?;
            return Ok(Expr::Unary(UnaryOp::Neg, Box::new(inner)));
        }
        self.parse_pow()
    }

    fn parse_pow(&mut self) -> Result<Expr, ExprError> {
        let mut left = self.parse_postfix()?;
        if matches!(self.peek().1, Token::Pow) {
            self.bump();
            let right = self.parse_unary()?;
            left = Expr::Binary(BinOp::Pow, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_postfix(&mut self) -> Result<Expr, ExprError> {
        let mut expr = self.parse_primary()?;
        loop {
            match self.peek().1 {
                Token::Dot => {
                    self.bump();
                    let (pos, tok) = self.bump();
                    let Token::Ident(name) = tok else {
                        return Err(ExprError::Parse {
                            pos,
                            message: "expected attribute name after '.'".into(),
                        });
                    };
                    expr = Expr::Attr(Box::new(expr), str_key(&name));
                }
                Token::LBracket => {
                    self.bump();
                    let idx = self.parse_expr()?;
                    self.expect(Token::RBracket)?;
                    expr = Expr::Index(Box::new(expr), Box::new(idx));
                }
                Token::LParen => {
                    self.bump();
                    let args = self.parse_args()?;
                    expr = Expr::Call(Box::new(expr), args);
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, ExprError> {
        let (pos, tok) = self.bump();
        match tok {
            Token::Int(n) => Ok(Expr::Int(n)),
            Token::Float(f) => Ok(Expr::Float(f)),
            Token::String(s) => Ok(Expr::String(Arc::from(s.as_str()))),
            Token::True => Ok(Expr::Bool(true)),
            Token::False => Ok(Expr::Bool(false)),
            Token::Nil => Ok(Expr::Nil),
            Token::Ident(name) => Ok(Expr::Name(str_key(&name))),
            Token::LParen => {
                let inner = self.parse_expr()?;
                self.expect(Token::RParen)?;
                Ok(inner)
            }
            Token::LBracket => {
                let mut items = Vec::new();
                if !matches!(self.peek().1, Token::RBracket) {
                    loop {
                        items.push(self.parse_expr()?);
                        if matches!(self.peek().1, Token::Comma) {
                            self.bump();
                            if matches!(self.peek().1, Token::RBracket) {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                }
                self.expect(Token::RBracket)?;
                Ok(Expr::Array(items))
            }
            Token::LBrace => {
                let mut pairs = Vec::new();
                if !matches!(self.peek().1, Token::RBrace) {
                    loop {
                        let key = self.parse_key()?;
                        self.expect(Token::Colon)?;
                        let val = self.parse_expr()?;
                        pairs.push((key, val));
                        if matches!(self.peek().1, Token::Comma) {
                            self.bump();
                            if matches!(self.peek().1, Token::RBrace) {
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                }
                self.expect(Token::RBrace)?;
                Ok(Expr::Object(pairs))
            }
            _ => Err(ExprError::Parse {
                pos,
                message: format!("unexpected token {:?}", tok),
            }),
        }
    }

    fn parse_key(&mut self) -> Result<Arc<str>, ExprError> {
        let (pos, tok) = self.bump();
        match tok {
            Token::Ident(s) => Ok(str_key(&s)),
            Token::String(s) => Ok(Arc::from(s.as_str())),
            _ => Err(ExprError::Parse {
                pos,
                message: "expected object key".into(),
            }),
        }
    }

    fn parse_args(&mut self) -> Result<Vec<Expr>, ExprError> {
        let mut args = Vec::new();
        if !matches!(self.peek().1, Token::RParen) {
            loop {
                args.push(self.parse_expr()?);
                if matches!(self.peek().1, Token::Comma) {
                    self.bump();
                    if matches!(self.peek().1, Token::RParen) {
                        break;
                    }
                } else {
                    break;
                }
            }
        }
        self.expect(Token::RParen)?;
        Ok(args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ternary_python_style() {
        let c = parse("10 if x > 5 else 0").unwrap();
        assert!(matches!(c.expr, Expr::Ternary(_, _, _)));
    }

    #[test]
    fn parse_call_and_attr() {
        let c = parse("foo.bar(1, 2)").unwrap();
        assert!(matches!(c.expr, Expr::Call(_, _)));
    }
}
