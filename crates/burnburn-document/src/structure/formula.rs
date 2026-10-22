//! Formulas in the numbers of part shapes (G8-editor, S07): `breedte / 2 + 3` over the parameters
//! of the part. Numbers, names, `+ - * /`, brackets and a sign; a unit `mm` after a number is
//! ignored, a decimal comma is a decimal point.

use std::collections::BTreeMap;
use std::fmt;

use burnburn_types::Length;

/// Why a formula is not a formula, or has no value.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FormulaError {
    /// The text is not a formula.
    Syntax,
    /// A name that is not a parameter of the part.
    UnknownName(String),
    DivisionByZero,
}

impl fmt::Display for FormulaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormulaError::Syntax => f.write_str("not a formula"),
            FormulaError::UnknownName(n) => write!(f, "no parameter {n}"),
            FormulaError::DivisionByZero => f.write_str("division by zero"),
        }
    }
}

impl std::error::Error for FormulaError {}

/// A parsed formula.
#[derive(Debug, Clone, PartialEq)]
pub struct Formula {
    root: Node,
}

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Number(f64),
    Name(String),
    Neg(Box<Node>),
    Binary(Box<Node>, char, Box<Node>),
}

impl Formula {
    /// The names the formula uses, in order of first appearance, each once.
    pub fn names(&self) -> Vec<String> {
        fn walk(n: &Node, out: &mut Vec<String>) {
            match n {
                Node::Number(_) => {}
                Node::Name(name) => {
                    if !out.contains(name) {
                        out.push(name.clone());
                    }
                }
                Node::Neg(inner) => walk(inner, out),
                Node::Binary(l, _, r) => {
                    walk(l, out);
                    walk(r, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.root, &mut out);
        out
    }

    /// The value in millimetres with the parameters `values`.
    pub fn eval(&self, values: &BTreeMap<String, Length>) -> Result<f64, FormulaError> {
        fn go(n: &Node, values: &BTreeMap<String, Length>) -> Result<f64, FormulaError> {
            Ok(match n {
                Node::Number(v) => *v,
                Node::Name(name) => values.get(name).ok_or_else(|| FormulaError::UnknownName(name.clone()))?.mm(),
                Node::Neg(inner) => -go(inner, values)?,
                Node::Binary(l, op, r) => {
                    let (a, b) = (go(l, values)?, go(r, values)?);
                    match op {
                        '+' => a + b,
                        '-' => a - b,
                        '*' => a * b,
                        _ => {
                            if b == 0.0 {
                                return Err(FormulaError::DivisionByZero);
                            }
                            a / b
                        }
                    }
                }
            })
        }
        let v = go(&self.root, values)?;
        if v.is_finite() { Ok(v) } else { Err(FormulaError::Syntax) }
    }
}

/// Parses `text` as a formula.
pub fn parse_formula(text: &str) -> Result<Formula, FormulaError> {
    let mut p = Parser { chars: text.chars().collect(), at: 0 };
    let root = p.expression()?;
    p.skip_space();
    if p.at != p.chars.len() {
        return Err(FormulaError::Syntax);
    }
    Ok(Formula { root })
}

struct Parser {
    chars: Vec<char>,
    at: usize,
}

impl Parser {
    fn skip_space(&mut self) {
        while self.chars.get(self.at).is_some_and(|c| c.is_whitespace()) {
            self.at += 1;
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.skip_space();
        self.chars.get(self.at).copied()
    }

    fn expression(&mut self) -> Result<Node, FormulaError> {
        let mut left = self.term()?;
        while let Some(op @ ('+' | '-')) = self.peek() {
            self.at += 1;
            let right = self.term()?;
            left = Node::Binary(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    fn term(&mut self) -> Result<Node, FormulaError> {
        let mut left = self.factor()?;
        while let Some(op @ ('*' | '/')) = self.peek() {
            self.at += 1;
            let right = self.factor()?;
            left = Node::Binary(Box::new(left), op, Box::new(right));
        }
        Ok(left)
    }

    fn factor(&mut self) -> Result<Node, FormulaError> {
        match self.peek().ok_or(FormulaError::Syntax)? {
            '-' => {
                self.at += 1;
                Ok(Node::Neg(Box::new(self.factor()?)))
            }
            '+' => {
                self.at += 1;
                self.factor()
            }
            '(' => {
                self.at += 1;
                let inner = self.expression()?;
                if self.peek() != Some(')') {
                    return Err(FormulaError::Syntax);
                }
                self.at += 1;
                Ok(inner)
            }
            c if c.is_ascii_digit() || c == '.' || c == ',' => self.number(),
            c if c.is_alphabetic() || c == '_' => {
                let start = self.at;
                while self.chars.get(self.at).is_some_and(|c| c.is_alphanumeric() || *c == '_') {
                    self.at += 1;
                }
                Ok(Node::Name(self.chars[start..self.at].iter().collect()))
            }
            _ => Err(FormulaError::Syntax),
        }
    }

    fn number(&mut self) -> Result<Node, FormulaError> {
        let start = self.at;
        let mut dots = 0;
        while let Some(c) = self.chars.get(self.at).copied() {
            if c.is_ascii_digit() {
                self.at += 1;
            } else if c == '.' || c == ',' {
                dots += 1;
                self.at += 1;
            } else if (c == 'e' || c == 'E')
                && self.chars.get(self.at + 1).is_some_and(|n| n.is_ascii_digit() || *n == '-' || *n == '+')
            {
                self.at += 2;
                while self.chars.get(self.at).is_some_and(char::is_ascii_digit) {
                    self.at += 1;
                }
            } else {
                break;
            }
        }
        if dots > 1 {
            return Err(FormulaError::Syntax);
        }
        let text: String = self.chars[start..self.at].iter().map(|c| if *c == ',' { '.' } else { *c }).collect();
        let value: f64 = text.parse().map_err(|_| FormulaError::Syntax)?;
        // An optional unit: millimetres, which is what every number already is.
        let save = self.at;
        self.skip_space();
        let unit: String = self.chars[self.at..].iter().take(2).collect();
        if unit == "mm" && !self.chars.get(self.at + 2).is_some_and(|c| c.is_alphanumeric() || *c == '_') {
            self.at += 2;
        } else {
            self.at = save;
        }
        if value.is_finite() { Ok(Node::Number(value)) } else { Err(FormulaError::Syntax) }
    }
}
