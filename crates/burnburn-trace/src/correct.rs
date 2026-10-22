//! Local trace correction (IM19): re-trace one region with its own recipe while every contour
//! outside the region keeps its identity and its segments bit-for-bit.

use std::collections::BTreeSet;
use std::sync::Arc;

use std::collections::{BTreeMap, HashMap};

use burnburn_geometry::{Bounds, Point, Subpath};
use burnburn_image::{PreparedImage, RegionMask, RegionMode, RegionShape, SourceImage};
use burnburn_types::{
    CancellationToken, CanonicalEncode, CanonicalWriter, ContentHash, HashDomain, NonFiniteValue, TaskId,
};

use crate::contour::{self, ContourKind};
use crate::error::TraceError;
use crate::fit::{self, FitParams, Fitted};
use crate::recipe::TraceRecipe;
use crate::result::{ContourId, ContourOrigin, Outlines, TraceResult, TracedContour, bounds_of, fit_params, summarize};

/// Identity of one local correction within a trace lineage (1, 2, … in application order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CorrectionId(pub u64);

/// A user-fixed node: the refitted geometry must pass exactly through `at_px` (adjusted pixel
/// coordinates, y-down) if the corrected boundary comes within one pixel of it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anchor {
    pub at_px: [f64; 2],
}

/// Optional node-count goal for the refitted parts: the tolerance is raised (doubling) up to
/// `max_tolerance_mm` until the whole result has at most `max_nodes` segments.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeTarget {
    pub max_nodes: usize,
    pub max_tolerance_mm: f64,
}

/// What the node target achieved; `met == false` is reported, never hidden.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeTargetOutcome {
    pub met: bool,
    pub nodes: usize,
    pub tolerance_mm: f64,
}

/// What a correction was bound to in its base result: enough to recognise the same contour after
/// a re-segmentation, or to refuse a re-application (IM24).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContourBinding {
    pub id: ContourId,
    pub kind: ContourKind,
    pub area_mm2: f64,
    pub bounds: Bounds,
}

/// Source identity of one local correction: which region, which recipe, on which base.
#[derive(Debug, Clone, PartialEq)]
pub struct CorrectionRecord {
    pub id: CorrectionId,
    pub region: RegionShape,
    /// The local recipe (same scale, crop and rotation as the base).
    pub recipe: TraceRecipe,
    /// `recipe.key(source)`.
    pub local_key: ContentHash,
    /// `recipe_key` of the result this correction was applied to.
    pub base_inputs: ContentHash,
    pub anchors: Vec<Anchor>,
    pub protected: BTreeSet<ContourId>,
    pub node_target: Option<NodeTarget>,
    /// Base contours the region changed (partially refitted or removed), with their signature.
    pub touched: Vec<ContourBinding>,
    /// Contours of the base that the region overlapped but left unchanged (incl. protected ones).
    pub untouched_in_region: Vec<ContourBinding>,
    pub created: Vec<ContourId>,
    pub removed: Vec<ContourId>,
    pub node_outcome: Option<NodeTargetOutcome>,
}

/// One local correction request.
#[derive(Debug, Clone)]
pub struct CorrectionRequest {
    pub task: TaskId,
    pub previous: Arc<TraceResult>,
    pub source: Arc<SourceImage>,
    /// In adjusted pixel coordinates of the previous result.
    pub region: RegionShape,
    /// Local override; `image.scale`, `image.crop`, `image.rotation` must equal the base recipe.
    pub recipe: TraceRecipe,
    pub anchors: Vec<Anchor>,
    /// Contours that never change, also inside the region.
    pub protected: BTreeSet<ContourId>,
    pub node_target: Option<NodeTarget>,
}

