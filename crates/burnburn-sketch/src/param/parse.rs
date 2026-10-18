//! Lexer, recursive-descent parser and dimension-checked evaluator for parameter expressions.
//!
//! Grammar (no implicit multiplication; a name right after a number is its unit):
//! ```text
//! expr    := term (('+' | '-') term)*
//! term    := unary (('*' | '/') unary)*
//! unary   := '-' unary | primary
//! primary := number unit? | name | name '(' expr (',' expr)* ')' | '(' expr ')'
//! ```

use super::{Dim, ExprError, ExprErrorKind, FUNCTIONS, Quantity, Span, unit};

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Number(f64),
    Name(String),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
    Comma,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Literal { value: Quantity, span: Span },
    Param { name: String, span: Span },
    Neg { inner: Box<Node>, span: Span },
    Bin { op: BinOp, left: Box<Node>, right: Box<Node>, op_span: Span },
    Call { function: &'static str, args: Vec<Node>, name_span: Span, span: Span },
}

impl Node {
    fn span(&self) -> Span {
        match self {
            Node::Literal { span, .. }
            | Node::Param { span, .. }
            | Node::Neg { span, .. }
            | Node::Call { span, .. } => *span,
            Node::Bin { left, right, .. } => left.span().to(right.span()),
        }
    }
}

/// A parsed expression. Spans in its errors are byte positions in the parsed text, shifted by
/// the offset it was parsed at (so errors of `name = expr` point into the whole line).
#[derive(Debug, Clone, PartialEq)]
pub struct Expression {
    text: String,
    root: Node,
}

fn err(kind: ExprErrorKind, span: Span) -> ExprError {
    ExprError { kind, span }
}

fn lex(text: &str, offset: usize) -> Result<Vec<(Tok, Span)>, ExprError> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        let start = i;
        let single = |t: Tok| (t, Span::new(start + offset, start + 1 + offset));
        match c {
            b' ' | b'\t' => i += 1,
            b'+' => {
                out.push(single(Tok::Plus));
                i += 1;
            }
            b'-' => {
                out.push(single(Tok::Minus));
                i += 1;
            }
            b'*' => {
                out.push(single(Tok::Star));
                i += 1;
            }
            b'/' => {
                out.push(single(Tok::Slash));
                i += 1;
            }
            b'(' => {
                out.push(single(Tok::LParen));
                i += 1;
            }
            b')' => {
                out.push(single(Tok::RParen));
                i += 1;
            }
            b',' => {
                out.push(single(Tok::Comma));
                i += 1;
            }
            b'0'..=b'9' | b'.' => {
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                if i < bytes.len() && bytes[i] == b'.' {
                    i += 1;
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                // Exponent only when digits follow, so `2e` stays "2" + unit "e".
                if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
                    let mut j = i + 1;
                    if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
                        j += 1;
                    }
                    if j < bytes.len() && bytes[j].is_ascii_digit() {
                        while j < bytes.len() && bytes[j].is_ascii_digit() {
                            j += 1;
                        }
                        i = j;
                    }
                }
                let span = Span::new(start + offset, i + offset);
                let value: f64 = text
                    .get(start..i)
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| err(ExprErrorKind::UnexpectedToken, span))?;
                out.push((Tok::Number(value), span));
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                out.push((Tok::Name(text[start..i].to_owned()), Span::new(start + offset, i + offset)));
            }
            _ => {
                let ch = text[start..].chars().next().unwrap_or('\u{fffd}');
                let end = start + ch.len_utf8();
                return Err(err(ExprErrorKind::UnexpectedCharacter(ch), Span::new(start + offset, end + offset)));
            }
        }
    }
    Ok(out)
}

