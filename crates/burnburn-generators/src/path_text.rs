//! Text along a guide path: each glyph is placed with its baseline centre on the path (offset
//! along the left normal) and rotated to the tangent.

use burnburn_geometry::{Affine, Path, Point, ProductionTolerance, Segment, Subpath, Vector};
use burnburn_text::{FontSource, TextObject, shape};

use crate::error::{GenError, finite, non_negative};

/// How the letter shapes follow the guide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Distort {
    /// Every glyph keeps its shape and is rotated to the guide at its centre.
    #[default]
    Rigid,
    /// The outlines bend with the guide: a point at height `h` above the baseline and `x` along
    /// the line lands at `h` (plus `offset`) from the guide, at arc length `start + x`.
    Warp,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathTextParams {
    pub distort: Distort,
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
    let guide_line = GuideLine { pts: &pts, cumulative: &cumulative };
    let step = guide_line.max_step(p.tolerance.mm());
    for (g, outline) in shaped.glyphs.iter().zip(&shaped.glyph_outlines) {
        let anchor = Point::new(g.origin.x + g.advance_mm / 2.0, line.baseline_y);
        let at = start + (anchor.x - line.start_x);
        let (centre, dir) = guide_line.frame_at(at);
        let normal = Vector::new(-dir.y, dir.x);
        let centre = centre + normal * offset;
        match p.distort {
            Distort::Rigid => {
                let m = Affine::new([
                    dir.x,
                    dir.y,
                    -dir.y,
                    dir.x,
                    centre.x - (dir.x * anchor.x - dir.y * anchor.y),
                    centre.y - (dir.y * anchor.x + dir.x * anchor.y),
                ])?;
                subpaths.extend(outline.transform(&m).subpaths().iter().cloned());
            }
            Distort::Warp => {
                for poly in outline.flatten(&p.tolerance) {
                    let mut mapped = Vec::with_capacity(poly.points.len());
                    let n = poly.points.len();
                    let edges = if poly.closed { n } else { n.saturating_sub(1) };
                    for i in 0..edges {
                        let (a, b) = (poly.points[i], poly.points[(i + 1) % n]);
                        let parts = ((a.distance(b) / step).ceil() as usize).clamp(1, MAX_PARTS);
                        for k in 0..parts {
                            let q = a.lerp(b, k as f64 / parts as f64);
                            mapped.push(guide_line.warp(q, line.start_x, line.baseline_y, start, offset));
                        }
                    }
                    if !poly.closed
                        && let Some(&last) = poly.points.last()
                    {
                        mapped.push(guide_line.warp(last, line.start_x, line.baseline_y, start, offset));
                    }
                    if let Some((&first, rest)) = mapped.split_first() {
                        let segments = rest.iter().map(|&to| Segment::Line { to }).collect();
                        subpaths.push(Subpath::new(first, segments, poly.closed)?);
                    }
                }
            }
        }
        frames.push(GlyphFrame { centre, direction: dir, at });
    }
    Ok(PathText { path: Path::from_subpaths(subpaths), frames })
}

/// Upper bound of the pieces one glyph edge is cut into when it is bent.
const MAX_PARTS: usize = 2048;

/// The flattened guide with arc-length lookup. Positions outside the guide continue along the
/// first or last segment (glyphs may overhang their advance box by a side bearing).
struct GuideLine<'a> {
    pts: &'a [Point],
    cumulative: &'a [f64],
}

impl GuideLine<'_> {
    /// Point and unit direction at arc length `at` (unclamped).
    fn frame_at(&self, at: f64) -> (Point, Vector) {
        let k = self.cumulative.partition_point(|&c| c <= at).saturating_sub(1).min(self.pts.len() - 2);
        let (a, b) = (self.pts[k], self.pts[k + 1]);
        let seg = self.cumulative[k + 1] - self.cumulative[k];
        let t = if seg > 0.0 { (at - self.cumulative[k]) / seg } else { 0.0 };
        let dir = (b - a) * (1.0 / seg.max(f64::MIN_POSITIVE));
        (a.lerp(b, t.clamp(0.0, 1.0)) + dir * (at - self.cumulative[k] - t.clamp(0.0, 1.0) * seg), dir)
    }

    /// Where the text-space point `q` goes when the text bends with the guide.
    fn warp(&self, q: Point, line_x: f64, baseline_y: f64, start: f64, offset: f64) -> Point {
        let (p, dir) = self.frame_at(start + (q.x - line_x));
        p + Vector::new(-dir.y, dir.x) * (offset + (q.y - baseline_y))
    }

    /// Longest piece of an outline edge that still follows the guide's tightest bend within
    /// `tolerance` (chord error s²/8R), never shorter than 0.02 mm and at most 5 mm.
    fn max_step(&self, tolerance: f64) -> f64 {
        let mut min_radius = f64::INFINITY;
        for w in self.pts.windows(3).zip(self.cumulative.windows(3)) {
            let (p, c) = w;
            let (d0, d1) = (p[1] - p[0], p[2] - p[1]);
            let (l0, l1) = (c[1] - c[0], c[2] - c[1]);
            if l0 > 0.0 && l1 > 0.0 {
                let cos = (d0.dot(d1) / (l0 * l1)).clamp(-1.0, 1.0);
                let turn = cos.acos();
                if turn > 1e-9 {
                    min_radius = min_radius.min(l0.min(l1) / turn);
                }
            }
        }
        (8.0 * tolerance * min_radius).sqrt().clamp(0.02, 5.0)
    }
}
