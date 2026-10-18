//! A subpath as a ring of explicit segments (closing line included) addressed by a position
//! `g + t` along the outline, used by trim, break and corner cuts.

use crate::intersect::segment_intersections;
use crate::query::{CLOSING_NOISE_MM, segments_of};
use crate::{Path, Point, Segment, Subpath};

/// Positions closer than this are the same position (float noise, not a user tolerance).
const POS_EPS: f64 = 1e-9;

#[derive(Debug, Clone)]
pub(crate) struct Ring {
    pub segs: Vec<Segment>,
    /// Start point of every segment.
    pub starts: Vec<Point>,
    pub closed: bool,
}

impl Ring {
    pub(crate) fn new(sp: &Subpath) -> Ring {
        let mut segs = sp.segments().to_vec();
        let mut starts = Vec::with_capacity(segs.len() + 1);
        let mut from = sp.start();
        for s in &segs {
            starts.push(from);
            from = s.end();
        }
        if sp.is_closed() && from.distance(sp.start()) > CLOSING_NOISE_MM {
            starts.push(from);
            segs.push(Segment::Line { to: sp.start() });
        }
        Ring { segs, starts, closed: sp.is_closed() }
    }

    pub(crate) fn len(&self) -> usize {
        self.segs.len()
    }

    /// The piece `[a, b]` of the outline (`b` may exceed the ring length: it wraps).
    pub(crate) fn slice(&self, a: f64, b: f64) -> Option<(Point, Vec<Segment>)> {
        let m = self.len();
        if m == 0 || !(b > a) {
            return None;
        }
        let mut out = Vec::new();
        let mut start = None;
        let mut g = a.floor();
        while g < b {
            let idx = (g as usize) % m;
            let (lo, hi) = ((a - g).max(0.0), (b - g).min(1.0));
            if hi - lo > 1e-12 {
                let (from, seg) = sub_segment(self.starts[idx], &self.segs[idx], lo, hi);
                start.get_or_insert(from);
                out.push(seg);
            }
            g += 1.0;
        }
        start.map(|s| (s, out))
    }

    /// Sorted, de-duplicated positions where `cutters` cross the ring. On an open ring the
    /// end points themselves are not cuts; on a closed ring the position `m` is `0`.
    pub(crate) fn cuts(&self, cutters: &[&Path]) -> Vec<f64> {
        let m = self.len() as f64;
        let mut cuts = Vec::new();
        for (g, (from, seg)) in self.starts.iter().zip(&self.segs).enumerate() {
            for cutter in cutters {
                for (_, cf, cs) in segments_of(cutter) {
                    for (t, _, _) in segment_intersections(*from, seg, cf, &cs) {
                        cuts.push(g as f64 + t);
                    }
                }
            }
        }
        if self.closed {
            for c in &mut cuts {
                if *c >= m - POS_EPS {
                    *c -= m;
                }
            }
        } else {
            cuts.retain(|c| *c > POS_EPS && *c < m - POS_EPS);
        }
        cuts.sort_by(f64::total_cmp);
        cuts.dedup_by(|a, b| (*a - *b).abs() < POS_EPS);
        cuts
    }
}

/// The part `[t0, t1]` of `seg` (which starts at `from`) and its start point.
fn sub_segment(from: Point, seg: &Segment, t0: f64, t1: f64) -> (Point, Segment) {
    let head = if t1 < 1.0 { seg.split(from, t1).0 } else { *seg };
    if t0 > 0.0 && t1 > 0.0 {
        let u = t0 / t1;
        (head.point_at(from, u), head.split(from, u).1)
    } else {
        (from, head)
    }
}

/// A subpath from parts, `None` when there is nothing to draw.
pub(crate) fn make_subpath(start: Point, segs: Vec<Segment>, closed: bool) -> Option<Subpath> {
    if segs.is_empty() {
        return None;
    }
    Subpath::new(start, segs, closed).ok()
}
