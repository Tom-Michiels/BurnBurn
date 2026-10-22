//! The tracer window: source image, recipe form and the newest trace result (T01, T02, T04, T10).
//!
//! The panel sends raw field text (`TraceFormInput`); this module parses it into a
//! [`TraceRecipe`], starts one trace task per valid change and shows only the result of the
//! newest task. Contour and centre-line tracing share the form; the mode decides the engine.

use std::path::PathBuf;
use std::sync::Arc;

use burnburn_document::trace::TraceId;
use burnburn_geometry::Path;
use burnburn_image::{PixelScale, SourceImage, Threshold};
use burnburn_trace::{
    CenterlineDecision, CenterlineEditError, CenterlineRequest, CenterlineResult, CenterlineSettings, ChainId,
    Connections, NodeId, NodeKind, Primitive, TraceRecipe, TraceRequest, TraceResult,
};
use burnburn_types::{CancellationSource, DocumentId, TaskId};

use crate::centerline_panel::{
    CenterlineEdit, CenterlineEditViewModel, ChainRow, JunctionRow, JunctionState, PrimitiveKind, PrimitiveRow,
};
use crate::field::{format_mm, parse_number};
use crate::image_panel::{
    ImageField, ImageIssue, ThresholdChoice, TraceFormInput, TraceFormViewModel, TraceMode, TracePreset,
};
use crate::task::{TaskError, TaskInput, TaskKind, TaskOutput, TaskRequest, revision_key};
use crate::view_model::{MessageKey, StatusMessage, TraceSummary, TracerViewModel};

use super::{AppController, AppUpdate, Session, error, submit};

/// Resolution range accepted for a source image (dots per inch).
const DPI_MIN: f64 = 1.0;
const DPI_MAX: f64 = 100_000.0;
/// Gray level of the local threshold when a recipe asks for it without its own window.
const ADAPTIVE_RADIUS_PX: u32 = 15;
const ADAPTIVE_OFFSET: u8 = 10;
/// Bounds of the fit budget the panel accepts (mm) and of "ignore less than" (mm²).
const SMOOTHNESS_RANGE_MM: (f64, f64) = (0.001, 10.0);
const IGNORE_MAX_MM2: f64 = 1_000_000.0;

type FieldIssue = (ImageField, ImageIssue);

/// A centre-line result with what the panel and the apply command need from it.
#[derive(Clone)]
pub(super) struct CenterlineOutcome {
    pub(super) result: Arc<CenterlineResult>,
    /// One path per stroke, open strokes stay open.
    pub(super) strokes: Vec<Path>,
    pub(super) preview: Arc<Path>,
}

/// The open tracer of one window: decoded source, its stored bytes and the newest request.
#[derive(Clone)]
pub(super) struct Tracer {
    pub(super) source: Arc<SourceImage>,
    pub(super) bytes: Arc<[u8]>,
    pub(super) media: &'static str,
    /// What the panel shows: the text as typed, valid or not.
    pub(super) form: TraceFormInput,
    pub(super) issue: Option<FieldIssue>,
    /// The recipe the newest valid form describes; `None` before one exists.
    pub(super) recipe: Option<TraceRecipe>,
    /// Newest submitted trace; only its result may be shown (check_fresh).
    pub(super) latest: Option<TaskId>,
    pub(super) result: Option<Arc<TraceResult>>,
    pub(super) centerline: Option<CenterlineOutcome>,
    /// The file's own resolution makes a recipe at once: the first trace starts without input.
    pub(super) autostart: bool,
    /// The trace of the document this tracer edits: applying as a trace replaces its outputs.
    pub(super) editing: Option<TraceId>,
    /// The operator's decisions on the centre-line result in force, in the order they were taken.
    /// A fresh centre-line trace applies them again; the ones that no longer fit are counted.
    pub(super) decisions: Vec<CenterlineDecision>,
    pub(super) dropped: usize,
    /// Why the last preparation (local threshold, correction, mask) was refused.
    pub(super) prep_issue: Option<crate::trace_prep::PrepIssue>,
    /// The local corrections in force, in the order they are applied to a fresh contour trace
    /// (T05), including those the last trace could not bind.
    pub(super) corrections: Vec<burnburn_trace::CorrectionRecord>,
    /// Positions in `corrections` the last trace did not apply, and why.
    pub(super) unbound: Vec<burnburn_trace::UnboundCorrection>,
    pub(super) failed: Vec<usize>,
    /// What simplifying cost for the contour result in force (T06).
    pub(super) report: Option<crate::trace_prep::PrepReport>,
    /// Contours the next area retrace leaves alone, and its node goal (G7-trace, T05).
    pub(super) protect: std::collections::BTreeSet<burnburn_trace::ContourId>,
    pub(super) node_target: Option<burnburn_trace::NodeTarget>,
    /// Pointing in the image: corners being clicked or a brush in hand (G7-trace, T03).
    pub(super) pick: Option<super::trace_g7::PickState>,
    /// Own recipes, the A/B comparison and the batch (G7-trace, T10).
    pub(super) study: super::trace_recipes::StudyState,
}

