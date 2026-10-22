use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use burnburn_cam::{CompileError, CompileOptions, PreflightReport, compile_with, preflight, preflight_with};
use burnburn_document::DocumentSnapshot;
use burnburn_geometry::{
    BooleanError, BooleanOp, Bounds, OffsetError, OffsetJoin, Path, ProductionTolerance, boolean, offset,
};
use burnburn_image::{DecodeError, DecodeLimits, RasterError, SourceImage, decode};
use burnburn_io::fs::{FileSystem, FsError};
use burnburn_io::import::{
    Analysis, AnalyzeError, DxfDocument, ImportDraft, ImportFormat, Lbrn2Draft, ParseError, PdfDocument, PdfPages,
    analyze,
};
use burnburn_io::{
    AssetStore, LoadError, Project, SaveError, load_project, save_project, save_project_keeping_previous,
};
use burnburn_job::{JobPlan, JobPlanId, MachineProfile, PlanPreview, preview};
use burnburn_ruida::{EncodeError, LowerError, RuidaProfile, lower};
use burnburn_trace::{
    CenterlineRequest, CenterlineResult, TraceError, TraceRequest, TraceResult, replay_corrections, retrace_region,
    trace,
};
use burnburn_types::{
    CancellationToken, ContentHash, DocumentId, GeometryId, HashDomain, RevisionId, StableId, TaskId,
};

use crate::files_task::{FilesError, FilesOutput, FilesTask};
use crate::image_panel::ImagePreview;
use crate::image_task::{BitmapInput, BitmapJob};

/// One background job, bound to the document revision of its input snapshot.
#[derive(Debug, Clone)]
pub struct TaskRequest {
    pub id: TaskId,
    pub document: DocumentId,
    /// Revision of the snapshot inside `input`.
    pub source: RevisionId,
    /// Cache key. Until the canonical snapshot hash (ADR 0004) exists this is revision-scoped:
    /// task kind, document and revision packed into the hash bytes.
    pub key: ContentHash,
    pub cancel: CancellationToken,
    pub input: TaskInput,
}

/// A contour trace and the local corrections to apply to it (T05).
#[derive(Clone)]
pub struct CorrectedTrace {
    pub request: TraceRequest,
    pub corrections: Arc<Vec<burnburn_trace::CorrectionRecord>>,
}

#[derive(Clone)]
#[non_exhaustive]
pub enum TaskInput {
    /// Object count and overall extents of a snapshot (status bar; first worker contract).
    Measure(Arc<DocumentSnapshot>),
    /// Atomic save of one revision (temp + sync + replace, IM04).
    Save { project: Box<Project>, path: PathBuf, fs: Arc<dyn FileSystem> },
    /// Read a project container; nothing on disk changes.
    Open { path: PathBuf, fs: Arc<dyn FileSystem> },
    /// Boolean of the operands' outlines (live preview, E05). Difference: first minus the rest.
    Boolean { snapshot: Arc<DocumentSnapshot>, op: BooleanOp, operands: Vec<GeometryId> },
    /// Compile one revision for one machine profile into an immutable JobPlan, with its preview
    /// and preflight (B12).
    /// `assets` holds the source pixels of image operations (decoded on the worker).
    Compile {
        snapshot: Arc<DocumentSnapshot>,
        profile: Arc<MachineProfile>,
        options: CompileOptions,
        assets: AssetStore,
    },
    /// The same compile with the laser window's setup: only the included objects, and the plan put
    /// on the chosen start position / job origin before the preview and preflight are derived.
    CompilePlaced {
        snapshot: Arc<DocumentSnapshot>,
        profile: Arc<MachineProfile>,
        options: CompileOptions,
        assets: AssetStore,
        laser: Arc<crate::laser::LaserCompile>,
        /// Identifies the setup, compared later with the current one (a changed setup = stale plan).
        setup_key: u64,
    },
    /// Lower exactly this plan to RD and write it atomically (M03). Never touches the document.
    ExportRd { plan: Arc<JobPlan>, ruida: RuidaProfile, path: PathBuf, fs: Arc<dyn FileSystem> },
    /// Lower exactly this plan to G-code for a GRBL machine and write it atomically (G5-grbl-ui).
    /// Never touches the document; a refused plan writes nothing.
    ExportGcode { plan: Arc<JobPlan>, grbl: burnburn_gcode::GrblProfile, path: PathBuf, fs: Arc<dyn FileSystem> },
    /// Read and decode an image for the tracer (bounded by `DecodeLimits`).
    LoadImage { path: PathBuf, fs: Arc<dyn FileSystem> },
    /// Decode image bytes already in memory for the tracer (a trace's stored source, T02).
    DecodeImage { bytes: Arc<[u8]> },
    /// Contour trace of a source image with one recipe (IM12).
    Trace(Box<TraceRequest>),
    /// A contour trace followed by the stored local corrections (T05).
    TraceCorrected(Box<CorrectedTrace>),
    /// One area traced again on top of a finished result (T05).
    CorrectRegion(Box<burnburn_trace::CorrectionRequest>),
    /// Read and parse an SVG/DXF for the import preview (P03); nothing is committed.
    ParseImport { path: PathBuf, fs: Arc<dyn FileSystem> },
    /// A job of the file features: templates, libraries, bundles and exports.
    Files(FilesTask),
    /// The objects of other pages of a PDF/AI file already parsed for the import preview.
    ImportPages { pdf: Arc<PdfDocument>, pages: PdfPages },
    /// Lower exactly this plan to RD bytes for the prepared job (C1); same lowering as `ExportRd`.
    PrepareJob { plan: Arc<JobPlan>, ruida: RuidaProfile },
    /// Read and decode a bitmap for the image flows: import, replace/refresh, or the pixels of an
    /// image operation already in the project (W2-L1-beeld).
    LoadBitmap { input: BitmapInput },
    /// The processed pixels of an image operation, for the live preview of Adjust Image.
    ImagePreview(Box<BitmapJob>),
    /// The same pixels, written as a gray PNG ("Save Processed Bitmap").
    SaveBitmap { job: Box<BitmapJob>, path: PathBuf, fs: Arc<dyn FileSystem> },
    /// Centre-line trace of a source image (T04).
    Centerline(Box<CenterlineRequest>),
    /// Autosave: the same atomic save as `Save`, to a recovery file (never the project file).
    SaveRecovery { project: Box<Project>, path: PathBuf, fs: Arc<dyn FileSystem> },
    /// Recovery files (`*.burnburn`) in `dir`; a missing directory has none.
    ListRecovery { dir: PathBuf, fs: Arc<dyn FileSystem> },
    /// Write a text file atomically (the diagnostic report); independent of any document.
    WriteText { text: Arc<str>, path: PathBuf, fs: Arc<dyn FileSystem> },
    /// Remove one recovery file (after a save, or when the user declines the offer).
    RemoveRecovery { path: PathBuf, fs: Arc<dyn FileSystem> },
    /// Compile a private test job (Material / Interval / Focus Test) with the ordinary compiler;
    /// the open document is not involved.
    CompileTest { job: Arc<burnburn_cam::test_job::TestJob>, profile: Arc<MachineProfile>, options: CompileOptions },
    /// Lower exactly this test plan to RD and write it atomically; never the project.
    ExportTest { plan: Arc<JobPlan>, ruida: RuidaProfile, path: PathBuf, fs: Arc<dyn FileSystem> },
    /// Offset of the targets' outlines by a signed distance (live preview, E05); never touches the
    /// document.
    OffsetPreview {
        snapshot: Arc<DocumentSnapshot>,
        targets: Vec<GeometryId>,
        distance: burnburn_types::Length,
        join: OffsetJoin,
    },
    /// What closing the gaps of the targets at this tolerance would give (E09 before/after); a
    /// scratch copy of the snapshot, never the document.
    RepairPreview {
        snapshot: Arc<DocumentSnapshot>,
        targets: Vec<GeometryId>,
        tolerance: burnburn_geometry::DiagnosticTolerance,
    },
    /// Pack the parts of a nest job on its sheet (W3-UI-schikken, C10); cancellable.
    Nest(Box<crate::nest_task::NestJob>),
    /// A compile (plain or placed) whose plan gets the feeder applied before its preview is made
    /// (FE01), so preview, preflight and RD attempt refer to the same fed plan.
    Fed { inner: Box<TaskInput>, feeder: burnburn_cam::FeederSettings },
}

