use burnburn_types::Length;

use crate::segment::{EllipticalArc, Segment};
use crate::{Affine, Bounds, GeometryError, Point};

/// Gap below which an arc is considered to start at the current point. This is a structural
/// float-noise bound (far below every user tolerance), not a fitting or production tolerance.
const ARC_CONTINUITY_MM: f64 = 1e-9;

/// An axis-aligned rectangle given by its minimum corner and physical size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    origin: Point,
    width: Length,
    height: Length,
}

impl Rect {
    pub fn new(origin: Point, width: Length, height: Length) -> Result<Self, GeometryError> {
        if !origin.is_finite() {
            return Err(GeometryError::NotFinite);
        }
        if width.mm() < 0.0 || height.mm() < 0.0 {
            return Err(GeometryError::NegativeExtent);
        }
        Ok(Rect { origin, width, height })
    }

    pub fn origin(&self) -> Point {
        self.origin
    }

    pub fn width(&self) -> Length {
        self.width
    }

    pub fn height(&self) -> Length {
        self.height
    }
}

/// Orientation of a closed contour in the y-up coordinate system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Winding {
    CounterClockwise,
    Clockwise,
    /// Zero signed area (collapsed or self-cancelling contour).
    Degenerate,
}

/// A connected run of segments from `start`, optionally closed back to `start` by a line.
#[derive(Debug, Clone, PartialEq)]
pub struct Subpath {
    start: Point,
    segments: Vec<Segment>,
    closed: bool,
}

impl Subpath {
    /// A subpath from stored parts (for example a loaded file). Segments are kept exactly;
    /// unlike [`PathBuilder::arc`] no connecting line is inserted.
    pub fn new(start: Point, segments: Vec<Segment>, closed: bool) -> Result<Self, GeometryError> {
        if !start.is_finite() || !segments.iter().all(Segment::is_finite) {
            return Err(GeometryError::NotFinite);
        }
        Ok(Subpath { start, segments, closed })
    }

    pub fn start(&self) -> Point {
        self.start
    }

    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }

    pub fn end(&self) -> Point {
        self.segments.last().map_or(self.start, Segment::end)
    }

    /// Signed enclosed area in mm² (positive = counter-clockwise); `None` for an open subpath.
    pub fn signed_area(&self) -> Option<f64> {
        if !self.closed {
            return None;
        }
        let mut current = self.start;
        let mut twice = 0.0;
        for seg in &self.segments {
            twice += seg.double_area(current);
            current = seg.end();
        }
        twice += Segment::Line { to: self.start }.double_area(current);
        Some(twice / 2.0)
    }

    /// Orientation of a closed subpath; `None` when open.
    pub fn winding(&self) -> Option<Winding> {
        self.signed_area().map(|a| {
            if a > 0.0 {
                Winding::CounterClockwise
            } else if a < 0.0 {
                Winding::Clockwise
            } else {
                Winding::Degenerate
            }
        })
    }

    fn transform(&self, m: &Affine) -> Subpath {
        Subpath {
            start: m.apply(self.start),
            segments: self.segments.iter().map(|s| s.transform(m)).collect(),
            closed: self.closed,
        }
    }

    fn include_in(&self, b: &mut Bounds) {
        let mut current = self.start;
        b.include(current);
        for seg in &self.segments {
            seg.include_in(current, b);
            current = seg.end();
        }
    }
}

/// A shape outline: zero or more subpaths (outer contours, holes, open strokes).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Path {
    subpaths: Vec<Subpath>,
}

impl Path {
    /// A closed counter-clockwise rectangle starting at its minimum corner.
    pub fn rectangle(rect: Rect) -> Path {
        let o = rect.origin();
        let (w, h) = (rect.width().mm(), rect.height().mm());
        let segments = vec![
            Segment::Line { to: Point::new(o.x + w, o.y) },
            Segment::Line { to: Point::new(o.x + w, o.y + h) },
            Segment::Line { to: Point::new(o.x, o.y + h) },
        ];
        Path { subpaths: vec![Subpath { start: o, segments, closed: true }] }
    }

    pub fn from_subpaths(subpaths: Vec<Subpath>) -> Path {
        Path { subpaths }
    }

    /// An open straight line.
    pub fn line(from: Point, to: Point) -> Result<Path, GeometryError> {
        PathBuilder::new().move_to(from).line_to(to).build()
    }

    pub fn subpaths(&self) -> &[Subpath] {
        &self.subpaths
    }

    pub fn is_empty(&self) -> bool {
        self.subpaths.is_empty()
    }

    /// Exact bounds of the curves (not of control points); `None` for an empty path.
    pub fn bounds(&self) -> Option<Bounds> {
        let first = self.subpaths.first()?;
        let mut b = Bounds::from_point(first.start);
        for sp in &self.subpaths {
            sp.include_in(&mut b);
        }
        Some(b)
    }

    /// Sum of the closed subpaths' signed areas (holes with opposite winding subtract).
    pub fn signed_area(&self) -> f64 {
        self.subpaths.iter().filter_map(Subpath::signed_area).sum()
    }

    /// The exact image of this path under `m`. Arcs stay arcs; nothing is flattened.
    pub fn transform(&self, m: &Affine) -> Path {
        Path { subpaths: self.subpaths.iter().map(|sp| sp.transform(m)).collect() }
    }
}

/// Incremental path construction; validation errors are reported once by [`PathBuilder::build`].
#[derive(Debug, Default)]
pub struct PathBuilder {
    done: Vec<Subpath>,
    current: Option<Subpath>,
    error: Option<GeometryError>,
}

impl PathBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts a new subpath (finishing the current one open).
    pub fn move_to(mut self, p: Point) -> Self {
        if !p.is_finite() {
            self.fail(GeometryError::NotFinite);
        }
        self.finish();
        self.current = Some(Subpath { start: p, segments: Vec::new(), closed: false });
        self
    }

    pub fn line_to(self, to: Point) -> Self {
        self.push(Segment::Line { to })
    }

    pub fn cubic_to(self, c1: Point, c2: Point, to: Point) -> Self {
        self.push(Segment::Cubic { c1, c2, to })
    }

    /// Appends an arc. When the current point is not at the arc start (beyond float noise),
    /// a connecting line is inserted, as SVG does.
    pub fn arc(mut self, arc: EllipticalArc) -> Self {
        if let Some(sp) = &self.current
            && sp.end().distance(arc.start_point()) > ARC_CONTINUITY_MM
        {
            self = self.line_to(arc.start_point());
        }
        self.push(Segment::Arc(arc))
    }

    /// Closes the current subpath with an implicit line back to its start.
    pub fn close(mut self) -> Self {
        match self.current.as_mut() {
            Some(sp) => sp.closed = true,
            None => self.fail(GeometryError::MissingStart),
        }
        self.finish();
        self
    }

    pub fn build(mut self) -> Result<Path, GeometryError> {
        self.finish();
        match self.error {
            Some(e) => Err(e),
            None => Ok(Path { subpaths: self.done }),
        }
    }

    fn push(mut self, seg: Segment) -> Self {
        if !seg.is_finite() {
            self.fail(GeometryError::NotFinite);
        }
        match self.current.as_mut() {
            Some(sp) => sp.segments.push(seg),
            None => self.fail(GeometryError::MissingStart),
        }
        self
    }

    fn finish(&mut self) {
        if let Some(sp) = self.current.take() {
            self.done.push(sp);
        }
    }

    fn fail(&mut self, e: GeometryError) {
        self.error.get_or_insert(e);
    }
}
