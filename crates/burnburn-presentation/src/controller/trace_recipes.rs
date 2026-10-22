//! Own recipes, the A/B comparison and batch tracing of the open tracer (G7-trace, T10).
//!
//! The recipes of the user are app state: one file in the user's folder, read when the folder is
//! set and written in the background, one write at a time. A comparison or a batch is a background
//! job of its own kind; only the newest of each is shown, a batch can be cancelled, and nothing of
//! either touches the document or the recipe in force.

use std::path::PathBuf;
use std::sync::Arc;

use burnburn_io::trace_recipe_library::{
    LibraryError, StoredRecipe, TraceRecipeLibrary, decode_trace_recipe_library, encode_trace_recipe_library,
};
use burnburn_trace::{RecipeComparison, RecipeSummary, TraceRecipe, recipe_from_engine, recipe_to_engine};
use burnburn_types::{CancellationSource, DocumentId, TaskId};

use crate::image_panel::TraceMode;
use crate::task::{TaskError, TaskInput, TaskKind, TaskOutcome, TaskOutput, TaskRequest, revision_key};
use crate::trace_recipes::{
    BatchProblemView, BatchRowView, BatchViewModel, CompareViewModel, RecipeChoice, RecipeIssue, SavedRecipeRow,
    TraceRecipeIntent, TraceRecipesViewModel, VariantSummary,
};
use crate::trace_study::{FileOutcome, FileProblem, StudyJob, StudyOutput};

use super::app_panels::AppPanels;
use super::trace::{Tracer, form_of, preset_recipe};
use super::{AppController, AppUpdate, Session, submit};

/// The recipe file in the user's folder.
pub const RECIPES_FILE: &str = "trace-recipes.json";
const MAX_RECIPES_FILE_BYTES: u64 = 16 << 20;

/// The comparison of the open tracer.
#[derive(Clone)]
pub(super) struct CompareState {
    choice: RecipeChoice,
    /// B as it was traced: the picture's own settings with the other recipe's on top.
    b: TraceRecipe,
    latest: TaskId,
    result: Option<Arc<RecipeComparison>>,
}

/// The batch of the open tracer.
#[derive(Clone)]
pub(super) struct BatchState {
    latest: TaskId,
    total: usize,
    running: bool,
    cancelled: bool,
    rows: Vec<BatchRowView>,
}

/// Recipe state of one tracer.
#[derive(Clone, Default)]
pub(super) struct StudyState {
    issue: Option<RecipeIssue>,
    compare: Option<CompareState>,
    batch: Option<BatchState>,
}

/// What belongs to the kind of picture and is saved: everything but what belongs to one picture
/// (its resolution, crop, rotation, plane correction, masks and kept pieces).
fn saved_form(recipe: &TraceRecipe) -> TraceRecipe {
    let mut r = recipe.clone();
    r.image.scale = None;
    r.image.crop = None;
    r.image.rotation = Default::default();
    r.image.regions.clear();
    r.image.perspective = None;
    r.contour.keep.clear();
    r
}

/// A saved recipe on top of a picture: its settings, with the picture's own parts kept. The lens is
/// the recipe's when it has one (it describes a camera), the picture's otherwise.
fn on_picture(saved: &TraceRecipe, current: &TraceRecipe) -> TraceRecipe {
    let mut r = saved.clone();
    r.image.scale = current.image.scale;
    r.image.crop = current.image.crop;
    r.image.rotation = current.image.rotation;
    r.image.regions.clone_from(&current.image.regions);
    r.image.perspective.clone_from(&current.image.perspective);
    r.contour.keep.clone_from(&current.contour.keep);
    if r.image.lens.is_none() {
        r.image.lens = current.image.lens;
    }
    r
}

fn library_issue(e: LibraryError) -> RecipeIssue {
    match e {
        LibraryError::EmptyName => RecipeIssue::EmptyName,
        LibraryError::NameTooLong => RecipeIssue::NameTooLong,
        LibraryError::TooMany => RecipeIssue::TooMany,
        _ => RecipeIssue::Invalid,
    }
}

