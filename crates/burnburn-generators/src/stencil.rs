//! Stencil bridges: every island (the sheet inside a hole of the cut-out, such as the counter of
//! an O) gets a bridge to the surrounding sheet, so nothing falls out.

use burnburn_geometry::{
    BooleanOp, BooleanOptions, Bounds, FillRule, Path, Point, Polyline, ProductionTolerance, Rect, boolean,
};
use burnburn_types::{CancellationToken, Length};

use crate::error::{GenError, positive};

#[derive(Debug, Clone, PartialEq)]
pub struct Stencil {
    pub path: Path,
    /// Number of bridges placed (one per hole).
    pub bridges: usize,
    /// Holes still enclosed after bridging (0 unless a bridge could not reach the outside).
    pub islands_left: usize,
}

fn inside(pt: Point, poly: &[Point]) -> bool {
    let mut c = false;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        if (a.y > pt.y) != (b.y > pt.y) && pt.x < a.x + (pt.y - a.y) * (b.x - a.x) / (b.y - a.y) {
            c = !c;
        }
    }
    c
}

/// Nesting depth of each contour (how many other contours contain its first point).
fn depths(polys: &[Polyline]) -> Vec<usize> {
    polys
        .iter()
        .enumerate()
        .map(|(i, p)| match p.points.first() {
            Some(&pt) => polys.iter().enumerate().filter(|(j, q)| *j != i && inside(pt, &q.points)).count(),
            None => 0,
        })
        .collect()
}

fn bounds_of(p: &Polyline) -> Option<Bounds> {
    let (first, rest) = p.points.split_first()?;
    let mut b = Bounds::from_point(*first);
    for q in rest {
        b.include(*q);
    }
    Some(b)
}

/// Cuts a vertical bridge of `width` through the centre of every hole of the closed
/// `outlines` (even-odd), spanning the contour that encloses the hole, so the island stays
/// attached above and below. A hole narrower than `width` is refused (`TooSmall`: the hole
/// width is `value_mm`, the bridge width `min_mm`).
/// Cancellable through `cancel` (the overlay is the heavy part).
pub fn stencil_bridges(
    outlines: &Path,
    width: f64,
    tolerance: ProductionTolerance,
    cancel: &CancellationToken,
) -> Result<Stencil, GenError> {
    let width = positive("width", width)?;
    let polys = outlines.flatten(&tolerance);
    let depth = depths(&polys);
    let mut strips = Vec::new();
    for (i, p) in polys.iter().enumerate() {
        if depth[i].is_multiple_of(2) {
            continue;
        }
        let Some(hole) = bounds_of(p) else { continue };
        if hole.width() <= width {
            return Err(GenError::TooSmall { param: "width", min_mm: width, value_mm: hole.width() });
        }
        // The enclosing contour one level up (smallest such contour).
        let first = p.points.first().copied().unwrap_or(Point::ORIGIN);
        let parent = polys
            .iter()
            .enumerate()
            .filter(|(j, q)| *j != i && depth[*j] + 1 == depth[i] && inside(first, &q.points))
            .filter_map(|(_, q)| bounds_of(q))
            .min_by(|a, b| (a.width() * a.height()).total_cmp(&(b.width() * b.height())))
            .unwrap_or(hole);
        let (y0, y1) = (parent.min().y - 1.0, parent.max().y + 1.0);
        let len = |v: f64| Length::from_mm(v).map_err(|_| GenError::NotFinite { param: "width" });
        let rect = Rect::new(Point::new(hole.center().x - width / 2.0, y0), len(width)?, len(y1 - y0)?)?;
        strips.extend(Path::rectangle(rect).subpaths().iter().cloned());
    }
    if strips.is_empty() {
        return Ok(Stencil { path: outlines.clone(), bridges: 0, islands_left: 0 });
    }
    let bridges = strips.len();
    let options = BooleanOptions { fill: FillRule::EvenOdd, tolerance };
    let path = boolean(outlines, &Path::from_subpaths(strips), BooleanOp::Difference, &options, cancel)?;
    let islands_left = depths(&path.flatten(&tolerance)).iter().filter(|d| *d % 2 == 1).count();
    Ok(Stencil { path, bridges, islands_left })
}
