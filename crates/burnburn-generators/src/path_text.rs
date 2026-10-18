//! Text along a guide path: each glyph is placed with its baseline centre on the path (offset
//! along the left normal) and rotated to the tangent.

use burnburn_geometry::{Affine, Path, Point, ProductionTolerance, Vector};
use burnburn_text::{FontSource, TextObject, shape};

use crate::error::{GenError, finite, non_negative};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathTextParams {
    /// Distance along the guide where the text starts, mm.
    pub start: f64,
    /// Offset of the baseline along the left normal of the travel direction, mm.
    pub offset: f64,
    /// Travel the guide backwards (flips side and reading direction).
    pub reverse: bool,
    pub tolerance: ProductionTolerance,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphFrame {
    /// Baseline centre of the glyph in document mm.
    pub centre: Point,
    /// Unit direction of the glyph's x-axis (the guide tangent in travel direction).
    pub direction: Vector,
    /// Arc length along the (possibly reversed) guide.
    pub at: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PathText {
    pub path: Path,
    pub frames: Vec<GlyphFrame>,
}

/// Places the single-line `text` along the first subpath of `guide` (flattened within
/// `p.tolerance`). The glyph's baseline centre lands at its arc length; the glyph is rotated to
/// the chord direction there. `text.placement` is ignored (the guide places the text).
pub fn text_on_path(
    text: &TextObject,
    fonts: &dyn FontSource,
    guide: &Path,
    p: &PathTextParams,
) -> Result<PathText, GenError> {
    let start = non_negative("start", p.start)?;
    let offset = finite("offset", p.offset)?;
    let mut local = text.clone();
    local.placement = Affine::IDENTITY;
    let shaped = shape(&local, fonts)?;
    if shaped.lines.len() > 1 {
        return Err(GenError::MultiLineText);
    }
    let line = shaped.lines.first().ok_or(GenError::MultiLineText)?;
    let polys = guide.flatten(&p.tolerance);
    let first = polys.first().ok_or(GenError::EmptyGuide)?;
    let mut pts = first.points.clone();
    if first.closed
        && let Some(&s0) = pts.first()
    {
        pts.push(s0);
    }
    pts.dedup();
    if p.reverse {
        pts.reverse();
    }
    let mut cumulative = Vec::with_capacity(pts.len());
    let mut total = 0.0;
    for (i, q) in pts.iter().enumerate() {
        if i > 0 {
            total += q.distance(pts[i - 1]);
        }
        cumulative.push(total);
    }
    if !(total > 0.0) {
        return Err(GenError::EmptyGuide);
    }
    let needed = start + line.width_mm;
    if needed > total + 1e-9 {
        return Err(GenError::TextLongerThanPath { needed_mm: needed, available_mm: total });
    }
    let mut frames = Vec::with_capacity(shaped.glyphs.len());
    let mut subpaths = Vec::new();
    for (g, outline) in shaped.glyphs.iter().zip(&shaped.glyph_outlines) {
        let anchor = Point::new(g.origin.x + g.advance_mm / 2.0, line.baseline_y);
        let at = start + (anchor.x - line.start_x);
        // Segment k with cumulative[k] <= at <= cumulative[k + 1].
        let k = cumulative.partition_point(|&c| c <= at).saturating_sub(1).min(pts.len() - 2);
        let (a, b) = (pts[k], pts[k + 1]);
        let seg = cumulative[k + 1] - cumulative[k];
        let t = if seg > 0.0 { ((at - cumulative[k]) / seg).clamp(0.0, 1.0) } else { 0.0 };
        let dir = (b - a) * (1.0 / seg.max(f64::MIN_POSITIVE));
        let normal = Vector::new(-dir.y, dir.x);
        let centre = a.lerp(b, t) + normal * offset;
        let m = Affine::new([
            dir.x,
            dir.y,
            -dir.y,
            dir.x,
            centre.x - (dir.x * anchor.x - dir.y * anchor.y),
            centre.y - (dir.y * anchor.x + dir.x * anchor.y),
        ])?;
        subpaths.extend(outline.transform(&m).subpaths().iter().cloned());
        frames.push(GlyphFrame { centre, direction: dir, at });
    }
    Ok(PathText { path: Path::from_subpaths(subpaths), frames })
}