impl AppController {
    /// Reads the recipes of the user from `dir`; a missing file is no recipes, a file that cannot be
    /// read is no recipes plus a visible problem, and the file is left alone until the user saves.
    pub(super) fn load_trace_recipes(&mut self, dir: &std::path::Path) {
        let loaded = match self.ports.fs.read(&dir.join(RECIPES_FILE), MAX_RECIPES_FILE_BYTES) {
            Ok(bytes) => Some(decode_trace_recipe_library(&bytes).map_err(|_| ())),
            Err(e) if e.kind == burnburn_io::fs::FsErrorKind::NotFound => None,
            Err(_) => Some(Err(())),
        };
        let app = &mut self.app;
        match loaded {
            None => {}
            Some(Ok(library)) => app.trace_recipes.library = library,
            Some(Err(())) => app.trace_recipes.load_problem = true,
        }
    }

    pub(super) fn trace_recipe_intent(&mut self, document: DocumentId, intent: TraceRecipeIntent) -> AppUpdate {
        match intent {
            TraceRecipeIntent::Save { name } => self.save_recipe(document, name),
            TraceRecipeIntent::Apply { name } => self.apply_recipe(document, &name),
            TraceRecipeIntent::Delete { name } => self.delete_recipe(document, &name),
            TraceRecipeIntent::Compare { with } => self.compare(document, with),
            TraceRecipeIntent::ClearCompare => self.with_study(document, |_, study| study.compare = None),
            TraceRecipeIntent::AdoptCompared => self.adopt_compared(document),
            TraceRecipeIntent::RunBatch { paths } => self.run_batch(document, paths),
            TraceRecipeIntent::CancelBatch => self.with_session(document, |s| {
                if let Some(batch) = s.tracer.as_mut().and_then(|t| t.study.batch.as_mut()) {
                    let id = batch.latest;
                    if std::mem::take(&mut batch.running) {
                        batch.cancelled = true;
                        if let Some((_, cancel)) = s.pending.get(&id) {
                            cancel.cancel();
                        }
                    }
                }
                AppUpdate::redraw(document)
            }),
            TraceRecipeIntent::ClearBatch => self.with_study(document, |_, study| study.batch = None),
        }
    }

    /// Runs `change` on the study state of the open tracer (and forgets the last issue).
    fn with_study(&mut self, document: DocumentId, change: impl FnOnce(&mut Tracer, &mut StudyState)) -> AppUpdate {
        self.with_session(document, |s| {
            if let Some(t) = s.tracer.as_mut() {
                let mut study = std::mem::take(&mut t.study);
                study.issue = None;
                change(t, &mut study);
                t.study = study;
            }
            AppUpdate::redraw(document)
        })
    }

    fn refuse(&mut self, document: DocumentId, issue: RecipeIssue) -> AppUpdate {
        self.with_session(document, |s| {
            if let Some(t) = s.tracer.as_mut() {
                t.study.issue = Some(issue);
            }
            AppUpdate::redraw(document)
        })
    }

    fn save_recipe(&mut self, document: DocumentId, name: String) -> AppUpdate {
        let stored = self.sessions.get(&document).and_then(|s| s.tracer.as_ref()).map(|t| StoredRecipe {
            name,
            centerline: t.form.mode == TraceMode::Centerline,
            recipe: recipe_to_engine(&saved_form(&t.recipe.clone().unwrap_or_default())),
        });
        let Some(stored) = stored else { return AppUpdate::default() };
        match self.app.trace_recipes.library.set(stored) {
            Ok(()) => {
                let update = self.with_study(document, |_, _| {});
                let persisted = self.persist_trace_recipes();
                AppUpdate { effects: persisted.effects, ..update }
            }
            Err(e) => self.refuse(document, library_issue(e)),
        }
    }

    fn delete_recipe(&mut self, document: DocumentId, name: &str) -> AppUpdate {
        if !self.app.trace_recipes.library.remove(name.trim()) {
            return self.refuse(document, RecipeIssue::Unknown);
        }
        let update = self.with_study(document, |_, _| {});
        let persisted = self.persist_trace_recipes();
        AppUpdate { effects: persisted.effects, ..update }
    }

