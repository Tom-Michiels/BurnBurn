//! Cut Shapes (W1-L2a): splitting shapes along the outline of a closed cutter.

use std::fmt;

use burnburn_types::{CancellationToken, Length};

use crate::{
    BooleanError, BooleanOp, BooleanOptions, HitKind, HitTestTolerance, Path, Point, Segment, Subpath, boolean,
};

/// Which side of the cutter a piece lies on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CutSide {
    Inside,
    Outside,
}

/// One result of cutting a subject.
#[derive(Debug, Clone, PartialEq)]
pub struct CutPiece {
    pub path: Path,
    pub side: CutSide,
}

/// What became of one subject.
#[derive(Debug, Clone, PartialEq)]
pub struct CutResult {
    /// False when the cutter does not cross the subject; `pieces` then holds the subject untouched.
    pub changed: bool,
    pub pieces: Vec<CutPiece>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CutError {
    /// The cutter must consist of closed contours.
    OpenCutter,
    Boolean(BooleanError),
    Cancelled,
}

impl fmt::Display for CutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CutError::OpenCutter => f.write_str("the cutting shape must be closed"),
            CutError::Boolean(e) => write!(f, "cut failed: {e}"),
            CutError::Cancelled => f.write_str("cut cancelled"),
        }
    }
}

impl std::error::Error for CutError {}

/// Parameters closer than this to a segment end are the end itself (float noise).
const END_EPS: f64 = 1e-12;
/// Points this close to the cutter outline count as on it (never "inside").
const ON_OUTLINE_MM: f64 = 1e-9;

/// Cuts every subject along the outline of `cutter`.
///
/// Closed areas are split by the boolean pair (inside = subject ∩ cutter, outside = subject −
/// cutter) into separate pieces, one per island with its holes; the pieces are polygonal. Open
/// paths are split exactly at their crossings with the cutter outline; curves stay curves, and
/// pieces that only touch the outline are rejoined. A subject the cutter does not cross is
/// returned untouched (`changed == false`).
///
/// Cancellation is checked between subjects and around each boolean overlay.
pub fn cut_shapes(
    subjects: &[&Path],
    cutter: &Path,
    options: &BooleanOptions,
    cancel: &CancellationToken,
) -> Result<Vec<CutResult>, CutError> {
    if cutter.subpaths().iter().any(|s| !s.is_closed()) {
        return Err(CutError::OpenCutter);
    }
    let mut out = Vec::with_capacity(subjects.len());
    for subject in subjects {
        cancel.check().map_err(|_| CutError::Cancelled)?;
        out.push(cut_one(subject, cutter, options, cancel)?);
    }
    Ok(out)
}

fn boolean_error(e: BooleanError) -> CutError {
    match e {
        BooleanError::Cancelled => CutError::Cancelled,
        other => CutError::Boolean(other),
    }
}

fn cut_one(
    subject: &Path,
    cutter: &Path,
    options: &BooleanOptions,
    cancel: &CancellationToken,
) -> Result<CutResult, CutError> {
    let mut pieces: Vec<CutPiece> = Vec::new();
    let mut changed = false;
    let mut first_side = None;
    let closed: Vec<Subpath> = subject.subpaths().iter().filter(|s| s.is_closed()).cloned().collect();
    if !closed.is_empty() {
        let area = Path::from_subpaths(closed);
        let inside = boolean(&area, cutter, BooleanOp::Intersection, options, cancel).map_err(boolean_error)?;
        let outside = boolean(&area, cutter, BooleanOp::Difference, options, cancel).map_err(boolean_error)?;
        if inside.is_empty() || outside.is_empty() {
            let side = if inside.is_empty() { CutSide::Outside } else { CutSide::Inside };
            first_side.get_or_insert(side);
            pieces.push(CutPiece { path: area, side });
        } else {
            changed = true;
            pieces.extend(islands(&inside).into_iter().map(|path| CutPiece { path, side: CutSide::Inside }));
            pieces.extend(islands(&outside).into_iter().map(|path| CutPiece { path, side: CutSide::Outside }));
        }
    }
    for sp in subject.subpaths().iter().filter(|s| !s.is_closed()) {
        cancel.check().map_err(|_| CutError::Cancelled)?;
        let parts = split_open(sp, cutter);
        if let Some((_, side)) = parts.first() {
            first_side.get_or_insert(*side);
        }
        changed |= parts.len() > 1;
        pieces.extend(parts.into_iter().map(|(sub, side)| CutPiece { path: Path::from_subpaths(vec![sub]), side }));
    }
    if !changed {
        let side = first_side.unwrap_or(CutSide::Outside);
        return Ok(CutResult { changed: false, pieces: vec![CutPiece { path: subject.clone(), side }] });
    }
    Ok(CutResult { changed, pieces })
}

fn on_outline_tolerance() -> HitTestTolerance {
    // A positive constant; the conversions cannot fail.
    match Length::from_mm(ON_OUTLINE_MM).ok().and_then(|l| HitTestTolerance::new(l).ok()) {
        Some(t) => t,
        None => unreachable!("1e-9 mm is a valid length and tolerance"),
    }
}