impl Tracer {
    pub(super) fn new(source: Arc<SourceImage>, bytes: Arc<[u8]>, media: &'static str) -> Self {
        let mut recipe = TraceRecipe::default();
        let proposal = source.metadata().dpi_suggestion;
        if let Some((x, y)) = proposal {
            recipe.image.scale = PixelScale::from_dpi(x, y).ok();
        }
        let form = form_of(&recipe, TraceMode::Contour);
        let autostart = recipe.image.scale.is_some();
        Tracer {
            source,
            bytes,
            media,
            form,
            issue: None,
            recipe: autostart.then_some(recipe),
            latest: None,
            result: None,
            centerline: None,
            autostart,
            editing: None,
            decisions: Vec::new(),
            dropped: 0,
            prep_issue: None,
            corrections: Vec::new(),
            unbound: Vec::new(),
            failed: Vec::new(),
            report: None,
            protect: std::collections::BTreeSet::new(),
            node_target: None,
            pick: None,
            study: super::trace_recipes::StudyState::default(),
        }
    }

    /// The tracer of an existing trace: its stored recipe is in force and the first trace starts.
    pub(super) fn reopen(&mut self, reopening: super::trace_link::Reopening) {
        let super::trace_link::Reopening { trace, recipe, mode, decisions, stored } = reopening;
        let recipe = *recipe;
        self.decisions = decisions;
        // Local corrections need the decoded source (their keys include its hash).
        match burnburn_trace::corrections_from_engine(&stored, &recipe, self.source.content_hash()) {
            Ok(corrections) => self.corrections = corrections,
            Err(_) => self.prep_issue = Some(crate::trace_prep::PrepIssue::StoredCorrectionsUnreadable),
        }
        self.form = form_of(&recipe, mode);
        self.recipe = Some(recipe);
        self.editing = Some(trace);
        self.autostart = true;
    }

    /// There is something to apply for the mode in force.
    pub(super) fn has_result(&self) -> bool {
        match self.form.mode {
            TraceMode::Contour => self.result.is_some(),
            TraceMode::Centerline => self.centerline.is_some(),
        }
    }

    fn centerline_request(&self, task: TaskId, revision: burnburn_types::RevisionId) -> Option<CenterlineRequest> {
        let recipe = self.recipe.as_ref()?;
        Some(CenterlineRequest {
            task,
            source_revision: revision,
            source: Arc::clone(&self.source),
            image: recipe.image.clone(),
            settings: CenterlineSettings { tolerance_mm: recipe.fit.tolerance_mm, ..CenterlineSettings::default() },
        })
    }
}

// ---- recipe <-> form ---------------------------------------------------------------------------

fn dpi_text(recipe: &TraceRecipe) -> String {
    recipe.image.scale.map_or_else(String::new, |s| format_mm(25.4 / s.mm_per_pixel_x()))
}

pub(super) fn form_of(recipe: &TraceRecipe, mode: TraceMode) -> TraceFormInput {
    let (threshold_choice, threshold) = match recipe.image.threshold {
        Threshold::Otsu => (ThresholdChoice::Otsu, "128".to_owned()),
        Threshold::Fixed(v) => (ThresholdChoice::Fixed, v.to_string()),
        Threshold::Adaptive { .. } => (ThresholdChoice::Adaptive, "128".to_owned()),
    };
    TraceFormInput {
        mode,
        dpi: dpi_text(recipe),
        threshold_choice,
        threshold,
        invert: recipe.image.invert,
        ignore_below_mm2: format_mm(recipe.contour.min_area_mm2),
        smoothness_mm: format_mm(recipe.fit.tolerance_mm),
        curves: recipe.fit.curves,
    }
}

