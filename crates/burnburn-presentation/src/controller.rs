use crate::catalog::VariantMemory;
use crate::view_model::ToolMeasurement;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use burnburn_document::sketch::SketchCommand;
use burnburn_document::{CommandError, CommandOutcome, CommandPayload, DocumentCommand, DocumentSession};
use burnburn_editor::{
    Camera, Editor, GeometryIssue, Selection, SelectionChange, SnapSettings, Viewport, geometry_issues,
};
use burnburn_generators::{BoundTextEvaluator, GeneratorEvaluator};
use burnburn_geometry::Point;
use burnburn_io::fs::FileSystem;
use burnburn_io::{AssetStore, ProjectUuid};
use burnburn_job::JobPlan;
use burnburn_machine::ExecutionId;
use burnburn_sketch::SketchEvaluator;
use burnburn_sketch::solver::SketchSolverAdapter;
use burnburn_types::{CancellationSource, DocumentId, IdAllocator, RequestId, RevisionId, StableId, TaskId};

use crate::commit::NameKind;
use burnburn_text::FontRef;

use crate::camera::{CameraDeviceId, CameraMode, CameraPort};
use crate::intent::{CloseChoice, CommandKey, DialogKind, FieldKey, SelectMode, UiIntent};
use crate::production::{ActionSubject, HistoryStore, MachinePort, OrderId, Role};
use crate::task::{TaskKind, TaskPort, TaskRequest};
use crate::view_model::{
    BooleanPreview, ExportViewModel, MeasureViewModel, MessageKey, ProblemViewModel, SelectionCycle, StatusMessage,
};

mod app_panels;
mod arrange;
mod array_dialog;
mod barcode;
mod camera;
mod camera_marks;
mod camera_wizards;
mod canvas;
mod clipboard;
mod commands;
mod cut_options;
mod device;
mod document;
mod feeder_dialog;
mod fields;
mod generator;
mod geometry;
mod grbl;
mod history_panel;
mod image;
mod import;
mod kerf_view;
mod laser;
mod layers;
mod library;
mod machine;
mod machine_hub;
mod machine_rest;
mod nest;
mod notes;
mod offset_preview;
mod order_dialog;
mod path_lock;
mod plate_nest;
mod preflight_dialog;
mod prefs;
mod preview;
mod process;
mod recent;
mod recognition;
mod recovery;
mod registration;
mod remnants;
mod repair_preview;
mod reshape;
mod shapes;
mod shortcuts;
mod sketch;
mod sketch_param;
mod structure;

pub(crate) use sketch_param::normalise_decimal_commas;
mod start_confirm;
mod sublayers_dialog;
mod support;
mod tasks;
mod tests_gen;
mod text;
mod trace;
mod trace_g7;
mod trace_link;
mod trace_overlay;
mod trace_prep;
mod trace_recipes;
mod variable;
mod view;

pub use app_panels::{AppIntent, NudgeDirection, NudgeStep};
pub use camera::CameraSpawner;
pub use history_panel::{HistoryRow, HistoryViewModel};
pub use machine_rest::MachineActivity;
pub use prefs::{
    DisplayQuality, LanguageChoice, LengthUnit, PrefError, PrefKey, PrefProblem, PrefRow, PrefUnit, PrefValue,
    Preferences, PrefsViewModel,
};
pub use preview::{
    PartialMove, PlaybackSpeed, PreviewIntent, PreviewSelection, PreviewViewModel, RdInspectViewModel, RdRecordRow,
};
pub use shortcuts::{KeyBinding, ShortcutError, ShortcutProblem, ShortcutRow, ShortcutTarget, ShortcutsViewModel};
pub use support::{DEBUG_LOG_LINES, DebugLogViewModel, HelpInfo};

use geometry::{offset_selection, repair_issue};
use import::PendingImport;
use machine::{Declared, Machine, Order, Prepared};
use process::{Compiled, add_operation_text, set_operation_field};
use text::SharedFonts;
use trace::Tracer;

/// Default zoom for a new window: 2 logical points per millimetre.
const DEFAULT_POINTS_PER_MM: f64 = 2.0;
/// Line interval of a fill added from the operations panel (mm).
const DEFAULT_FILL_INTERVAL_MM: f64 = 0.1;

/// Em size of text created by text input (mm).
const DEFAULT_TEXT_MM: f64 = 10.0;

/// Grid spacing used when grid snapping is switched on.
const DEFAULT_SNAP_GRID_MM: f64 = 1.0;
/// Chord tolerance for production flattening when compiling (ProductionTolerance, not display).
const COMPILE_TOLERANCE_MM: f64 = 0.01;

/// Injected boundaries. The clock joins when the first flow needs it.
#[derive(Clone)]
pub struct AppPorts {
    pub tasks: Arc<dyn TaskPort>,
    /// Used only inside save/open tasks, never on the UI thread.
    pub fs: Arc<dyn FileSystem>,
    /// Identity for a new project file (random in production, a counter in tests).
    pub new_project_uuid: Arc<dyn Fn() -> ProjectUuid + Send + Sync>,
}

