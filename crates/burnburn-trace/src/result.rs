use std::collections::HashMap;
use std::sync::Arc;

use burnburn_geometry::{Bounds, Path, Point, Subpath};
use burnburn_image::{BinaryMask, PixelScale, PreparedImage, SourceImage};
use burnburn_types::{
    CancellationToken, CanonicalEncode, CanonicalWriter, ContentHash, HashDomain, RevisionId, TaskId,
};

use crate::contour::{self, ContourKind, ContourSet, RawContour};
use crate::correct::{CorrectionId, CorrectionRecord};
use crate::error::{StaleTrace, TraceError};
use crate::fit::{self, FitParams};
use crate::recipe::{ENGINE_VERSION, TraceRecipe};

/// One trace submission: an immutable source snapshot, its document revision and the recipe.
#[derive(Debug, Clone)]
pub struct TraceRequest {
    pub task: TaskId,
    pub source_revision: RevisionId,
    pub source: Arc<SourceImage>,
    pub recipe: TraceRecipe,
}

/// Stable identity of a contour within one trace lineage. A fresh trace numbers contours in
/// extraction order (raster order of their first boundary edge); a local correction keeps the
/// identity of every contour it preserves or partially refits and gives new contours ids above all
/// existing ones (never reused).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContourId(pub u64);

/// Where a contour's current geometry came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContourOrigin {
    /// The full trace with the result's base recipe.
    Trace,
    /// Created or (partly) refitted by this local correction.
    Correction(CorrectionId),
}

/// One traced contour.
#[derive(Debug, Clone, PartialEq)]
pub struct TracedContour {
    pub id: ContourId,
    pub origin: ContourOrigin,
    pub kind: ContourKind,
    /// Enclosing contour: a hole's parent is its part's outer contour, an island's parent is the hole.
    pub parent: Option<usize>,
    /// 0 for top-level parts, 1 for their holes, 2 for islands in holes, …
    pub depth: u32,
    /// Fitted, closed path in millimetres.
    pub path: Subpath,
    /// Sub-pixel source contour in millimetres (the fitting input), for comparison overlays.
    pub source_points: Vec<Point>,
    /// For segment `k` of `path`: the half-open range of `source_points` it approximates
    /// (indices may wrap past the end for the closing segment).
    pub segment_spans: Vec<(usize, usize)>,
    /// Unsigned enclosed area in mm².
    pub area_mm2: f64,
}

/// A foreground part: its outer contour and direct holes (indices into `contours`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TracedComponent {
    pub outer: usize,
    pub holes: Vec<usize>,
}

/// A part or hole dropped by `contour.min_area_mm2`; reported, never silent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RemovedFeature {
    pub kind: ContourKind,
    pub area_mm2: f64,
    pub bounds: Bounds,
}

/// Summary numbers for the tracer panel.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TraceStats {
    pub paths: usize,
    pub closed_shapes: usize,
    pub holes: usize,
    /// Total number of fitted segments (nodes).
    pub nodes: usize,
    /// Smallest kept contour area (mm²), `None` when nothing was traced.
    pub smallest_feature_mm2: Option<f64>,
    pub source_mm_per_pixel: (f64, f64),
}

/// A finished trace. It carries the identity it was computed for; the caller applies it only when
/// [`TraceResult::check_fresh`] passes.
#[derive(Debug, Clone)]
pub struct TraceResult {
    pub task: TaskId,
    /// Document revision at submission; diagnosis only (validity follows `recipe_key`, DR-14).
    pub source_revision: RevisionId,
    pub source_hash: ContentHash,
    /// Identity of every input this result was computed from: [`TraceRecipe::key`] for a fresh
    /// trace, the correction-chain key ([`crate::correction_chain_key`]) after local corrections.
    pub recipe_key: ContentHash,
    /// [`TraceRecipe::key`] of the full trace this lineage started from.
    pub base_key: ContentHash,
    /// The recipe of the full trace (local corrections carry their own recipe in their record).
    pub recipe: TraceRecipe,
    /// Local corrections applied on top of the base trace, oldest first.
    pub corrections: Vec<CorrectionRecord>,
    /// Next unused [`ContourId`] value.
    pub next_contour_id: u64,
    pub engine: &'static str,
    pub scale: PixelScale,
    pub width_px: u32,
    pub height_px: u32,
    /// The binary mask that was traced (adjusted pixel coordinates), for the comparison view.
    pub mask: BinaryMask,
    pub contours: Vec<TracedContour>,
    pub components: Vec<TracedComponent>,
    pub removed: Vec<RemovedFeature>,
    pub stats: TraceStats,
    /// Signed sub-pixel field that was traced (same layout as `mask`); local corrections compose
    /// from it so that untouched contours re-extract bit-identically.
    pub(crate) field: Vec<f32>,
}

