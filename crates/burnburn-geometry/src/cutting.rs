//! Trim, extend and break for lines and curves.

use std::f64::consts::TAU;

use crate::ring::{Ring, make_subpath};
use crate::{
    EllipticalArc, EndpointRef, ExtendError, Path, Point, Segment, Subpath, TrimError,
    intersect::segment_intersections, query::segments_of,
};
use burnburn_types::Angle;

/// Positions closer than this are the same position (float noise, not a user tolerance).
const POS_EPS: f64 = 1e-9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BreakError {
    EmptyPath,
    /// The pick is at an open end: breaking there would not split anything.
    AtEnd,
}

impl std::fmt::Display for BreakError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            BreakError::EmptyPath => "the path is empty",
            BreakError::AtEnd => "cannot break at an open end",
        })
    }
}

impl std::error::Error for BreakError {}

impl Path {
    /// Removes the picked piece of a line, arc or Bézier between the nearest crossings with
    /// `cutters` (or the end of an open outline). A cut open subpath splits in two; a closed
    /// subpath needs two crossings and becomes one open subpath.
    pub fn trim_at(&self, pick: Point, cutters: &[&Path]) -> Result<Path, TrimError> {
        let near = self.nearest(pick).ok_or(TrimError::EmptyPath)?;
        let si = near.at.subpath;
        let sp = self.subpaths().get(si).ok_or(TrimError::EmptyPath)?;
        let ring = Ring::new(sp);
        let m = ring.len() as f64;
        if ring.len() == 0 {
            return Err(TrimError::EmptyPath);
        }
        let pos = near.at.segment as f64 + near.t;
        let cuts = ring.cuts(cutters);
        if cuts.is_empty() {
            return Err(TrimError::NoIntersection);
        }
        let mut pieces: Vec<Subpath> = Vec::new();
        if ring.closed {
            if cuts.len() < 2 {
                return Err(TrimError::NoIntersection);
            }
            let first = cuts.first().copied().unwrap_or(0.0);
            let last = cuts.last().copied().unwrap_or(0.0);
            let lo = cuts.iter().rev().find(|c| **c < pos).copied().unwrap_or(last - m);
            let hi = cuts.iter().find(|c| **c > pos).copied().unwrap_or(first + m);
            if let Some((start, segs)) = ring.slice(hi, lo + m) {
                pieces.extend(make_subpath(start, segs, false));
            }
        } else {
            let lo = cuts.iter().rev().find(|c| **c < pos).copied().unwrap_or(0.0);
            let hi = cuts.iter().find(|c| **c > pos).copied().unwrap_or(m);
            for (a, b) in [(0.0, lo), (hi, m)] {
                if let Some((start, segs)) = ring.slice(a, b) {
                    pieces.extend(make_subpath(start, segs, false));
                }
            }
        }
        Ok(replace_subpath(self, si, pieces))
    }

    /// Extends an open line, arc or Bézier end to the nearest boundary ahead (lines move along
    /// their direction, arcs along their circle or ellipse, a Bézier along its own polynomial
    /// for up to three parameter lengths and straight along its end tangent beyond that).
    pub fn extend_end(&self, end: EndpointRef, boundaries: &[&Path]) -> Result<Path, ExtendError> {
        let (si, at_end) = match end {
            EndpointRef::Start(s) => (s, false),
            EndpointRef::End(s) => (s, true),
        };
        let sp = self.subpaths().get(si).ok_or(ExtendError::NoSuchEnd)?;
        if sp.is_closed() {
            return Err(ExtendError::Closed);
        }
        let seg = if at_end { sp.segments().last() } else { sp.segments().first() };
        match seg.ok_or(ExtendError::NoSuchEnd)? {
            Segment::Line { .. } => self.extend_line(end, boundaries),
            Segment::Arc(arc) => {
                let longer = extended_arc(arc, at_end, boundaries)?;
                let mut segs = sp.segments().to_vec();
                let mut start = sp.start();
                if at_end {
                    if let Some(last) = segs.last_mut() {
                        *last = Segment::Arc(longer);
                    }
                } else {
                    if let Some(first) = segs.first_mut() {
                        *first = Segment::Arc(longer);
                    }
                    start = longer.start_point();
                }
                let rebuilt = Subpath::new(start, segs, false).map_err(|_| ExtendError::NoBoundaryAhead)?;
                Ok(replace_subpath(self, si, vec![rebuilt]))
            }
            Segment::Cubic { .. } => {
                let rebuilt = extended_cubic_subpath(sp, at_end, boundaries)?;
                Ok(replace_subpath(self, si, vec![rebuilt]))
            }
        }
    }