/// What a host must do natively after an intent or task outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum HostEffect {
    OpenWindow {
        document: DocumentId,
    },
    /// Recovery files found at start: ask the user to restore or discard each.
    OfferRecovery {
        files: Vec<PathBuf>,
    },
    /// NSSavePanel as sheet / IFileSaveDialog.
    ShowSavePanel {
        document: DocumentId,
        suggested_name: String,
    },
    /// Native "Save changes?" sheet / TaskDialog.
    ConfirmClose {
        document: DocumentId,
    },
    ShowError {
        document: Option<DocumentId>,
        message: MessageKey,
    },
    CloseWindow {
        document: DocumentId,
    },
    Redraw {
        document: DocumentId,
    },
    /// Save panel for an RD export (never the project file).
    ShowExportPanel {
        document: DocumentId,
        suggested_name: String,
    },
    /// The GRBL macros changed: the host stores this text form (`GrblIntent::LoadMacros` reads it).
    GrblMacrosChanged {
        text: String,
    },
    /// Save panel for a G-code export (never the project file). The host answers with
    /// `UiIntent::ExportGcode`.
    ShowGcodeExportPanel {
        document: DocumentId,
        suggested_name: String,
    },
    /// Start editing a sketch dimension's text in the constraint list (double-click on its label,
    /// or right after "Maat" added it). The host answers with `SetSketchDimension`.
    EditSketchDimension {
        document: DocumentId,
        sketch: burnburn_document::sketch::SketchId,
        constraint: burnburn_document::sketch::ConstraintId,
        text: String,
    },
    /// Put these bytes on the OS pasteboard under BurnBurn's own type (Copy and Cut). The host
    /// answers a later Paste with `UiIntent::PasteData` when the pasteboard holds that type.
    SetClipboard {
        bytes: Vec<u8>,
    },
    /// The recent-projects list changed: the host stores it and rebuilds Archief ▸ Recente projecten.
    RecentProjectsChanged {
        paths: Vec<PathBuf>,
    },
    /// Save panel for a template (`.burnburntemplate`, never the project file); the host also asks
    /// what the template holds and answers with `FilesIntent::SaveAsTemplate`.
    ShowTemplateSavePanel {
        document: DocumentId,
        suggested_name: String,
    },
    /// Open panel for a template; the host answers with `FilesIntent::NewFromTemplate`.
    ShowTemplateOpenPanel,
    /// Open panel for a machine profile file (`.json`); the host answers with
    /// `UiIntent::OpenMachineProfile`.
    ShowMachineProfileOpenPanel,
    /// Open panel for a bundle file; the host answers with `FilesIntent::OpenBundleImport`.
    ShowBundleOpenPanel {
        document: DocumentId,
    },
    /// The recent-templates list changed: the host stores it and rebuilds the menu.
    RecentTemplatesChanged {
        paths: Vec<PathBuf>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppUpdate {
    pub effects: Vec<HostEffect>,
    pub view_changed: bool,
    /// Windows whose `AppController::scene` changed and must be redrawn.
    pub scenes_changed: Vec<DocumentId>,
}

impl AppUpdate {
    fn redraw(document: DocumentId) -> Self {
        AppUpdate { effects: Vec::new(), view_changed: true, scenes_changed: vec![document] }
    }
}

struct Session {
    doc: DocumentSession,
    /// Palette colours and canvas visibility of the operations (window state, see `layers`).
    layers: layers::LayerState,
    /// The plate Compile uses, and the plate each compile in flight was started for (P06).
    compile_plate: Option<burnburn_document::structure::PlateId>,
    compile_plates: BTreeMap<TaskId, Option<burnburn_document::structure::PlateId>>,
    /// The job clock when each compile in flight was started (the plan shows it).
    compile_clocks: BTreeMap<TaskId, Option<burnburn_generators::JobClock>>,
    /// When the plan in `compiled` was compiled.
    plan_compiled_at: Option<burnburn_generators::JobClock>,
    /// The plate the plan in `compiled` was made for.
    plan_plate: Option<burnburn_document::structure::PlateId>,
    /// The open repair preview (E09; window state).
    repair: repair_preview::RepairState,
    /// Offset options and the open preview (E05; window state).
    offset: offset_preview::OffsetState,
    /// The project notes panel is open (P04; window state).
    notes_open: bool,
    /// The explicit part name of the project (W01/W02, G9) and the one last saved: production
    /// bookkeeping saved with the project, no undo step.
    part_name: Option<String>,
    saved_part_name: Option<String>,
    /// The part name the save in flight carries.
    saving_part_name: Option<Option<String>>,
    /// Text evaluator input (from `AppController::attach_fonts`).
    fonts: Option<SharedFonts>,
    /// Why the last operations-panel input sent no command.
    operation_error: Option<MessageKey>,
    /// Revision held by this document's recovery file, and that file.
    autosaved: Option<RevisionId>,
    recovery_path: Option<PathBuf>,
    editor: Editor,
    /// Window-local; never shared between sessions.
    selection: Selection,
    camera: Camera,
    viewport: Viewport,
    /// Whether the camera still has to frame the bed once the real canvas size is known (opens and
    /// new documents); any user pan/zoom or explicit zoom command clears it.
    fit_pending: bool,
    title: String,
    path: Option<PathBuf>,
    field_errors: BTreeMap<FieldKey, MessageKey>,
    messages: Vec<StatusMessage>,
    measure: Option<MeasureViewModel>,
    tool_measurement: Option<ToolMeasurement>,
    /// Where the Text tool was clicked: the next committed text is placed there.
    text_anchor: Option<Point>,
    pending: BTreeMap<TaskId, (TaskKind, CancellationSource)>,
    /// Progress (per mille) the running tasks reported; dropped with the task.
    task_progress: BTreeMap<TaskId, u16>,
    /// Default-name counters per created kind ("Rechthoek 2", "Cirkel 1").
    created: BTreeMap<NameKind, u64>,
    snap: SnapSettings,
    /// The last canvas pointer event: Tab replays it to show the next snap candidate (E04).
    last_pointer: Option<burnburn_editor::PointerEvent>,
    cycle: Option<SelectionCycle>,
    tool_size_error: Option<MessageKey>,
    /// Problems of the current revision (recomputed on every revision change).
    issues: Vec<GeometryIssue>,
    boolean_preview: Option<BooleanPreview>,
    /// Machine artefacts: never document data, never part of undo (AGENTS.md).
    compiled: Option<Compiled>,
    export: Option<ExportViewModel>,
    gcode_export: Option<crate::GcodeExportViewModel>,
    tracer: Option<Tracer>,
    /// The plan a PrepareJob task is lowering, and the frozen job of this document (C1).
    preparing: Option<Arc<JobPlan>>,
    prepared: Option<Prepared>,
    /// The open import preview (P03).
    import: Option<PendingImport>,
    /// The last refused, located edit (offset, trim, repair).
    problem: Option<ProblemViewModel>,
    /// Trim mode (E05): canvas clicks trim instead of select.
    trim_mode: bool,
    /// File identity and the stored assets this session does not edit yet (kept on re-save, P01).
    uuid: ProjectUuid,
    assets: AssetStore,
    close_after_save: bool,
    /// Where the previous file was kept by the last save over an older or unreadable one (X07).
    previous_version: Option<PathBuf>,
    /// Sketch mode: the open sketch and its selection (window state, never document data).
    sketch: sketch::SketchState,
    /// The geometry dialog (join, round corners, optimize) while it is open.
    geometry_dialog: Option<shapes::GeometryDialog>,
    /// Options of the pattern and mirror tools.
    options: shapes::ToolOptions,
    /// Bitmap import sheet, Adjust Image panel and decoded sources (window state).
    image: image::ImageState,
    /// Tracers being re-opened from a stored trace (window state).
    trace_link: trace_link::TraceLinkState,
    /// Inspector reference point, aspect lock and wireframe view (window state).
    arrange: arrange::ArrangeState,
    /// The open CAM dialogs of this window: Optimization Settings and the test generators.
    cam: process::CamState,
    /// The nest panel (Quick Nest, Nest Selected) and its result (window state).
    nest: nest::NestState,
    /// A distribute-and-nest run over the plates that awaits its answer (G9-editor).
    plate_nest: Option<plate_nest::PlateRun>,
    /// The Print and Cut and Center Finder panels (window state).
    registration: registration::RegState,
    /// Text toolbar refusals and the text-on-path dialog (window state).
    text: text::TextState,
    /// Serial counters, data rows and the record the last compile was bound to (window state).
    variable: variable::VariableState,
    /// The barcode dialog and the barcodes this window created (window state).
    barcode: barcode::BarcodeState,
    /// The generator dialog (box, living hinge) while it is open (window state).
    generator: generator::GeneratorState,
    /// Which library entry each recipe was copied from (provenance, saved with the project).
    library_links: Vec<burnburn_io::LibraryLink>,
    /// Window state of the file panels (Material Library, Art Library, bundles, export).
    files: library::FilesUi,
}

impl Session {
    fn new(
        doc: DocumentSession,
        title: String,
        path: Option<PathBuf>,
        project: (ProjectUuid, AssetStore),
    ) -> Option<Self> {
        let camera = Camera::new(Point::new(0.0, 0.0), DEFAULT_POINTS_PER_MM).ok()?;
        let viewport = Viewport::new(800.0, 600.0, 1.0).ok()?;
        let issues = geometry_issues(&doc.snapshot());
        Some(Session {
            doc,
            layers: layers::LayerState::default(),
            notes_open: false,
            compile_plate: None,
            compile_plates: BTreeMap::new(),
            compile_clocks: BTreeMap::new(),
            plan_compiled_at: None,
            plan_plate: None,
            repair: repair_preview::RepairState::default(),
            offset: offset_preview::OffsetState::default(),
            editor: Editor::new(),
            selection: Selection::default(),
            camera,
            viewport,
            fit_pending: true,
            title,
            path,
            field_errors: BTreeMap::new(),
            messages: Vec::new(),
            measure: None,
            tool_measurement: None,
            text_anchor: None,
            pending: BTreeMap::new(),
            task_progress: BTreeMap::new(),
            created: BTreeMap::new(),
            snap: SnapSettings::default(),
            last_pointer: None,
            cycle: None,
            tool_size_error: None,
            issues,
            boolean_preview: None,
            compiled: None,
            export: None,
            gcode_export: None,
            tracer: None,
            preparing: None,
            prepared: None,
            problem: None,
            fonts: None,
            operation_error: None,
            autosaved: None,
            recovery_path: None,
            trim_mode: false,
            import: None,
            uuid: project.0,
            assets: project.1,
            close_after_save: false,
            previous_version: None,
            sketch: sketch::SketchState::default(),
            geometry_dialog: None,
            options: shapes::ToolOptions::default(),
            image: image::ImageState::default(),
            trace_link: trace_link::TraceLinkState::default(),
            arrange: arrange::ArrangeState::default(),
            cam: process::CamState::default(),
            nest: nest::NestState::default(),
            plate_nest: None,
            registration: registration::RegState::default(),
            text: text::TextState::default(),
            variable: variable::VariableState::default(),
            barcode: barcode::BarcodeState::default(),
            generator: generator::GeneratorState::default(),
            library_links: Vec::new(),
            part_name: None,
            saved_part_name: None,
            saving_part_name: None,
            files: library::FilesUi::default(),
        })
    }

    /// Cancels work that only describes the old revision (best effort; revision checks decide).
    fn cancel_revision_sensitive(&mut self) {
        for (kind, source) in self.pending.values() {
            if kind.is_revision_sensitive() {
                source.cancel();
            }
        }
    }

    fn cancel_all(&mut self) {
        for (_, source) in self.pending.values() {
            source.cancel();
        }
    }

    fn next_name(&self, kind: NameKind) -> String {
        format!("{} {}", crate::commit::base_name(kind), self.created.get(&kind).copied().unwrap_or(0) + 1)
    }
}

fn file_title(path: &std::path::Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string())
}

