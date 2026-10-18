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

    /// Extends an open line or arc end to the nearest boundary ahead (lines move along their
    /// direction, arcs along their circle or ellipse).
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
            Segment::Cubic { .. } => Err(ExtendError::NotExtendable),
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
