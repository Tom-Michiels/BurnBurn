//! Module data to millimetre geometry: closed rectangles, y up, origin at the bottom-left corner of the
//! quiet zone. No kerf (CAM applies it once).

use burnburn_geometry::{Path, Point, Rect, Subpath};
use burnburn_types::Length;

use super::{SymbolError, SymbolLayout};
use crate::error::GenError;

fn len(v: f64, param: &'static str) -> Result<Length, SymbolError> {
    Length::from_mm(v).map_err(|_| SymbolError::Param(GenError::NotFinite { param }))
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Result<Vec<Subpath>, SymbolError> {
    let r = Rect::new(Point::new(x, y), len(w, "module")?, len(h, "module")?)
        .map_err(|e| SymbolError::Param(GenError::Geometry(e)))?;
    Ok(Path::rectangle(r).subpaths().to_vec())
}

/// The path and the overall size (width, height) in mm, quiet zone included.
pub(super) fn build(
    layout: &SymbolLayout,
    module_mm: f64,
    quiet_modules: u32,
    bar_height_mm: f64,
) -> Result<(Path, (f64, f64)), SymbolError> {
    let q = f64::from(quiet_modules);
    let mut subpaths: Vec<Subpath> = Vec::new();
    match layout {
        SymbolLayout::Linear { widths } => {
            let mut x = q;
            for (i, &w) in widths.iter().enumerate() {
                let w = f64::from(w);
                if i % 2 == 0 && w > 0.0 {
                    subpaths.extend(rect(x * module_mm, 0.0, w * module_mm, bar_height_mm)?);
                }
                x += w;
            }
            let total = (layout.width_modules() as f64 + 2.0 * q) * module_mm;
            Ok((Path::from_subpaths(subpaths), (total, bar_height_mm)))
        }
        SymbolLayout::Matrix { width, height, modules } => {
            // Rows that repeat (PDF417 row height) are drawn once, as taller rectangles.
            let row = |r: usize| &modules[r * width..(r + 1) * width];
            let mut r = 0;
            while r < *height {
                let mut repeat = 1;
                while r + repeat < *height && row(r + repeat) == row(r) {
                    repeat += 1;
                }
                // y of the lowest of the `repeat` rows (row 0 is at the top).
                let y = (*height - r - repeat) as f64 + q;
                let cells = row(r);
                let mut col = 0;
                while col < *width {
                    if !cells[col] {
                        col += 1;
                        continue;
                    }
                    let start = col;
                    while col < *width && cells[col] {
                        col += 1;
                    }
                    subpaths.extend(rect(
                        (start as f64 + q) * module_mm,
                        y * module_mm,
                        (col - start) as f64 * module_mm,
                        repeat as f64 * module_mm,
                    )?);
                }
                r += repeat;
            }
            let w = (*width as f64 + 2.0 * q) * module_mm;
            let h = (*height as f64 + 2.0 * q) * module_mm;
            Ok((Path::from_subpaths(subpaths), (w, h)))
        }
    }
}
