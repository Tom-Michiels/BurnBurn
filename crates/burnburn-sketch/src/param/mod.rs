//! Named parameters with units and expressions: `width = 120 mm`, `hole_offset = width/2 - 15 mm`,
//! `1in`, `min/max/sqrt`, cycle detection, unit checking (length vs angle) and errors that carry a
//! byte position in the text the caller parsed.
//!
//! Values are canonical: lengths in millimetres, angles in radians. A bare number is
//! dimensionless; `10 mm + 5` is a unit error, not "5 mm".

mod parse;
mod table;

use std::fmt;

use burnburn_types::{Angle, Length};

pub use parse::Expression;
pub use table::{Parameters, Values};

/// Byte range `[start, end)` in the text that was parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }

    pub(crate) fn to(self, other: Span) -> Span {
        Span { start: self.start.min(other.start), end: self.end.max(other.end) }
    }

    pub(crate) fn shifted(self, by: usize) -> Span {
        Span { start: self.start + by, end: self.end + by }
    }
}

/// Physical dimension as exponents: length¹ is millimetres, angle¹ is radians.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Dim {
    pub length: i8,
    pub angle: i8,
}

impl Dim {
    pub const NONE: Dim = Dim { length: 0, angle: 0 };
    pub const LENGTH: Dim = Dim { length: 1, angle: 0 };
    pub const ANGLE: Dim = Dim { length: 0, angle: 1 };
}

impl fmt::Display for Dim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Dim::NONE => f.write_str("number"),
            Dim::LENGTH => f.write_str("length"),
            Dim::ANGLE => f.write_str("angle"),
            Dim { length, angle } => write!(f, "mm^{length}·rad^{angle}"),
        }
    }
}

/// A value with its dimension, in canonical units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quantity {
    pub value: f64,
    pub dim: Dim,
}

/// A parameter's or dimension's final value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Length(Length),
    Angle(Angle),
    Number(f64),
}

impl Value {
    pub fn dim(&self) -> Dim {
        match self {
            Value::Length(_) => Dim::LENGTH,
            Value::Angle(_) => Dim::ANGLE,
            Value::Number(_) => Dim::NONE,
        }
    }

    pub(crate) fn quantity(&self) -> Quantity {
        match *self {
            Value::Length(l) => Quantity { value: l.mm(), dim: Dim::LENGTH },
            Value::Angle(a) => Quantity { value: a.radians(), dim: Dim::ANGLE },
            Value::Number(n) => Quantity { value: n, dim: Dim::NONE },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExprErrorKind {
    UnexpectedCharacter(char),
    UnexpectedToken,
    UnexpectedEnd,
    UnknownUnit(String),
    UnknownFunction(String),
    WrongArgumentCount {
        function: &'static str,
        expected: &'static str,
        found: usize,
    },
    UnknownParameter(String),
    UnitMismatch {
        left: Dim,
        right: Dim,
    },
    DivisionByZero,
    NegativeSqrt,
    OddDimensionSqrt(Dim),
    /// A literal or result is NaN or infinite.
    NotFinite,
    /// The result is not a length, angle or plain number (for example mm²).
    UnsupportedDimension(Dim),
    /// The result has a valid dimension, but not the one required here.
    WrongDimension {
        expected: Dim,
        found: Dim,
    },
    /// `name = expression` expected.
    ExpectedAssignment,
}

/// An expression error at a position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExprError {
    pub kind: ExprErrorKind,
    pub span: Span,
}

impl fmt::Display for ExprError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at {}..{}: ", self.span.start, self.span.end)?;
        match &self.kind {
            ExprErrorKind::UnexpectedCharacter(c) => write!(f, "unexpected character '{c}'"),
            ExprErrorKind::UnexpectedToken => f.write_str("unexpected token"),
            ExprErrorKind::UnexpectedEnd => f.write_str("unexpected end of expression"),
            ExprErrorKind::UnknownUnit(u) => write!(f, "unknown unit '{u}'"),
            ExprErrorKind::UnknownFunction(n) => write!(f, "unknown function '{n}'"),
            ExprErrorKind::WrongArgumentCount { function, expected, found } => {
                write!(f, "{function} takes {expected} argument(s), got {found}")
            }
            ExprErrorKind::UnknownParameter(n) => write!(f, "unknown parameter '{n}'"),
            ExprErrorKind::UnitMismatch { left, right } => write!(f, "cannot combine {left} with {right}"),
            ExprErrorKind::DivisionByZero => f.write_str("division by zero"),
            ExprErrorKind::NegativeSqrt => f.write_str("square root of a negative value"),
            ExprErrorKind::OddDimensionSqrt(d) => write!(f, "square root of {d} has no unit"),
            ExprErrorKind::NotFinite => f.write_str("value is not finite"),
            ExprErrorKind::UnsupportedDimension(d) => write!(f, "result {d} is not a length, angle or number"),
            ExprErrorKind::WrongDimension { expected, found } => write!(f, "expected {expected}, got {found}"),
            ExprErrorKind::ExpectedAssignment => f.write_str("expected 'name = expression'"),
        }
    }
}

impl std::error::Error for ExprError {}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParameterError {
    /// The parameter's expression does not parse or evaluate. Spans are relative to the text
    /// given to `define` (the whole line) or `set` (the expression).
    Expression {
        parameter: String,
        error: ExprError,
    },
    /// Definitions depend on each other in a loop; the first name repeats at the end.
    Cycle {
        cycle: Vec<String>,
    },
    InvalidName {
        name: String,
        span: Span,
    },
    UnknownParameter(String),
    /// Cannot remove a parameter other definitions use.
    StillReferenced {
        parameter: String,
        by: Vec<String>,
    },
}

impl fmt::Display for ParameterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParameterError::Expression { parameter, error } => write!(f, "parameter {parameter}: {error}"),
            ParameterError::Cycle { cycle } => write!(f, "parameters depend on each other: {}", cycle.join(" → ")),
            ParameterError::InvalidName { name, .. } => write!(f, "'{name}' is not a valid parameter name"),
            ParameterError::UnknownParameter(n) => write!(f, "no parameter '{n}'"),
            ParameterError::StillReferenced { parameter, by } => {
                write!(f, "{parameter} is used by {}", by.join(", "))
            }
        }
    }
}

impl std::error::Error for ParameterError {}

/// Unit names accepted after a number, with their canonical factor and dimension.
pub(crate) fn unit(name: &str) -> Option<(f64, Dim)> {
    Some(match name {
        "mm" => (1.0, Dim::LENGTH),
        "cm" => (10.0, Dim::LENGTH),
        "m" => (1000.0, Dim::LENGTH),
        "in" => (25.4, Dim::LENGTH),
        "deg" => (std::f64::consts::PI / 180.0, Dim::ANGLE),
        "rad" => (1.0, Dim::ANGLE),
        _ => return None,
    })
}

pub(crate) const FUNCTIONS: [&str; 3] = ["min", "max", "sqrt"];

/// Whether `name` can name a parameter: an identifier that is not a unit or function.
pub(crate) fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
    first_ok
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && unit(name).is_none()
        && !FUNCTIONS.contains(&name)
}
