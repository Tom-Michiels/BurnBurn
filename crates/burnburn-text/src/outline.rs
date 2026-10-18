//! skrifa glyph outlines → `geometry::Path` (quadratics become exact cubics).

use burnburn_geometry::{Path, Point, Segment, Subpath};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{GlyphId, MetadataProvider};

use crate::font::FontData;
use crate::shape::TextError;

/// Maps font units to text-space millimetres: `origin + units * scale`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct UnitMap {
    pub origin: Point,
    pub scale: f64,
}

impl UnitMap {
    fn map(&self, x: f32, y: f32) -> Point {
        Point::new(self.origin.x + f64::from(x) * self.scale, self.origin.y + f64::from(y) * self.scale)
    }
}

struct Pen {
    map: UnitMap,
    start: Option<Point>,
    current: Point,
    segments: Vec<Segment>,
    done: Vec<(Point, Vec<Segment>)>,
}

impl Pen {
    fn flush(&mut self) {
        if let Some(start) = self.start.take() {
            let mut segments = std::mem::take(&mut self.segments);
            // A closing line back to the start is implicit in a closed subpath.
            if let Some(Segment::Line { to }) = segments.last()
                && *to == start
            {
                segments.pop();
            }
            if !segments.is_empty() {
                self.done.push((start, segments));
            }
        }
    }
}

impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.flush();
        let p = self.map.map(x, y);
        self.start = Some(p);
        self.current = p;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let to = self.map.map(x, y);
        if to != self.current {
            self.segments.push(Segment::Line { to });
        }
        self.current = to;
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (p0, q, to) = (self.current, self.map.map(cx0, cy0), self.map.map(x, y));
        let c1 = p0 + (q - p0) * (2.0 / 3.0);
        let c2 = to + (q - to) * (2.0 / 3.0);
        self.segments.push(Segment::Cubic { c1, c2, to });
        self.current = to;
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let to = self.map.map(x, y);
        self.segments.push(Segment::Cubic { c1: self.map.map(cx0, cy0), c2: self.map.map(cx1, cy1), to });
        self.current = to;
    }

    fn close(&mut self) {
        self.flush();
    }
}

/// Closed contours of glyph `glyph_id`, unhinted at the default location, mapped by `map`.
pub(crate) fn glyph_subpaths(font: &FontData, glyph_id: u32, map: UnitMap) -> Result<Vec<Subpath>, TextError> {
    let face = font.outline_font()?;
    let outlines = face.outline_glyphs();
    let Some(glyph) = outlines.get(GlyphId::new(glyph_id)) else {
        return Ok(Vec::new());
    };
    let mut pen = Pen { map, start: None, current: map.origin, segments: Vec::new(), done: Vec::new() };
    glyph
        .draw(DrawSettings::unhinted(Size::unscaled(), LocationRef::default()), &mut pen)
        .map_err(|_| TextError::Shaping)?;
    pen.flush();
    pen.done.into_iter().map(|(start, segments)| Subpath::new(start, segments, true).map_err(TextError::from)).collect()
}

pub(crate) fn path_of(subpaths: Vec<Subpath>) -> Path {
    Path::from_subpaths(subpaths)
}