/// Re-traces `req.region` with the local recipe on top of `req.previous`.
///
/// The working field is the previous field outside the region (and in a 2.5 px band around
/// protected contours) and the local recipe's field inside it. Contours are re-extracted from that
/// field; a contour whose sub-pixel outline is unchanged is taken over verbatim (id, origin, path),
/// a contour that changed only partly keeps its id and every segment whose source data is
/// unchanged, and only the changed stretches are refitted between those fixed nodes.
pub fn retrace_region(req: &CorrectionRequest, cancel: &CancellationToken) -> Result<TraceResult, TraceError> {
    let prev = &*req.previous;
    req.recipe.validate()?;
    req.region.validate()?;
    cancel.check()?;
    if req.source.content_hash() != prev.source_hash {
        return Err(TraceError::CorrectionMismatch("source"));
    }
    let (base_image, local_image) = (&prev.recipe.image, &req.recipe.image);
    if base_image.scale != local_image.scale {
        return Err(TraceError::CorrectionMismatch("image.scale"));
    }
    if base_image.crop != local_image.crop || base_image.rotation != local_image.rotation {
        return Err(TraceError::CorrectionMismatch("image.crop/rotation"));
    }
    if req.recipe.contour.diagonal != prev.recipe.contour.diagonal {
        return Err(TraceError::CorrectionMismatch("contour.diagonal"));
    }
    if let Some(t) = req.node_target
        && !(t.max_tolerance_mm.is_finite() && t.max_tolerance_mm >= req.recipe.fit.tolerance_mm)
    {
        return Err(TraceError::InvalidRecipe("node_target.max_tolerance_mm"));
    }
    if req.anchors.iter().any(|a| !(a.at_px[0].is_finite() && a.at_px[1].is_finite())) {
        return Err(TraceError::InvalidRecipe("anchors"));
    }
    for id in &req.protected {
        if prev.contour(*id).is_none() {
            return Err(TraceError::UnknownContour(*id));
        }
    }
    let local = burnburn_image::prepare(&req.source, local_image, cancel)?;
    let (w, h) = (prev.width_px, prev.height_px);
    if local.width() != w || local.height() != h || prev.field.len() != w as usize * h as usize {
        return Err(TraceError::CorrectionMismatch("size"));
    }
    let frame = Frame::new(prev);

    // A protected detail is its outline (2.5 px band) plus everything it encloses; there the
    // previous field is kept.
    let mut keep_prev = vec![false; w as usize * h as usize];
    for c in prev.contours.iter().filter(|c| req.protected.contains(&c.id)) {
        let poly: Vec<(f64, f64)> = c.source_points.iter().map(|&p| frame.to_px(p)).collect();
        let (mut x_lo, mut y_lo, mut x_hi, mut y_hi) =
            (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for &(x, y) in &poly {
            (x_lo, y_lo, x_hi, y_hi) = (x_lo.min(x), y_lo.min(y), x_hi.max(x), y_hi.max(y));
        }
        let clamp = |v: f64, max: u32| (v.max(0.0) as u32).min(max);
        for y in clamp(y_lo.floor(), h)..clamp(y_hi.ceil() + 1.0, h) {
            for x in clamp(x_lo.floor(), w)..clamp(x_hi.ceil() + 1.0, w) {
                if inside_polygon(&poly, f64::from(x) + 0.5, f64::from(y) + 0.5) {
                    keep_prev[(y * w + x) as usize] = true;
                }
            }
        }
        for &p in &c.source_points {
            let (px, py) = frame.to_px(p);
            let (x0, x1) = ((px - 3.0).floor().max(0.0) as u32, ((px + 3.0).ceil().max(0.0) as u32).min(w));
            let (y0, y1) = ((py - 3.0).floor().max(0.0) as u32, ((py + 3.0).ceil().max(0.0) as u32).min(h));
            for y in y0..y1 {
                for x in x0..x1 {
                    if (f64::from(x) + 0.5 - px).abs() <= 2.5 && (f64::from(y) + 0.5 - py).abs() <= 2.5 {
                        keep_prev[(y * w + x) as usize] = true;
                    }
                }
            }
        }
    }
    cancel.check()?;
    let mut field = prev.field.clone();
    let mut mask = prev.mask.clone();
    for y in 0..h {
        if y % 64 == 0 {
            cancel.check()?;
        }
        for x in 0..w {
            let i = (y * w + x) as usize;
            if !keep_prev[i] && req.region.contains_pixel(x, y) {
                field[i] = local.field[i];
                mask.set(x, y, local.field[i] > 0.0);
            }
        }
    }
    let prepared = PreparedImage {
        source_hash: prev.source_hash,
        scale: prev.scale,
        threshold: local.threshold,
        mask,
        field,
        perspective: local.perspective,
    };
    let set = contour::extract(&prepared, prev.recipe.contour.diagonal, cancel)?;
    let outlines = Outlines::new(&set, &prepared, &prev.recipe.contour)?;
    let kept = outlines.kept();
    cancel.check()?;

    // Match every new outline to the previous contours by exact sub-pixel points.
    let mut owner: HashMap<(u64, u64), usize> = HashMap::new();
    for (oi, c) in prev.contours.iter().enumerate() {
        for &p in &c.source_points {
            owner.entry(bits(p)).or_insert(oi);
        }
    }
    let mut claimed = vec![false; prev.contours.len()];
    let mut plan: Vec<Plan> = Vec::with_capacity(kept.len());
    for &i in &kept {
        let pts = &outlines.raws[i].0;
        let mut shared: BTreeMap<usize, usize> = BTreeMap::new();
        for &p in pts {
            if let Some(&oi) = owner.get(&bits(p)) {
                *shared.entry(oi).or_default() += 1;
            }
        }
        let exact = shared.keys().copied().find(|&oi| !claimed[oi] && prev.contours[oi].source_points == *pts);
        if let Some(oi) = exact {
            claimed[oi] = true;
            plan.push(Plan::Keep(oi));
            continue;
        }
        let best = shared
            .iter()
            .filter(|&(&oi, _)| !claimed[oi] && !req.protected.contains(&prev.contours[oi].id))
            .max_by_key(|&(&oi, &count)| (count, std::cmp::Reverse(oi)))
            .map(|(&oi, _)| oi);
        match best {
            Some(oi) => {
                claimed[oi] = true;
                plan.push(Plan::Partial(oi));
            }
            None => plan.push(Plan::New),
        }
    }
    for c in prev.contours.iter().filter(|c| req.protected.contains(&c.id)) {
        let kept_verbatim = plan.iter().any(|p| matches!(p, Plan::Keep(oi) if prev.contours[*oi].id == c.id));
        if !kept_verbatim {
            return Err(TraceError::ProtectedChanged(c.id));
        }
    }

    let correction = CorrectionId(prev.corrections.last().map_or(1, |r| r.id.0 + 1));
    let anchors_mm: Vec<Point> = req.anchors.iter().map(|a| frame.to_mm(a.at_px)).collect();
    let anchor_reach = frame.mm_per_px();
    let base_params = fit_params(&req.recipe, &prev.scale);
    let fixed_nodes: usize = plan
        .iter()
        .filter_map(|p| match p {
            Plan::Keep(oi) => Some(prev.contours[*oi].path.segments().len()),
            _ => None,
        })
        .sum();

    let build = |params: &FitParams| -> Result<Vec<TracedContour>, TraceError> {
        let mut next_id = prev.next_contour_id;
        let mut out = Vec::with_capacity(plan.len());
        for (step, &i) in plan.iter().zip(&kept) {
            cancel.check()?;
            let (pts, area) = &outlines.raws[i];
            let anchors = anchors_on(pts, &anchors_mm, anchor_reach);
            let contour = match *step {
                Plan::Keep(oi) => prev.contours[oi].clone(),
                Plan::Partial(oi) => {
                    let old = &prev.contours[oi];
                    let fitted = refit_partial(old, pts, &anchors, params)
                        .unwrap_or_else(|| fit::fit_closed_anchored(pts, &anchors, params));
                    let path = Subpath::new(fitted.start, fitted.segments, true).map_err(|_| TraceError::NonFinite)?;
                    TracedContour {
                        id: old.id,
                        origin: ContourOrigin::Correction(correction),
                        kind: old.kind,
                        parent: None,
                        depth: 0,
                        path,
                        source_points: pts.clone(),
                        segment_spans: fitted.spans,
                        area_mm2: area.abs(),
                    }
                }
                Plan::New => {
                    let fitted = fit::fit_closed_anchored(pts, &anchors, params);
                    let path = Subpath::new(fitted.start, fitted.segments, true).map_err(|_| TraceError::NonFinite)?;
                    let id = ContourId(next_id);
                    next_id += 1;
                    TracedContour {
                        id,
                        origin: ContourOrigin::Correction(correction),
                        kind: ContourKind::Outer,
                        parent: None,
                        depth: 0,
                        path,
                        source_points: pts.clone(),
                        segment_spans: fitted.spans,
                        area_mm2: area.abs(),
                    }
                }
            };
            out.push(contour);
        }
        outlines.link_parents(&kept, &mut out);
        Ok(out)
    };

    let mut tolerance = base_params.tol;
    let mut contours = build(&base_params)?;
    let mut node_outcome = None;
    if let Some(target) = req.node_target {
        loop {
            let nodes: usize = contours.iter().map(|c| c.path.segments().len()).sum();
            let met = nodes <= target.max_nodes;
            let next = tolerance * 2.0;
            if met || next > target.max_tolerance_mm || fixed_nodes > target.max_nodes {
                node_outcome = Some(NodeTargetOutcome { met, nodes, tolerance_mm: tolerance });
                break;
            }
            tolerance = next;
            contours = build(&FitParams { tol: tolerance, ..base_params })?;
        }
    }
    cancel.check()?;

    let binding = |c: &TracedContour| ContourBinding {
        id: c.id,
        kind: c.kind,
        area_mm2: c.area_mm2,
        bounds: bounds_of(&c.source_points),
    };
    let mut touched = Vec::new();
    let mut untouched_in_region = Vec::new();
    let mut removed = Vec::new();
    for (oi, c) in prev.contours.iter().enumerate() {
        let kept_verbatim = plan.iter().any(|p| matches!(p, Plan::Keep(k) if *k == oi));
        if !claimed[oi] {
            removed.push(c.id);
            touched.push(binding(c));
        } else if !kept_verbatim {
            touched.push(binding(c));
        } else if c.source_points.iter().any(|&p| frame.in_region(&req.region, p)) {
            untouched_in_region.push(binding(c));
        }
    }
    let created: Vec<ContourId> = contours.iter().filter(|c| c.id.0 >= prev.next_contour_id).map(|c| c.id).collect();
    let next_contour_id = contours.iter().map(|c| c.id.0 + 1).max().unwrap_or(0).max(prev.next_contour_id);
    let record = CorrectionRecord {
        id: correction,
        region: req.region.clone(),
        recipe: req.recipe.clone(),
        local_key: req.recipe.key(prev.source_hash).map_err(|_| TraceError::NonFinite)?,
        base_inputs: prev.recipe_key,
        anchors: req.anchors.clone(),
        protected: req.protected.clone(),
        node_target: req.node_target,
        touched,
        untouched_in_region,
        created,
        removed,
        node_outcome,
    };
    let mut corrections = prev.corrections.clone();
    corrections.push(record);
    let recipe_key = correction_chain_key(prev.base_key, &corrections).map_err(|_| TraceError::NonFinite)?;
    let (components, mut stats) = summarize(&contours, &prev.scale);
    stats.keep_unmatched = outlines.keep_unmatched;
    Ok(TraceResult {
        task: req.task,
        source_revision: prev.source_revision,
        source_hash: prev.source_hash,
        recipe_key,
        base_key: prev.base_key,
        recipe: prev.recipe.clone(),
        corrections,
        next_contour_id,
        engine: prev.engine,
        scale: prev.scale,
        width_px: w,
        height_px: h,
        mask: prepared.mask,
        contours,
        components,
        removed: outlines.removed,
        stats,
        field: prepared.field,
    })
}

/// Inputs key of a corrected result: the base key followed by every correction's identity
/// (id, local recipe key, region, anchors, protected ids, node target).
pub fn correction_chain_key(
    base_key: ContentHash,
    corrections: &[CorrectionRecord],
) -> Result<ContentHash, NonFiniteValue> {
    let mut w = CanonicalWriter::new();
    w.tag("trace-correction-chain-v1");
    w.bytes(base_key.as_bytes());
    w.u64(corrections.len() as u64);
    for r in corrections {
        w.u64(r.id.0);
        w.bytes(r.local_key.as_bytes());
        RegionMask { mode: RegionMode::Include, shape: r.region.clone() }.encode(&mut w);
        w.u64(r.anchors.len() as u64);
        for a in &r.anchors {
            w.f64(a.at_px[0]);
            w.f64(a.at_px[1]);
        }
        w.u64(r.protected.len() as u64);
        for id in &r.protected {
            w.u64(id.0);
        }
        match r.node_target {
            Some(t) => {
                w.tag("node-target");
                w.u64(t.max_nodes as u64);
                w.f64(t.max_tolerance_mm);
            }
            None => w.tag("no-node-target"),
        }
    }
    Ok(ContentHash::of(HashDomain::TraceKey, &w.finish()?))
}

#[derive(Debug, Clone, Copy)]
enum Plan {
    /// Outline unchanged: take the previous contour verbatim.
    Keep(usize),
    /// Outline shares points with this previous contour: keep its unchanged segments.
    Partial(usize),
    New,
}

/// Even-odd point-in-polygon test.
fn inside_polygon(poly: &[(f64, f64)], x: f64, y: f64) -> bool {
    let mut inside = false;
    let mut j = poly.len().wrapping_sub(1);
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.1 > y) != (b.1 > y) && x < a.0 + (y - a.1) / (b.1 - a.1) * (b.0 - a.0) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn bits(p: Point) -> (u64, u64) {
    (p.x.to_bits(), p.y.to_bits())
}

/// Pixel ↔ millimetre mapping of a result (the inverse of the tracer's `to_mm`).
pub(crate) struct Frame {
    sx: f64,
    sy: f64,
    height: f64,
}

impl Frame {
    pub(crate) fn new(r: &TraceResult) -> Frame {
        Frame { sx: r.scale.mm_per_pixel_x(), sy: r.scale.mm_per_pixel_y(), height: f64::from(r.height_px) }
    }

    fn to_px(&self, p: Point) -> (f64, f64) {
        (p.x / self.sx, self.height - p.y / self.sy)
    }

    fn to_mm(&self, p: [f64; 2]) -> Point {
        Point::new(p[0] * self.sx, (self.height - p[1]) * self.sy)
    }

    fn mm_per_px(&self) -> f64 {
        0.5 * (self.sx + self.sy)
    }

    pub(crate) fn in_region(&self, region: &RegionShape, p: Point) -> bool {
        let (x, y) = self.to_px(p);
        x >= 0.0 && y >= 0.0 && region.contains_pixel(x.floor() as u32, y.floor() as u32)
    }
}

/// Anchors within `reach` of the outline, as (nearest point index, exact anchor position).
fn anchors_on(pts: &[Point], anchors: &[Point], reach: f64) -> Vec<(usize, Point)> {
    anchors
        .iter()
        .filter_map(|&a| {
            let (idx, d) =
                pts.iter().enumerate().map(|(i, p)| (i, p.distance(a))).min_by(|x, y| x.1.total_cmp(&y.1))?;
            (d <= reach).then_some((idx, a))
        })
        .collect()
}

/// Keeps every segment of `old` whose source points re-appear unchanged and consecutively in `pts`
/// and refits the stretches between them with those segments' end nodes fixed. `None` when no
/// segment survives (the caller refits the whole contour).
fn refit_partial(old: &TracedContour, pts: &[Point], anchors: &[(usize, Point)], params: &FitParams) -> Option<Fitted> {
    let (n_old, n) = (old.source_points.len(), pts.len());
    if n_old == 0 || n == 0 {
        return None;
    }
    let index: HashMap<(u64, u64), usize> = pts.iter().enumerate().map(|(i, &p)| (bits(p), i)).collect();
    let segs = old.path.segments();
    let mut froms = Vec::with_capacity(segs.len());
    let mut from = old.path.start();
    for s in segs {
        froms.push(from);
        from = s.end();
    }
    // Kept segment k: new index of its first source point.
    let mut kept: Vec<(usize, usize)> = Vec::new();
    for (k, &(a, b)) in old.segment_spans.iter().enumerate() {
        let Some(&start) = index.get(&bits(old.source_points[a % n_old])) else { continue };
        let unchanged =
            (a..=b).all(|i| index.get(&bits(old.source_points[i % n_old])) == Some(&((start + (i - a)) % n)));
        if unchanged && b - a < n {
            kept.push((k, start));
        }
    }
    let &(k0, base) = kept.first()?;
    let unwrap = |i: usize| base + (i + n - base % n) % n;
    // Kept segments must appear in the same cyclic order in the new outline.
    let positions: Vec<usize> = kept.iter().map(|&(_, s)| unwrap(s)).collect();
    if positions.windows(2).any(|w| w[1] <= w[0]) {
        return None;
    }
    let mut out = Fitted { start: froms[k0], segments: Vec::new(), spans: Vec::new() };
    for (t, &(k, s)) in kept.iter().enumerate() {
        let (a, b) = old.segment_spans[k];
        let s = unwrap(s);
        let e = s + (b - a);
        out.segments.push(segs[k]);
        out.spans.push((s, e));
        let (next_k, next_s) = match kept.get(t + 1) {
            Some(&(nk, ns)) => (nk, unwrap(ns)),
            None => (k0, base + n),
        };
        let contiguous = next_k == (k + 1) % segs.len() && next_s == e;
        if contiguous || (kept.len() == segs.len() && next_s == e) {
            continue;
        }
        if next_s < e {
            return None;
        }
        // Refit [e, next_s] between the fixed end node of k and the start node of next_k.
        let mut data: Vec<(Point, usize)> = vec![(segs[k].end(), e)];
        let mut splits = Vec::new();
        for j in e + 1..next_s {
            let anchor = anchors.iter().find(|a| a.0 == j % n);
            if let Some(&(_, at)) = anchor {
                splits.push(data.len());
                data.push((at, j));
            } else {
                data.push((pts[j % n], j));
            }
        }
        data.push((froms[next_k], next_s));
        fit::fit_open_indexed(&data, &splits, params, &mut out);
    }
    Some(out)
}