    /// Splits the outline at the point nearest to `pick`: an open subpath becomes two, a closed
    /// one becomes one open subpath that starts and ends at the break.
    pub fn break_at(&self, pick: Point) -> Result<Path, BreakError> {
        let near = self.nearest(pick).ok_or(BreakError::EmptyPath)?;
        let si = near.at.subpath;
        let sp = self.subpaths().get(si).ok_or(BreakError::EmptyPath)?;
        let ring = Ring::new(sp);
        let m = ring.len() as f64;
        if ring.len() == 0 {
            return Err(BreakError::EmptyPath);
        }
        let pos = near.at.segment as f64 + near.t;
        let mut pieces = Vec::new();
        if ring.closed {
            if let Some((start, segs)) = ring.slice(pos, pos + m) {
                pieces.extend(make_subpath(start, segs, false));
            }
        } else {
            if pos <= POS_EPS || pos >= m - POS_EPS {
                return Err(BreakError::AtEnd);
            }
            for (a, b) in [(0.0, pos), (pos, m)] {
                if let Some((start, segs)) = ring.slice(a, b) {
                    pieces.extend(make_subpath(start, segs, false));
                }
            }
        }
        Ok(replace_subpath(self, si, pieces))
    }
}

fn replace_subpath(path: &Path, index: usize, pieces: Vec<Subpath>) -> Path {
    let mut out = path.subpaths()[..index].to_vec();
    out.extend(pieces);
    out.extend_from_slice(&path.subpaths()[index + 1..]);
    Path::from_subpaths(out)
}

/// How far past its end a Bézier is followed, in curve parameter (the polynomial runs on past
/// `t = 1`; far beyond that it no longer says anything about the drawn shape).
const BEZIER_REACH: f64 = 3.0;

/// The point of the control polygon's blossom for parameters `u`, `v`, `w` (de Casteljau with a
/// different parameter per level; any real parameters, also outside `[0, 1]`).
fn blossom(poly: [Point; 4], u: f64, v: f64, w: f64) -> Point {
    let a = [poly[0].lerp(poly[1], u), poly[1].lerp(poly[2], u), poly[2].lerp(poly[3], u)];
    let b = [a[0].lerp(a[1], v), a[1].lerp(a[2], v)];
    b[0].lerp(b[1], w)
}

/// The cubic `poly` over the parameter range `[a, b]` as control points.
fn cubic_over(poly: [Point; 4], a: f64, b: f64) -> [Point; 4] {
    [blossom(poly, a, a, a), blossom(poly, a, a, b), blossom(poly, a, b, b), blossom(poly, b, b, b)]
}

/// The subpath `sp` with its end (or start) Bézier segment continued along the same cubic
/// polynomial to the first boundary crossing ahead. The result is still one cubic there.
fn extended_cubic_subpath(sp: &Subpath, at_end: bool, boundaries: &[&Path]) -> Result<Subpath, ExtendError> {
    let segs = sp.segments();
    // Work on the end: a start is the end of the reversed segment.
    let (from, c1, c2, to, index) = if at_end {
        let last = segs.len().checked_sub(1).ok_or(ExtendError::NoSuchEnd)?;
        let from = last.checked_sub(1).and_then(|i| segs.get(i)).map_or(sp.start(), Segment::end);
        match segs.get(last) {
            Some(Segment::Cubic { c1, c2, to }) => (from, *c1, *c2, *to, last),
            _ => return Err(ExtendError::NoSuchEnd),
        }
    } else {
        match segs.first() {
            Some(Segment::Cubic { c1, c2, to }) => (*to, *c2, *c1, sp.start(), 0),
            _ => return Err(ExtendError::NoSuchEnd),
        }
    };
    let poly = [from, c1, c2, to];
    let ahead_poly = cubic_over(poly, 1.0, 1.0 + BEZIER_REACH);
    let ahead = Segment::Cubic { c1: ahead_poly[1], c2: ahead_poly[2], to: ahead_poly[3] };
    let mut best: Option<f64> = None;
    for boundary in boundaries {
        for (_, bf, bs) in segments_of(boundary) {
            for (t, _, _) in segment_intersections(to, &ahead, bf, &bs) {
                if t * BEZIER_REACH > POS_EPS && best.is_none_or(|b| t < b) {
                    best = Some(t);
                }
            }
        }
    }
    let Some(t) = best else {
        return tangent_continuation(sp, poly, at_end, boundaries);
    };
    let longer = cubic_over(poly, 0.0, 1.0 + t * BEZIER_REACH);
    let new = Segment::Cubic { c1: longer[1], c2: longer[2], to: longer[3] };
    let (start, mut rebuilt) = (sp.start(), segs.to_vec());
    if at_end {
        if let Some(slot) = rebuilt.get_mut(index) {
            *slot = new;
        }
        Subpath::new(start, rebuilt, false).map_err(|_| ExtendError::NoBoundaryAhead)
    } else {
        // The cubic was reversed: turn the lengthened one around again.
        let reversed = Segment::Cubic { c1: longer[2], c2: longer[1], to: longer[0] };
        if let Some(slot) = rebuilt.get_mut(index) {
            *slot = reversed;
        }
        Subpath::new(longer[3], rebuilt, false).map_err(|_| ExtendError::NoBoundaryAhead)
    }
}