impl TraceResult {
    /// Normalised geometry hash: every contour's id, kind and path in order (T10).
    pub fn geometry_hash(&self) -> ContentHash {
        let mut w = CanonicalWriter::new();
        w.tag("trace-geometry-v1");
        w.str(self.engine);
        w.u64(self.contours.len() as u64);
        for c in &self.contours {
            w.u64(c.id.0);
            w.tag(match c.kind {
                ContourKind::Outer => "outer",
                ContourKind::Hole => "hole",
            });
            Path::from_subpaths(vec![c.path.clone()]).encode(&mut w);
        }
        // Paths hold only finite coordinates (checked when built), so `finish` cannot fail.
        ContentHash::of(HashDomain::TraceKey, &w.finish().unwrap_or_default())
    }

    pub fn contour(&self, id: ContourId) -> Option<&TracedContour> {
        self.contours.iter().find(|c| c.id == id)
    }

    /// All contours as one path (outer and hole subpaths; nonzero and even-odd fill agree).
    pub fn path(&self) -> Path {
        Path::from_subpaths(self.contours.iter().map(|c| c.path.clone()).collect())
    }

    /// One part with its holes.
    pub fn component_path(&self, component: usize) -> Option<Path> {
        let c = self.components.get(component)?;
        let mut subs = vec![self.contours.get(c.outer)?.path.clone()];
        for &h in &c.holes {
            subs.push(self.contours.get(h)?.path.clone());
        }
        Some(Path::from_subpaths(subs))
    }

    /// `Ok` only if this result belongs to the latest submitted task for this trace and its inputs
    /// (source pixels, recipe incl. crop/scale, engine) still equal `current_inputs`, the
    /// [`TraceRecipe::key`] the document holds now (DR-14). Unrelated document edits do not make a
    /// trace stale; `source_revision` is kept for diagnosis only.
    pub fn check_fresh(&self, latest_task: TaskId, current_inputs: ContentHash) -> Result<(), StaleTrace> {
        if self.task != latest_task {
            return Err(StaleTrace::Superseded { result: self.task, latest: latest_task });
        }
        if self.recipe_key != current_inputs {
            return Err(StaleTrace::InputsChanged { traced: self.recipe_key, current: current_inputs });
        }
        Ok(())
    }
}

/// Progress points at which the tracer has just checked its cancellation token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceStage {
    Prepared,
    Contoured { contours: usize },
    Fitted { index: usize, of: usize },
}

/// Observes progress (tests, progress bars). Called on the tracing thread.
pub trait TraceObserver {
    fn stage(&mut self, stage: TraceStage);
}

struct NoObserver;

impl TraceObserver for NoObserver {
    fn stage(&mut self, _: TraceStage) {}
}

pub fn trace(request: &TraceRequest, cancel: &CancellationToken) -> Result<TraceResult, TraceError> {
    trace_observed(request, cancel, &mut NoObserver)
}

