use std::fmt;

/// A finite physical length, stored in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Length(f64);

/// Why a value cannot be a [`Length`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthError {
    NotFinite,
}

impl fmt::Display for LengthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LengthError::NotFinite => f.write_str("length must be a finite number"),
        }
    }
}

impl std::error::Error for LengthError {}

impl Length {
    /// Rejects NaN and infinities at the input boundary.
    pub fn from_mm(mm: f64) -> Result<Self, LengthError> {
        if mm.is_finite() { Ok(Length(mm)) } else { Err(LengthError::NotFinite) }
    }

    pub fn mm(self) -> f64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_finite_length() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(Length::from_mm(bad), Err(LengthError::NotFinite), "{bad}");
        }
        assert_eq!(Length::from_mm(-2.5).map(Length::mm), Ok(-2.5));
    }
}
