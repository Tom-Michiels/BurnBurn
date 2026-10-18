use std::fmt;

/// Why a geometric construction or operation was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum GeometryError {
    /// A coordinate, length or matrix entry was NaN or infinite.
    NotFinite,
    /// A dimension that must be non-negative was negative.
    NegativeExtent,
    /// A transform has no inverse (its linear part collapses area).
    Singular,
    /// An arc with collapsed axes or a sweep beyond a full turn.
    DegenerateArc,
    /// A segment or close was added before any `move_to`.
    MissingStart,
    /// A shape needs more points, sides or star tips than given.
    TooFewPoints,
}

impl fmt::Display for GeometryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GeometryError::NotFinite => f.write_str("geometry value must be finite"),
            GeometryError::NegativeExtent => f.write_str("extent must not be negative"),
            GeometryError::Singular => f.write_str("transform is singular and has no inverse"),
            GeometryError::DegenerateArc => f.write_str("arc is degenerate"),
            GeometryError::MissingStart => f.write_str("path segment has no start point"),
            GeometryError::TooFewPoints => f.write_str("shape has too few points"),
        }
    }
}

impl std::error::Error for GeometryError {}