/// One controller per app process. Driven sequentially on the UI thread by the native host, the
/// CLI or the headless harness — all through the same `handle` / `task_finished` entry points.
pub struct AppController {
    ports: AppPorts,
    ids: IdAllocator,
    sessions: BTreeMap<DocumentId, Session>,
    active: Option<DocumentId>,
    untitled: u64,
    /// Open tasks whose session does not exist yet: task → (reserved document id, path).
    opening: BTreeMap<TaskId, (DocumentId, CancellationSource)>,
    machine: Option<Machine>,
    /// The machine service (one owner of the connection, shared with other windows/views).
    machine_port: Option<Arc<dyn MachinePort>>,
    /// A declared model/firmware was handed to the machine port (so clearing it must reach the port too).
    declared_pushed: bool,
    /// The start the user asked for and has not confirmed yet (M06).
    pending_start: Option<start_confirm::PendingStart>,
    /// Template recognition in the camera picture (W06).
    recognition: recognition::RecognitionState,
    /// Plans the designer released (W01), as the history keeps them: a prepared job whose plan is
    /// in here is approved. A changed plan has another id, so it is not.
    approved_plans: std::collections::BTreeSet<burnburn_types::ContentHash>,
    /// The released jobs the history holds, rebuilt from their frozen blobs (no window of their own).
    released_jobs: Vec<Arc<crate::production::PreparedJob>>,
    /// The names of the parts in the project of each released job, by plan hash: a job of the
    /// history has no project in memory, an order still finds it by the name of a part (W01/W02).
    job_parts: std::collections::BTreeMap<burnburn_types::ContentHash, Vec<String>>,
    /// Machine hub state: a repeat in preparation and the calibration measurements (G5).
    hub: machine_hub::HubState,
    role: Role,
    orders: Vec<Order>,
    /// Which order an upload was made for: only these executions count for that order.
    order_executions: BTreeMap<ExecutionId, OrderId>,
    /// Durable orders (IM31); without it orders live only in this controller.
    history: Option<Arc<dyn HistoryStore>>,
    /// Fonts for text input and the face new text uses.
    fonts: Option<(SharedFonts, FontRef)>,
    /// Bed camera port and the device/mode it was started with (IM27).
    camera: Option<Arc<dyn CameraPort>>,
    camera_active: Option<(CameraDeviceId, CameraMode)>,
    /// Camera picture on the bed, calibrations and their wizards (W3-UI-camera).
    camera_ws: camera::Workspace,
    /// Where autosave writes recovery files.
    recovery_dir: Option<PathBuf>,
    /// Tasks without a window: recovery list/remove (task → kind).
    app_tasks: BTreeMap<TaskId, TaskKind>,
    /// Open tasks that restore a recovery file (the window opens unsaved and untitled).
    restoring: BTreeMap<TaskId, PathBuf>,
    /// The last chosen variant of each toolbar group (window-independent preference).
    toolbar_variants: VariantMemory,
    /// The laser window's setup (window state, never document data).
    laser: laser::LaserState,
    /// The last Copy or Cut: all windows paste from it; the host mirrors it to the OS pasteboard.
    clipboard: Option<burnburn_io::clipboard::Clipboard>,
    /// The job clock for `{date}` in variable text (injected; none: dates cannot be resolved).
    clock: Option<Arc<dyn Fn() -> Option<burnburn_generators::JobClock> + Send + Sync>>,
    /// Archief ▸ Recente projecten, most recent first; the host persists it.
    recent: Vec<PathBuf>,
    /// Archief ▸ Recente templates, most recent first; the host persists it.
    recent_templates: Vec<PathBuf>,
    /// Worker jobs of the file features that wait for their result.
    files_jobs: BTreeMap<TaskId, library::FilesJob>,
    /// Material and art libraries (shared by all windows).
    libs: library::Libraries,
    /// Counts openings of the bundle dialog (a worker result for an older one is dropped).
    bundle_serial: u64,
    /// The path each pending open task was asked for (to forget it when it fails).
    opening_paths: BTreeMap<TaskId, PathBuf>,
    /// Device Settings: the draft of the machine profile dialog (app-wide, like the profile).
    device: device::DeviceState,
    /// The GRBL window: driver over the host's serial port, console and macros (G5-grbl-ui).
    grbl_panel: grbl::GrblPanel,
    /// Preferences, shortcuts, history, preview and support panels (W3-UI-app).
    app: app_panels::AppPanels,
}