fn side_of(cutter: &Path, q: Point) -> CutSide {
    match cutter.hit(q, &on_outline_tolerance()) {
        Some(HitKind::Interior) => CutSide::Inside,
        _ => CutSide::Outside,
    }
}

/// Splits closed output contours into islands: each outer contour with the holes inside it.
fn islands(path: &Path) -> Vec<Path> {
    let subs = path.subpaths();
    let area = |i: usize| subs[i].signed_area().unwrap_or(0.0);
    let outers: Vec<usize> = (0..subs.len()).filter(|&i| area(i) > 0.0).collect();
    let outer_paths: Vec<Path> = outers.iter().map(|&i| Path::from_subpaths(vec![subs[i].clone()])).collect();
    let tol = on_outline_tolerance();
    let mut members: Vec<Vec<Subpath>> = outers.iter().map(|&i| vec![subs[i].clone()]).collect();
    let mut loose: Vec<Path> = Vec::new();
    for h in (0..subs.len()).filter(|&i| area(i) <= 0.0) {
        let host = outers
            .iter()
            .enumerate()
            .filter(|(k, _)| outer_paths[*k].hit(subs[h].start(), &tol).is_some())
            .min_by(|(_, a), (_, b)| area(**a).total_cmp(&area(**b)))
            .map(|(k, _)| k);
        match host {
            Some(k) => members[k].push(subs[h].clone()),
            None => loose.push(Path::from_subpaths(vec![subs[h].clone()])),
        }
    }
    members.into_iter().map(Path::from_subpaths).chain(loose).collect()
}

/// A sub-curve `[t0, t1]` of `seg` (starting at `from`) with its start point.
fn sub_segment(from: Point, seg: &Segment, t0: f64, t1: f64) -> (Point, Segment) {
    let head = if t1 < 1.0 { seg.split(from, t1).0 } else { *seg };
    if t0 > 0.0 {
        let (before, tail) = head.split(from, t0 / t1);
        (before.end(), tail)
    } else {
        (from, head)
    }
}

/// Splits one open subpath at its crossings with the cutter; pieces on the same side are rejoined.
fn split_open(sp: &Subpath, cutter: &Path) -> Vec<(Subpath, CutSide)> {
    let n = sp.segments().len();
    let whole = || vec![(sp.clone(), middle_side(sp, cutter))];
    if n == 0 {
        return whole();
    }
    let single = Path::from_subpaths(vec![sp.clone()]);
    let mut cuts: Vec<(usize, f64)> = single
        .intersections(cutter)
        .iter()
        .map(|x| {
            let (r, t) = x.a;
            if t >= 1.0 - END_EPS && r.segment + 1 < n { (r.segment + 1, 0.0) } else { (r.segment, t) }
        })
        .filter(|&(k, t)| !(k == 0 && t <= END_EPS) && !(k == n - 1 && t >= 1.0 - END_EPS))
        .collect();
    cuts.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    cuts.dedup_by(|a, b| a.0 == b.0 && (a.1 - b.1).abs() <= END_EPS);
    if cuts.is_empty() {
        return whole();
    }
    let mut froms = Vec::with_capacity(n);
    let mut at = sp.start();
    for seg in sp.segments() {
        froms.push(at);
        at = seg.end();
    }
    let mut bounds = vec![(0usize, 0.0f64)];
    bounds.extend(cuts);
    bounds.push((n - 1, 1.0));
    let mut parts: Vec<(Point, Vec<Segment>, CutSide)> = Vec::new();
    for w in bounds.windows(2) {
        let ((k0, t0), (k1, t1)) = (w[0], w[1]);
        let mut start = None;
        let mut segs = Vec::new();
        for k in k0..=k1 {
            let (a, b) = (if k == k0 { t0 } else { 0.0 }, if k == k1 { t1 } else { 1.0 });
            if b - a <= END_EPS {
                continue;
            }
            let (from, seg) = sub_segment(froms[k], &sp.segments()[k], a, b);
            start.get_or_insert(from);
            segs.push(seg);
        }
        let Some(start) = start else { continue };
        let side = middle_side_of(start, &segs, cutter);
        match parts.last_mut() {
            Some(prev) if prev.2 == side => prev.1.extend(segs),
            _ => parts.push((start, segs, side)),
        }
    }
    if parts.len() <= 1 {
        return whole();
    }
    parts
        .into_iter()
        .filter_map(|(start, segs, side)| Subpath::new(start, segs, false).ok().map(|s| (s, side)))
        .collect()
}

fn middle_side(sp: &Subpath, cutter: &Path) -> CutSide {
    middle_side_of(sp.start(), sp.segments(), cutter)
}

/// Side of the middle of the middle segment of a run (a run is entirely on one side).
fn middle_side_of(start: Point, segs: &[Segment], cutter: &Path) -> CutSide {
    let Some(k) = segs.len().checked_sub(1).map(|l| l / 2) else { return side_of(cutter, start) };
    let from = if k == 0 { start } else { segs[k - 1].end() };
    side_of(cutter, segs[k].point_at(from, 0.5))
}