fn number(text: &str, field: ImageField, suffixes: &[&str], range: (f64, f64)) -> Result<f64, FieldIssue> {
    let value = parse_number(text, suffixes).map_err(|_| (field, ImageIssue::InvalidNumber(field)))?;
    if (range.0..=range.1).contains(&value) { Ok(value) } else { Err((field, ImageIssue::OutOfRange(field))) }
}

/// The recipe the form describes, on top of the recipe in force (what the form cannot say stays).
fn recipe_from(form: &TraceFormInput, base: Option<&TraceRecipe>) -> Result<TraceRecipe, FieldIssue> {
    let mut recipe = base.cloned().unwrap_or_default();
    if form.dpi.trim().is_empty() {
        recipe.image.scale = None;
    } else if base.is_none_or(|b| dpi_text(b) != form.dpi.trim()) {
        // Retyping the very same resolution keeps a file's two-axis scale.
        let dpi = number(&form.dpi, ImageField::Dpi, &["dpi"], (DPI_MIN, DPI_MAX))?;
        recipe.image.scale = PixelScale::from_dpi(dpi, dpi).ok();
    }
    recipe.image.threshold = match form.threshold_choice {
        ThresholdChoice::Otsu => Threshold::Otsu,
        ThresholdChoice::Fixed => {
            let level = number(&form.threshold, ImageField::TraceThreshold, &[], (0.0, 255.0))?;
            Threshold::Fixed(level.round() as u8)
        }
        ThresholdChoice::Adaptive => match recipe.image.threshold {
            keep @ Threshold::Adaptive { .. } => keep,
            _ => Threshold::Adaptive { radius: ADAPTIVE_RADIUS_PX, offset: ADAPTIVE_OFFSET },
        },
    };
    recipe.image.invert = form.invert;
    recipe.contour.min_area_mm2 =
        number(&form.ignore_below_mm2, ImageField::TraceIgnoreBelow, &["mm²", "mm2"], (0.0, IGNORE_MAX_MM2))?;
    recipe.fit.tolerance_mm = number(&form.smoothness_mm, ImageField::TraceSmoothness, &["mm"], SMOOTHNESS_RANGE_MM)?;
    recipe.fit.curves = form.curves;
    recipe
        .validate()
        .map_err(|_| (ImageField::TraceSmoothness, ImageIssue::OutOfRange(ImageField::TraceSmoothness)))?;
    Ok(recipe)
}

/// The recipe a built-in preset stands for, on the declared scale of the source.
pub(super) fn preset_recipe(preset: TracePreset, scale: Option<PixelScale>) -> (TraceRecipe, TraceMode) {
    let mut recipe = TraceRecipe::default();
    recipe.image.scale = scale;
    let mut mode = TraceMode::Contour;
    match preset {
        TracePreset::Logo => {
            recipe.contour.min_area_mm2 = 0.1;
            recipe.fit.tolerance_mm = 0.05;
        }
        TracePreset::Photo => {
            recipe.image.threshold = Threshold::Adaptive { radius: ADAPTIVE_RADIUS_PX, offset: ADAPTIVE_OFFSET };
            recipe.image.median_radius = 1;
            recipe.contour.min_area_mm2 = 0.5;
            recipe.fit.tolerance_mm = 0.1;
        }
        TracePreset::Technical => {
            recipe.contour.min_area_mm2 = 0.05;
            recipe.fit.tolerance_mm = 0.05;
            recipe.fit.curves = false;
        }
        TracePreset::Handwriting => {
            recipe.fit.tolerance_mm = CenterlineSettings::default().tolerance_mm;
            mode = TraceMode::Centerline;
        }
    }
    (recipe, mode)
}

fn preset_of(recipe: &TraceRecipe, mode: TraceMode) -> Option<TracePreset> {
    TracePreset::ALL.iter().copied().find(|p| preset_recipe(*p, recipe.image.scale) == (recipe.clone(), mode))
}