impl AppController {
    /// Where the last save of `document` kept the previous file (an older or unreadable one), if it did.
    pub fn previous_version(&self, document: DocumentId) -> Option<PathBuf> {
        self.sessions.get(&document)?.previous_version.clone()
    }

    pub fn new(ports: AppPorts) -> Self {
        AppController {
            ports,
            ids: IdAllocator::new(),
            sessions: BTreeMap::new(),
            active: None,
            untitled: 0,
            opening: BTreeMap::new(),
            machine: None,
            machine_port: None,
            declared_pushed: false,
            pending_start: None,
            recognition: recognition::RecognitionState::default(),
            approved_plans: std::collections::BTreeSet::new(),
            released_jobs: Vec::new(),
            job_parts: std::collections::BTreeMap::new(),
            hub: machine_hub::HubState::default(),
            role: Role::Designer,
            orders: Vec::new(),
            order_executions: BTreeMap::new(),
            history: None,
            fonts: None,
            camera: None,
            camera_active: None,
            camera_ws: camera::Workspace::default(),
            recovery_dir: None,
            app_tasks: BTreeMap::new(),
            restoring: BTreeMap::new(),
            toolbar_variants: VariantMemory::new(),
            laser: laser::LaserState::default(),
            clipboard: None,
            recent: Vec::new(),
            clock: None,
            recent_templates: Vec::new(),
            files_jobs: BTreeMap::new(),
            libs: library::Libraries::default(),
            bundle_serial: 0,
            opening_paths: BTreeMap::new(),
            device: device::DeviceState::default(),
            grbl_panel: grbl::GrblPanel::default(),
            app: app_panels::AppPanels::default(),
        }
    }

    pub fn handle(&mut self, intent: UiIntent) -> AppUpdate {
        self.app.debug_log.record(&intent);
        if self.role == Role::Operator && !operator_may(&intent) {
            return error(intent_document(&intent), MessageKey::NotAllowedForRole);
        }
        // Job events the machine reported since the last intent reach the counters first.
        let production = self.sync_production();
        // The document keeps the selection in its undo records (P02): tell it before and after.
        self.sync_selections();
        let mut update = self.dispatch(intent);
        self.sync_selections();
        // The machine in use is kept for the next start whenever it changed (M01).
        self.persist_active_machine();
        update.view_changed |= production.view_changed;
        update
    }

    fn sync_selections(&mut self) {
        for s in self.sessions.values_mut() {
            s.doc.set_selection(s.selection.ids());
        }
    }

