use std::f64::consts::PI;
use std::fmt;

/// A finite angle, stored in radians. Positive is counter-clockwise (y-up).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Angle(f64);

/// Why a value cannot be an [`Angle`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AngleError {
    NotFinite,
}

impl fmt::Display for AngleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AngleError::NotFinite => f.write_str("angle must be a finite number"),
        }
    }
}

impl std::error::Error for AngleError {}

impl Angle {
    pub const ZERO: Angle = Angle(0.0);

    pub fn from_radians(rad: f64) -> Result<Self, AngleError> {
        if rad.is_finite() { Ok(Angle(rad)) } else { Err(AngleError::NotFinite) }
    }

    pub fn from_degrees(deg: f64) -> Result<Self, AngleError> {
        Angle::from_radians(deg.to_radians())
    }

    pub fn radians(self) -> f64 {
        self.0
    }

    pub fn degrees(self) -> f64 {
        self.0 * 180.0 / PI
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angle_rejects_non_finite_and_converts_degrees() {
        assert_eq!(Angle::from_degrees(f64::NAN), Err(AngleError::NotFinite));
        assert_eq!(Angle::from_radians(f64::INFINITY), Err(AngleError::NotFinite));
        let right = Angle::from_degrees(90.0).unwrap();
        assert!((right.radians() - PI / 2.0).abs() < 1e-15);
        assert!((right.degrees() - 90.0).abs() < 1e-12);
    }
}
