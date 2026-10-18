use std::fmt;

use burnburn_geometry::{BooleanError, GeometryError};
use burnburn_text::TextError;

/// Why a generator refused its parameters or could not build its geometry. Concrete, so the
/// inspector can point at the parameter (E10: no silent clamping).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum GenError {
    NotFinite {
        param: &'static str,
    },
    NotPositive {
        param: &'static str,
    },
    Negative {
        param: &'static str,
    },
    /// A detail smaller than the generator can make meaningfully (for example a finger narrower
    /// than the material, or a bridge wider than the counter).
    TooSmall {
        param: &'static str,
        min_mm: f64,
        value_mm: f64,
    },
    /// A count outside its range.
    Count {
        param: &'static str,
        min: u64,
        max: u64,
        value: u64,
    },
    Geometry(GeometryError),
    Boolean(BooleanError),
    Text(TextError),
    /// Text on a path needs more length than the guide has after `start`.
    TextLongerThanPath {
        needed_mm: f64,
        available_mm: f64,
    },
    /// Text on a path is one line.
    MultiLineText,
    /// The guide path has no length.
    EmptyGuide,
    /// A barcode cannot encode this character.
    UnsupportedChar {
        ch: char,
        index: usize,
    },
}

impl fmt::Display for GenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GenError::NotFinite { param } => write!(f, "{param} must be a finite number"),
            GenError::NotPositive { param } => write!(f, "{param} must be positive"),
            GenError::Negative { param } => write!(f, "{param} must not be negative"),
            GenError::TooSmall { param, min_mm, value_mm } => {
                write!(f, "{param} is {value_mm} mm; at least {min_mm} mm is needed")
            }
            GenError::Count { param, min, max, value } => write!(f, "{param} is {value}; allowed {min}..={max}"),
            GenError::Geometry(e) => write!(f, "{e}"),
            GenError::Boolean(e) => write!(f, "{e}"),
            GenError::Text(e) => write!(f, "{e}"),
            GenError::TextLongerThanPath { needed_mm, available_mm } => {
                write!(f, "text needs {needed_mm} mm of path, {available_mm} mm available")
            }
            GenError::MultiLineText => f.write_str("text on a path must be a single line"),
            GenError::EmptyGuide => f.write_str("the guide path has no length"),
            GenError::UnsupportedChar { ch, index } => write!(f, "character {ch:?} at {index} cannot be encoded"),
        }
    }
}

impl std::error::Error for GenError {}

impl From<GeometryError> for GenError {
    fn from(e: GeometryError) -> Self {
        GenError::Geometry(e)
    }
}

impl From<BooleanError> for GenError {
    fn from(e: BooleanError) -> Self {
        GenError::Boolean(e)
    }
}

impl From<TextError> for GenError {
    fn from(e: TextError) -> Self {
        GenError::Text(e)
    }
}

pub(crate) fn finite(param: &'static str, v: f64) -> Result<f64, GenError> {
    if v.is_finite() { Ok(v) } else { Err(GenError::NotFinite { param }) }
}

pub(crate) fn positive(param: &'static str, v: f64) -> Result<f64, GenError> {
    if finite(param, v)? > 0.0 { Ok(v) } else { Err(GenError::NotPositive { param }) }
}

pub(crate) fn non_negative(param: &'static str, v: f64) -> Result<f64, GenError> {
    if finite(param, v)? >= 0.0 { Ok(v) } else { Err(GenError::Negative { param }) }
}