    fn dispatch(&mut self, intent: UiIntent) -> AppUpdate {
        match intent {
            UiIntent::NewDocument => self.new_document(),
            UiIntent::Open { path } => self.open(path),
            UiIntent::Focus { document } => {
                if self.sessions.contains_key(&document) && self.active != Some(document) {
                    self.active = Some(document);
                    AppUpdate { view_changed: true, ..AppUpdate::default() }
                } else {
                    AppUpdate::default()
                }
            }
            UiIntent::Save { document, path } => self.save(document, path),
            UiIntent::Close { document } => self.close(document),
            UiIntent::ConfirmClose { document, choice } => match choice {
                CloseChoice::Discard => self.close_session(document),
                CloseChoice::Cancel => AppUpdate::default(),
                CloseChoice::Save => {
                    if let Some(s) = self.sessions.get_mut(&document) {
                        s.close_after_save = true;
                    }
                    self.save(document, None)
                }
            },
            UiIntent::DialogCancelled { document, dialog } => {
                // Cancelling a save panel or close sheet changes nothing (A5/A7); a pending
                // "save, then close" is abandoned with the panel.
                if dialog == DialogKind::SavePanel
                    && let Some(s) = self.sessions.get_mut(&document)
                {
                    s.close_after_save = false;
                }
                AppUpdate::default()
            }
            UiIntent::SetNotes { document, text } => {
                let request = self.next_id::<RequestId>();
                self.with_session(document, |s| notes::set(s, document, text, request))
            }
            UiIntent::SetPartName { document, name } => {
                self.with_session(document, |s| notes::set_part_name(s, document, &name))
            }
            UiIntent::TracePrep { document, intent } => self.trace_prep_intent(document, intent),
            UiIntent::TracePick { document, pick } => self.trace_pick(document, pick),
            UiIntent::TraceRecipe { document, intent } => self.trace_recipe_intent(document, intent),
            UiIntent::CenterlineEdit { document, edit } => {
                self.with_session(document, |s| trace::centerline_edit(s, document, edit))
            }
            UiIntent::ChoosePlate { document, plate } => {
                self.with_session(document, |s| structure::choose_plate(s, document, plate))
            }
            UiIntent::Structure { document, edit: crate::StructureEdit::DistributeAndNestOverPlates { gap } } => {
                self.distribute_and_nest(document, gap)
            }
            UiIntent::Structure { document, edit } => {
                let request = self.next_id::<RequestId>();
                self.with_session(document, |s| structure::edit(s, document, edit, request))
            }
            UiIntent::PreviewRepair { document, issue, tolerance } => self.preview_repair(document, issue, tolerance),
            UiIntent::PreviewOffset { document, distance } => self.preview_offset(document, distance),
            UiIntent::SetOffsetOptions { document, options } => self.set_offset_options(document, options),
            UiIntent::CloseNotes { document } => self.with_session(document, |s| notes::close(s, document)),
            UiIntent::SelectByKind { document, kind, mode } => {
                self.with_session(document, |s| arrange::select_by_kind(s, document, kind, mode))
            }
            UiIntent::Select { document, ids, mode } => {
                let change = match mode {
                    SelectMode::Replace => SelectionChange::Replace(ids),
                    SelectMode::Toggle => match ids.as_slice() {
                        [one] => SelectionChange::Toggle(*one),
                        _ => return AppUpdate::default(),
                    },
                };
                self.with_session(document, |s| {
                    apply_selection(s, &change);
                    s.cycle = None;
                    AppUpdate::redraw(document)
                })
            }
            UiIntent::CommitField { document, field, text } => self.commit_field(document, field, &text),
            UiIntent::CommitToolSize { document, width, height } => self.commit_tool_size(document, &width, &height),
            UiIntent::SelectTool { document, id } => self.select_catalog_tool(document, &id, None),
            UiIntent::SelectVariant { document, group, id } => self.select_catalog_tool(document, &id, Some(&group)),
            UiIntent::Invoke { document, command } => self.invoke(document, command),
            UiIntent::Pointer { document, event } => self.pointer(document, event),
            UiIntent::Key { document, key, modifiers } => self.key(document, key, modifiers),
            UiIntent::Camera { document, intent } => self.camera(document, intent),
            UiIntent::SetMachineProfile { profile, ruida } => {
                // Profile data, not document data: existing plans become stale, nothing else changes.
                self.replace_active_machine(Machine {
                    profile: Arc::from(profile),
                    ruida,
                    grbl: None,
                    declared: Declared::default(),
                });
                AppUpdate { view_changed: true, ..AppUpdate::default() }
            }
            UiIntent::SetGrblMachineProfile { profile, grbl } => {
                // Like `SetMachineProfile`: profile data only. The Ruida part is a neutral
                // placeholder that the GRBL backend never reads.
                let ruida = burnburn_ruida::RuidaProfile {
                    magic: burnburn_ruida::Magic::FIXTURE_0X88,
                    overscan: burnburn_ruida::OverscanPolicy::Reject,
                };
                self.replace_active_machine(Machine {
                    profile: Arc::from(profile),
                    ruida,
                    grbl: Some(*grbl),
                    declared: Declared::default(),
                });
                AppUpdate { view_changed: true, ..AppUpdate::default() }
            }
            UiIntent::ChooseMachineProfile { id } => self.choose_machine_profile(&id),
            UiIntent::OpenMachineProfile { path } => self.open_machine_profile(path),
            UiIntent::RepeatExecution { execution } => self.hub_repeat(execution),
            UiIntent::ConfirmRepeat { document } => self.hub_confirm_repeat(document),
            UiIntent::CancelRepeat => self.hub_cancel_repeat(),
            UiIntent::RequestStart { document } => self.request_start(document),
            UiIntent::ConfirmStart { document, action, seen } => self.confirm_start(document, action, seen),
            UiIntent::CancelStart => self.cancel_start(),
            UiIntent::Recognition(intent) => self.recognition_intent(intent),
            UiIntent::Remnant(intent) => self.remnant_intent(intent),
            UiIntent::SetOrderRemnant { order, remnant } => self.set_order_remnant(order, remnant),
            UiIntent::RecordCalibration { input } => self.hub_record_calibration(input),
            UiIntent::RemoveCalibration { id } => self.hub_remove_calibration(id),
            UiIntent::AddOperation { document, draft } => self.add_operation(document, draft),
            UiIntent::ExportRd { document, path } => self.export_rd(document, path),
            UiIntent::ExportGcode { document, path } => self.export_gcode(document, path),
            UiIntent::Grbl(intent) => self.grbl_intent(intent),
            UiIntent::OpenTraceSource { document, path } => self.open_trace_source(document, path),
            UiIntent::SetTraceRecipe { document, recipe } => self.set_trace_recipe(document, *recipe),
            UiIntent::Machine { document, action, seen } => self.machine_action(document, action, seen, None).0,
            UiIntent::Laser { document, action, seen, command } => self.laser_call(document, action, seen, command),
            UiIntent::LaserSetup { setup } => self.laser_setup(setup),
            UiIntent::LaserReleaseJog => self.laser_release_jog(),
            UiIntent::ExportDiagnostics { options, path } => self.export_diagnostics(options, path),
            UiIntent::AddSketchConstraint { document, sketch, constraint } => {
                self.sketch_edit(document, SketchCommand::AddConstraint { sketch, constraint })
            }
            UiIntent::RemoveSketchConstraint { document, sketch, constraint } => {
                self.sketch_edit(document, SketchCommand::RemoveConstraint { sketch, constraint })
            }
            UiIntent::SetSketchDimension { document, sketch, constraint, text } => {
                let text = sketch_param::normalise_decimal_commas(&text);
                self.sketch_edit(document, SketchCommand::SetDimensionValue { sketch, constraint, text })
            }
            UiIntent::Import { document, path } => self.parse_import(document, path),
            UiIntent::SetImageDpi { document, dpi } => self.set_image_dpi(document, &dpi),
            UiIntent::SetImageAdjust { document, input } => self.set_image_adjust(document, *input),
            UiIntent::ReplaceImage { document, path } => self.replace_image(document, path),
            UiIntent::ReplaceImageToFit { document, path } => self.replace_image_to_fit(document, path),
            UiIntent::SaveProcessedBitmap { document, path } => self.save_processed_bitmap(document, path),
            UiIntent::SetTraceForm { document, input } => self.set_trace_form(document, *input),
            UiIntent::ApplyTracePreset { document, preset } => self.apply_trace_preset(document, preset),
            UiIntent::SetImportUnit { document, unit } => self.set_import_unit(document, unit),
            UiIntent::Files(intent) => self.files(intent),
            UiIntent::Offset { document, distance } => {
                let request = self.next_id::<RequestId>();
                self.with_session(document, |s| offset_selection(s, document, &distance, request))
            }
            UiIntent::RepairIssue { document, issue, tolerance } => {
                let request = self.next_id::<RequestId>();
                self.with_session(document, |s| repair_issue(s, document, issue, &tolerance, request))
            }
            UiIntent::AddOrder { order } => {
                let id = OrderId(self.orders.last().map_or(1, |o| o.id.0 + 1));
                let order = Order { id, draft: order, chosen: None };
                if let Err(e) = self.store_order(&order) {
                    return e;
                }
                self.orders.push(order);
                AppUpdate { view_changed: true, ..AppUpdate::default() }
            }
            UiIntent::ChooseOrderJob { order, plan } => {
                // Only one of the candidates can be chosen; nothing is picked automatically.
                let Some(o) = self.orders.iter().find(|o| o.id == order) else { return AppUpdate::default() };
                if !self.candidates(&o.draft.part).iter().any(|p| p.job.plan == plan) {
                    return error(None, MessageKey::MachineActionRefused);
                }
                let Some(mut changed) = self.orders.iter().find(|o| o.id == order).cloned() else {
                    return AppUpdate::default();
                };
                changed.chosen = Some(plan.hash());
                if let Err(e) = self.store_order(&changed) {
                    return e;
                }
                if let Some(o) = self.orders.iter_mut().find(|o| o.id == order) {
                    *o = changed;
                }
                AppUpdate { view_changed: true, ..AppUpdate::default() }
            }
            UiIntent::InsertText { document, text } => self.insert_text(document, text),
            UiIntent::AddOperationText { document, name, kind, speed, power } => {
                let request = self.next_id::<RequestId>();
                self.with_session(document, |s| add_operation_text(s, document, name, kind, &speed, &power, request))
            }
            UiIntent::SetOperationField { document, operation, field, text } => {
                let request = self.next_id::<RequestId>();
                self.with_session(document, |s| set_operation_field(s, document, operation, field, &text, request))
            }
            UiIntent::AssignLayer { document, slot, add } => {
                let ids = &mut self.ids;
                match self.sessions.get_mut(&document) {
                    Some(s) => layers::assign_layer(s, document, slot, add, &mut || ids.allocate().ok()),
                    None => AppUpdate::default(),
                }
            }
            UiIntent::SetOperationVisible { document, operation, visible } => {
                self.with_session(document, |s| layers::set_visible(s, document, operation, visible))
            }
            UiIntent::SetOperationOutput { document, operation, enabled } => {
                let request = self.next_id::<RequestId>();
                self.with_session(document, |s| layers::set_output(s, document, operation, enabled, request))
            }
            UiIntent::OpenCutSettings { document, operation } => {
                self.with_session(document, |s| layers::open_cut_settings(s, operation))
            }
            UiIntent::ApplyCutSettings { document, operation, settings } => {
                // Mode settings and line options are two document commands: two request ids.
                let requests = [self.next_id::<RequestId>(), self.next_id::<RequestId>()];
                let mut requests = requests.into_iter();
                let capabilities = self.machine.as_ref().map(|m| m.profile.capabilities().clone());
                self.with_session(document, |s| {
                    let mut next = || requests.next().flatten();
                    layers::apply_cut_settings(s, document, operation, &settings, capabilities.as_ref(), &mut next)
                })
            }
            UiIntent::CancelCutSettings { document } => self.with_session(document, layers::cancel_cut_settings),
            UiIntent::BedCamera(intent) => self.bed_camera(intent),
            UiIntent::CameraWorkspace(intent) => self.camera_workspace(intent),
            UiIntent::Autosave => self.autosave(),
            UiIntent::CheckRecovery => self.check_recovery(),
            UiIntent::RestoreRecovery { path } => self.restore_recovery(path),
            UiIntent::DiscardRecovery { path } => self.discard_recovery(path),
            UiIntent::UploadForOrder { order, action, seen } => {
                let job = self.orders.iter().find(|o| o.id == order).and_then(|o| self.order_job(o));
                match job {
                    Some(p) if action.subject == ActionSubject::Job(p.job.plan) => {
                        let (update, execution) = self.machine_action(None, action, seen, Some(order));
                        if let Some(execution) = execution {
                            self.order_executions.insert(execution, order);
                        }
                        update
                    }
                    _ => error(None, MessageKey::MachineActionRefused),
                }
            }
            UiIntent::SetRecentProjects { paths } => {
                self.restore_recent(paths);
                AppUpdate { view_changed: true, ..AppUpdate::default() }
            }
            UiIntent::ClearRecentProjects => {
                AppUpdate { effects: self.clear_recent(), view_changed: true, ..AppUpdate::default() }
            }
            UiIntent::PasteData { document, bytes, in_place } => self.paste_data(document, bytes, in_place),
            UiIntent::SetReferencePoint { document, point } => {
                self.with_session(document, |s| arrange::set_reference_point(s, point))
            }
            UiIntent::SetAspectLock { document, locked } => {
                self.with_session(document, |s| arrange::set_aspect_lock(s, locked))
            }
            UiIntent::App(intent) => self.app_intent(intent),
            UiIntent::SketchSelect { document, entities, constraints, mode } => {
                self.sketch_select(document, entities, constraints, mode)
            }
            UiIntent::AddSketchDimension { document, kind, text } => self.add_sketch_dimension(document, kind, text),
            UiIntent::SetSketchDimensionRole { document, sketch, constraint, role } => {
                self.sketch_edit(document, SketchCommand::SetDimensionRole { sketch, constraint, role })
            }
            UiIntent::SelectSketchProblem { document } => self.select_sketch_problem(document),
            UiIntent::RestoreLastValidSketch { document, sketch } => self.restore_last_valid_sketch(document, sketch),
            UiIntent::SetSketchParameter { document, sketch, name, text } => {
                let text = sketch_param::normalise_decimal_commas(&text);
                self.set_sketch_parameter(document, sketch, name, text)
            }
            UiIntent::RemoveSketchParameter { document, sketch, name } => {
                self.remove_sketch_parameter(document, sketch, name)
            }
            UiIntent::ApplyGeometryDialog { document, text } => {
                let request = self.next_id::<RequestId>();
                self.with_session(document, |s| shapes::apply_dialog(s, document, &text, request))
            }
            UiIntent::CancelGeometryDialog { document } => {
                self.with_session(document, |s| shapes::cancel_dialog(s, document))
            }
            UiIntent::SetToolOption { document, key, text } => {
                self.with_session(document, |s| shapes::set_tool_option(s, document, key, &text))
            }
            UiIntent::OpenCamDialog { document, dialog } => self.cam_open(document, dialog),
            UiIntent::SetCamField { document, dialog, field, value } => self.cam_set(document, dialog, field, value),
            UiIntent::CamDialogAction { document, dialog, action } => self.cam_action(document, dialog, action),
            UiIntent::Text { document, edit } => {
                let request = self.next_id::<RequestId>();
                let fonts = self.fonts.as_ref().map(|(f, _)| f.clone());
                self.with_session(document, |s| text::edit(s, document, fonts.as_ref(), edit, request))
            }
            UiIntent::Variable { document, edit } => {
                let request = self.next_id::<RequestId>();
                let fonts = self.fonts.as_ref().map(|(f, _)| f.clone());
                let clock = self.clock.as_ref().and_then(|c| c());
                self.with_session(document, |s| variable::edit(s, document, fonts.as_ref(), clock, edit, request))
            }
            UiIntent::Barcode { document, edit } => {
                let request = self.next_id::<RequestId>();
                self.with_session(document, |s| barcode::edit(s, document, edit, request))
            }
            UiIntent::Generator { document, edit } => {
                let request = self.next_id::<RequestId>();
                self.with_session(document, |s| generator::edit(s, document, edit, request))
            }
            UiIntent::TextPath { document, edit } => {
                let request = self.next_id::<RequestId>();
                let fonts = self.fonts.as_ref().map(|(f, _)| f.clone());
                self.with_session(document, |s| text::path_edit(s, document, fonts.as_ref(), edit, request))
            }
            UiIntent::CommitShapeParam { document, key, text } => {
                let request = self.next_id::<RequestId>();
                self.with_session(document, |s| shapes::commit_shape_param(s, document, key, &text, request))
            }
            UiIntent::Schikken(intent) => self.schikken(intent),
            UiIntent::CancelTask { document, task } => self.with_session(document, |s| {
                if let Some((kind, source)) = s.pending.get(&task)
                    && kind.is_cancellable()
                {
                    source.cancel();
                }
                AppUpdate::default()
            }),
        }
    }