pub fn trace_observed(
    request: &TraceRequest,
    cancel: &CancellationToken,
    observer: &mut dyn TraceObserver,
) -> Result<TraceResult, TraceError> {
    let recipe = &request.recipe;
    recipe.validate()?;
    cancel.check()?;
    let recipe_key = recipe.key(request.source.content_hash()).map_err(|_| TraceError::NonFinite)?;
    let prepared = burnburn_image::prepare(&request.source, &recipe.image, cancel)?;
    cancel.check()?;
    observer.stage(TraceStage::Prepared);

    let set = contour::extract(&prepared, recipe.contour.diagonal, cancel)?;
    cancel.check()?;
    let outlines = Outlines::new(&set, &prepared, recipe.contour.min_area_mm2)?;
    observer.stage(TraceStage::Contoured { contours: outlines.raws.len() });
    cancel.check()?;

    let params = fit_params(recipe, &prepared.scale);
    let kept = outlines.kept();
    let mut contours = Vec::with_capacity(kept.len());
    for (k, &i) in kept.iter().enumerate() {
        cancel.check()?;
        let (pts, area) = &outlines.raws[i];
        let fitted = fit::fit_closed(pts, &params);
        let path = Subpath::new(fitted.start, fitted.segments, true).map_err(|_| TraceError::NonFinite)?;
        contours.push(TracedContour {
            id: ContourId(k as u64),
            origin: ContourOrigin::Trace,
            kind: outlines.tree[i].kind,
            parent: None,
            depth: outlines.tree[i].depth,
            path,
            source_points: pts.clone(),
            segment_spans: fitted.spans,
            area_mm2: area.abs(),
        });
        observer.stage(TraceStage::Fitted { index: k, of: kept.len() });
    }
    outlines.link_parents(&kept, &mut contours);
    cancel.check()?;
    let (components, stats) = summarize(&contours, &prepared.scale);
    Ok(TraceResult {
        task: request.task,
        source_revision: request.source_revision,
        source_hash: request.source.content_hash(),
        recipe_key,
        base_key: recipe_key,
        recipe: recipe.clone(),
        corrections: Vec::new(),
        next_contour_id: contours.len() as u64,
        engine: ENGINE_VERSION,
        scale: prepared.scale,
        width_px: prepared.width(),
        height_px: prepared.height(),
        mask: prepared.mask,
        contours,
        components,
        removed: outlines.removed,
        stats,
        field: prepared.field,
    })
}

pub(crate) fn fit_params(recipe: &TraceRecipe, scale: &PixelScale) -> FitParams {
    let (sx, sy) = (scale.mm_per_pixel_x(), scale.mm_per_pixel_y());
    FitParams {
        tol: recipe.fit.tolerance_mm,
        corner_cos: recipe.fit.corner_angle_deg.to_radians().cos(),
        window: recipe.fit.corner_window_px * 0.5 * (sx + sy),
        curves: recipe.fit.curves,
    }
}

/// Millimetre outlines of an extracted contour set, their tree and the area filter.
pub(crate) struct Outlines {
    /// Points in mm (y-up) and signed area (outer > 0).
    pub raws: Vec<(Vec<Point>, f64)>,
    pub tree: Vec<Node>,
    pub dropped: Vec<bool>,
    pub removed: Vec<RemovedFeature>,
}

impl Outlines {
    pub(crate) fn new(set: &ContourSet, prepared: &PreparedImage, min_area: f64) -> Result<Outlines, TraceError> {
        let (sx, sy) = (prepared.scale.mm_per_pixel_x(), prepared.scale.mm_per_pixel_y());
        let height = f64::from(prepared.height());
        let to_mm = |p: &[f64; 2]| Point::new(p[0] * sx, (height - p[1]) * sy);
        let mut raws = Vec::with_capacity(set.contours.len());
        for raw in &set.contours {
            let pts: Vec<Point> = raw.points.iter().map(to_mm).collect();
            if !pts.iter().all(|p| p.is_finite()) {
                return Err(TraceError::NonFinite);
            }
            let area = shoelace(&pts);
            raws.push((pts, area));
        }
        let tree = build_tree(&raws, &set.contours, set.outside_label);
        // Drop parts/holes below the area threshold, with their whole subtree; report each dropped root.
        let mut dropped = vec![false; raws.len()];
        let mut removed = Vec::new();
        for i in order_by_depth(&tree) {
            let parent_dropped = tree[i].parent.is_some_and(|p| dropped[p]);
            if parent_dropped {
                dropped[i] = true;
            } else if raws[i].1.abs() < min_area {
                dropped[i] = true;
                removed.push(RemovedFeature {
                    kind: tree[i].kind,
                    area_mm2: raws[i].1.abs(),
                    bounds: bounds_of(&raws[i].0),
                });
            }
        }
        Ok(Outlines { raws, tree, dropped, removed })
    }

    pub(crate) fn kept(&self) -> Vec<usize> {
        (0..self.raws.len()).filter(|&i| !self.dropped[i]).collect()
    }