    /// The saved recipe `name` on the picture of the tracer.
    fn stored_on_picture(&self, document: DocumentId, name: &str) -> Result<(TraceRecipe, TraceMode), RecipeIssue> {
        let stored = self.app.trace_recipes.library.get(name.trim()).ok_or(RecipeIssue::Unknown)?;
        let saved = recipe_from_engine(&stored.recipe).map_err(|_| RecipeIssue::Invalid)?;
        let current = self
            .sessions
            .get(&document)
            .and_then(|s| s.tracer.as_ref())
            .and_then(|t| t.recipe.clone())
            .unwrap_or_default();
        let mode = if stored.centerline { TraceMode::Centerline } else { TraceMode::Contour };
        Ok((on_picture(&saved, &current), mode))
    }

    fn apply_recipe(&mut self, document: DocumentId, name: &str) -> AppUpdate {
        let (recipe, mode) = match self.stored_on_picture(document, name) {
            Ok(found) => found,
            Err(issue) => return self.refuse(document, issue),
        };
        self.put_in_force(document, recipe, mode)
    }

    /// `recipe` becomes the recipe in force: the form follows it and the picture is traced again.
    fn put_in_force(&mut self, document: DocumentId, recipe: TraceRecipe, mode: TraceMode) -> AppUpdate {
        self.with_session(document, |s| {
            let Some(t) = s.tracer.as_mut() else { return AppUpdate::default() };
            t.form = form_of(&recipe, mode);
            t.recipe = Some(recipe);
            t.issue = None;
            t.study.issue = None;
            t.study.compare = None;
            AppUpdate::redraw(document)
        });
        self.submit_trace(document)
    }