    fn next_id<I: StableId>(&mut self) -> Option<I> {
        self.ids.allocate().ok()
    }

    fn with_session(&mut self, document: DocumentId, f: impl FnOnce(&mut Session) -> AppUpdate) -> AppUpdate {
        match self.sessions.get_mut(&document) {
            Some(s) => f(s),
            None => AppUpdate::default(),
        }
    }
}

/// Every document command goes through the evaluators, so derived geometry (sketches) follows
/// its definition in the same transaction and history entry (contract §11).
fn apply_command(
    fonts: &Option<SharedFonts>,
    doc: &mut DocumentSession,
    cmd: DocumentCommand,
) -> Result<CommandOutcome, CommandError> {
    // Moving a generated object edits its definition (C-8), wherever the move comes from.
    let cmd = DocumentCommand { payload: generator::retarget(&doc.snapshot(), cmd.payload), ..cmd };
    // Moving text that follows a guide edits where it starts on the guide (G8-editor, E08).
    let cmd = DocumentCommand { payload: text::retarget_move(&doc.snapshot(), fonts.as_ref(), cmd.payload), ..cmd };
    let sketches = SketchEvaluator::new(SketchSolverAdapter);
    match fonts {
        Some(f) => doc.apply_with(cmd, &[&sketches, &GeneratorEvaluator, &BoundTextEvaluator::new(f.clone())]),
        None => doc.apply_with(cmd, &[&sketches, &GeneratorEvaluator]),
    }
}