    /// Sets `parent` of `contours[k]` (built from `kept[k]`) to the index of its kept parent.
    pub(crate) fn link_parents(&self, kept: &[usize], contours: &mut [TracedContour]) {
        let mut new_index = vec![usize::MAX; self.raws.len()];
        for (k, &i) in kept.iter().enumerate() {
            new_index[i] = k;
        }
        for (k, &i) in kept.iter().enumerate() {
            if let Some(c) = contours.get_mut(k) {
                c.parent = self.tree[i].parent.map(|p| new_index[p]);
                c.kind = self.tree[i].kind;
                c.depth = self.tree[i].depth;
            }
        }
    }
}

/// Components (outer + direct holes) and panel numbers for a contour list.
pub(crate) fn summarize(contours: &[TracedContour], scale: &PixelScale) -> (Vec<TracedComponent>, TraceStats) {
    let mut components: Vec<TracedComponent> = Vec::new();
    let mut component_of = vec![usize::MAX; contours.len()];
    for (k, c) in contours.iter().enumerate() {
        if c.kind == ContourKind::Outer {
            component_of[k] = components.len();
            components.push(TracedComponent { outer: k, holes: Vec::new() });
        }
    }
    for (k, c) in contours.iter().enumerate() {
        if let (ContourKind::Hole, Some(p)) = (c.kind, c.parent)
            && let Some(comp) = component_of.get(p).and_then(|&ci| components.get_mut(ci))
        {
            comp.holes.push(k);
        }
    }
    let stats = TraceStats {
        paths: contours.len(),
        closed_shapes: components.len(),
        holes: contours.iter().filter(|c| c.kind == ContourKind::Hole).count(),
        nodes: contours.iter().map(|c| c.path.segments().len()).sum(),
        smallest_feature_mm2: contours.iter().map(|c| c.area_mm2).min_by(f64::total_cmp),
        source_mm_per_pixel: (scale.mm_per_pixel_x(), scale.mm_per_pixel_y()),
    };
    (components, stats)
}

fn shoelace(pts: &[Point]) -> f64 {
    let n = pts.len();
    let twice: f64 = (0..n).map(|i| pts[i].to_vector().cross(pts[(i + 1) % n].to_vector())).sum();
    0.5 * twice
}

pub(crate) fn bounds_of(pts: &[Point]) -> Bounds {
    let mut b = Bounds::from_point(pts.first().copied().unwrap_or(Point::ORIGIN));
    for p in pts {
        b.include(*p);
    }
    b
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Node {
    pub kind: ContourKind,
    pub parent: Option<usize>,
    pub depth: u32,
}

/// Tree from component labels: an outer contour bounds foreground part F against the background
/// region B around it; a hole contour bounds a background region B enclosed by part F. A part's
/// parent is the hole contour of the region around it; a hole's parent is its part's outer contour.
fn build_tree(raws: &[(Vec<Point>, f64)], set: &[RawContour], outside: u32) -> Vec<Node> {
    let mut outer_of_part: HashMap<u32, usize> = HashMap::new();
    let mut hole_of_region: HashMap<u32, usize> = HashMap::new();
    for (i, ((_, area), raw)) in raws.iter().zip(set).enumerate() {
        if *area > 0.0 {
            outer_of_part.insert(raw.fg_label, i);
        } else {
            hole_of_region.insert(raw.bg_label, i);
        }
    }
    let parents: Vec<(ContourKind, Option<usize>)> = raws
        .iter()
        .zip(set)
        .map(|((_, area), raw)| {
            if *area > 0.0 {
                let p = if raw.bg_label == outside { None } else { hole_of_region.get(&raw.bg_label).copied() };
                (ContourKind::Outer, p)
            } else {
                (ContourKind::Hole, outer_of_part.get(&raw.fg_label).copied())
            }
        })
        .collect();
    (0..raws.len())
        .map(|i| {
            let mut depth = 0u32;
            let mut at = parents[i].1;
            while let Some(p) = at {
                depth += 1;
                at = if depth as usize > raws.len() { None } else { parents[p].1 };
            }
            Node { kind: parents[i].0, parent: parents[i].1, depth }
        })
        .collect()
}

/// Indices sorted by depth so parents are visited before children (stable within a depth).
fn order_by_depth(tree: &[Node]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..tree.len()).collect();
    idx.sort_by_key(|&i| tree[i].depth);
    idx
}
