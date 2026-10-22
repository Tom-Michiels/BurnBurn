//! Advanced recipes (IM24): A/B comparison of two recipes on one source, batch tracing with a
//! per-image problem report, and re-applying a recorded local correction to a new base result
//! only when its bindings still hold.

use std::collections::BTreeSet;
use std::sync::Arc;

use burnburn_image::SourceImage;
use burnburn_types::{CancellationToken, ContentHash, RevisionId, TaskId};

use crate::correct::{ContourBinding, CorrectionRecord, CorrectionRequest, Frame, retrace_region};
use crate::error::TraceError;
use crate::preview::comparison_preview;
use crate::recipe::TraceRecipe;
use crate::result::{ContourId, TraceRequest, TraceResult, TracedContour, bounds_of, trace};

/// Panel numbers of one traced variant.
#[derive(Debug, Clone, PartialEq)]
pub struct RecipeSummary {
    pub recipe_key: ContentHash,
    /// [`TraceResult::geometry_hash`]: equal source/recipe/engine ⇒ equal hash.
    pub geometry_hash: ContentHash,
    pub paths: usize,
    pub closed_shapes: usize,
    pub holes: usize,
    pub nodes: usize,
    pub removed: usize,
    pub max_deviation_mm: f64,
}

/// How many of the stored local corrections each variant of a comparison carries (G8-trace, T10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CompareCorrections {
    /// Corrections handed to the comparison.
    pub stored: usize,
    /// Applied to variant A / variant B. A correction that no longer binds on a variant's contours
    /// (another recipe changed them) is not applied there, never to something else.
    pub a_applied: usize,
    pub b_applied: usize,
}

/// Two variants of the same source in one frame (same zoom/position in the UI).
#[derive(Debug, Clone)]
pub struct RecipeComparison {
    pub a: RecipeSummary,
    pub b: RecipeSummary,
    pub a_result: Arc<TraceResult>,
    pub b_result: Arc<TraceResult>,
    pub corrections: CompareCorrections,
    /// Corrections that do not bind on variant B and were not relinked (G9): the operator may link them
    /// to B's contours with [`compare_recipes_corrected_relinking`].
    pub b_unbound: Vec<UnboundCorrection>,
    /// Corrections the operator linked to B's contours although their bindings no longer held.
    pub b_relinked: Vec<usize>,
}

pub fn compare_recipes(
    source: &Arc<SourceImage>,
    a: &TraceRecipe,
    b: &TraceRecipe,
    task: TaskId,
    cancel: &CancellationToken,
) -> Result<RecipeComparison, TraceError> {
    let run = |recipe: &TraceRecipe| -> Result<Arc<TraceResult>, TraceError> {
        let req =
            TraceRequest { task, source_revision: RevisionId::INITIAL, source: source.clone(), recipe: recipe.clone() };
        Ok(Arc::new(trace(&req, cancel)?))
    };
    let (a_result, b_result) = (run(a)?, run(b)?);
    Ok(RecipeComparison {
        a: summary(&a_result)?,
        b: summary(&b_result)?,
        a_result,
        b_result,
        corrections: CompareCorrections::default(),
        b_unbound: Vec::new(),
        b_relinked: Vec::new(),
    })
}

/// As [`compare_recipes`], with the stored local corrections applied to both variants after their
/// base trace, the way the tracer shows variant A. A correction that does not bind on a variant is
/// left out of that variant and counted in [`RecipeComparison::corrections`].
pub fn compare_recipes_corrected(
    source: &Arc<SourceImage>,
    a: &TraceRecipe,
    b: &TraceRecipe,
    records: &[CorrectionRecord],
    task: TaskId,
    cancel: &CancellationToken,
) -> Result<RecipeComparison, TraceError> {
    compare_recipes_corrected_relinking(source, a, b, records, &BTreeSet::new(), task, cancel)
}