impl fmt::Debug for TaskInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskInput::Measure(s) => write!(f, "Measure(revision {})", s.revision()),
            TaskInput::Save { project, path, .. } => {
                write!(f, "Save({} at revision {})", path.display(), project.document.revision())
            }
            TaskInput::Open { path, .. } => write!(f, "Open({})", path.display()),
            TaskInput::Boolean { snapshot, op, operands } => {
                write!(f, "Boolean({op:?} of {operands:?} at revision {})", snapshot.revision())
            }
            TaskInput::OffsetPreview { snapshot, targets, distance, join } => write!(
                f,
                "OffsetPreview({join:?} {} mm of {targets:?} at revision {})",
                distance.mm(),
                snapshot.revision()
            ),
            TaskInput::RepairPreview { snapshot, targets, .. } => {
                write!(f, "RepairPreview({targets:?} at revision {})", snapshot.revision())
            }
            TaskInput::CompilePlaced { snapshot, profile, .. } => {
                write!(f, "CompilePlaced(revision {} for {})", snapshot.revision(), profile.revision().id)
            }
            TaskInput::Compile { snapshot, profile, .. } => {
                write!(f, "Compile(revision {} for {})", snapshot.revision(), profile.revision().id)
            }
            TaskInput::ExportRd { plan, path, .. } => write!(f, "ExportRd({:?} to {})", plan.id(), path.display()),
            TaskInput::ExportGcode { plan, path, .. } => {
                write!(f, "ExportGcode({:?} to {})", plan.id(), path.display())
            }
            TaskInput::LoadImage { path, .. } => write!(f, "LoadImage({})", path.display()),
            TaskInput::DecodeImage { bytes } => write!(f, "DecodeImage({} bytes)", bytes.len()),
            TaskInput::Trace(r) => write!(f, "Trace(task {})", r.task),
            TaskInput::TraceCorrected(r) => {
                write!(f, "TraceCorrected(task {}, {} corrections)", r.request.task, r.corrections.len())
            }
            TaskInput::CorrectRegion(r) => write!(f, "CorrectRegion(task {})", r.task),
            TaskInput::PrepareJob { plan, .. } => write!(f, "PrepareJob({:?})", plan.id()),
            TaskInput::LoadBitmap { input } => write!(f, "LoadBitmap({input:?})"),
            TaskInput::ImagePreview(_) => f.write_str("ImagePreview"),
            TaskInput::SaveBitmap { path, .. } => write!(f, "SaveBitmap({})", path.display()),
            TaskInput::Centerline(r) => write!(f, "Centerline(task {})", r.task),
            TaskInput::ParseImport { path, .. } => write!(f, "ParseImport({})", path.display()),
            TaskInput::ImportPages { pages, .. } => write!(f, "ImportPages({pages:?})"),
            TaskInput::Files(task) => write!(f, "Files({task:?})"),
            TaskInput::SaveRecovery { path, .. } => write!(f, "SaveRecovery({})", path.display()),
            TaskInput::ListRecovery { dir, .. } => write!(f, "ListRecovery({})", dir.display()),
            TaskInput::RemoveRecovery { path, .. } => write!(f, "RemoveRecovery({})", path.display()),
            TaskInput::WriteText { path, .. } => write!(f, "WriteText({})", path.display()),
            TaskInput::CompileTest { job, profile, .. } => {
                write!(f, "CompileTest({} cells for {})", job.cells().len(), profile.revision().id)
            }
            TaskInput::ExportTest { plan, path, .. } => write!(f, "ExportTest({:?} to {})", plan.id(), path.display()),
            TaskInput::Nest(job) => write!(f, "Nest({} parts)", job.part_count()),
            TaskInput::Fed { inner, feeder } => write!(f, "Fed({inner:?}, repeat {})", feeder.repeat()),
        }
    }
}

