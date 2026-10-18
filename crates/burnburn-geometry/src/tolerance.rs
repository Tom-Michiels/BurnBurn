//! Distinct tolerance types. There is deliberately no global epsilon and no conversion
//! between these types: a coarse screen budget can never leak into production geometry.

use std::fmt;

use burnburn_types::Length;

/// Why a value cannot be a tolerance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToleranceError {
    /// Tolerances must be finite and strictly positive.
    NotPositive,
}

impl fmt::Display for ToleranceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("tolerance must be a finite, strictly positive length")
    }
}

impl std::error::Error for ToleranceError {}

fn positive(len: Length) -> Result<f64, ToleranceError> {
    let mm = len.mm();
    if mm > 0.0 { Ok(mm) } else { Err(ToleranceError::NotPositive) }
}

macro_rules! tolerance {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
        pub struct $name(f64);

        impl $name {
            pub fn new(len: Length) -> Result<Self, ToleranceError> {
                positive(len).map($name)
            }

            pub fn mm(self) -> f64 {
                self.0
            }
        }
    };
}

tolerance!(
    /// Maximum chord deviation for on-screen flattening. Derived from zoom; never used for CAM.
    DisplayTolerance
);
tolerance!(
    /// Maximum chord deviation for production flattening (CAM/export). Independent of zoom.
    ProductionTolerance
);
tolerance!(
    /// Pick radius in document millimetres for hit-testing. Not a flattening budget.
    HitTestTolerance
);

tolerance!(
    /// Distance below which geometry checks and repairs treat points as coincident (E09). Chosen
    /// per check by the user; never a flattening budget.
    DiagnosticTolerance
);

/// Tolerances that may drive curve flattening. Hit-test tolerance intentionally does not.
pub trait FlatteningTolerance {
    fn max_deviation_mm(&self) -> f64;
}

impl FlatteningTolerance for DisplayTolerance {
    fn max_deviation_mm(&self) -> f64 {
        self.0
    }
}

impl FlatteningTolerance for ProductionTolerance {
    fn max_deviation_mm(&self) -> f64 {
        self.0
    }
}
