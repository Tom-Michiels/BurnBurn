//! Transcendental functions for geometry (DR-13). `f64::sin` and friends call the operating
//! system's libm, which may differ by an ulp between macOS and Windows; geometry can reach
//! project and plan hashes, so everything here goes through the pure-Rust `libm` crate.
//! `sqrt` and the basic operations are IEEE-exact and need no wrapper. Other crates whose results
//! reach hashes (io, sketch, trace, cam) use these functions too.

pub fn asin(x: f64) -> f64 {
    libm::asin(x)
}

pub fn sin(x: f64) -> f64 {
    libm::sin(x)
}

pub fn cos(x: f64) -> f64 {
    libm::cos(x)
}

/// `(sin x, cos x)`, like `f64::sin_cos`.
pub fn sin_cos(x: f64) -> (f64, f64) {
    libm::sincos(x)
}

pub fn tan(x: f64) -> f64 {
    libm::tan(x)
}

pub fn acos(x: f64) -> f64 {
    libm::acos(x)
}

/// `atan2(y, x)`, like `y.atan2(x)`.
pub fn atan2(y: f64, x: f64) -> f64 {
    libm::atan2(y, x)
}

pub fn hypot(x: f64, y: f64) -> f64 {
    libm::hypot(x, y)
}

pub fn cbrt(x: f64) -> f64 {
    libm::cbrt(x)
}