    fn compare(&mut self, document: DocumentId, with: RecipeChoice) -> AppUpdate {
        let Some(id) = self.next_id::<TaskId>() else { return self.refuse(document, RecipeIssue::Invalid) };
        let b = match &with {
            RecipeChoice::Saved(name) => self.stored_on_picture(document, name).and_then(|(recipe, mode)| {
                if mode == TraceMode::Centerline { Err(RecipeIssue::NeedsContourMode) } else { Ok(recipe) }
            }),
            RecipeChoice::Preset(preset) => {
                let current =
                    self.sessions.get(&document).and_then(|s| s.tracer.as_ref()).and_then(|t| t.recipe.clone());
                let (recipe, _) = preset_recipe(*preset, current.as_ref().and_then(|r| r.image.scale));
                Ok(on_picture(&recipe, &current.unwrap_or_default()))
            }
        };
        let tasks = Arc::clone(&self.ports.tasks);
        self.with_session(document, |s| {
            let revision = s.doc.revision();
            let Some(t) = s.tracer.as_mut() else { return AppUpdate::default() };
            let ready = (|| {
                if t.form.mode != TraceMode::Contour {
                    return Err(RecipeIssue::NeedsContourMode);
                }
                let a = t.recipe.clone().filter(|r| r.image.scale.is_some()).ok_or(RecipeIssue::NeedsScale)?;
                let b = b?;
                b.validate().map_err(|_| RecipeIssue::Invalid)?;
                Ok((a, b))
            })();
            let (a, b) = match ready {
                Ok(pair) => pair,
                Err(issue) => {
                    t.study.issue = Some(issue);
                    return AppUpdate::redraw(document);
                }
            };
            t.study.issue = None;
            let job =
                StudyJob::Compare { task: id, source: Arc::clone(&t.source), a: Box::new(a), b: Box::new(b.clone()) };
            t.study.compare = Some(CompareState { choice: with, b, latest: id, result: None });
            let kind = TaskKind::TraceStudy;
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document,
                source: revision,
                key: revision_key(kind, document, revision),
                cancel: cancel.token(),
                input: TaskInput::TraceStudy(Box::new(job)),
            };
            submit(s, &tasks, request, cancel, document)
        })
    }

    fn adopt_compared(&mut self, document: DocumentId) -> AppUpdate {
        let b = self
            .sessions
            .get(&document)
            .and_then(|s| s.tracer.as_ref())
            .and_then(|t| t.study.compare.as_ref())
            .map(|c| c.b.clone());
        match b {
            Some(recipe) => self.put_in_force(document, recipe, TraceMode::Contour),
            None => self.refuse(document, RecipeIssue::Unknown),
        }
    }

    fn run_batch(&mut self, document: DocumentId, paths: Vec<PathBuf>) -> AppUpdate {
        let Some(id) = self.next_id::<TaskId>() else { return self.refuse(document, RecipeIssue::Invalid) };
        let (tasks, fs) = (Arc::clone(&self.ports.tasks), Arc::clone(&self.ports.fs));
        self.with_session(document, |s| {
            let revision = s.doc.revision();
            let Some(t) = s.tracer.as_mut() else { return AppUpdate::default() };
            if t.form.mode != TraceMode::Contour {
                t.study.issue = Some(RecipeIssue::NeedsContourMode);
                return AppUpdate::redraw(document);
            }
            if paths.is_empty() {
                t.study.issue = Some(RecipeIssue::NoFiles);
                return AppUpdate::redraw(document);
            }
            t.study.issue = None;
            let previous = t.study.batch.as_ref().filter(|b| b.running).map(|b| b.latest);
            let recipe = t.recipe.clone().unwrap_or_default();
            t.study.batch =
                Some(BatchState { latest: id, total: paths.len(), running: true, cancelled: false, rows: Vec::new() });
            if let Some((_, cancel)) = previous.and_then(|old| s.pending.get(&old)) {
                cancel.cancel();
            }
            let job = StudyJob::Batch { task: id, revision, paths, recipe: Box::new(recipe), fs };
            let kind = TaskKind::TraceStudy;
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document,
                source: revision,
                key: revision_key(kind, document, revision),
                cancel: cancel.token(),
                input: TaskInput::TraceStudy(Box::new(job)),
            };
            submit(s, &tasks, request, cancel, document)
        })
    }

    // ---- persistence of the recipes ----------------------------------------------------------

    /// Writes the recipe file in the background: one write at a time, the latest text last.
    pub(super) fn persist_trace_recipes(&mut self) -> AppUpdate {
        let changed = AppUpdate { view_changed: true, ..AppUpdate::default() };
        let Some(dir) = self.app.user_dir().map(std::path::Path::to_path_buf) else { return changed };
        if self.app.trace_recipes.write_in_flight.is_some() {
            self.app.trace_recipes.write_again = true;
            return changed;
        }
        let text = encode_trace_recipe_library(&self.app.trace_recipes.library)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok());
        let id = text.and_then(|text| {
            let (document, id) = (self.next_id()?, self.next_id::<TaskId>()?);
            let input = TaskInput::WriteText {
                text: Arc::from(text),
                path: dir.join(RECIPES_FILE),
                fs: Arc::clone(&self.ports.fs),
            };
            let kind = input.kind();
            let source = burnburn_types::RevisionId::INITIAL;
            let request = TaskRequest {
                id,
                document,
                source,
                key: revision_key(kind, document, source),
                cancel: CancellationSource::new().token(),
                input,
            };
            self.ports.tasks.submit(request).ok().map(|()| id)
        });
        match id {
            Some(id) => self.app.trace_recipes.write_in_flight = Some(id),
            None => self.app.trace_recipes.save_failed = true,
        }
        changed
    }

    /// A finished write of the recipe file; `None` when the task is not ours.
    pub(super) fn trace_recipes_write_finished(&mut self, outcome: &TaskOutcome) -> Option<AppUpdate> {
        if self.app.trace_recipes.write_in_flight != Some(outcome.id) {
            return None;
        }
        self.app.trace_recipes.write_in_flight = None;
        self.app.trace_recipes.save_failed = outcome.result.is_err();
        if std::mem::take(&mut self.app.trace_recipes.write_again) {
            return Some(self.persist_trace_recipes());
        }
        Some(AppUpdate { view_changed: true, ..AppUpdate::default() })
    }
}

/// The recipes of the user and what happened to their file.
#[derive(Debug, Default)]
pub(super) struct RecipeStore {
    pub(super) library: TraceRecipeLibrary,
    pub(super) load_problem: bool,
    pub(super) save_failed: bool,
    pub(super) write_in_flight: Option<TaskId>,
    pub(super) write_again: bool,
}

// ---- results of the jobs ---------------------------------------------------------------------

