use crate::length::{Length, LengthError};

/// A user-facing length unit. Canonical storage is always millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Unit {
    Millimeter,
    Inch,
}

impl Unit {
    /// Millimetres per one of this unit (exact by definition: 1 in = 25.4 mm).
    pub fn mm_per_unit(self) -> f64 {
        match self {
            Unit::Millimeter => 1.0,
            Unit::Inch => 25.4,
        }
    }
}

impl Length {
    /// A length expressed in `unit`; rejects non-finite input and results.
    pub fn new(value: f64, unit: Unit) -> Result<Self, LengthError> {
        Length::from_mm(value * unit.mm_per_unit())
    }

    /// This length expressed in `unit`.
    pub fn to_unit(self, unit: Unit) -> f64 {
        self.mm() / unit.mm_per_unit()
    }
}
