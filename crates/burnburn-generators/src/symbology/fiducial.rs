//! Fiducial marker families as module grids: a black border ring around an `n x n` code area.
//!
//! The code tables (`fiducial_tables`) hold the OpenCV packing: row-major `idx = row * n + col`, bit 0 =
//! top-left, black = 1. OpenCV's own AprilTag dictionaries are the official images turned by 180 degrees,
//! so the four AprilTag families are drawn turned back to the official orientation of the AprilTag
//! reference sources (`apriltag_to_image`); the OpenCV 7x7 family keeps OpenCV's orientation. Both claims
//! are tested against independent sources in `tests/symbology_reference.rs`.

use super::fiducial_tables as t;
use super::{SymbolError, Symbology};

struct Family {
    /// Cells per side of the code area (the border adds one cell on each side).
    n: usize,
    codes: &'static [u64],
    /// Draw the grid turned by 180 degrees (official AprilTag orientation).
    turn: bool,
}

fn family(symbology: Symbology) -> Option<Family> {
    Some(match symbology {
        Symbology::AprilTag16h5 => Family { n: 4, codes: &t::APRILTAG_16H5, turn: true },
        Symbology::AprilTag25h9 => Family { n: 5, codes: &t::APRILTAG_25H9, turn: true },
        Symbology::AprilTag36h10 => Family { n: 6, codes: &t::APRILTAG_36H10, turn: true },
        Symbology::AprilTag36h11 => Family { n: 6, codes: &t::APRILTAG_36H11, turn: true },
        Symbology::ArucoOpenCv7x7 => Family { n: 7, codes: &t::OPENCV_7X7_100, turn: false },
        _ => return None,
    })
}

/// The `(n + 2) x (n + 2)` module grid (row 0 at the top, black = true) of marker `id`.
pub(super) fn marker(symbology: Symbology, id: u32) -> Result<(usize, Vec<bool>), SymbolError> {
    let f = family(symbology).ok_or(SymbolError::Unsupported { symbology, reason: "not a marker family" })?;
    let count = f.codes.len() as u32;
    let code = *f.codes.get(id as usize).ok_or(SymbolError::MarkerId { symbology, id, count })?;
    let size = f.n + 2;
    let mut grid = vec![true; size * size];
    for r in 0..f.n {
        for c in 0..f.n {
            grid[(r + 1) * size + c + 1] = (code >> (r * f.n + c)) & 1 == 1;
        }
    }
    if f.turn {
        grid.reverse();
    }
    Ok((size, grid))
}
