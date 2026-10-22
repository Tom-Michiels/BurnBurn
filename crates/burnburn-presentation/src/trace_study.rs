//! The background jobs behind the recipe panel of the tracer (G7-trace, T10): two recipes traced on
//! one picture, and one recipe traced on a batch of files. Both are cancellable and both only ever
//! read: the document and the tracer's source are untouched.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use burnburn_image::{DecodeLimits, PixelScale, SourceImage, decode};
use burnburn_io::fs::FileSystem;
use burnburn_trace::{
    BatchItem, BatchProblem, RecipeComparison, RecipeSummary, TraceError, TraceRecipe, compare_recipes, trace_batch,
};
use burnburn_types::{CancellationToken, RevisionId, TaskId};

use crate::task::TaskError;

/// Largest image file a batch reads (the decoder also limits pixels).
const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;

/// One study job.
#[derive(Clone)]
pub enum StudyJob {
    /// The recipe in force (A) and another (B) on the same picture.
    Compare { task: TaskId, source: Arc<SourceImage>, a: Box<TraceRecipe>, b: Box<TraceRecipe> },
    /// One recipe on every file, in order.
    Batch { task: TaskId, revision: RevisionId, paths: Vec<PathBuf>, recipe: Box<TraceRecipe>, fs: Arc<dyn FileSystem> },
}

impl fmt::Debug for StudyJob {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StudyJob::Compare { task, .. } => write!(f, "Compare(task {task})"),
            StudyJob::Batch { task, paths, .. } => write!(f, "Batch(task {task}, {} files)", paths.len()),
        }
    }
}

impl StudyJob {
    pub fn task(&self) -> TaskId {
        match self {
            StudyJob::Compare { task, .. } | StudyJob::Batch { task, .. } => *task,
        }
    }
}

/// What went wrong with one file, besides what the trace itself reports.
#[derive(Debug, Clone, PartialEq)]
pub enum FileProblem {
    /// Not readable, or not a picture.
    Unreadable,
    /// No resolution in the recipe and none in the file.
    NeedsScale,
    Trace(BatchProblem),
}

/// The outcome for one file of a batch.
#[derive(Debug, Clone, PartialEq)]
pub struct FileOutcome {
    pub name: String,
    pub summary: Option<RecipeSummary>,
    pub problems: Vec<FileProblem>,
}

#[derive(Debug, Clone)]
pub enum StudyOutput {
    Compared(Arc<RecipeComparison>),
    Batched { task: TaskId, files: Vec<FileOutcome> },
}

pub fn run_study(job: &StudyJob, cancel: &CancellationToken) -> Result<StudyOutput, TaskError> {
    match job {
        StudyJob::Compare { task, source, a, b } => compare_recipes(source, a, b, *task, cancel)
            .map(|c| StudyOutput::Compared(Arc::new(c)))
            .map_err(trace_error),
        StudyJob::Batch { task, revision, paths, recipe, fs } => {
            let mut files = Vec::with_capacity(paths.len());
            for path in paths {
                cancel.check().map_err(|_| TaskError::Cancelled)?;
                files.push(trace_file(path, recipe, *task, *revision, fs.as_ref(), cancel)?);
            }
            Ok(StudyOutput::Batched { task: *task, files })
        }
    }
}

fn trace_file(
    path: &std::path::Path,
    recipe: &TraceRecipe,
    task: TaskId,
    revision: RevisionId,
    fs: &dyn FileSystem,
    cancel: &CancellationToken,
) -> Result<FileOutcome, TaskError> {
    let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
    let unreadable = |name: String| FileOutcome { name, summary: None, problems: vec![FileProblem::Unreadable] };
    let Ok(bytes) = fs.read(path, MAX_FILE_BYTES) else { return Ok(unreadable(name)) };
    cancel.check().map_err(|_| TaskError::Cancelled)?;
    let Ok(source) = decode(&bytes, DecodeLimits::default()) else { return Ok(unreadable(name)) };
    // The scale of the recipe holds for every file; without one a file brings its own.
    let mut recipe = recipe.clone();
    if recipe.image.scale.is_none() {
        recipe.image.scale = source.metadata().dpi_suggestion.and_then(|(x, y)| PixelScale::from_dpi(x, y).ok());
    }
    if recipe.image.scale.is_none() {
        return Ok(FileOutcome { name, summary: None, problems: vec![FileProblem::NeedsScale] });
    }
    let item = BatchItem { name: name.clone(), source: Arc::new(source) };
    let report = trace_batch(&[item], &recipe, task, revision, cancel).map_err(trace_error)?;
    let Some(outcome) = report.outcomes.into_iter().next() else { return Ok(unreadable(name)) };
    Ok(FileOutcome {
        name,
        summary: outcome.summary,
        problems: outcome.problems.into_iter().map(FileProblem::Trace).collect(),
    })
}

fn trace_error(e: TraceError) -> TaskError {
    match e {
        TraceError::Cancelled(_) => TaskError::Cancelled,
        other => TaskError::Trace(other),
    }
}