/// The boundary lies beyond the reach of the cubic's polynomial, which far from the drawn
/// curve no longer says anything about its shape. The end then continues straight along its
/// own end tangent (tangent continuous with the curve, which stays exactly as drawn) to the
/// first boundary crossing. `poly` is the cubic walked towards the extended end.
fn tangent_continuation(sp: &Subpath, poly: [Point; 4], at_end: bool, boundaries: &[&Path]) -> Result<Subpath, ExtendError> {
    let end = poly[3];
    // The last control point that differs from the end gives the heading.
    let heading = [poly[2], poly[1], poly[0]]
        .into_iter()
        .map(|c| end - c)
        .find(|d| d.length() > POS_EPS)
        .ok_or(ExtendError::NoBoundaryAhead)?;
    let heading = heading * (1.0 / heading.length());
    // Long enough to cross everything the boundaries contain.
    let mut reach = 1.0;
    for boundary in boundaries {
        if let Some(b) = boundary.bounds() {
            reach = f64::max(reach, 2.0 * (b.min().distance(end) + b.max().distance(end)) + 1.0);
        }
    }
    let ray = Segment::Line { to: end + heading * reach };
    let mut best: Option<(f64, Point)> = None;
    for boundary in boundaries {
        for (_, bf, bs) in segments_of(boundary) {
            for (t, _, hit) in segment_intersections(end, &ray, bf, &bs) {
                if t * reach > POS_EPS && best.is_none_or(|b| t < b.0) {
                    best = Some((t, hit));
                }
            }
        }
    }
    let (_, hit) = best.ok_or(ExtendError::NoBoundaryAhead)?;
    let mut segs = sp.segments().to_vec();
    if at_end {
        segs.push(Segment::Line { to: hit });
        Subpath::new(sp.start(), segs, false).map_err(|_| ExtendError::NoBoundaryAhead)
    } else {
        segs.insert(0, Segment::Line { to: sp.start() });
        Subpath::new(hit, segs, false).map_err(|_| ExtendError::NoBoundaryAhead)
    }
}

/// `arc` lengthened at its end (or start) along its own ellipse to the first boundary crossing.
fn extended_arc(arc: &EllipticalArc, at_end: bool, boundaries: &[&Path]) -> Result<EllipticalArc, ExtendError> {
    let angle = |r: f64| Angle::from_radians(r).map_err(|_| ExtendError::NoBoundaryAhead);
    let (u, v) = arc.semi_diameters();
    let (start, sweep) = (arc.start_parameter(), arc.sweep_parameter());
    if sweep == 0.0 {
        return Err(ExtendError::NotALine);
    }
    let direction = sweep.signum();
    let rest = TAU - sweep.abs();
    if rest <= POS_EPS {
        return Err(ExtendError::NoBoundaryAhead);
    }
    // The remainder of the ellipse, walked away from the chosen end.
    let (from_param, walk) = if at_end { (start + sweep, direction) } else { (start, -direction) };
    let ahead = EllipticalArc::new(arc.center(), u, v, angle(from_param)?, angle(walk * rest)?)
        .map_err(|_| ExtendError::NoBoundaryAhead)?;
    let origin = ahead.start_point();
    let ahead = Segment::Arc(ahead);
    let mut best: Option<f64> = None;
    for boundary in boundaries {
        for (_, bf, bs) in segments_of(boundary) {
            for (t, _, _) in segment_intersections(origin, &ahead, bf, &bs) {
                if t * rest > POS_EPS && best.is_none_or(|b| t < b) {
                    best = Some(t);
                }
            }
        }
    }
    let delta = best.ok_or(ExtendError::NoBoundaryAhead)? * rest;
    let new_start = if at_end { start } else { start - direction * delta };
    EllipticalArc::new(arc.center(), u, v, angle(new_start)?, angle(sweep + direction * delta)?)
        .map_err(|_| ExtendError::NoBoundaryAhead)
}