/// Delivery of a `TraceStudy` task result.
pub(super) fn finished(s: &mut Session, id: TaskId, result: Result<TaskOutput, TaskError>, update: &mut AppUpdate) {
    let document = s.doc.id();
    let Some(t) = s.tracer.as_mut() else { return };
    match result {
        Ok(TaskOutput::Studied(StudyOutput::Compared(comparison))) => {
            if let Some(c) = t.study.compare.as_mut().filter(|c| c.latest == id) {
                c.result = Some(comparison);
                update.scenes_changed.push(document);
            }
        }
        Ok(TaskOutput::Studied(StudyOutput::Batched { task, files })) => {
            if let Some(b) = t.study.batch.as_mut().filter(|b| b.latest == task && !b.cancelled) {
                b.rows = files.iter().map(row_of).collect();
                b.running = false;
            }
        }
        Err(TaskError::Cancelled) => {
            // A cancelled batch was marked when the user cancelled it; a cancelled comparison was
            // replaced by a newer one.
            if let Some(b) = t.study.batch.as_mut().filter(|b| b.latest == id) {
                b.running = false;
                b.cancelled = true;
            }
        }
        Err(_) => {
            if t.study.compare.as_ref().is_some_and(|c| c.latest == id) {
                t.study.compare = None;
                t.study.issue = Some(RecipeIssue::Invalid);
            }
            if t.study.batch.as_ref().is_some_and(|b| b.latest == id) {
                t.study.batch = None;
                t.study.issue = Some(RecipeIssue::Invalid);
            }
        }
        Ok(_) => {}
    }
}

fn summary_of(r: &RecipeSummary) -> VariantSummary {
    VariantSummary {
        paths: r.paths,
        closed_shapes: r.closed_shapes,
        holes: r.holes,
        nodes: r.nodes,
        removed: r.removed,
        max_deviation_mm: r.max_deviation_mm,
    }
}

fn row_of(file: &FileOutcome) -> BatchRowView {
    use burnburn_trace::BatchProblem;
    BatchRowView {
        name: file.name.clone(),
        summary: file.summary.as_ref().map(summary_of),
        problems: file
            .problems
            .iter()
            .map(|p| match p {
                FileProblem::Unreadable => BatchProblemView::Unreadable,
                FileProblem::NeedsScale => BatchProblemView::NeedsScale,
                FileProblem::Trace(BatchProblem::NoGeometry) => BatchProblemView::NoGeometry,
                FileProblem::Trace(BatchProblem::RemovedFeatures { count }) => {
                    BatchProblemView::RemovedFeatures { count: *count }
                }
                FileProblem::Trace(BatchProblem::Failed(burnburn_trace::TraceError::Image(
                    burnburn_image::ImageError::Scale(burnburn_image::ScaleError::NeedsScale),
                ))) => BatchProblemView::NeedsScale,
                FileProblem::Trace(_) => BatchProblemView::Failed,
            })
            .collect(),
    }
}

pub(super) fn view(s: &Session, app: &AppPanels) -> Option<TraceRecipesViewModel> {
    let t = s.tracer.as_ref()?;
    let store = &app.trace_recipes;
    let saved = store
        .library
        .recipes
        .iter()
        .map(|r| SavedRecipeRow {
            name: r.name.clone(),
            mode: if r.centerline { TraceMode::Centerline } else { TraceMode::Contour },
        })
        .collect();
    Some(TraceRecipesViewModel {
        saved,
        issue: t.study.issue,
        compare: t.study.compare.as_ref().map(|c| CompareViewModel {
            b_choice: c.choice.clone(),
            computing: c.result.is_none(),
            a: c.result.as_ref().map(|r| summary_of(&r.a)),
            b: c.result.as_ref().map(|r| summary_of(&r.b)),
            same_geometry: c.result.as_ref().is_some_and(|r| r.a.geometry_hash == r.b.geometry_hash),
            a_outline: c.result.as_ref().map(|r| Arc::new(r.a_result.path())),
            b_outline: c.result.as_ref().map(|r| Arc::new(r.b_result.path())),
        }),
        batch: t.study.batch.as_ref().map(|b| BatchViewModel {
            total: b.total,
            running: b.running,
            cancelled: b.cancelled,
            rows: b.rows.clone(),
        }),
        load_problem: store.load_problem,
        save_failed: store.save_failed,
    })
}
