//! Advanced recipes (IM24): A/B comparison of two recipes on one source, batch tracing with a
//! per-image problem report, and re-applying a recorded local correction to a new base result
//! only when its bindings still hold.

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

/// Two variants of the same source in one frame (same zoom/position in the UI).
#[derive(Debug, Clone)]
pub struct RecipeComparison {
    pub a: RecipeSummary,
    pub b: RecipeSummary,
    pub a_result: Arc<TraceResult>,
    pub b_result: Arc<TraceResult>,
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
    Ok(RecipeComparison { a: summary(&a_result)?, b: summary(&b_result)?, a_result, b_result })
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