// ---- controller --------------------------------------------------------------------------------

impl AppController {
    pub(super) fn open_trace_source(&mut self, document: DocumentId, path: PathBuf) -> AppUpdate {
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::ImageFailed) };
        let (tasks, fs) = (Arc::clone(&self.ports.tasks), Arc::clone(&self.ports.fs));
        self.with_session(document, |s| {
            let source = s.doc.revision();
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document,
                source,
                key: revision_key(TaskKind::LoadImage, document, source),
                cancel: cancel.token(),
                input: TaskInput::LoadImage { path, fs },
            };
            submit(s, &tasks, request, cancel, document)
        })
    }

    /// A full recipe (script or test): the form follows it, then the usual trace task.
    pub(super) fn set_trace_recipe(&mut self, document: DocumentId, recipe: TraceRecipe) -> AppUpdate {
        self.with_session(document, |s| {
            let Some(t) = s.tracer.as_mut() else { return AppUpdate::default() };
            t.form = form_of(&recipe, t.form.mode);
            t.recipe = Some(recipe);
            t.issue = None;
            AppUpdate { view_changed: true, ..AppUpdate::default() }
        });
        self.submit_trace(document)
    }

    /// Every valid form change is a new trace task; invalid input is shown at its field and
    /// leaves the last valid result alone.
    pub(super) fn set_trace_form(&mut self, document: DocumentId, input: TraceFormInput) -> AppUpdate {
        let valid = self.with_session(document, |s| {
            let Some(t) = s.tracer.as_mut() else { return AppUpdate::default() };
            let parsed = recipe_from(&input, t.recipe.as_ref());
            t.form = input;
            match parsed {
                Ok(recipe) => {
                    t.recipe = Some(recipe);
                    t.issue = None;
                    AppUpdate { view_changed: true, ..AppUpdate::default() }
                }
                Err(issue) => {
                    t.issue = Some(issue);
                    cancel_traces(s);
                    if let Some(t) = s.tracer.as_mut() {
                        t.latest = None;
                    }
                    AppUpdate { view_changed: true, ..AppUpdate::default() }
                }
            }
        });
        let ok = self.sessions.get(&document).and_then(|s| s.tracer.as_ref()).is_some_and(|t| t.issue.is_none());
        if ok { self.submit_trace(document) } else { valid }
    }

    pub(super) fn apply_trace_preset(&mut self, document: DocumentId, preset: TracePreset) -> AppUpdate {
        self.with_session(document, |s| {
            let Some(t) = s.tracer.as_mut() else { return AppUpdate::default() };
            let typed = number(&t.form.dpi, ImageField::Dpi, &["dpi"], (DPI_MIN, DPI_MAX))
                .ok()
                .and_then(|d| PixelScale::from_dpi(d, d).ok());
            let scale = t.recipe.as_ref().and_then(|r| r.image.scale).or(typed);
            let (recipe, mode) = preset_recipe(preset, scale);
            t.form = form_of(&recipe, mode);
            t.recipe = Some(recipe);
            t.issue = None;
            AppUpdate { view_changed: true, ..AppUpdate::default() }
        });
        self.submit_trace(document)
    }

    /// Starts the trace of the recipe in force (the engine follows the mode); the previous one is
    /// cancelled (best effort) and its result can no longer pass the freshness check.
    pub(super) fn submit_trace(&mut self, document: DocumentId) -> AppUpdate {
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::TraceFailed) };
        let tasks = Arc::clone(&self.ports.tasks);
        self.with_session(document, |s| {
            let revision = s.doc.revision();
            cancel_traces(s);
            let Some(tracer) = s.tracer.as_mut() else { return AppUpdate::default() };
            tracer.latest = None;
            // A new full trace numbers its contours afresh: protected ids of the old one mean nothing.
            tracer.protect.clear();
            let Some(recipe) = tracer.recipe.clone() else {
                return AppUpdate { view_changed: true, ..AppUpdate::default() };
            };
            // T01: without a declared scale there is nothing physical to trace.
            if recipe.image.scale.is_none() {
                return AppUpdate { view_changed: true, ..AppUpdate::default() };
            }
            let input = match tracer.form.mode {
                TraceMode::Contour => {
                    let request = TraceRequest {
                        task: id,
                        source_revision: revision,
                        source: Arc::clone(&tracer.source),
                        recipe,
                    };
                    if tracer.corrections.is_empty() {
                        TaskInput::Trace(Box::new(request))
                    } else {
                        // The stored local corrections are applied to the fresh trace (T05).
                        TaskInput::TraceCorrected(Box::new(crate::task::CorrectedTrace {
                            request,
                            corrections: Arc::new(tracer.corrections.clone()),
                        }))
                    }
                }
                TraceMode::Centerline => match tracer.centerline_request(id, revision) {
                    Some(r) => TaskInput::Centerline(Box::new(r)),
                    None => return AppUpdate { view_changed: true, ..AppUpdate::default() },
                },
            };
            tracer.latest = Some(id);
            let kind = input.kind();
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document,
                source: revision,
                key: revision_key(kind, document, revision),
                cancel: cancel.token(),
                input,
            };
            submit(s, &tasks, request, cancel, document)
        })
    }
}