struct Parser {
    toks: Vec<(Tok, Span)>,
    pos: usize,
    end: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|(t, _)| t)
    }

    fn next(&mut self) -> Result<(Tok, Span), ExprError> {
        let t = self.toks.get(self.pos).cloned().ok_or_else(|| self.at_end())?;
        self.pos += 1;
        Ok(t)
    }

    fn at_end(&self) -> ExprError {
        err(ExprErrorKind::UnexpectedEnd, Span::new(self.end, self.end))
    }

    fn expr(&mut self) -> Result<Node, ExprError> {
        let mut left = self.term()?;
        while let Some(op) = match self.peek() {
            Some(Tok::Plus) => Some(BinOp::Add),
            Some(Tok::Minus) => Some(BinOp::Sub),
            _ => None,
        } {
            let (_, op_span) = self.next()?;
            let right = self.term()?;
            left = Node::Bin { op, left: Box::new(left), right: Box::new(right), op_span };
        }
        Ok(left)
    }

    fn term(&mut self) -> Result<Node, ExprError> {
        let mut left = self.unary()?;
        while let Some(op) = match self.peek() {
            Some(Tok::Star) => Some(BinOp::Mul),
            Some(Tok::Slash) => Some(BinOp::Div),
            _ => None,
        } {
            let (_, op_span) = self.next()?;
            let right = self.unary()?;
            left = Node::Bin { op, left: Box::new(left), right: Box::new(right), op_span };
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Node, ExprError> {
        if self.peek() == Some(&Tok::Minus) {
            let (_, s) = self.next()?;
            let inner = self.unary()?;
            let span = s.to(inner.span());
            return Ok(Node::Neg { inner: Box::new(inner), span });
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<Node, ExprError> {
        let (tok, span) = self.next()?;
        match tok {
            Tok::Number(n) => {
                if let Some((Tok::Name(u), us)) = self.toks.get(self.pos).cloned() {
                    let (factor, dim) = unit(&u).ok_or_else(|| err(ExprErrorKind::UnknownUnit(u.clone()), us))?;
                    self.pos += 1;
                    let span = span.to(us);
                    return literal(n * factor, dim, span);
                }
                literal(n, Dim::NONE, span)
            }
            Tok::Name(name) => {
                if self.peek() == Some(&Tok::LParen) {
                    let function = FUNCTIONS
                        .iter()
                        .copied()
                        .find(|f| *f == name)
                        .ok_or_else(|| err(ExprErrorKind::UnknownFunction(name.clone()), span))?;
                    self.next()?;
                    let mut args = vec![self.expr()?];
                    loop {
                        let (t, s) = self.next()?;
                        match t {
                            Tok::Comma => args.push(self.expr()?),
                            Tok::RParen => {
                                return Ok(Node::Call { function, args, name_span: span, span: span.to(s) });
                            }
                            _ => return Err(err(ExprErrorKind::UnexpectedToken, s)),
                        }
                    }
                }
                Ok(Node::Param { name, span })
            }
            Tok::LParen => {
                let inner = self.expr()?;
                match self.next()? {
                    (Tok::RParen, _) => Ok(inner),
                    (_, s) => Err(err(ExprErrorKind::UnexpectedToken, s)),
                }
            }
            _ => Err(err(ExprErrorKind::UnexpectedToken, span)),
        }
    }
}

fn literal(value: f64, dim: Dim, span: Span) -> Result<Node, ExprError> {
    if !value.is_finite() {
        return Err(err(ExprErrorKind::NotFinite, span));
    }
    Ok(Node::Literal { value: Quantity { value, dim }, span })
}

impl Expression {
    /// Parses `text`; error spans are positions in `text`.
    pub fn parse(text: &str) -> Result<Expression, ExprError> {
        Self::parse_at(text, 0)
    }

    /// Parses `text` that starts at byte `offset` of a larger line.
    pub(crate) fn parse_at(text: &str, offset: usize) -> Result<Expression, ExprError> {
        let toks = lex(text, offset)?;
        let mut p = Parser { toks, pos: 0, end: text.len() + offset };
        let root = p.expr()?;
        if let Some((_, s)) = p.toks.get(p.pos) {
            return Err(err(ExprErrorKind::UnexpectedToken, *s));
        }
        Ok(Expression { text: text.to_owned(), root })
    }

    /// The source text.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Parameter names used, in order of first appearance.
    pub fn references(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        collect(&self.root, &mut out);
        out
    }

    /// Evaluates with `lookup` for parameter values (canonical quantities).
    pub fn evaluate(&self, lookup: &dyn Fn(&str) -> Option<Quantity>) -> Result<Quantity, ExprError> {
        let q = eval(&self.root, lookup)?;
        if !q.value.is_finite() {
            return Err(err(ExprErrorKind::NotFinite, self.root.span()));
        }
        Ok(q)
    }

    /// The whole expression's span.
    pub fn span(&self) -> Span {
        self.root.span()
    }
}

fn collect<'a>(n: &'a Node, out: &mut Vec<&'a str>) {
    match n {
        Node::Literal { .. } => {}
        Node::Param { name, .. } => {
            if !out.contains(&name.as_str()) {
                out.push(name);
            }
        }
        Node::Neg { inner, .. } => collect(inner, out),
        Node::Bin { left, right, .. } => {
            collect(left, out);
            collect(right, out);
        }
        Node::Call { args, .. } => args.iter().for_each(|a| collect(a, out)),
    }
}

fn eval(n: &Node, lookup: &dyn Fn(&str) -> Option<Quantity>) -> Result<Quantity, ExprError> {
    match n {
        Node::Literal { value, .. } => Ok(*value),
        Node::Param { name, span } => {
            lookup(name).ok_or_else(|| err(ExprErrorKind::UnknownParameter(name.clone()), *span))
        }
        Node::Neg { inner, .. } => {
            let q = eval(inner, lookup)?;
            Ok(Quantity { value: -q.value, dim: q.dim })
        }
        Node::Bin { op, left, right, op_span } => {
            let (a, b) = (eval(left, lookup)?, eval(right, lookup)?);
            let mismatch = || err(ExprErrorKind::UnitMismatch { left: a.dim, right: b.dim }, *op_span);
            let dim = |l: i8, r: i8, sign: i8| l.checked_add(sign * r).ok_or_else(mismatch);
            let q = match op {
                BinOp::Add | BinOp::Sub => {
                    if a.dim != b.dim {
                        return Err(mismatch());
                    }
                    let value = if *op == BinOp::Add { a.value + b.value } else { a.value - b.value };
                    Quantity { value, dim: a.dim }
                }
                BinOp::Mul => Quantity {
                    value: a.value * b.value,
                    dim: Dim { length: dim(a.dim.length, b.dim.length, 1)?, angle: dim(a.dim.angle, b.dim.angle, 1)? },
                },
                BinOp::Div => {
                    if b.value == 0.0 {
                        return Err(err(ExprErrorKind::DivisionByZero, *op_span));
                    }
                    Quantity {
                        value: a.value / b.value,
                        dim: Dim {
                            length: dim(a.dim.length, b.dim.length, -1)?,
                            angle: dim(a.dim.angle, b.dim.angle, -1)?,
                        },
                    }
                }
            };
            if !q.value.is_finite() {
                return Err(err(ExprErrorKind::NotFinite, *op_span));
            }
            Ok(q)
        }
        Node::Call { function, args, span, .. } => {
            let vals = args.iter().map(|a| eval(a, lookup)).collect::<Result<Vec<_>, _>>()?;
            match *function {
                "sqrt" => {
                    let [q] = vals.as_slice() else {
                        return Err(arity(function, "1", vals.len(), *span));
                    };
                    if q.dim.length % 2 != 0 || q.dim.angle % 2 != 0 {
                        return Err(err(ExprErrorKind::OddDimensionSqrt(q.dim), *span));
                    }
                    if q.value < 0.0 {
                        return Err(err(ExprErrorKind::NegativeSqrt, *span));
                    }
                    Ok(Quantity {
                        value: q.value.sqrt(),
                        dim: Dim { length: q.dim.length / 2, angle: q.dim.angle / 2 },
                    })
                }
                _ => {
                    // min / max: two or more arguments of one dimension.
                    let Some((first, rest)) = vals.split_first().filter(|_| vals.len() >= 2) else {
                        return Err(arity(function, "2 or more", vals.len(), *span));
                    };
                    let mut best = *first;
                    for (q, node) in rest.iter().zip(&args[1..]) {
                        if q.dim != best.dim {
                            return Err(err(ExprErrorKind::UnitMismatch { left: best.dim, right: q.dim }, node.span()));
                        }
                        let better = if *function == "min" { q.value < best.value } else { q.value > best.value };
                        if better {
                            best = *q;
                        }
                    }
                    Ok(best)
                }
            }
        }
    }
}

fn arity(function: &'static str, expected: &'static str, found: usize, span: Span) -> ExprError {
    err(ExprErrorKind::WrongArgumentCount { function, expected, found }, span)
}