impl TaskInput {
    pub fn kind(&self) -> TaskKind {
        match self {
            TaskInput::Measure(_) => TaskKind::Measure,
            TaskInput::Save { .. } => TaskKind::Save,
            TaskInput::Open { .. } => TaskKind::Open,
            TaskInput::Boolean { .. } => TaskKind::Boolean,
            TaskInput::RepairPreview { .. } => TaskKind::RepairPreview,
            TaskInput::OffsetPreview { .. } => TaskKind::OffsetPreview,
            TaskInput::Compile { .. } | TaskInput::CompilePlaced { .. } => TaskKind::Compile,
            TaskInput::ExportRd { .. } => TaskKind::ExportRd,
            TaskInput::ExportGcode { .. } => TaskKind::ExportGcode,
            TaskInput::LoadImage { .. } | TaskInput::DecodeImage { .. } => TaskKind::LoadImage,
            TaskInput::Trace(_) | TaskInput::TraceCorrected(_) | TaskInput::CorrectRegion(_) => TaskKind::Trace,
            TaskInput::PrepareJob { .. } => TaskKind::PrepareJob,
            TaskInput::LoadBitmap { .. } => TaskKind::LoadBitmap,
            TaskInput::ImagePreview(_) => TaskKind::ImagePreview,
            TaskInput::SaveBitmap { .. } => TaskKind::SaveBitmap,
            TaskInput::Centerline(_) => TaskKind::Centerline,
            TaskInput::ParseImport { .. } => TaskKind::ParseImport,
            TaskInput::ImportPages { .. } => TaskKind::ImportPages,
            TaskInput::Files(_) => TaskKind::Files,
            TaskInput::SaveRecovery { .. } => TaskKind::Autosave,
            TaskInput::ListRecovery { .. } | TaskInput::RemoveRecovery { .. } => TaskKind::Recovery,
            TaskInput::WriteText { .. } => TaskKind::TextExport,
            TaskInput::CompileTest { .. } | TaskInput::ExportTest { .. } => TaskKind::TestJob,
            TaskInput::Nest(_) => TaskKind::Nest,
            TaskInput::Fed { inner, .. } => inner.kind(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TaskKind {
    Measure,
    Save,
    Open,
    Boolean,
    Compile,
    ExportRd,
    /// Writes G-code of one plan; like the RD export it finishes what it started.
    ExportGcode,
    LoadImage,
    /// Freshness is checked by task identity and recipe key (DR-14), not by document revision:
    /// an unrelated edit keeps a trace valid.
    Trace,
    /// Names its plan; an edit does not invalidate it (the job is frozen, W01).
    PrepareJob,
    /// Reads a file; independent of the document revision.
    ParseImport,
    /// The objects of the chosen PDF pages; freshness by task identity (only the newest choice counts).
    ImportPages,
    /// Template, library, bundle and export jobs; not tied to a document revision.
    Files,
    /// Reads a bitmap; independent of the document revision (the controller keeps the purpose).
    LoadBitmap,
    /// Freshness is by task identity: only the newest preview of the open Adjust Image panel counts.
    ImagePreview,
    /// Writes a processed bitmap; finishes what it started.
    SaveBitmap,
    /// Like `Trace`: freshness by task identity and input key.
    Centerline,
    /// Writes a recovery file of one revision; like a save it finishes what it started.
    Autosave,
    /// Lists or removes recovery files; not tied to a document revision.
    Recovery,
    /// Writes a text export (diagnostic report); not tied to a document revision.
    TextExport,
    /// Compiles or writes a private test job; freshness is by task identity (only the newest
    /// counts), never by document revision: an unrelated edit keeps a test valid.
    TestJob,
    /// A nest of one revision: cancelled by an edit, and a result of another revision is dropped.
    Nest,
    /// The preview of an offset (E05): worthless after an edit.
    OffsetPreview,
    /// The before/after of a repair (E09): worthless after an edit.
    RepairPreview,
}

impl TaskKind {
    /// Results that describe one revision and are worthless after an edit: cancelled when the
    /// document changes. A save is not: it must finish writing the revision it started with.
    pub fn is_revision_sensitive(self) -> bool {
        matches!(
            self,
            TaskKind::Measure
                | TaskKind::Boolean
                | TaskKind::Compile
                | TaskKind::Nest
                | TaskKind::OffsetPreview
                | TaskKind::RepairPreview
        )
    }
}

#[derive(Debug, Clone)]
pub struct TaskOutcome {
    pub id: TaskId,
    pub document: DocumentId,
    pub source: RevisionId,
    pub result: Result<TaskOutput, TaskError>,
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum TaskOutput {
    Measured(DocumentMeasure),
    Saved {
        path: PathBuf,
    },
    /// Saved over a file the current build could not reproduce: that file was kept at `backup` (X07).
    SavedWithBackup {
        path: PathBuf,
        backup: PathBuf,
    },
    Opened {
        project: Box<Project>,
        path: PathBuf,
    },
    Boolean {
        op: BooleanOp,
        operands: Vec<GeometryId>,
        result: Path,
    },
    OffsetPreview {
        distance: burnburn_types::Length,
        join: OffsetJoin,
        result: Path,
    },
    RepairPreview {
        result: Path,
        closes: usize,
        stays_open: usize,
    },
    Compiled {
        plan: Arc<JobPlan>,
        preview: Arc<PlanPreview>,
        report: PreflightReport,
    },
    /// A compile with the laser window's setup applied.
    CompiledPlaced {
        plan: Arc<JobPlan>,
        preview: Arc<PlanPreview>,
        report: PreflightReport,
        placement: crate::laser::AppliedPlacement,
    },
    ExportedRd {
        plan: JobPlanId,
        path: PathBuf,
        rd_hash: ContentHash,
        bytes: usize,
    },
    ExportedGcode {
        plan: JobPlanId,
        path: PathBuf,
        bytes: usize,
        lines: usize,
    },
    ImageLoaded {
        source: Arc<SourceImage>,
        bytes: Arc<[u8]>,
        path: PathBuf,
    },
    Traced(Arc<TraceResult>),
    /// A fresh trace with the stored corrections applied, and what became of each (T05).
    TracedCorrected(Arc<burnburn_trace::CorrectionReplay>),
    /// A finished result with one more area traced again (T05).
    Corrected(Arc<TraceResult>),
    Centerlined(Arc<CenterlineResult>),
    /// The processed pixels of an image operation (live preview).
    ImagePreview {
        preview: ImagePreview,
        /// The decoded original pixels, so the controller can reuse them for the next preview.
        source: Arc<SourceImage>,
    },
    BitmapSaved {
        path: PathBuf,
        width: u32,
        height: u32,
    },
    /// RD bytes of exactly `plan`.
    Prepared {
        plan: JobPlanId,
        rd: Arc<[u8]>,
        rd_hash: ContentHash,
    },
    /// A parsed import file with its bytes (they become the source asset on commit).
    ImportParsed {
        path: PathBuf,
        bytes: Arc<[u8]>,
        parsed: ParsedImport,
    },
    /// The result of a file-feature job.
    Files(FilesOutput),
    /// The objects of the chosen pages of a PDF/AI import.
    ImportPagesDrafted {
        pages: PdfPages,
        draft: Box<ImportDraft>,
    },
    RecoverySaved {
        path: PathBuf,
    },
    RecoveryFound {
        files: Vec<PathBuf>,
    },
    RecoveryRemoved {
        path: PathBuf,
    },
    TextWritten {
        path: PathBuf,
    },
    /// The placements of a nest.
    Nested(Arc<crate::nest_task::NestOutput>),
}

/// A parsed import file. A DXF stays in drawing units until the unit decision.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ParsedImport {
    Svg(Box<ImportDraft>),
    Dxf(Box<DxfDocument>),
    /// HP-GL/PLT: plotter units, so no unit question.
    Hpgl(Box<ImportDraft>),
    /// PDF or Illustrator: the pages, and the objects of the first one.
    Pdf(Box<PdfImport>),
    /// A LightBurn project: shapes, layer proposals and the report.
    Lbrn2(Box<Lbrn2Import>),
}

/// A PDF/AI file read for the import preview.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfImport {
    pub doc: Arc<PdfDocument>,
    /// The objects of page 1 (the preview starts there).
    pub first: ImportDraft,
}

/// A LightBurn project read for the import preview, with its embedded bitmaps decoded.
#[derive(Debug, Clone)]
pub struct Lbrn2Import {
    pub draft: Lbrn2Draft,
    /// The decoded pixels of `draft.report.assets[i]`; `None` when the asset has no readable bitmap.
    pub images: Vec<Option<Arc<SourceImage>>>,
}

impl PartialEq for Lbrn2Import {
    fn eq(&self, other: &Self) -> bool {
        self.draft == other.draft
            && self.images.len() == other.images.len()
            && self.images.iter().zip(&other.images).all(|(a, b)| match (a, b) {
                (Some(a), Some(b)) => a.content_hash() == b.content_hash(),
                (None, None) => true,
                _ => false,
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DocumentMeasure {
    pub objects: usize,
    pub bounds: Option<Bounds>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TaskError {
    Cancelled,
    Save(SaveError),
    Open(LoadError),
    Boolean(BooleanError),
    /// An operand id is not in the snapshot.
    UnknownObject(GeometryId),
    Compile(CompileError),
    /// The laser window's placement was refused (position unknown, off the bed, ...).
    Placement(crate::laser::LaserWhy),
    Lower(LowerError),
    /// The GRBL backend refuses the plan (every reason, as the preflight names them).
    GcodeLower(burnburn_gcode::LowerError),
    Encode(EncodeError),
    Write(FsError),
    Read(FsError),
    Decode(DecodeError),
    Trace(TraceError),
    Parse(ParseError),
    Raster(RasterError),
    /// The source pixels an image operation names are not among the project's assets.
    SourceMissing,
    /// Neither `.svg` nor `.dxf`.
    UnsupportedFormat,
    /// The nest could not be made.
    Nest(burnburn_cam::nest::NestError),
    /// A file-feature job failed.
    Files(FilesError),
    /// The offset of a preview is impossible (collapsed, open contour, ...).
    Offset(OffsetError),
    /// The repair preview could not be made.
    Repair(burnburn_document::CommandError),
    /// The feeder could not be planned around the compiled plan.
    Feeder(burnburn_cam::FeederError),
}

impl fmt::Display for TaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskError::Cancelled => f.write_str("task cancelled"),
            TaskError::Save(e) => write!(f, "save failed: {e}"),
            TaskError::Open(e) => write!(f, "open failed: {e}"),
            TaskError::Boolean(e) => write!(f, "boolean failed: {e}"),
            TaskError::UnknownObject(id) => write!(f, "no object {id}"),
            TaskError::Compile(e) => write!(f, "compile failed: {e:?}"),
            TaskError::Placement(why) => write!(f, "placement refused: {why}"),
            TaskError::Lower(e) => write!(f, "RD lowering failed: {e:?}"),
            TaskError::GcodeLower(e) => write!(f, "{e}"),
            TaskError::Encode(e) => write!(f, "RD encoding failed: {e:?}"),
            TaskError::Write(e) => write!(f, "writing failed: {e:?}"),
            TaskError::Read(e) => write!(f, "reading failed: {e:?}"),
            TaskError::Decode(e) => write!(f, "image decoding failed: {e:?}"),
            TaskError::Trace(e) => write!(f, "trace failed: {e:?}"),
            TaskError::Parse(e) => write!(f, "import failed: {e}"),
            TaskError::Raster(e) => write!(f, "image processing failed: {e:?}"),
            TaskError::SourceMissing => f.write_str("source image not in the project"),
            TaskError::UnsupportedFormat => f.write_str("unsupported import format"),
            TaskError::Nest(e) => write!(f, "nesting failed: {e}"),
            TaskError::Files(e) => write!(f, "{e}"),
            TaskError::Offset(e) => write!(f, "offset failed: {e:?}"),
            TaskError::Repair(e) => write!(f, "repair preview failed: {e}"),
            TaskError::Feeder(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for TaskError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskPortError {
    /// The worker pool is shut down (app quitting).
    Unavailable,
}

impl fmt::Display for TaskPortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("task port unavailable")
    }
}

impl std::error::Error for TaskPortError {}

/// Port: runs a request off the UI thread and later delivers its outcome to
/// `AppController::task_finished` through the host's wake-up. Must never block the UI thread.
pub trait TaskPort: Send + Sync {
    fn submit(&self, request: TaskRequest) -> Result<(), TaskPortError>;
}

/// The task work itself. Lives here so runtime only provides threads, and the headless harness
/// runs exactly the same function as production.
pub fn run_task(input: TaskInput, cancel: &CancellationToken) -> Result<TaskOutput, TaskError> {
    match input {
        TaskInput::Fed { inner, feeder } => {
            let profile = match &*inner {
                TaskInput::Compile { profile, .. } | TaskInput::CompilePlaced { profile, .. } => Arc::clone(profile),
                // Only compiles carry a plan to wrap.
                _ => return run_task(*inner, cancel),
            };
            let fed = |plan: &JobPlan| match burnburn_cam::plan_feeder(plan, &feeder, cancel) {
                Ok(p) => Ok(p),
                Err(burnburn_cam::FeederError::Cancelled) => Err(TaskError::Cancelled),
                Err(e) => Err(TaskError::Feeder(e)),
            };
            match run_task(*inner, cancel)? {
                TaskOutput::Compiled { plan, report, .. } => {
                    let plan = Arc::new(fed(&plan)?);
                    let preview = Arc::new(preview(&plan, &profile));
                    Ok(TaskOutput::Compiled { plan, preview, report })
                }
                TaskOutput::CompiledPlaced { plan, report, placement, .. } => {
                    let plan = Arc::new(fed(&plan)?);
                    let preview = Arc::new(preview(&plan, &profile));
                    Ok(TaskOutput::CompiledPlaced { plan, preview, report, placement })
                }
                other => Ok(other),
            }
        }
        TaskInput::Measure(snapshot) => {
            let mut bounds: Option<Bounds> = None;
            for (_, obj) in snapshot.objects() {
                cancel.check().map_err(|_| TaskError::Cancelled)?;
                if let Some(b) = obj.bounds() {
                    bounds = Some(bounds.map_or(b, |acc| acc.union(b)));
                }
            }
            cancel.check().map_err(|_| TaskError::Cancelled)?;
            Ok(TaskOutput::Measured(DocumentMeasure { objects: snapshot.len(), bounds }))
        }
        TaskInput::Save { project, path, fs } => {
            // Checked once before writing; an atomic save is never abandoned half-way.
            cancel.check().map_err(|_| TaskError::Cancelled)?;
            let outcome = save_project_keeping_previous(fs.as_ref(), &path, &project).map_err(TaskError::Save)?;
            Ok(match outcome.backup {
                Some(backup) => TaskOutput::SavedWithBackup { path, backup },
                None => TaskOutput::Saved { path },
            })
        }
        TaskInput::Open { path, fs } => {
            cancel.check().map_err(|_| TaskError::Cancelled)?;
            let project = load_project(fs.as_ref(), &path).map_err(TaskError::Open)?;
            Ok(TaskOutput::Opened { project: Box::new(project), path })
        }
        TaskInput::Boolean { snapshot, op, operands } => {
            let result = boolean_of(&snapshot, op, &operands, cancel)?;
            Ok(TaskOutput::Boolean { op, operands, result })
        }
        TaskInput::RepairPreview { snapshot, targets, tolerance } => {
            cancel.check().map_err(|_| TaskError::Cancelled)?;
            let (result, closes, stays_open) = repair_of(&snapshot, &targets, tolerance)?;
            Ok(TaskOutput::RepairPreview { result, closes, stays_open })
        }
        TaskInput::OffsetPreview { snapshot, targets, distance, join } => {
            let result = offset_of(&snapshot, &targets, distance.mm(), join, cancel)?;
            Ok(TaskOutput::OffsetPreview { distance, join, result })
        }
        TaskInput::Compile { snapshot, profile, options, assets } => {
            let rasters = crate::image_task::compile_rasters(&snapshot, &assets);
            let plan = compile_with(&snapshot, &profile, &options, &rasters, cancel).map_err(|e| match e {
                CompileError::Cancelled => TaskError::Cancelled,
                e => TaskError::Compile(e),
            })?;
            cancel.check().map_err(|_| TaskError::Cancelled)?;
            let preview = Arc::new(preview(&plan, &profile));
            // The source pixels are at hand: findings about images can be judged.
            let report = preflight_with(&snapshot, &plan, &profile, &rasters);
            Ok(TaskOutput::Compiled { plan: Arc::new(plan), preview, report })
        }
        TaskInput::CompilePlaced { snapshot, profile, options, assets, laser, setup_key } => {
            let rasters = crate::image_task::compile_rasters(&snapshot, &assets);
            let (plan, report, placement) =
                crate::laser::compile_placed(&snapshot, &profile, &options, &rasters, &laser, setup_key, cancel)?;
            let preview = Arc::new(preview(&plan, &profile));
            Ok(TaskOutput::CompiledPlaced { plan: Arc::new(plan), preview, report, placement })
        }
        TaskInput::ExportRd { plan, ruida, path, fs } => {
            cancel.check().map_err(|_| TaskError::Cancelled)?;
            let program = lower(&plan, &ruida).map_err(TaskError::Lower)?;
            let bytes = program.to_rd_file(ruida.magic).map_err(TaskError::Encode)?;
            write_atomically(fs.as_ref(), &path, &bytes).map_err(TaskError::Write)?;
            Ok(TaskOutput::ExportedRd {
                plan: program.plan(),
                path,
                rd_hash: ContentHash::of(HashDomain::Rd, &bytes),
                bytes: bytes.len(),
            })
        }
        TaskInput::ExportGcode { plan, grbl, path, fs } => {
            cancel.check().map_err(|_| TaskError::Cancelled)?;
            let program = burnburn_gcode::lower(&plan, &grbl).map_err(TaskError::GcodeLower)?;
            let text = program.to_text();
            write_atomically(fs.as_ref(), &path, text.as_bytes()).map_err(TaskError::Write)?;
            Ok(TaskOutput::ExportedGcode {
                plan: program.plan(),
                path,
                bytes: text.len(),
                lines: program.lines().len(),
            })
        }
        TaskInput::LoadImage { path, fs } => {
            cancel.check().map_err(|_| TaskError::Cancelled)?;
            let bytes = fs.read(&path, MAX_IMAGE_BYTES).map_err(TaskError::Read)?;
            cancel.check().map_err(|_| TaskError::Cancelled)?;
            let source = decode(&bytes, DecodeLimits::default()).map_err(TaskError::Decode)?;
            Ok(TaskOutput::ImageLoaded { source: Arc::new(source), bytes: Arc::from(bytes), path })
        }
        TaskInput::DecodeImage { bytes } => {
            cancel.check().map_err(|_| TaskError::Cancelled)?;
            let source = decode(&bytes, DecodeLimits::default()).map_err(TaskError::Decode)?;
            Ok(TaskOutput::ImageLoaded { source: Arc::new(source), bytes, path: PathBuf::new() })
        }
        TaskInput::ParseImport { path, fs } => {
            cancel.check().map_err(|_| TaskError::Cancelled)?;
            let format = path.extension().and_then(|e| e.to_str()).and_then(ImportFormat::from_extension);
            let bytes = fs.read(&path, MAX_IMPORT_BYTES).map_err(TaskError::Read)?;
            cancel.check().map_err(|_| TaskError::Cancelled)?;
            let Some(format) = format else { return Err(TaskError::UnsupportedFormat) };
            let analysis = analyze(format, &bytes).map_err(|e| match e {
                AnalyzeError::Parse(e) => TaskError::Parse(e),
                _ => TaskError::UnsupportedFormat,
            })?;
            cancel.check().map_err(|_| TaskError::Cancelled)?;
            let parsed = match (format, analysis) {
                (ImportFormat::Hpgl, Analysis::Vector(d)) => ParsedImport::Hpgl(Box::new(d)),
                (_, Analysis::Vector(d)) => ParsedImport::Svg(Box::new(d)),
                (_, Analysis::Dxf(d)) => ParsedImport::Dxf(Box::new(d)),
                (_, Analysis::Pdf(doc)) => {
                    let first = doc.to_draft(PdfPages::One(0)).map_err(TaskError::Parse)?;
                    ParsedImport::Pdf(Box::new(PdfImport { doc: Arc::new(doc), first }))
                }
                (_, Analysis::Lbrn2(draft)) => {
                    let images = draft
                        .report
                        .assets
                        .iter()
                        .map(|a| {
                            let bitmap = a.data.as_ref()?;
                            decode(&bitmap.bytes, DecodeLimits::default()).ok().map(Arc::new)
                        })
                        .collect();
                    ParsedImport::Lbrn2(Box::new(Lbrn2Import { draft, images }))
                }
                _ => return Err(TaskError::UnsupportedFormat),
            };
            Ok(TaskOutput::ImportParsed { path, bytes: Arc::from(bytes), parsed })
        }
        TaskInput::Files(task) => crate::files_task::run(task, cancel).map(TaskOutput::Files),
        TaskInput::ImportPages { pdf, pages } => {
            cancel.check().map_err(|_| TaskError::Cancelled)?;
            let draft = pdf.to_draft(pages).map_err(TaskError::Parse)?;
            cancel.check().map_err(|_| TaskError::Cancelled)?;
            Ok(TaskOutput::ImportPagesDrafted { pages, draft: Box::new(draft) })
        }
        TaskInput::LoadBitmap { input } => crate::image_task::load_bitmap(input, cancel),
        TaskInput::ImagePreview(job) => crate::image_task::render_preview(&job, cancel),
        TaskInput::SaveBitmap { job, path, fs } => {
            let (bytes, width, height) = crate::image_task::processed_png(&job, cancel)?;
            cancel.check().map_err(|_| TaskError::Cancelled)?;
            write_atomically(fs.as_ref(), &path, &bytes).map_err(TaskError::Write)?;
            Ok(TaskOutput::BitmapSaved { path, width, height })
        }
        TaskInput::Centerline(request) => match burnburn_trace::trace_centerline(&request, cancel) {
            Ok(result) => Ok(TaskOutput::Centerlined(Arc::new(result))),
            Err(TraceError::Cancelled(_)) => Err(TaskError::Cancelled),
            Err(e) => Err(TaskError::Trace(e)),
        },
        TaskInput::PrepareJob { plan, ruida } => {
            cancel.check().map_err(|_| TaskError::Cancelled)?;
            let program = lower(&plan, &ruida).map_err(TaskError::Lower)?;
            let bytes = program.to_rd_file(ruida.magic).map_err(TaskError::Encode)?;
            Ok(TaskOutput::Prepared {
                plan: program.plan(),
                rd_hash: ContentHash::of(HashDomain::Rd, &bytes),
                rd: Arc::from(bytes),
            })
        }
        TaskInput::SaveRecovery { project, path, fs } => {
            cancel.check().map_err(|_| TaskError::Cancelled)?;
            save_project(fs.as_ref(), &path, &project).map_err(TaskError::Save)?;
            Ok(TaskOutput::RecoverySaved { path })
        }
        TaskInput::ListRecovery { dir, fs } => {
            cancel.check().map_err(|_| TaskError::Cancelled)?;
            if !fs.exists(&dir).map_err(TaskError::Read)? {
                return Ok(TaskOutput::RecoveryFound { files: Vec::new() });
            }
            let mut files: Vec<PathBuf> = fs
                .list_dir(&dir)
                .map_err(TaskError::Read)?
                .into_iter()
                .filter(|p| p.extension().is_some_and(|e| e == "burnburn"))
                .collect();
            files.sort();
            Ok(TaskOutput::RecoveryFound { files })
        }
        TaskInput::WriteText { text, path, fs } => {
            cancel.check().map_err(|_| TaskError::Cancelled)?;
            write_atomically(fs.as_ref(), &path, text.as_bytes()).map_err(TaskError::Write)?;
            Ok(TaskOutput::TextWritten { path })
        }
        TaskInput::CompileTest { job, profile, options } => {
            let plan = job.compile(&profile, &options, cancel).map_err(|e| match e {
                CompileError::Cancelled => TaskError::Cancelled,
                e => TaskError::Compile(e),
            })?;
            cancel.check().map_err(|_| TaskError::Cancelled)?;
            let preview = Arc::new(preview(&plan, &profile));
            let report = preflight(job.snapshot(), &plan, &profile);
            Ok(TaskOutput::Compiled { plan: Arc::new(plan), preview, report })
        }
        TaskInput::ExportTest { plan, ruida, path, fs } => {
            cancel.check().map_err(|_| TaskError::Cancelled)?;
            let program = lower(&plan, &ruida).map_err(TaskError::Lower)?;
            let bytes = program.to_rd_file(ruida.magic).map_err(TaskError::Encode)?;
            write_atomically(fs.as_ref(), &path, &bytes).map_err(TaskError::Write)?;
            Ok(TaskOutput::ExportedRd {
                plan: program.plan(),
                path,
                rd_hash: ContentHash::of(HashDomain::Rd, &bytes),
                bytes: bytes.len(),
            })
        }
        TaskInput::RemoveRecovery { path, fs } => {
            fs.remove_file(&path).map_err(TaskError::Write)?;
            Ok(TaskOutput::RecoveryRemoved { path })
        }
        TaskInput::Nest(job) => crate::nest_task::run_nest(&job, cancel).map(|o| TaskOutput::Nested(Arc::new(o))),
        TaskInput::Trace(request) => match trace(&request, cancel) {
            Ok(result) => Ok(TaskOutput::Traced(Arc::new(result))),
            Err(e) => Err(trace_error(e)),
        },
        TaskInput::TraceCorrected(job) => {
            let base = Arc::new(trace(&job.request, cancel).map_err(trace_error)?);
            let replay = replay_corrections(&base, &job.request.source, &job.corrections, job.request.task, cancel)
                .map_err(trace_error)?;
            Ok(TaskOutput::TracedCorrected(Arc::new(replay)))
        }
        TaskInput::CorrectRegion(request) => {
            retrace_region(&request, cancel).map(|r| TaskOutput::Corrected(Arc::new(r))).map_err(trace_error)
        }
    }
}

/// A trace error as a task error: a cancelled trace is a cancelled task.
fn trace_error(e: TraceError) -> TaskError {
    match e {
        TraceError::Cancelled(_) => TaskError::Cancelled,
        other => TaskError::Trace(other),
    }
}

/// Largest SVG/DXF file read for import (the parsers have their own limits too).
const MAX_IMPORT_BYTES: u64 = 64 * 1024 * 1024;

/// Largest image file the tracer reads (the decoder also limits pixels).
const MAX_IMAGE_BYTES: u64 = 256 * 1024 * 1024;

/// Temp file in the target directory, write, flush, atomic replace, directory sync (the IM04
/// pattern). A failure leaves any previous file at `path` intact and removes the temp file.
pub(crate) fn write_atomically(fs: &dyn FileSystem, path: &std::path::Path, bytes: &[u8]) -> Result<(), FsError> {
    let dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let stem = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let temp = fs.create_temp_in(dir, &format!(".{stem}"))?;
    let written = fs.write_all(&temp, bytes).and_then(|()| fs.sync_file(&temp)).and_then(|()| fs.replace(&temp, path));
    if let Err(e) = written {
        // Best effort: the temp file is ours; the target was not replaced.
        let _ = fs.remove_file(&temp);
        return Err(e);
    }
    fs.sync_dir(dir)
}

/// The offset of every target, as one path (preview only; the document command offsets per target).
fn offset_of(
    snapshot: &DocumentSnapshot,
    targets: &[GeometryId],
    distance: f64,
    join: OffsetJoin,
    cancel: &CancellationToken,
) -> Result<Path, TaskError> {
    let tolerance = burnburn_types::Length::from_mm(OFFSET_PREVIEW_TOLERANCE_MM)
        .ok()
        .and_then(|l| ProductionTolerance::new(l).ok())
        .ok_or(TaskError::Offset(OffsetError::NotFinite))?;
    let mut subpaths = Vec::new();
    for id in targets {
        let object = snapshot.object(*id).ok_or(TaskError::UnknownObject(*id))?;
        let grown = offset(object.path(), distance, join, &tolerance, cancel).map_err(|e| match e {
            OffsetError::Cancelled => TaskError::Cancelled,
            e => TaskError::Offset(e),
        })?;
        subpaths.extend(grown.subpaths().iter().cloned());
    }
    Ok(Path::from_subpaths(subpaths))
}

/// Closes the gaps of `targets` on a scratch copy and reports the repaired outlines, how many open
/// contours closed and how many of the targets' nearly-closed contours stay open.
fn repair_of(
    snapshot: &Arc<DocumentSnapshot>,
    targets: &[GeometryId],
    tolerance: burnburn_geometry::DiagnosticTolerance,
) -> Result<(Path, usize, usize), TaskError> {
    use burnburn_document::{
        CommandError, CommandPayload, DocumentCommand, DocumentSession, EditError, GeometryCommand,
    };
    let open_of = |s: &DocumentSnapshot| -> usize {
        targets
            .iter()
            .filter_map(|id| s.object(*id))
            .flat_map(|o| o.path().subpaths().iter())
            .filter(|sp| !sp.is_closed())
            .count()
    };
    let before = open_of(snapshot);
    let mut scratch = DocumentSession::from_snapshot(DocumentId::from_raw(0), (**snapshot).clone());
    let cmd = DocumentCommand {
        document: scratch.id(),
        expected: scratch.revision(),
        request: burnburn_types::RequestId::from_raw(0),
        payload: CommandPayload::Edit(GeometryCommand::CloseGaps { targets: targets.to_vec(), tolerance }),
    };
    match scratch.apply(cmd) {
        Ok(_) | Err(CommandError::Edit(EditError::NothingChanged)) => {}
        Err(e) => return Err(TaskError::Repair(e)),
    }
    let after = scratch.snapshot();
    let mut subpaths = Vec::new();
    for id in targets {
        if let Some(o) = after.object(*id) {
            subpaths.extend(o.path().subpaths().iter().cloned());
        }
    }
    let nearly = burnburn_editor::geometry_issues(&after)
        .iter()
        .filter(|i| i.kind == burnburn_editor::IssueKind::OpenContour && i.objects.iter().any(|o| targets.contains(o)))
        .count();
    Ok((Path::from_subpaths(subpaths), before.saturating_sub(open_of(&after)), nearly))
}

/// Chord tolerance of the offset preview: the same as the one applied from the panel.
const OFFSET_PREVIEW_TOLERANCE_MM: f64 = 0.01;

/// Folds the domain `boolean` over the operands: union of all; first minus the union of the rest;
/// intersection of all.
fn boolean_of(
    snapshot: &DocumentSnapshot,
    op: BooleanOp,
    operands: &[GeometryId],
    cancel: &CancellationToken,
) -> Result<Path, TaskError> {
    let options = crate::commit::boolean_options().ok_or(TaskError::Boolean(BooleanError::OutOfRange))?;
    let path =
        |id: &GeometryId| snapshot.object(*id).map(|o| o.path().as_ref().clone()).ok_or(TaskError::UnknownObject(*id));
    let (first, rest) = operands.split_first().ok_or(TaskError::Boolean(BooleanError::OutOfRange))?;
    let run = |a: &Path, b: &Path, op| {
        boolean(a, b, op, &options, cancel).map_err(|e| match e {
            BooleanError::Cancelled => TaskError::Cancelled,
            e => TaskError::Boolean(e),
        })
    };
    let mut acc = path(first)?;
    match op {
        BooleanOp::Difference => {
            let mut clip: Option<Path> = None;
            for id in rest {
                let p = path(id)?;
                clip = Some(match clip {
                    Some(c) => run(&c, &p, BooleanOp::Union)?,
                    None => p,
                });
            }
            if let Some(c) = clip {
                acc = run(&acc, &c, BooleanOp::Difference)?;
            }
        }
        op => {
            for id in rest {
                acc = run(&acc, &path(id)?, op)?;
            }
        }
    }
    Ok(acc)
}

pub(crate) fn revision_key(kind: TaskKind, document: DocumentId, source: RevisionId) -> ContentHash {
    let mut bytes = [0u8; 32];
    bytes[0] = match kind {
        TaskKind::Measure => 1,
        TaskKind::Save => 2,
        TaskKind::Open => 3,
        TaskKind::Boolean => 4,
        TaskKind::Compile => 5,
        TaskKind::ExportRd => 6,
        TaskKind::LoadImage => 7,
        TaskKind::Trace => 8,
        TaskKind::PrepareJob => 9,
        TaskKind::ParseImport => 10,
        TaskKind::Autosave => 11,
        TaskKind::Recovery => 12,
        TaskKind::LoadBitmap => 13,
        TaskKind::ImagePreview => 14,
        TaskKind::SaveBitmap => 15,
        TaskKind::Centerline => 16,
        TaskKind::TextExport => 17,
        TaskKind::TestJob => 18,
        TaskKind::Nest => 41,
        TaskKind::ImportPages => 42,
        TaskKind::Files => 43,
        TaskKind::OffsetPreview => 60,
        TaskKind::RepairPreview => 61,
        TaskKind::ExportGcode => 44,
    };
    bytes[8..16].copy_from_slice(&document.raw().to_le_bytes());
    bytes[16..24].copy_from_slice(&source.raw().to_le_bytes());
    ContentHash::from_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use burnburn_document::{GeometryObject, GeometrySource, ObjectKind};
    use burnburn_geometry::{Point, Rect};
    use burnburn_types::{CancellationSource, Length};

    fn snapshot() -> Arc<DocumentSnapshot> {
        let r = |raw: u64, x: f64, y: f64, w: f64, h: f64| {
            let rect = Rect::new(Point::new(x, y), Length::from_mm(w).unwrap(), Length::from_mm(h).unwrap()).unwrap();
            GeometryObject::new(
                GeometryId::from_raw(raw),
                "r".into(),
                ObjectKind::Rectangle,
                GeometrySource::Drawn,
                Path::rectangle(rect),
            )
        };
        Arc::new(
            DocumentSnapshot::from_parts(vec![r(1, 0.0, 0.0, 120.0, 80.0), r(2, 100.0, 50.0, 50.0, 50.0)], 3).unwrap(),
        )
    }

    #[test]
    fn measure_reports_count_and_union_bounds() {
        let out = run_task(TaskInput::Measure(snapshot()), &CancellationToken::never()).unwrap();
        let TaskOutput::Measured(m) = out else { panic!("expected a measure") };
        assert_eq!(m.objects, 2);
        let b = m.bounds.unwrap();
        assert_eq!((b.min(), b.max()), (Point::new(0.0, 0.0), Point::new(150.0, 100.0)));
    }

    #[test]
    fn cancelled_measure_returns_cancelled_not_partial_result() {
        let source = CancellationSource::new();
        source.cancel();
        assert_eq!(run_task(TaskInput::Measure(snapshot()), &source.token()).err(), Some(TaskError::Cancelled));
    }

    /// Two overlapping 120×80 and 50×50 rectangles: union area = 9600 + 2500 − 20·30.
    #[test]
    fn boolean_task_folds_the_domain_boolean_over_the_operands() {
        let ids = vec![GeometryId::from_raw(1), GeometryId::from_raw(2)];
        let run = |op| {
            run_task(
                TaskInput::Boolean { snapshot: snapshot(), op, operands: ids.clone() },
                &CancellationToken::never(),
            )
        };
        let Ok(TaskOutput::Boolean { result, .. }) = run(BooleanOp::Union) else { panic!("union") };
        assert!((result.signed_area().abs() - (9600.0 + 2500.0 - 600.0)).abs() < 1e-6, "{}", result.signed_area());
        let Ok(TaskOutput::Boolean { result, .. }) = run(BooleanOp::Difference) else { panic!("difference") };
        assert!((result.signed_area().abs() - (9600.0 - 600.0)).abs() < 1e-6);
        let Ok(TaskOutput::Boolean { result, .. }) = run(BooleanOp::Intersection) else { panic!("intersection") };
        assert!((result.signed_area().abs() - 600.0).abs() < 1e-6);
        let source = CancellationSource::new();
        source.cancel();
        let cancelled = run_task(
            TaskInput::Boolean { snapshot: snapshot(), op: BooleanOp::Union, operands: ids.clone() },
            &source.token(),
        );
        assert_eq!(cancelled.err(), Some(TaskError::Cancelled), "never a partial result");
    }
}
