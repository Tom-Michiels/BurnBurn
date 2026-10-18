//! A finger-jointed box (closed or open) from outer dimensions.

use burnburn_geometry::{Path, Point, Vector};

use crate::error::{GenError, non_negative, positive};
use crate::joints::{EdgeProfile, EdgeStart, FingerJoint, SegmentKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lid {
    Closed,
    Open,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PanelRole {
    Bottom,
    Top,
    Front,
    Back,
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BoxParams {
    /// Outer dimensions in mm (x, y, z).
    pub width: f64,
    pub depth: f64,
    pub height: f64,
    pub thickness: f64,
    pub finger_width: f64,
    pub lid: Lid,
    /// Gap between panels in the flat layout.
    pub spacing: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Panel {
    pub role: PanelRole,
    /// Outer size of the panel (its edges are the box's outer dimensions).
    pub width: f64,
    pub height: f64,
    /// Closed outline in the flat layout.
    pub path: Path,
    /// Edge profiles in order bottom, right, top, left (counter-clockwise from the panel origin).
    pub edges: [Option<crate::joints::EdgeProfile>; 4],
}

/// The six (closed) or five (open) panels, laid out flat in a row. Edges with the same outer
/// length share one segment count, so mating edges are exactly complementary: front/back
/// start every edge with a finger, left/right start their vertical edges with a slot, bottom/top
/// start every edge with a slot. Kerf is not applied (CAM does that once).
pub fn finger_box(p: &BoxParams) -> Result<Vec<Panel>, GenError> {
    let t = positive("thickness", p.thickness)?;
    positive("width", p.width)?;
    positive("depth", p.depth)?;
    positive("height", p.height)?;
    let fw = positive("finger_width", p.finger_width)?;
    let gap = non_negative("spacing", p.spacing)?;
    if fw <= t {
        return Err(GenError::TooSmall { param: "finger_width", min_mm: t, value_mm: fw });
    }
    let joint =
        |length: f64, start: EdgeStart| FingerJoint { length, thickness: t, finger_width: fw, fit: 0.0 }.profile(start);
    let (w, d, h) = (p.width, p.depth, p.height);
    for (param, len) in [("width", w), ("depth", d), ("height", h)] {
        let n = FingerJoint { length: len, thickness: t, finger_width: fw, fit: 0.0 }.segments()?;
        if len / f64::from(n) <= t {
            return Err(GenError::TooSmall { param, min_mm: 3.0 * t, value_mm: len });
        }
    }
    let closed = p.lid == Lid::Closed;
    let top = |len: f64, start| if closed { joint(len, start).map(Some) } else { Ok(None) };
    use EdgeStart::{Finger, Slot};
    let mut specs: Vec<(PanelRole, f64, f64, [Option<EdgeProfile>; 4])> = Vec::new();
    for role in [PanelRole::Bottom, PanelRole::Top] {
        if role == PanelRole::Top && !closed {
            continue;
        }
        specs.push((
            role,
            w,
            d,
            [Some(joint(w, Slot)?), Some(joint(d, Slot)?), Some(joint(w, Slot)?), Some(joint(d, Slot)?)],
        ));
    }
    for role in [PanelRole::Front, PanelRole::Back] {
        specs.push((
            role,
            w,
            h,
            [Some(joint(w, Finger)?), Some(joint(h, Finger)?), top(w, Finger)?, Some(joint(h, Finger)?)],
        ));
    }
    for role in [PanelRole::Left, PanelRole::Right] {
        specs.push((
            role,
            d,
            h,
            [Some(joint(d, Finger)?), Some(joint(h, Slot)?), top(d, Finger)?, Some(joint(h, Slot)?)],
        ));
    }
    let mut x = 0.0;
    let mut panels = Vec::with_capacity(specs.len());
    for (role, pw, ph, edges) in specs {
        let path = panel_outline(Point::new(x, 0.0), pw, ph, &edges)?;
        x += pw + gap;
        panels.push(Panel { role, width: pw, height: ph, path, edges });
    }
    Ok(panels)
}

/// Depth of each segment (0 at the outer edge, `depth` for slots); a flat edge is one segment.
fn depths(e: &Option<EdgeProfile>, length: f64) -> Vec<(f64, f64, f64)> {
    match e {
        Some(p) => {
            p.segments.iter().map(|s| (s.from, s.to, if s.kind == SegmentKind::Slot { p.depth } else { 0.0 })).collect()
        }
        None => vec![(0.0, length, 0.0)],
    }
}

/// Walks the four edges counter-clockwise. Edge `i` runs from its corner along `u` with inward
/// normal `v`; its first/last segments are clipped by the neighbours' depths at the corners.
fn panel_outline(origin: Point, w: f64, h: f64, edges: &[Option<EdgeProfile>; 4]) -> Result<Path, GenError> {
    let frames = [
        (origin, Vector::new(1.0, 0.0), Vector::new(0.0, 1.0), w),
        (origin + Vector::new(w, 0.0), Vector::new(0.0, 1.0), Vector::new(-1.0, 0.0), h),
        (origin + Vector::new(w, h), Vector::new(-1.0, 0.0), Vector::new(0.0, -1.0), w),
        (origin + Vector::new(0.0, h), Vector::new(0.0, -1.0), Vector::new(1.0, 0.0), h),
    ];
    let segs: Vec<Vec<(f64, f64, f64)>> = (0..4).map(|i| depths(&edges[i], frames[i].3)).collect();
    let mut pts: Vec<Point> = Vec::new();
    for i in 0..4 {
        let (c, u, v, len) = frames[i];
        let prev_last = segs[(i + 3) % 4].last().map_or(0.0, |s| s.2);
        let next_first = segs[(i + 1) % 4].first().map_or(0.0, |s| s.2);
        let (s0, s1) = (prev_last, len - next_first);
        for &(from, to, depth) in &segs[i] {
            let (a, b) = (from.max(s0), to.min(s1));
            if b <= a {
                continue;
            }
            for s in [a, b] {
                let pt = c + u * s + v * depth;
                if pts.last() != Some(&pt) {
                    pts.push(pt);
                }
            }
        }
    }
    if pts.len() > 1 && pts.first() == pts.last() {
        pts.pop();
    }
    // Drop collinear middle points (adjacent segments of equal depth meet in a straight run).
    let n = pts.len();
    let keep: Vec<Point> = (0..n)
        .filter(|&i| {
            let (a, b, c) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
            (b - a).cross(c - b).abs() > 1e-12
        })
        .map(|i| pts[i])
        .collect();
    Ok(Path::polyline(&keep, true)?)
}