fn cancel_traces(s: &Session) {
    for (kind, older) in s.pending.values() {
        if matches!(kind, TaskKind::Trace | TaskKind::Centerline) {
            older.cancel();
        }
    }
}

/// Delivery of a `Centerline` task result.
pub(super) fn centerline_finished(
    s: &mut Session,
    id: TaskId,
    result: Result<TaskOutput, TaskError>,
    update: &mut AppUpdate,
) {
    match result {
        Ok(TaskOutput::Centerlined(result)) => {
            let revision = s.doc.revision();
            let Some(t) = s.tracer.as_mut() else { return };
            let key = t.centerline_request(id, revision).and_then(|r| r.key().ok());
            let fresh = matches!((t.latest, key), (Some(latest), Some(key)) if latest == id && result.check_fresh(latest, key).is_ok());
            if fresh {
                // The operator's decisions apply again where their place is still there.
                let result = if t.decisions.is_empty() {
                    t.dropped = 0;
                    result
                } else {
                    let replayed = result.replay(&t.decisions);
                    t.dropped = replayed.skipped.len();
                    Arc::new(replayed.result)
                };
                t.centerline = Some(CenterlineOutcome::of(result));
                t.latest = None;
                update.scenes_changed.push(s.doc.id());
            }
        }
        Err(TaskError::Trace(_)) => s.messages.push(StatusMessage { key: MessageKey::TraceFailed }),
        _ => {}
    }
}

impl CenterlineOutcome {
    /// The outcome of `result`: its strokes (through resolved junctions only) and their preview.
    fn of(result: Arc<CenterlineResult>) -> Self {
        let strokes: Vec<Path> = result.strokes().into_iter().map(|st| Path::from_subpaths(vec![st.path])).collect();
        let preview =
            Arc::new(Path::from_subpaths(strokes.iter().flat_map(|p| p.subpaths().iter().cloned()).collect()));
        CenterlineOutcome { result, strokes, preview }
    }
}

/// An operator decision on the centre-line result: the new result replaces the old one (strokes
/// and preview follow); a decision that does not apply says so and changes nothing. Re-tracing
/// (a changed form) starts again without the decisions.
pub(super) fn centerline_edit(s: &mut Session, document: DocumentId, edit: CenterlineEdit) -> AppUpdate {
    let Some(current) =
        s.tracer.as_ref().filter(|t| t.form.mode == TraceMode::Centerline).and_then(|t| t.centerline.as_ref())
    else {
        return AppUpdate::default();
    };
    let result = Arc::clone(&current.result);
    let next = match edit {
        CenterlineEdit::ResolveJunction { node, candidate } => {
            let node = NodeId(node);
            let pairs = result.node(node).and_then(|n| match &n.connections {
                Connections::Ambiguous { candidates } => candidates.get(candidate).map(|c| c.pairs.clone()),
                _ => None,
            });
            match pairs {
                Some(pairs) => result.resolve_junction(node, pairs),
                None => Err(CenterlineEditError::InvalidPairing),
            }
        }
        CenterlineEdit::ConfirmPrimitive { chain } => {
            result.propose_replacement(ChainId(chain)).and_then(|p| result.confirm_replacement(&p))
        }
    };
    match next {
        Ok(next) => {
            if let Some(t) = s.tracer.as_mut() {
                // The edit added one decision to the recipe in force.
                if let Some(taken) = next.decisions().last() {
                    t.decisions.push(taken.clone());
                }
                t.centerline = Some(CenterlineOutcome::of(Arc::new(next)));
            }
            AppUpdate { view_changed: true, scenes_changed: vec![document], ..AppUpdate::default() }
        }
        Err(_) => {
            s.messages.push(StatusMessage { key: MessageKey::TraceFailed });
            AppUpdate { view_changed: true, ..AppUpdate::default() }
        }
    }
}