fn submit(
    s: &mut Session,
    tasks: &Arc<dyn TaskPort>,
    request: TaskRequest,
    cancel: CancellationSource,
    document: DocumentId,
) -> AppUpdate {
    let (id, kind) = (request.id, request.input.kind());
    match tasks.submit(request) {
        Ok(()) => {
            s.pending.insert(id, (kind, cancel));
            AppUpdate { view_changed: true, ..AppUpdate::default() }
        }
        Err(_) => error(Some(document), MessageKey::CommandRejected),
    }
}

fn error(document: Option<DocumentId>, message: MessageKey) -> AppUpdate {
    AppUpdate { effects: vec![HostEffect::ShowError { document, message }], ..AppUpdate::default() }
}

impl Session {
    /// Unsaved work: document changes, or counters that moved since the last save (E11).
    fn unsaved(&self) -> bool {
        self.doc.is_dirty() || self.variable.unsaved() || self.part_name != self.saved_part_name
    }
}

fn apply_selection(s: &mut Session, change: &SelectionChange) {
    s.selection.apply(change);
    s.selection.retain_existing(&s.doc.snapshot());
    s.field_errors.clear();
}

/// Common tail of every document mutation: selection follows the change, older tasks are
/// cancelled (best effort; revision rejection stays decisive), issues are recomputed and a
/// failure becomes a message.
fn after_change(s: &mut Session, document: DocumentId, result: Result<CommandOutcome, CommandError>) -> AppUpdate {
    match result {
        Ok(outcome) => {
            s.cancel_revision_sensitive();
            if let Some(kept) = outcome.selection.clone() {
                // Undo/redo: the document's record knows what was selected (P02).
                s.selection.apply(&SelectionChange::Replace(kept));
            } else {
                let mut focus: Vec<_> = outcome.changes.created.clone();
                focus.extend(outcome.changes.modified.iter().copied());
                if !focus.is_empty() {
                    s.selection.apply(&SelectionChange::Replace(focus));
                }
            }
            let snapshot = s.doc.snapshot();
            s.selection.retain_existing(&snapshot);
            s.issues = geometry_issues(&snapshot);
            s.cycle = None;
            s.messages.clear();
            if let Some(dropped) = machine_rest::trimmed_steps(&outcome.warnings) {
                s.messages.push(StatusMessage { key: MessageKey::HistoryTrimmed(dropped) });
            }
            s.problem = None;
            layers::sync(s);
            sketch::sync(s);
            image::sync(s);
            AppUpdate::redraw(document)
        }
        Err(refusal) => {
            let key = refusal_key(&s.doc.snapshot(), &refusal);
            s.messages.push(StatusMessage { key });
            let mut update = error(Some(document), key);
            update.view_changed = true;
            update
        }
    }
}