/// As [`compare_recipes_corrected`], and the corrections at the indices in `relink_b` are linked to the
/// contours of variant B even where their bindings no longer hold ([`relink_correction`]).
pub fn compare_recipes_corrected_relinking(
    source: &Arc<SourceImage>,
    a: &TraceRecipe,
    b: &TraceRecipe,
    records: &[CorrectionRecord],
    relink_b: &BTreeSet<usize>,
    task: TaskId,
    cancel: &CancellationToken,
) -> Result<RecipeComparison, TraceError> {
    let run = |recipe: &TraceRecipe, relink: &BTreeSet<usize>| -> Result<CorrectionReplay, TraceError> {
        let req =
            TraceRequest { task, source_revision: RevisionId::INITIAL, source: source.clone(), recipe: recipe.clone() };
        let base = Arc::new(trace(&req, cancel)?);
        replay_corrections_relinking(&base, source, records, relink, task, cancel)
    };
    let (a_replay, b_replay) = (run(a, &BTreeSet::new())?, run(b, relink_b)?);
    Ok(RecipeComparison {
        a: summary(&a_replay.result)?,
        b: summary(&b_replay.result)?,
        corrections: CompareCorrections {
            stored: records.len(),
            a_applied: a_replay.applied,
            b_applied: b_replay.applied,
        },
        b_unbound: b_replay.unbound,
        b_relinked: b_replay.relinked,
        a_result: a_replay.result,
        b_result: b_replay.result,
    })
}

pub(crate) fn summary(r: &TraceResult) -> Result<RecipeSummary, TraceError> {
    Ok(RecipeSummary {
        recipe_key: r.recipe_key,
        geometry_hash: r.geometry_hash(),
        paths: r.stats.paths,
        closed_shapes: r.stats.closed_shapes,
        holes: r.stats.holes,
        nodes: r.stats.nodes,
        removed: r.removed.len(),
        max_deviation_mm: comparison_preview(r)?.max_deviation_mm,
    })
}

/// One image of a batch.
#[derive(Debug, Clone)]
pub struct BatchItem {
    pub name: String,
    pub source: Arc<SourceImage>,
}