fn kind_of(p: &Primitive) -> PrimitiveKind {
    match p {
        Primitive::Line { .. } => PrimitiveKind::Line,
        Primitive::Arc { .. } => PrimitiveKind::Arc,
        Primitive::Circle { .. } => PrimitiveKind::Circle,
    }
}

/// What can be decided on the open centre-line result (centre-line mode only).
pub(super) fn centerline_panel(s: &Session) -> Option<CenterlineEditViewModel> {
    let t = s.tracer.as_ref().filter(|t| t.form.mode == TraceMode::Centerline)?;
    t.centerline.as_ref().map(|c| edit_view(c, t.dropped))
}

fn edit_view(c: &CenterlineOutcome, dropped: usize) -> CenterlineEditViewModel {
    let r = &c.result;
    CenterlineEditViewModel {
        junctions: r
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Junction)
            .map(|n| JunctionRow {
                node: n.id.0,
                at: (n.at.x, n.at.y),
                state: match &n.connections {
                    Connections::Ambiguous { candidates } => JunctionState::Ambiguous { candidates: candidates.len() },
                    _ => JunctionState::Resolved,
                },
            })
            .collect(),
        chains: r
            .chains
            .iter()
            .map(|ch| ChainRow {
                chain: ch.id.0,
                candidates: ch
                    .candidates
                    .iter()
                    .map(|k| PrimitiveRow {
                        kind: kind_of(&k.primitive),
                        max_residual_mm: k.max_residual_mm,
                        within_tolerance: k.within_tolerance,
                    })
                    .collect(),
                confirmed: ch.confirmed.as_ref().map(kind_of),
            })
            .collect(),
        edits: r.revision,
        dropped,
    }
}

pub(super) fn tracer_view(t: &Tracer, s: &Session) -> TracerViewModel {
    let computing = s.pending.values().any(|(kind, _)| matches!(kind, TaskKind::Trace | TaskKind::Centerline));
    let tolerance = t.recipe.as_ref().map_or(0.0, |r| r.fit.tolerance_mm);
    let result = match t.form.mode {
        TraceMode::Contour => t.result.as_ref().map(|r| {
            let path = r.path();
            let b = path.bounds();
            TraceSummary {
                components: r.stats.closed_shapes,
                holes: r.stats.holes,
                width_mm: b.map_or(0.0, |b| b.width()),
                height_mm: b.map_or(0.0, |b| b.height()),
                mm_per_pixel: r.stats.source_mm_per_pixel,
                fit_tolerance_mm: tolerance,
                preview: Arc::new(path),
            }
        }),
        TraceMode::Centerline => t.centerline.as_ref().map(|c| {
            let b = c.preview.bounds();
            TraceSummary {
                components: c.strokes.len(),
                holes: 0,
                width_mm: b.map_or(0.0, |b| b.width()),
                height_mm: b.map_or(0.0, |b| b.height()),
                mm_per_pixel: (c.result.scale.mm_per_pixel_x(), c.result.scale.mm_per_pixel_y()),
                fit_tolerance_mm: tolerance,
                preview: Arc::clone(&c.preview),
            }
        }),
    };
    TracerViewModel {
        width_px: t.source.width(),
        height_px: t.source.height(),
        needs_scale: t.recipe.as_ref().is_none_or(|r| r.image.scale.is_none()),
        computing,
        result,
        form: TraceFormViewModel {
            input: t.form.clone(),
            preset: t.recipe.as_ref().and_then(|r| preset_of(r, t.form.mode)),
            issue: t.issue,
            dpi_suggestion: t.source.metadata().dpi_suggestion.map(|(x, _)| x),
        },
    }
}