/// The words for a refused command: a turn, scale or shear of a text that follows a guide says
/// why it cannot be done (G9-editor, E08); everything else is the general refusal.
fn refusal_key(snapshot: &burnburn_document::DocumentSnapshot, refusal: &CommandError) -> MessageKey {
    match refusal {
        CommandError::DerivedObject { owner: burnburn_document::DerivedOwner::Text(text) }
            if snapshot.text(*text).is_some_and(|t| t.path.is_some()) =>
        {
            MessageKey::BoundTextFollowsGuide
        }
        _ => MessageKey::CommandRejected,
    }
}

/// Applies one payload as one command against the current revision.
fn apply_edit(s: &mut Session, document: DocumentId, payload: CommandPayload, request: Option<RequestId>) -> AppUpdate {
    let Some(request) = request else { return error(Some(document), MessageKey::CommandRejected) };
    if arrange::touches_locked(s, &payload) {
        return error(Some(document), MessageKey::ObjectsLocked);
    }
    let cmd = DocumentCommand { document, expected: s.doc.revision(), request, payload };
    let result = apply_command(&s.fonts, &mut s.doc, cmd);
    after_change(s, document, result)
}

/// What an operator may do (IM31): look, close, run approved jobs and handle orders. No editing.
fn operator_may(intent: &UiIntent) -> bool {
    match intent {
        UiIntent::Focus { .. }
        | UiIntent::Close { .. }
        | UiIntent::ConfirmClose { choice: CloseChoice::Discard | CloseChoice::Cancel, .. }
        | UiIntent::DialogCancelled { .. }
        | UiIntent::Camera { .. }
        | UiIntent::Machine { .. }
        | UiIntent::RequestStart { .. }
        | UiIntent::ConfirmStart { .. }
        | UiIntent::CancelStart
        | UiIntent::SetOrderRemnant { .. }
        | UiIntent::Recognition(_)
        | UiIntent::AddOrder { .. }
        | UiIntent::ChooseOrderJob { .. }
        | UiIntent::UploadForOrder { .. } => true,
        UiIntent::Invoke { command, .. } => *command == CommandKey::Close,
        _ => false,
    }
}

fn intent_document(intent: &UiIntent) -> Option<DocumentId> {
    match intent {
        UiIntent::Focus { document }
        | UiIntent::Save { document, .. }
        | UiIntent::Close { document }
        | UiIntent::ConfirmClose { document, .. }
        | UiIntent::DialogCancelled { document, .. }
        | UiIntent::Select { document, .. }
        | UiIntent::SelectByKind { document, .. }
        | UiIntent::SetNotes { document, .. }
        | UiIntent::SetPartName { document, .. }
        | UiIntent::PreviewOffset { document, .. }
        | UiIntent::PreviewRepair { document, .. }
        | UiIntent::Structure { document, .. }
        | UiIntent::ChoosePlate { document, .. }
        | UiIntent::CenterlineEdit { document, .. }
        | UiIntent::TracePrep { document, .. }
        | UiIntent::TracePick { document, .. }
        | UiIntent::TraceRecipe { document, .. }
        | UiIntent::SetOffsetOptions { document, .. }
        | UiIntent::CloseNotes { document }
        | UiIntent::CommitField { document, .. }
        | UiIntent::CommitToolSize { document, .. }
        | UiIntent::AddOperation { document, .. }
        | UiIntent::ExportRd { document, .. }
        | UiIntent::ExportGcode { document, .. }
        | UiIntent::OpenTraceSource { document, .. }
        | UiIntent::SetTraceRecipe { document, .. }
        | UiIntent::SetImageDpi { document, .. }
        | UiIntent::SetImageAdjust { document, .. }
        | UiIntent::ReplaceImage { document, .. }
        | UiIntent::ReplaceImageToFit { document, .. }
        | UiIntent::SaveProcessedBitmap { document, .. }
        | UiIntent::SetTraceForm { document, .. }
        | UiIntent::ApplyTracePreset { document, .. }
        | UiIntent::SetSketchParameter { document, .. }
        | UiIntent::RemoveSketchParameter { document, .. }
        | UiIntent::SketchSelect { document, .. }
        | UiIntent::AddSketchDimension { document, .. }
        | UiIntent::SetSketchDimensionRole { document, .. }
        | UiIntent::SelectSketchProblem { document }
        | UiIntent::RestoreLastValidSketch { document, .. }
        | UiIntent::ApplyGeometryDialog { document, .. }
        | UiIntent::CancelGeometryDialog { document }
        | UiIntent::SetToolOption { document, .. }
        | UiIntent::CommitShapeParam { document, .. }
        | UiIntent::OpenCamDialog { document, .. }
        | UiIntent::SetCamField { document, .. }
        | UiIntent::CamDialogAction { document, .. }
        | UiIntent::Pointer { document, .. }
        | UiIntent::Key { document, .. }
        | UiIntent::SetReferencePoint { document, .. }
        | UiIntent::SetAspectLock { document, .. }
        | UiIntent::Text { document, .. }
        | UiIntent::TextPath { document, .. }
        | UiIntent::Variable { document, .. }
        | UiIntent::Barcode { document, .. }
        | UiIntent::Generator { document, .. }
        | UiIntent::Camera { document, .. } => Some(*document),
        UiIntent::PasteData { document, .. } => *document,
        UiIntent::Schikken(intent) => Some(intent.document()),
        UiIntent::CancelTask { document, .. } => Some(*document),
        UiIntent::Invoke { document, .. } | UiIntent::Machine { document, .. } | UiIntent::Laser { document, .. } => {
            *document
        }
        _ => None,
    }
}