/// Something the operator should look at for one image.
#[derive(Debug, Clone, PartialEq)]
pub enum BatchProblem {
    /// Nothing was traced.
    NoGeometry,
    /// Parts or holes were dropped by the area filter.
    RemovedFeatures { count: usize },
    /// The trace failed (e.g. missing scale); the other images are still processed.
    Failed(TraceError),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BatchOutcome {
    pub name: String,
    pub summary: Option<RecipeSummary>,
    pub problems: Vec<BatchProblem>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BatchReport {
    pub recipe_key_by_item: Vec<Option<ContentHash>>,
    pub outcomes: Vec<BatchOutcome>,
}

impl BatchReport {
    pub fn problem_count(&self) -> usize {
        self.outcomes.iter().filter(|o| !o.problems.is_empty()).count()
    }
}

/// Traces every item with the same recipe, in order. A failing image is reported and the batch
/// continues; cancellation aborts the whole batch (no partial report).
pub fn trace_batch(
    items: &[BatchItem],
    recipe: &TraceRecipe,
    task: TaskId,
    revision: RevisionId,
    cancel: &CancellationToken,
) -> Result<BatchReport, TraceError> {
    let mut outcomes = Vec::with_capacity(items.len());
    let mut keys = Vec::with_capacity(items.len());
    for item in items {
        cancel.check()?;
        keys.push(recipe.key(item.source.content_hash()).ok());
        let req = TraceRequest { task, source_revision: revision, source: item.source.clone(), recipe: recipe.clone() };
        let outcome = match trace(&req, cancel) {
            Ok(r) => {
                let mut problems = Vec::new();
                if r.contours.is_empty() {
                    problems.push(BatchProblem::NoGeometry);
                }
                if !r.removed.is_empty() {
                    problems.push(BatchProblem::RemovedFeatures { count: r.removed.len() });
                }
                BatchOutcome { name: item.name.clone(), summary: Some(summary(&r)?), problems }
            }
            Err(TraceError::Cancelled(c)) => return Err(TraceError::Cancelled(c)),
            Err(e) => BatchOutcome { name: item.name.clone(), summary: None, problems: vec![BatchProblem::Failed(e)] },
        };
        outcomes.push(outcome);
    }
    Ok(BatchReport { recipe_key_by_item: keys, outcomes })
}

/// Result of re-applying a recorded correction to another base result.
#[derive(Debug, Clone)]
pub enum Reapplied {
    Applied(Box<TraceResult>),
    /// The correction no longer binds unambiguously: nothing was applied; the UI asks the user to
    /// re-link it. `lost`: recorded contours without a matching contour now; `new_in_region`:
    /// contours inside the region that the correction never saw.
    NeedsRebind {
        lost: Vec<ContourId>,
        new_in_region: Vec<ContourId>,
    },
}

/// Re-applies `record` to `new_base` (e.g. after re-segmenting with another recipe or image).
/// On the exact base it was made for it applies directly; otherwise every recorded binding must
/// find the same contour (same id, kind, area within 10 %, bounds overlap ≥ 80 %) and the region
/// may contain no contour the correction never saw. Otherwise nothing is applied.
pub fn reapply_correction(
    record: &CorrectionRecord,
    new_base: &Arc<TraceResult>,
    source: &Arc<SourceImage>,
    task: TaskId,
    cancel: &CancellationToken,
) -> Result<Reapplied, TraceError> {
    cancel.check()?;
    if new_base.recipe_key != record.base_inputs {
        let bindings: Vec<&ContourBinding> = record.touched.iter().chain(&record.untouched_in_region).collect();
        let mut lost: Vec<ContourId> = bindings
            .iter()
            .filter(|b| new_base.contour(b.id).is_none_or(|c| !binding_matches(b, c)))
            .map(|b| b.id)
            .collect();
        for id in &record.protected {
            if new_base.contour(*id).is_none() && !lost.contains(id) {
                lost.push(*id);
            }
        }
        let frame = Frame::new(new_base);
        let new_in_region: Vec<ContourId> = new_base
            .contours
            .iter()
            .filter(|c| c.source_points.iter().any(|&p| frame.in_region(&record.region, p)))
            .filter(|c| !bindings.iter().any(|b| b.id == c.id && binding_matches(b, c)))
            .map(|c| c.id)
            .collect();
        if !lost.is_empty() || !new_in_region.is_empty() {
            return Ok(Reapplied::NeedsRebind { lost, new_in_region });
        }
    }
    let req = CorrectionRequest {
        task,
        previous: new_base.clone(),
        source: source.clone(),
        region: record.region.clone(),
        recipe: record.recipe.clone(),
        anchors: record.anchors.clone(),
        protected: record.protected.clone(),
        node_target: record.node_target,
    };
    Ok(Reapplied::Applied(Box::new(retrace_region(&req, cancel)?)))
}

fn binding_matches(b: &ContourBinding, c: &TracedContour) -> bool {
    let area_ok = (c.area_mm2 - b.area_mm2).abs() <= 0.1 * b.area_mm2.max(1e-12);
    let cb = bounds_of(&c.source_points);
    let inter_w = (cb.max().x.min(b.bounds.max().x) - cb.min().x.max(b.bounds.min().x)).max(0.0);
    let inter_h = (cb.max().y.min(b.bounds.max().y) - cb.min().y.max(b.bounds.min().y)).max(0.0);
    let area = |x: &burnburn_geometry::Bounds| (x.max().x - x.min().x) * (x.max().y - x.min().y);
    let inter = inter_w * inter_h;
    let union = area(&cb) + area(&b.bounds) - inter;
    c.kind == b.kind && area_ok && union > 0.0 && inter / union >= 0.8
}

/// A stored correction that was not applied because its contours are no longer the same.
#[derive(Debug, Clone, PartialEq)]
pub struct UnboundCorrection {
    /// Position in the stored list.
    pub index: usize,
    /// Contours the correction was bound to that have no match any more.
    pub lost: Vec<ContourId>,
    /// Contours inside its region that it never saw.
    pub new_in_region: Vec<ContourId>,
}

/// The result of applying stored corrections to a fresh base trace, in order.
#[derive(Debug, Clone)]
pub struct CorrectionReplay {
    pub result: Arc<TraceResult>,
    pub applied: usize,
    pub unbound: Vec<UnboundCorrection>,
    /// Corrections the retrace itself refused, with the reason.
    pub failed: Vec<(usize, TraceError)>,
    /// Indices of the corrections that were linked to the new contours by [`relink_correction`].
    pub relinked: Vec<usize>,
}

/// Applies `records` to `base` one after the other.
pub fn replay_corrections(
    base: &Arc<TraceResult>,
    source: &Arc<SourceImage>,
    records: &[CorrectionRecord],
    task: TaskId,
    cancel: &CancellationToken,
) -> Result<CorrectionReplay, TraceError> {
    replay_corrections_relinking(base, source, records, &BTreeSet::new(), task, cancel)
}

/// As [`replay_corrections`]; a correction at an index in `relink` that does not bind is linked to the
/// contours now in its area by [`relink_correction`] instead of being reported as unbound.
pub fn replay_corrections_relinking(
    base: &Arc<TraceResult>,
    source: &Arc<SourceImage>,
    records: &[CorrectionRecord],
    relink: &BTreeSet<usize>,
    task: TaskId,
    cancel: &CancellationToken,
) -> Result<CorrectionReplay, TraceError> {
    let mut result = Arc::clone(base);
    let (mut applied, mut unbound, mut failed, mut relinked) = (0, Vec::new(), Vec::new(), Vec::new());
    for (index, record) in records.iter().enumerate() {
        match reapply_correction(record, &result, source, task, cancel) {
            Ok(Reapplied::Applied(next)) => {
                result = Arc::new(*next);
                applied += 1;
            }
            Ok(Reapplied::NeedsRebind { lost, new_in_region }) => {
                if relink.contains(&index) {
                    match relink_correction(record, &result, source, task, cancel) {
                        Ok(next) => {
                            result = Arc::new(*next.result);
                            applied += 1;
                            relinked.push(index);
                        }
                        Err(e @ TraceError::Cancelled(_)) => return Err(e),
                        Err(e) => failed.push((index, e)),
                    }
                } else {
                    unbound.push(UnboundCorrection { index, lost, new_in_region });
                }
            }
            Err(e @ TraceError::Cancelled(_)) => return Err(e),
            Err(e) => failed.push((index, e)),
        }
    }
    Ok(CorrectionReplay { result, applied, unbound, failed, relinked })
}

/// A correction linked to other contours than the ones it was made on.
#[derive(Debug, Clone)]
pub struct Relinked {
    pub result: Box<TraceResult>,
    /// Protected contours of the record that found a contour in the new base.
    pub protected_kept: usize,
    /// Protected contours of the record without a counterpart now (the correction no longer protects them).
    pub protected_lost: usize,
}

/// Applies `record` to `new_base` although its bindings no longer hold (the operator decided). The
/// region, local recipe and anchors are the record's; each protected contour is looked up again by
/// kind and bounds (not by id: ids belong to one trace), and one without a counterpart is dropped and
/// counted in [`Relinked::protected_lost`].
pub fn relink_correction(
    record: &CorrectionRecord,
    new_base: &Arc<TraceResult>,
    source: &Arc<SourceImage>,
    task: TaskId,
    cancel: &CancellationToken,
) -> Result<Relinked, TraceError> {
    cancel.check()?;
    if source.content_hash() != new_base.source_hash {
        return Err(TraceError::CorrectionMismatch("source"));
    }
    let bindings: Vec<&ContourBinding> = record.touched.iter().chain(&record.untouched_in_region).collect();
    let mut protected = BTreeSet::new();
    let (mut kept, mut lost) = (0, 0);
    for id in &record.protected {
        let found = bindings.iter().find(|b| b.id == *id).and_then(|b| counterpart(b, new_base, &protected));
        match found {
            Some(found) => {
                protected.insert(found);
                kept += 1;
            }
            None => lost += 1,
        }
    }
    let req = CorrectionRequest {
        task,
        previous: new_base.clone(),
        source: source.clone(),
        region: record.region.clone(),
        recipe: record.recipe.clone(),
        anchors: record.anchors.clone(),
        protected,
        node_target: record.node_target,
    };
    let result = retrace_region(&req, cancel)?;
    Ok(Relinked { result: Box::new(result), protected_kept: kept, protected_lost: lost })
}

/// The contour of `base` that is the same kind as `b`, overlaps its bounds by at least half and has an
/// area within a quarter of it (the best overlap wins, a contour in `taken` never).
fn counterpart(b: &ContourBinding, base: &TraceResult, taken: &BTreeSet<ContourId>) -> Option<ContourId> {
    base.contours
        .iter()
        .filter(|c| c.kind == b.kind && !taken.contains(&c.id))
        .filter(|c| (c.area_mm2 - b.area_mm2).abs() <= 0.25 * b.area_mm2.max(1e-12))
        .filter_map(|c| Some((overlap(&bounds_of(&c.source_points), &b.bounds)?, c.id)))
        .filter(|(iou, _)| *iou >= 0.5)
        .max_by(|x, y| x.0.total_cmp(&y.0).then(y.1.cmp(&x.1)))
        .map(|(_, id)| id)
}

/// Intersection over union of two boxes; `None` when they are degenerate or apart.
fn overlap(a: &burnburn_geometry::Bounds, b: &burnburn_geometry::Bounds) -> Option<f64> {
    let inter_w = (a.max().x.min(b.max().x) - a.min().x.max(b.min().x)).max(0.0);
    let inter_h = (a.max().y.min(b.max().y) - a.min().y.max(b.min().y)).max(0.0);
    let area = |x: &burnburn_geometry::Bounds| (x.max().x - x.min().x) * (x.max().y - x.min().y);
    let inter = inter_w * inter_h;
    let union = area(a) + area(b) - inter;
    (union > 0.0).then_some(inter / union)
}
