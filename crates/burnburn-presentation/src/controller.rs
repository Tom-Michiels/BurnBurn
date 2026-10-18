use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use burnburn_cam::{CompileOptions, Placement, PreflightReport, Severity, StartPoint};
use burnburn_document::process::{
    FillSettings, Kerf, OperationDraft, OperationMode, Passes, PowerPercent, PowerRange, ProcessCommand,
    ProcessSettings, ScanAxis, ScanDirection, Speed,
};
use burnburn_document::sketch::SketchCommand;
use burnburn_document::text::{FontSize, TextCommand, TextId, TextObject};
use burnburn_document::{
    AlignTarget, Anchor, Axis, CommandError, CommandOutcome, CommandPayload, DerivedOwner, DocumentCommand,
    DocumentSession, EditError, EvalDiagnosis, EvalStatus, GeometryCommand, GeometrySource, ImportItem,
};
use burnburn_editor::{
    CLICK_TOLERANCE_PT, Camera, CanvasInput, CanvasKey, Editor, GeometryIssue, KeyPhase, Modifiers, PointerEvent,
    PointerPhase, SceneRequest, SceneSnapshot, Selection, SelectionChange, SnapSettings, ToolContext, ToolKind,
    ToolStep, Viewport, boolean_operands, build_scene, geometry_issues, hit_test,
};
use burnburn_geometry::{
    Affine, AlignEdge, BooleanOp, DiagnosticTolerance, Direction, OffsetJoin, Point, ProductionTolerance, Spacing,
    Vector,
};
use burnburn_image::SourceImage;
use burnburn_io::fs::FileSystem;
use burnburn_io::import::{DxfScale, ImportDraft, ImportError};
use burnburn_io::{AssetStore, Project, ProjectUuid, SaveError};
use burnburn_job::{JobPlan, JobPlanId, MachineProfile, PlanPreview};
use burnburn_machine::{Evidence, ExecutionId, ExecutionState, FailReason, UnknownReason};
use burnburn_ruida::RuidaProfile;
use burnburn_sketch::SketchEvaluator;
use burnburn_sketch::solver::SketchSolverAdapter;
use burnburn_trace::{TraceRecipe, TraceRequest, TraceResult};
use burnburn_types::{
    Angle, CancellationSource, ContentHash, DocumentId, GeometryId, IdAllocator, Length, OperationId, RequestId,
    RevisionId, StableId, TaskId, Unit,
};

use crate::commit::{NameKind, boolean_options, name_kind, payload};
use burnburn_text::{FontData, FontError, FontRef, FontSource, TextEvaluator};

use crate::camera::{
    BedCameraIntent, CameraAuthorization, CameraDeviceId, CameraFrameInfo, CameraMode, CameraPort, CameraViewModel,
};
use crate::field::{format_mm, parse_angle, parse_length, parse_number, parse_position};
use crate::intent::{
    CameraIntent, CloseChoice, CommandKey, DialogKind, FieldKey, ImportUnit, OperationField, OperationKind, SelectMode,
    UiIntent, ViewPointerEvent,
};
use crate::production::{
    ActionId, ActionSubject, Dispatched, HistoryStore, HistoryStoreError, JobInputs, MachineAction, MachineError,
    MachineIntent, MachinePort, MachineStatus, OrderDraft, OrderId, Outcome, PreparedJob, Role, StoredOrder,
    file_may_remain, reconcilable,
};
use crate::task::{
    ParsedImport, TaskError, TaskInput, TaskKind, TaskOutcome, TaskOutput, TaskPort, TaskRequest, revision_key,
};
use crate::view_model::{
    AppViewModel, BooleanPreview, CommandState, DocumentViewModel, ExportViewModel, FieldViewModel, ImportViewModel,
    InspectorViewModel, JobCardViewModel, MachineButton, MachineStatusKey, MachineViewModel, MeasureViewModel,
    MessageKey, ObjectRow, OperatorJobRow, OperatorViewModel, OrderViewModel, PlanViewModel, ProblemViewModel,
    SelectionCycle, SketchStatus, SketchViewModel, StatusMessage, TaskBadge, TraceSummary, TracerViewModel,
};

mod layers;

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

/// One compiled plan with what was derived from exactly that plan.
#[derive(Clone)]
struct Compiled {
    plan: Arc<JobPlan>,
    preview: Arc<PlanPreview>,
    report: PreflightReport,
}

/// The open tracer of one window: decoded source, its stored bytes and the newest request.
#[derive(Clone)]
struct Tracer {
    source: Arc<SourceImage>,
    bytes: Arc<[u8]>,
    media: &'static str,
    recipe: Option<TraceRecipe>,
    /// Newest submitted trace; only its result may be shown (check_fresh).
    latest: Option<TaskId>,
    result: Option<Arc<TraceResult>>,
}

/// A parsed import waiting for its unit decision and commit (P03).
#[derive(Clone)]
struct PendingImport {
    file: String,
    media: &'static str,
    bytes: Arc<[u8]>,
    parsed: ParsedImport,
    unit: Option<ImportUnit>,
}

impl PendingImport {
    /// The import in millimetres for the current decision; `ScaleRequired` while unknown.
    fn draft(&self) -> Result<ImportDraft, ImportError> {
        match &self.parsed {
            ParsedImport::Svg(d) => Ok((**d).clone()),
            ParsedImport::Dxf(d) => d.to_draft(self.unit.and_then(|u| DxfScale::mm_per_unit(u.mm_per_unit()))),
        }
    }
}

/// A frozen job and whether the designer released it for the operator.
#[derive(Clone)]
struct Prepared {
    job: Arc<PreparedJob>,
    approved: bool,
}

/// An order (IM31) with the explicitly chosen job, if any.
#[derive(Clone)]
struct Order {
    id: OrderId,
    draft: OrderDraft,
    /// Hash of the chosen `JobPlanId` (so an order from the history can name it).
    chosen: Option<ContentHash>,
}

/// The active machine (B11): profile for compiling, controller dialect for RD.
#[derive(Clone)]
struct Machine {
    profile: Arc<MachineProfile>,
    ruida: RuidaProfile,
}

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
    title: String,
    path: Option<PathBuf>,
    field_errors: BTreeMap<FieldKey, MessageKey>,
    messages: Vec<StatusMessage>,
    measure: Option<MeasureViewModel>,
    pending: BTreeMap<TaskId, (TaskKind, CancellationSource)>,
    /// Default-name counters per created kind ("Rechthoek 2", "Cirkel 1").
    created: BTreeMap<NameKind, u64>,
    snap: SnapSettings,
    cycle: Option<SelectionCycle>,
    tool_size_error: Option<MessageKey>,
    /// Problems of the current revision (recomputed on every revision change).
    issues: Vec<GeometryIssue>,
    boolean_preview: Option<BooleanPreview>,
    /// Machine artefacts: never document data, never part of undo (AGENTS.md).
    compiled: Option<Compiled>,
    export: Option<ExportViewModel>,
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
            editor: Editor::new(),
            selection: Selection::default(),
            camera,
            viewport,
            title,
            path,
            field_errors: BTreeMap::new(),
            messages: Vec::new(),
            measure: None,
            pending: BTreeMap::new(),
            created: BTreeMap::new(),
            snap: SnapSettings::default(),
            cycle: None,
            tool_size_error: None,
            issues,
            boolean_preview: None,
            compiled: None,
            export: None,
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
    /// Where autosave writes recovery files.
    recovery_dir: Option<PathBuf>,
    /// Tasks without a window: recovery list/remove (task → kind).
    app_tasks: BTreeMap<TaskId, TaskKind>,
    /// Open tasks that restore a recovery file (the window opens unsaved and untitled).
    restoring: BTreeMap<TaskId, PathBuf>,
}

impl AppController {
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
            role: Role::Designer,
            orders: Vec::new(),
            order_executions: BTreeMap::new(),
            history: None,
            fonts: None,
            camera: None,
            camera_active: None,
            recovery_dir: None,
            app_tasks: BTreeMap::new(),
            restoring: BTreeMap::new(),
        }
    }

    /// Connects the machine service. Kept out of `AppPorts` so hosts without a machine are unchanged.
    pub fn attach_machine(&mut self, port: Arc<dyn MachinePort>) {
        self.machine_port = Some(port);
    }

    /// Connects the production history (orders and their executions are kept there, IM31) and
    /// loads what it holds.
    pub fn attach_history(&mut self, history: Arc<dyn HistoryStore>) -> Result<(), HistoryStoreError> {
        let stored = history.orders()?;
        self.orders = stored.orders.into_iter().map(|o| Order { id: o.id, draft: o.draft, chosen: o.chosen }).collect();
        self.order_executions = stored.executions.into_iter().collect();
        self.history = Some(history);
        Ok(())
    }

    /// Makes an order change durable before it is shown (no history: kept in memory only).
    fn store_order(&self, order: &Order) -> Result<(), AppUpdate> {
        let Some(history) = self.history.as_ref() else { return Ok(()) };
        let stored = StoredOrder { id: order.id, draft: order.draft.clone(), chosen: order.chosen };
        history.record_order(&stored).map_err(|_| error(None, MessageKey::IntentNotRecorded))
    }

    /// Fonts for text input (`UiIntent::InsertText`) and the face new text uses.
    pub fn attach_fonts(&mut self, fonts: Arc<dyn FontSource + Send + Sync>, default_font: FontRef) {
        let shared = SharedFonts(fonts);
        for s in self.sessions.values_mut() {
            s.fonts = Some(shared.clone());
        }
        self.fonts = Some((shared, default_font));
    }

    /// The bed camera (IM27); hosts implement the port.
    pub fn attach_camera(&mut self, camera: Arc<dyn CameraPort>) {
        self.camera = Some(camera);
        self.camera_active = None;
    }

    /// One committed text: extends the selected text object, or creates one at the origin.
    fn insert_text(&mut self, document: DocumentId, text: String) -> AppUpdate {
        if text.is_empty() {
            return AppUpdate::default();
        }
        let Some((_, font)) = self.fonts.clone() else { return error(Some(document), MessageKey::NoFontAvailable) };
        let request = self.next_id::<RequestId>();
        self.with_session(document, |s| {
            let snapshot = s.doc.snapshot();
            let selected: Vec<TextId> = s
                .selection
                .ids()
                .iter()
                .filter_map(|id| match snapshot.object(*id).map(|o| o.source()) {
                    Some(GeometrySource::Derived(k)) => match k.owner {
                        DerivedOwner::Text(t) => Some(t),
                        _ => None,
                    },
                    _ => None,
                })
                .collect();
            let edit = match selected.as_slice() {
                [first, rest @ ..] if rest.iter().all(|t| t == first) => {
                    let Some(current) = snapshot.text(*first) else { return AppUpdate::default() };
                    let mut source = current.clone();
                    source.content.push_str(&text);
                    TextCommand::Set { text: *first, source: Box::new(source) }
                }
                _ => {
                    let Ok(size) = Length::from_mm(DEFAULT_TEXT_MM)
                        .map_err(|_| ())
                        .and_then(|l| FontSize::from_mm(l).map_err(|_| ()))
                    else {
                        return error(Some(document), MessageKey::CommandRejected);
                    };
                    TextCommand::Create { text: Box::new(TextObject::new(text, font, size)) }
                }
            };
            apply_edit(s, document, CommandPayload::Text(edit), request)
        })
    }

    /// Content of every text object of `document` (tests and accessibility), in id order.
    pub fn text_contents(&self, document: DocumentId) -> Vec<String> {
        self.sessions
            .get(&document)
            .map_or_else(Vec::new, |s| s.doc.snapshot().text_layer().texts().map(|(_, t)| t.content.clone()).collect())
    }

    /// Camera panel, when a camera port is attached.
    pub fn camera_view(&self) -> Option<CameraViewModel> {
        let camera = self.camera.as_ref()?;
        let (devices, devices_error) = match camera.devices() {
            Ok(d) => (d, None),
            Err(e) => (Vec::new(), Some(e)),
        };
        let frame = self
            .camera_active
            .as_ref()
            .and_then(|(device, _)| camera.latest_frame(device))
            .map(|f| CameraFrameInfo { seq: f.seq, time: f.time, width: f.image.width(), height: f.image.height() });
        Some(CameraViewModel {
            authorization: camera.authorization(),
            devices,
            devices_error,
            active: self.camera_active.clone(),
            frame,
            note: MessageKey::VisualPlacementOnly,
        })
    }

    fn bed_camera(&mut self, intent: BedCameraIntent) -> AppUpdate {
        let Some(camera) = self.camera.clone() else { return error(None, MessageKey::CommandRejected) };
        let changed = AppUpdate { view_changed: true, ..AppUpdate::default() };
        match intent {
            BedCameraIntent::RequestAccess => {
                camera.request_authorization();
                changed
            }
            BedCameraIntent::Start { device, mode } => {
                if camera.authorization() != CameraAuthorization::Authorized {
                    return error(None, MessageKey::CameraNotAuthorized);
                }
                if let Some((old, _)) = self.camera_active.take() {
                    camera.stop(&old);
                }
                match camera.start(&device, &mode) {
                    Ok(()) => {
                        self.camera_active = Some((device, mode));
                        changed
                    }
                    Err(_) => error(None, MessageKey::CommandRejected),
                }
            }
            BedCameraIntent::Stop => {
                if let Some((device, _)) = self.camera_active.take() {
                    camera.stop(&device);
                }
                changed
            }
        }
    }

    /// Where autosave writes recovery files (one per document, `<project uuid>.burnburn`).
    pub fn set_recovery_dir(&mut self, dir: PathBuf) {
        self.recovery_dir = Some(dir);
    }

    /// Writes a recovery file for every dirty document changed since its last autosave.
    fn autosave(&mut self) -> AppUpdate {
        let Some(dir) = self.recovery_dir.clone() else { return AppUpdate::default() };
        let (tasks, fs) = (Arc::clone(&self.ports.tasks), Arc::clone(&self.ports.fs));
        let due: Vec<DocumentId> = self
            .sessions
            .iter()
            .filter(|(_, s)| s.doc.is_dirty() && s.autosaved != Some(s.doc.revision()))
            .filter(|(_, s)| !s.pending.values().any(|(k, _)| *k == TaskKind::Autosave))
            .map(|(id, _)| *id)
            .collect();
        let mut update = AppUpdate::default();
        for document in due {
            let Some(id) = self.next_id::<TaskId>() else { break };
            let (tasks, fs, dir) = (Arc::clone(&tasks), Arc::clone(&fs), dir.clone());
            let u = self.with_session(document, |s| {
                let snapshot = s.doc.snapshot();
                let source = snapshot.revision();
                let mut project = Project::new(s.uuid, (*snapshot).clone());
                project.assets = s.assets.clone();
                let path = dir.join(format!(
                    "{}.burnburn",
                    s.uuid.as_bytes().iter().map(|b| format!("{b:02x}")).collect::<String>()
                ));
                let cancel = CancellationSource::new();
                let request = TaskRequest {
                    id,
                    document,
                    source,
                    key: revision_key(TaskKind::Autosave, document, source),
                    cancel: cancel.token(),
                    input: TaskInput::SaveRecovery { project: Box::new(project), path, fs },
                };
                submit(s, &tasks, request, cancel, document)
            });
            update.effects.extend(u.effects);
        }
        update
    }

    /// A task that belongs to no window (recovery list/remove).
    fn app_task(&mut self, input: TaskInput) -> AppUpdate {
        let (Some(document), Some(id)) = (self.next_id::<DocumentId>(), self.next_id::<TaskId>()) else {
            return error(None, MessageKey::CommandRejected);
        };
        let kind = input.kind();
        let source = RevisionId::INITIAL;
        let request = TaskRequest {
            id,
            document,
            source,
            key: revision_key(kind, document, source),
            cancel: CancellationSource::new().token(),
            input,
        };
        match self.ports.tasks.submit(request) {
            Ok(()) => {
                self.app_tasks.insert(id, kind);
                AppUpdate::default()
            }
            Err(_) => error(None, MessageKey::CommandRejected),
        }
    }

    fn app_task_finished(&mut self, outcome: TaskOutcome) -> AppUpdate {
        match outcome.result {
            Ok(TaskOutput::RecoveryFound { files }) if !files.is_empty() => {
                AppUpdate { effects: vec![HostEffect::OfferRecovery { files }], ..AppUpdate::default() }
            }
            _ => AppUpdate::default(),
        }
    }

    /// Role of the person at this controller (from configuration, never from a `UiIntent`).
    pub fn set_role(&mut self, role: Role) {
        self.role = role;
    }

    pub fn role(&self) -> Role {
        self.role
    }

    /// Compact operator view (IM31): approved jobs, machine status and orders; no editor functions.
    pub fn operator_view(&self) -> OperatorViewModel {
        let status = self.machine_port.as_ref().map(|p| p.status());
        let jobs = self
            .sessions
            .iter()
            .filter_map(|(id, s)| s.prepared.as_ref().filter(|p| p.approved).map(|p| (*id, p)))
            .map(|(document, p)| OperatorJobRow {
                document,
                name: p.job.name.clone(),
                plan: p.job.plan,
                rd_hash: p.job.rd_hash,
                upload: status.as_ref().map(|st| self.upload_button(st, p)),
            })
            .collect();
        let orders = self.orders.iter().map(|o| self.order_view(o, status.as_ref())).collect();
        OperatorViewModel { machine: status.as_ref().map(machine_view_of), jobs, orders }
    }

    /// Machine panel: status from evidence, controls with their idempotency keys.
    pub fn machine_view(&self) -> Option<MachineViewModel> {
        self.machine_port.as_ref().map(|p| machine_view_of(&p.status()))
    }

    fn upload_button(&self, status: &MachineStatus, p: &Prepared) -> MachineButton {
        let state = status.current.as_ref().map(|c| c.execution.state.clone());
        let idle = state.as_ref().is_none_or(ExecutionState::is_terminal);
        MachineButton {
            action: status.action_id(MachineAction::Upload, ActionSubject::Job(p.job.plan)),
            enabled: idle && (self.role == Role::Designer || p.approved),
            seen: state,
            label: MessageKey::MachineAction(MachineAction::Upload),
        }
    }

    /// Approved prepared jobs whose name is the order's part, one per plan.
    fn candidates(&self, part: &str) -> Vec<&Prepared> {
        let mut out: Vec<&Prepared> = Vec::new();
        for p in self.sessions.values().filter_map(|s| s.prepared.as_ref()) {
            if p.approved && p.job.name == part && !out.iter().any(|q| q.job.plan == p.job.plan) {
                out.push(p);
            }
        }
        out
    }

    fn order_job(&self, order: &Order) -> Option<&Prepared> {
        let candidates = self.candidates(&order.draft.part);
        match (order.chosen, candidates.as_slice()) {
            (Some(plan), _) => candidates.into_iter().find(|p| p.job.plan.hash() == plan),
            (None, [only]) => Some(*only),
            _ => None,
        }
    }

    fn order_view(&self, o: &Order, status: Option<&MachineStatus>) -> OrderViewModel {
        let candidates: Vec<JobPlanId> = self.candidates(&o.draft.part).iter().map(|p| p.job.plan).collect();
        let job = self.order_job(o);
        let completed = status.map_or(0, |st| {
            st.executions()
                .filter(|r| r.execution.state == ExecutionState::Completed)
                .filter(|r| self.order_executions.get(&r.execution.id) == Some(&o.id))
                .count()
        });
        OrderViewModel {
            id: o.id,
            name: o.draft.name.clone(),
            part: o.draft.part.clone(),
            quantity: o.draft.quantity,
            completed: u32::try_from(completed).unwrap_or(u32::MAX),
            remnant: o.draft.remnant.clone(),
            job: job.map(|p| p.job.plan),
            needs_choice: job.is_none() && candidates.len() > 1,
            candidates,
            upload: job.zip(status).map(|(p, st)| self.upload_button(st, p)),
        }
    }

    /// The prepared job an upload action names: in `document` if given, else in any window.
    fn job_for(&self, document: Option<DocumentId>, plan: JobPlanId) -> Option<&Prepared> {
        let mut sessions: Box<dyn Iterator<Item = &Session>> = match document {
            Some(d) => Box::new(self.sessions.get(&d).into_iter()),
            None => Box::new(self.sessions.values()),
        };
        sessions.find_map(|s| s.prepared.as_ref().filter(|p| p.job.plan == plan))
    }

    /// Sends one machine action to the machine service. Never touches a document.
    fn machine_action(
        &mut self,
        document: Option<DocumentId>,
        action: ActionId,
        seen: Option<ExecutionState>,
        order: Option<OrderId>,
    ) -> (AppUpdate, Option<ExecutionId>) {
        let Some(port) = self.machine_port.clone() else {
            return (error(document, MessageKey::MachineActionRefused), None);
        };
        let job = match (action.kind, action.subject) {
            (MachineAction::Upload, ActionSubject::Job(plan)) => match self.job_for(document, plan) {
                Some(p) if self.role == Role::Operator && !p.approved => {
                    return (error(document, MessageKey::NotAllowedForRole), None);
                }
                Some(p) => Some(Arc::clone(&p.job)),
                None => return (error(document, MessageKey::MachineActionRefused), None),
            },
            _ => None,
        };
        let machine = port.status().machine;
        let intent = MachineIntent { action, machine, job, required_state: seen, order, role: self.role };
        let changed = AppUpdate { view_changed: true, ..AppUpdate::default() };
        match port.dispatch(intent) {
            Ok(Dispatched::Sent(execution)) => (changed, execution),
            Ok(Dispatched::AlreadyHandled(_)) => (changed, None),
            Err(MachineError::NotRecorded(_)) => (error(document, MessageKey::IntentNotRecorded), None),
            Err(MachineError::AlreadyRecorded) => (changed, None),
            Err(_) => (error(document, MessageKey::MachineActionRefused), None),
        }
    }

    pub fn handle(&mut self, intent: UiIntent) -> AppUpdate {
        if self.role == Role::Operator && !operator_may(&intent) {
            return error(intent_document(&intent), MessageKey::NotAllowedForRole);
        }
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
            UiIntent::Invoke { document, command } => self.invoke(document, command),
            UiIntent::Pointer { document, event } => self.pointer(document, event),
            UiIntent::Key { document, key, modifiers } => self.key(document, key, modifiers),
            UiIntent::Camera { document, intent } => self.camera(document, intent),
            UiIntent::SetMachineProfile { profile, ruida } => {
                // Profile data, not document data: existing plans become stale, nothing else changes.
                self.machine = Some(Machine { profile: Arc::from(profile), ruida });
                AppUpdate { view_changed: true, ..AppUpdate::default() }
            }
            UiIntent::AddOperation { document, draft } => {
                let request = self.next_id::<RequestId>();
                self.with_session(document, |s| {
                    let targets = s.selection.ids().to_vec();
                    let edit = ProcessCommand::AddOperation { draft, targets };
                    apply_edit(s, document, CommandPayload::Process(edit), request)
                })
            }
            UiIntent::ExportRd { document, path } => self.export_rd(document, path),
            UiIntent::OpenTraceSource { document, path } => self.open_trace_source(document, path),
            UiIntent::SetTraceRecipe { document, recipe } => self.set_trace_recipe(document, *recipe),
            UiIntent::Machine { document, action, seen } => self.machine_action(document, action, seen, None).0,
            UiIntent::AddSketchConstraint { document, sketch, constraint } => {
                let request = self.next_id::<RequestId>();
                let edit = SketchCommand::AddConstraint { sketch, constraint };
                self.with_session(document, |s| apply_edit(s, document, CommandPayload::Sketch(edit), request))
            }
            UiIntent::RemoveSketchConstraint { document, sketch, constraint } => {
                let request = self.next_id::<RequestId>();
                let edit = SketchCommand::RemoveConstraint { sketch, constraint };
                self.with_session(document, |s| apply_edit(s, document, CommandPayload::Sketch(edit), request))
            }
            UiIntent::SetSketchDimension { document, sketch, constraint, text } => {
                let request = self.next_id::<RequestId>();
                let edit = SketchCommand::SetDimensionValue { sketch, constraint, text };
                self.with_session(document, |s| apply_edit(s, document, CommandPayload::Sketch(edit), request))
            }
            UiIntent::Import { document, path } => {
                let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::ImportFailed) };
                let (tasks, fs) = (Arc::clone(&self.ports.tasks), Arc::clone(&self.ports.fs));
                self.with_session(document, |s| {
                    let source = s.doc.revision();
                    let cancel = CancellationSource::new();
                    let request = TaskRequest {
                        id,
                        document,
                        source,
                        key: revision_key(TaskKind::ParseImport, document, source),
                        cancel: cancel.token(),
                        input: TaskInput::ParseImport { path, fs },
                    };
                    submit(s, &tasks, request, cancel, document)
                })
            }
            UiIntent::SetImportUnit { document, unit } => self.with_session(document, |s| match s.import.as_mut() {
                Some(i) => {
                    i.unit = Some(unit);
                    AppUpdate { view_changed: true, ..AppUpdate::default() }
                }
                None => AppUpdate::default(),
            }),
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
                let request = self.next_id::<RequestId>();
                let capabilities = self.machine.as_ref().map(|m| m.profile.capabilities().clone());
                self.with_session(document, |s| {
                    layers::apply_cut_settings(s, document, operation, &settings, capabilities.as_ref(), request)
                })
            }
            UiIntent::CancelCutSettings { document } => self.with_session(document, layers::cancel_cut_settings),
            UiIntent::BedCamera(intent) => self.bed_camera(intent),
            UiIntent::Autosave => self.autosave(),
            UiIntent::CheckRecovery => match self.recovery_dir.clone() {
                Some(dir) => self.app_task(TaskInput::ListRecovery { dir, fs: Arc::clone(&self.ports.fs) }),
                None => AppUpdate::default(),
            },
            UiIntent::RestoreRecovery { path } => {
                let (update, task) = self.open_task(path.clone());
                if let Some(task) = task {
                    self.restoring.insert(task, path);
                }
                update
            }
            UiIntent::DiscardRecovery { path } => {
                self.app_task(TaskInput::RemoveRecovery { path, fs: Arc::clone(&self.ports.fs) })
            }
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
            UiIntent::SetSketchParameter { document, sketch, name, text } => {
                let request = self.next_id::<RequestId>();
                self.with_session(document, |s| {
                    let edit = SketchCommand::SetParameter { sketch, name, text };
                    apply_edit(s, document, CommandPayload::Sketch(edit), request)
                })
            }
        }
    }

    /// Delivers a worker result. Applied only while the session is open and the task is still
    /// pending. Revision-sensitive results must match the current revision; a save records
    /// exactly the revision it wrote (an older one leaves the document dirty).
    pub fn task_finished(&mut self, outcome: TaskOutcome) -> AppUpdate {
        if let Some((document, _)) = self.opening.remove(&outcome.id) {
            let restored = self.restoring.remove(&outcome.id);
            return self.opened(document, outcome, restored);
        }
        if self.app_tasks.remove(&outcome.id).is_some() {
            return self.app_task_finished(outcome);
        }
        let Some(s) = self.sessions.get_mut(&outcome.document) else { return AppUpdate::default() };
        let Some((kind, _)) = s.pending.remove(&outcome.id) else { return AppUpdate::default() };
        let document = outcome.document;
        let mut update = AppUpdate { view_changed: true, ..AppUpdate::default() };
        if kind.is_revision_sensitive() && outcome.source != s.doc.revision() {
            return update;
        }
        match outcome.result {
            Ok(TaskOutput::Measured(m)) => {
                s.measure = Some(MeasureViewModel {
                    revision: outcome.source,
                    objects: m.objects,
                    width_text: m.bounds.map(|b| format_mm(b.width())),
                    height_text: m.bounds.map(|b| format_mm(b.height())),
                });
                update.effects.push(HostEffect::Redraw { document });
            }
            Ok(TaskOutput::Saved { path }) => {
                s.doc.mark_saved(outcome.source);
                if let Some(recovery) = s.recovery_path.take().filter(|_| !s.doc.is_dirty()) {
                    s.autosaved = None;
                    if let Ok(id) = self.ids.allocate::<TaskId>() {
                        let request = TaskRequest {
                            id,
                            document,
                            source: outcome.source,
                            key: revision_key(TaskKind::Recovery, document, outcome.source),
                            cancel: CancellationSource::new().token(),
                            input: TaskInput::RemoveRecovery { path: recovery, fs: Arc::clone(&self.ports.fs) },
                        };
                        if self.ports.tasks.submit(request).is_ok() {
                            self.app_tasks.insert(id, TaskKind::Recovery);
                        }
                    }
                }
                s.title = file_title(&path);
                s.path = Some(path);
                if s.close_after_save && !s.doc.is_dirty() {
                    return self.close_session(document);
                }
            }
            Ok(TaskOutput::Boolean { op, operands, result }) => {
                s.boolean_preview =
                    Some(BooleanPreview { revision: outcome.source, op, operands, result: Arc::new(result) });
                update.scenes_changed.push(document);
                update.effects.push(HostEffect::Redraw { document });
            }
            Err(TaskError::Boolean(_) | TaskError::UnknownObject(_)) => {
                s.messages.push(StatusMessage { key: MessageKey::BooleanFailed });
            }
            Ok(TaskOutput::ImageLoaded { source, bytes, path }) => {
                let media = match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
                    Some("jpg" | "jpeg") => "image/jpeg",
                    _ => "image/png",
                };
                s.tracer = Some(Tracer { source, bytes, media, recipe: None, latest: None, result: None });
            }
            Ok(TaskOutput::Traced(result)) => {
                if let Some(t) = s.tracer.as_mut() {
                    let current = t.recipe.as_ref().and_then(|r| r.key(t.source.content_hash()).ok());
                    let fresh = matches!((t.latest, current), (Some(latest), Some(key)) if result.check_fresh(latest, key).is_ok());
                    if fresh {
                        t.result = Some(result);
                        update.scenes_changed.push(document);
                    }
                }
            }
            Err(TaskError::Read(_) | TaskError::Decode(_)) => {
                s.messages.push(StatusMessage { key: MessageKey::ImageFailed });
                update
                    .effects
                    .push(HostEffect::ShowError { document: Some(document), message: MessageKey::ImageFailed });
            }
            Err(TaskError::Trace(_)) => s.messages.push(StatusMessage { key: MessageKey::TraceFailed }),
            Ok(TaskOutput::Compiled { plan, preview, report }) => {
                s.compiled = Some(Compiled { plan, preview, report });
                update.scenes_changed.push(document);
            }
            Err(TaskError::Compile(_)) => s.messages.push(StatusMessage { key: MessageKey::CompileFailed }),
            Ok(TaskOutput::ExportedRd { plan, path, rd_hash, bytes }) => {
                let plan_revision = s
                    .compiled
                    .as_ref()
                    .filter(|c| c.plan.id() == plan)
                    .map_or(outcome.source, |c| c.plan.source().document_revision);
                s.export = Some(ExportViewModel { plan, plan_revision, path, rd_hash, bytes });
            }
            Err(TaskError::Lower(_) | TaskError::Encode(_) | TaskError::Write(_)) => {
                s.messages.push(StatusMessage { key: MessageKey::ExportFailed });
                update
                    .effects
                    .push(HostEffect::ShowError { document: Some(document), message: MessageKey::ExportFailed });
            }
            Ok(TaskOutput::ImportParsed { path, bytes, parsed }) => {
                let media = match parsed {
                    ParsedImport::Svg(_) => "image/svg+xml",
                    ParsedImport::Dxf(_) => "image/vnd.dxf",
                };
                s.import = Some(PendingImport { file: file_title(&path), media, bytes, parsed, unit: None });
            }
            Err(TaskError::Parse(_) | TaskError::UnsupportedFormat) => {
                s.messages.push(StatusMessage { key: MessageKey::ImportFailed });
                update
                    .effects
                    .push(HostEffect::ShowError { document: Some(document), message: MessageKey::ImportFailed });
            }
            Ok(TaskOutput::Prepared { plan, rd, rd_hash }) => {
                if let Some(source) = s.preparing.take().filter(|p| p.id() == plan) {
                    let mut project = Project::new(s.uuid, (*s.doc.snapshot()).clone());
                    project.assets = s.assets.clone();
                    let inputs = JobInputs {
                        plan: Arc::clone(&source),
                        project: Arc::new(project),
                        project_path: s.path.clone(),
                    };
                    let job = PreparedJob {
                        plan,
                        plan_revision: source.source().document_revision,
                        profile: source.source().machine_profile.clone(),
                        rd_hash,
                        rd,
                        name: s.title.clone(),
                        inputs: Some(Arc::new(inputs)),
                    };
                    s.prepared = Some(Prepared { job: Arc::new(job), approved: false });
                }
            }
            Ok(TaskOutput::RecoverySaved { path }) => {
                s.autosaved = Some(outcome.source);
                s.recovery_path = Some(path);
            }
            Ok(TaskOutput::Opened { .. } | TaskOutput::RecoveryFound { .. } | TaskOutput::RecoveryRemoved { .. })
            | Err(TaskError::Cancelled) => {}
            Err(TaskError::Save(SaveError::Unconfirmed(_))) => {
                // The replace happened: this revision is on disk, only durability is unconfirmed.
                s.doc.mark_saved(outcome.source);
                s.messages.push(StatusMessage { key: MessageKey::SaveNotConfirmedDurable });
                if s.close_after_save && !s.doc.is_dirty() {
                    return self.close_session(document);
                }
            }
            Err(TaskError::Save(_)) => {
                s.close_after_save = false;
                s.messages.push(StatusMessage { key: MessageKey::SaveFailed });
                update
                    .effects
                    .push(HostEffect::ShowError { document: Some(document), message: MessageKey::SaveFailed });
            }
            Err(TaskError::Open(_)) => {}
        }
        update
    }

    fn opened(&mut self, document: DocumentId, outcome: TaskOutcome, restored: Option<PathBuf>) -> AppUpdate {
        match outcome.result {
            Ok(TaskOutput::Opened { project, path }) => {
                let Project { uuid, document: snapshot, assets, .. } = *project;
                let mut doc = DocumentSession::from_snapshot(document, snapshot);
                let session = match &restored {
                    // A recovered document is unsaved and untitled: Save asks for a location and
                    // never writes over the recovery file or any project file by itself.
                    Some(_) => {
                        let unsaved = RevisionId::from_raw(doc.revision().raw().wrapping_add(1));
                        doc.mark_saved(unsaved);
                        self.untitled += 1;
                        let revision = doc.revision();
                        Session::new(doc, format!("Hersteld {}", self.untitled), None, (uuid, assets)).map(|mut s| {
                            s.autosaved = Some(revision);
                            s.recovery_path = Some(path);
                            s
                        })
                    }
                    None => Session::new(doc, file_title(&path), Some(path), (uuid, assets)),
                };
                let Some(session) = session else {
                    return error(None, MessageKey::OpenFailed);
                };
                let mut session = session;
                session.fonts = self.fonts.as_ref().map(|(f, _)| f.clone());
                self.sessions.insert(document, session);
                self.active = Some(document);
                AppUpdate {
                    effects: vec![HostEffect::OpenWindow { document }],
                    view_changed: true,
                    scenes_changed: vec![document],
                }
            }
            Err(TaskError::Cancelled) => AppUpdate::default(),
            _ => error(None, MessageKey::OpenFailed),
        }
    }

    /// Reads the file on a worker; the window opens when the project has been read and checked.
    fn open(&mut self, path: PathBuf) -> AppUpdate {
        self.open_task(path).0
    }

    /// Submits the open task; returns its id when submitted.
    fn open_task(&mut self, path: PathBuf) -> (AppUpdate, Option<TaskId>) {
        let (Some(document), Some(id)) = (self.next_id::<DocumentId>(), self.next_id::<TaskId>()) else {
            return (error(None, MessageKey::OpenFailed), None);
        };
        let cancel = CancellationSource::new();
        let source = burnburn_types::RevisionId::INITIAL;
        let request = TaskRequest {
            id,
            document,
            source,
            key: revision_key(TaskKind::Open, document, source),
            cancel: cancel.token(),
            input: TaskInput::Open { path, fs: Arc::clone(&self.ports.fs) },
        };
        match self.ports.tasks.submit(request) {
            Ok(()) => {
                self.opening.insert(id, (document, cancel));
                (AppUpdate::default(), Some(id))
            }
            Err(_) => (error(None, MessageKey::OpenFailed), None),
        }
    }

    pub fn view(&self) -> Arc<AppViewModel> {
        let status = self.machine_port.as_ref().map(|p| p.status());
        let documents = self
            .sessions
            .iter()
            .map(|(id, s)| {
                let mut vm = document_view(*id, s, self.machine.as_ref());
                vm.job = s.prepared.as_ref().map(|p| JobCardViewModel {
                    plan: p.job.plan,
                    plan_revision: p.job.plan_revision,
                    rd_hash: p.job.rd_hash,
                    bytes: p.job.rd.len(),
                    approved: p.approved,
                    design_changed: p.job.plan_revision != s.doc.revision(),
                    upload: status.as_ref().map(|st| self.upload_button(st, p)),
                });
                if vm.job.as_ref().is_some_and(|j| j.design_changed) {
                    vm.messages.push(StatusMessage { key: MessageKey::DesignChangedSinceJob });
                }
                vm
            })
            .collect();
        let active = self.active.and_then(|id| self.sessions.get(&id));
        let commands = CommandKey::ALL
            .iter()
            .map(|k| {
                let mut state = command_state(*k, active, self.machine.as_ref());
                if self.role == Role::Operator && *k != CommandKey::Close {
                    state.enabled = false;
                }
                (*k, state)
            })
            .collect();
        Arc::new(AppViewModel { documents, active: self.active, commands })
    }

    /// The scene one window draws (contract §5): document revision, selection, gesture preview,
    /// snap target, issues and boolean preview, with the window's camera.
    pub fn scene(&self, document: DocumentId) -> Option<Arc<SceneSnapshot>> {
        let s = self.sessions.get(&document)?;
        let snapshot = s.doc.snapshot();
        let preview = s.editor.preview();
        let result_preview =
            s.boolean_preview.as_ref().filter(|b| b.revision == snapshot.revision()).map(|b| Arc::clone(&b.result));
        let mut scene = build_scene(&SceneRequest {
            document,
            snapshot: &snapshot,
            selection: &s.selection,
            tool: s.editor.tool(),
            preview: preview.as_ref(),
            snap: s.editor.snap(),
            issues: &s.issues,
            result_preview,
            camera: s.camera,
            viewport: s.viewport,
        });
        layers::style_scene(s, &snapshot, &mut scene);
        Some(Arc::new(scene))
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

    fn new_document(&mut self) -> AppUpdate {
        let Some(id) = self.next_id::<DocumentId>() else {
            return error(None, MessageKey::CommandRejected);
        };
        self.untitled += 1;
        let title = if self.untitled == 1 { "Naamloos".to_owned() } else { format!("Naamloos {}", self.untitled) };
        let uuid = (self.ports.new_project_uuid)();
        let Some(session) = Session::new(DocumentSession::new(id), title, None, (uuid, AssetStore::default())) else {
            return error(None, MessageKey::CommandRejected);
        };
        let mut session = session;
        session.fonts = self.fonts.as_ref().map(|(f, _)| f.clone());
        self.sessions.insert(id, session);
        self.active = Some(id);
        AppUpdate {
            effects: vec![HostEffect::OpenWindow { document: id }],
            view_changed: true,
            scenes_changed: vec![id],
        }
    }

    /// Save with a known or chosen path writes on a worker; an untitled document first asks the
    /// host for a location (native save sheet). The document stays dirty until the write succeeds.
    fn save(&mut self, document: DocumentId, path: Option<PathBuf>) -> AppUpdate {
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::SaveFailed) };
        let (tasks, fs) = (Arc::clone(&self.ports.tasks), Arc::clone(&self.ports.fs));
        self.with_session(document, |s| {
            let Some(path) = path.or_else(|| s.path.clone()) else {
                return AppUpdate {
                    effects: vec![HostEffect::ShowSavePanel {
                        document,
                        suggested_name: format!("{}.burnburn", s.title),
                    }],
                    ..AppUpdate::default()
                };
            };
            let snapshot = s.doc.snapshot();
            let source = snapshot.revision();
            let mut project = Project::new(s.uuid, (*snapshot).clone());
            project.assets = s.assets.clone();
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document,
                source,
                key: revision_key(TaskKind::Save, document, source),
                cancel: cancel.token(),
                input: TaskInput::Save { project: Box::new(project), path, fs },
            };
            match tasks.submit(request) {
                Ok(()) => {
                    s.pending.insert(id, (TaskKind::Save, cancel));
                    AppUpdate { view_changed: true, ..AppUpdate::default() }
                }
                Err(_) => error(Some(document), MessageKey::SaveFailed),
            }
        })
    }

    fn close(&mut self, document: DocumentId) -> AppUpdate {
        match self.sessions.get(&document) {
            Some(s) if s.doc.is_dirty() => {
                AppUpdate { effects: vec![HostEffect::ConfirmClose { document }], ..AppUpdate::default() }
            }
            Some(_) => self.close_session(document),
            None => AppUpdate::default(),
        }
    }

    fn close_session(&mut self, document: DocumentId) -> AppUpdate {
        let Some(mut s) = self.sessions.remove(&document) else { return AppUpdate::default() };
        s.cancel_all();
        if self.active == Some(document) {
            self.active = self.sessions.keys().next_back().copied();
        }
        AppUpdate {
            effects: vec![HostEffect::CloseWindow { document }],
            view_changed: true,
            scenes_changed: Vec::new(),
        }
    }

    fn invoke(&mut self, document: Option<DocumentId>, command: CommandKey) -> AppUpdate {
        if command == CommandKey::New {
            return self.new_document();
        }
        let Some(document) = document.or(self.active) else { return AppUpdate::default() };
        match command {
            CommandKey::Save | CommandKey::SaveAs => self.save(document, None),
            CommandKey::Close => self.close(document),
            CommandKey::Measure => self.submit_measure(document),
            CommandKey::Compile => self.submit_compile(document),
            CommandKey::PrepareJob => self.submit_prepare(document),
            CommandKey::ApproveJob => self.with_session(document, |s| match s.prepared.as_mut() {
                Some(p) => {
                    p.approved = true;
                    AppUpdate { view_changed: true, ..AppUpdate::default() }
                }
                None => AppUpdate::default(),
            }),
            CommandKey::ExportRd => match self.sessions.get(&document) {
                Some(s) if s.compiled.is_some() => AppUpdate {
                    effects: vec![HostEffect::ShowExportPanel { document, suggested_name: format!("{}.rd", s.title) }],
                    ..AppUpdate::default()
                },
                _ => AppUpdate::default(),
            },
            CommandKey::BooleanUnion => self.submit_boolean(document, BooleanOp::Union),
            CommandKey::BooleanDifference => self.submit_boolean(document, BooleanOp::Difference),
            CommandKey::BooleanIntersection => self.submit_boolean(document, BooleanOp::Intersection),
            _ => {
                let request = self.next_id::<RequestId>();
                self.with_session(document, |s| invoke_in_session(s, document, command, request))
            }
        }
    }

    fn submit_measure(&mut self, document: DocumentId) -> AppUpdate {
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::CommandRejected) };
        let tasks = Arc::clone(&self.ports.tasks);
        self.with_session(document, |s| {
            let source = s.doc.revision();
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document,
                source,
                key: revision_key(TaskKind::Measure, document, source),
                cancel: cancel.token(),
                input: TaskInput::Measure(s.doc.snapshot()),
            };
            match tasks.submit(request) {
                Ok(()) => {
                    s.pending.insert(id, (TaskKind::Measure, cancel));
                    AppUpdate { view_changed: true, ..AppUpdate::default() }
                }
                Err(_) => error(Some(document), MessageKey::CommandRejected),
            }
        })
    }

    /// Compiles the current revision for the active machine (B12). Revision-checked: a plan for
    /// an older revision is never shown as the current one.
    fn submit_compile(&mut self, document: DocumentId) -> AppUpdate {
        let Some(machine) = self.machine.clone() else { return error(Some(document), MessageKey::NoMachineProfile) };
        let Some(options) = compile_options() else { return error(Some(document), MessageKey::CompileFailed) };
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::CommandRejected) };
        let tasks = Arc::clone(&self.ports.tasks);
        self.with_session(document, |s| {
            let snapshot = s.doc.snapshot();
            let source = snapshot.revision();
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document,
                source,
                key: revision_key(TaskKind::Compile, document, source),
                cancel: cancel.token(),
                input: TaskInput::Compile { snapshot, profile: machine.profile, options },
            };
            submit(s, &tasks, request, cancel, document)
        })
    }

    /// C1: freezes the current plan — fresh for this revision and profile, without preflight
    /// errors — with its RD bytes. The job never follows later edits.
    fn submit_prepare(&mut self, document: DocumentId) -> AppUpdate {
        let Some(machine) = self.machine.clone() else { return error(Some(document), MessageKey::NoMachineProfile) };
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::PrepareFailed) };
        let tasks = Arc::clone(&self.ports.tasks);
        self.with_session(document, |s| {
            if !preparable(s, Some(&machine)) {
                return error(Some(document), MessageKey::PrepareFailed);
            }
            let Some(compiled) = s.compiled.as_ref() else { return AppUpdate::default() };
            let plan = Arc::clone(&compiled.plan);
            let source = plan.source().document_revision;
            s.preparing = Some(Arc::clone(&plan));
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document,
                source,
                key: revision_key(TaskKind::PrepareJob, document, source),
                cancel: cancel.token(),
                input: TaskInput::PrepareJob { plan, ruida: machine.ruida },
            };
            submit(s, &tasks, request, cancel, document)
        })
    }

    /// Writes exactly the current plan (the one the preview shows) as RD; the document is untouched.
    fn export_rd(&mut self, document: DocumentId, path: PathBuf) -> AppUpdate {
        let Some(machine) = self.machine.clone() else { return error(Some(document), MessageKey::NoMachineProfile) };
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::ExportFailed) };
        let (tasks, fs) = (Arc::clone(&self.ports.tasks), Arc::clone(&self.ports.fs));
        self.with_session(document, |s| {
            let Some(compiled) = s.compiled.as_ref() else { return AppUpdate::default() };
            let plan = Arc::clone(&compiled.plan);
            let source = plan.source().document_revision;
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document,
                source,
                key: revision_key(TaskKind::ExportRd, document, source),
                cancel: cancel.token(),
                input: TaskInput::ExportRd { plan, ruida: machine.ruida, path, fs },
            };
            submit(s, &tasks, request, cancel, document)
        })
    }

    fn open_trace_source(&mut self, document: DocumentId, path: PathBuf) -> AppUpdate {
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

    /// Every recipe change is a new trace task; the previous one is cancelled (best effort) and
    /// its result can no longer pass `check_fresh`.
    fn set_trace_recipe(&mut self, document: DocumentId, recipe: TraceRecipe) -> AppUpdate {
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::TraceFailed) };
        let tasks = Arc::clone(&self.ports.tasks);
        self.with_session(document, |s| {
            let revision = s.doc.revision();
            let Some(tracer) = s.tracer.as_mut() else { return AppUpdate::default() };
            tracer.recipe = Some(recipe.clone());
            tracer.latest = None;
            for (kind, older) in s.pending.values() {
                if *kind == TaskKind::Trace {
                    older.cancel();
                }
            }
            // T01: without a declared scale there is nothing physical to trace.
            if recipe.image.scale.is_none() {
                return AppUpdate { view_changed: true, ..AppUpdate::default() };
            }
            tracer.latest = Some(id);
            let trace =
                TraceRequest { task: id, source_revision: revision, source: Arc::clone(&tracer.source), recipe };
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document,
                source: revision,
                key: revision_key(TaskKind::Trace, document, revision),
                cancel: cancel.token(),
                input: TaskInput::Trace(Box::new(trace)),
            };
            submit(s, &tasks, request, cancel, document)
        })
    }

    /// Live boolean preview (E05): a revision-checked task; a newer request supersedes the older.
    fn submit_boolean(&mut self, document: DocumentId, op: BooleanOp) -> AppUpdate {
        let Some(id) = self.next_id::<TaskId>() else { return error(Some(document), MessageKey::CommandRejected) };
        let tasks = Arc::clone(&self.ports.tasks);
        self.with_session(document, |s| {
            let snapshot = s.doc.snapshot();
            let Ok(operands) = boolean_operands(&snapshot, &s.selection) else { return AppUpdate::default() };
            let source = snapshot.revision();
            let cancel = CancellationSource::new();
            let request = TaskRequest {
                id,
                document,
                source,
                key: revision_key(TaskKind::Boolean, document, source),
                cancel: cancel.token(),
                input: TaskInput::Boolean { snapshot, op, operands },
            };
            match tasks.submit(request) {
                Ok(()) => {
                    for (kind, older) in s.pending.values() {
                        if *kind == TaskKind::Boolean {
                            older.cancel();
                        }
                    }
                    s.pending.insert(id, (TaskKind::Boolean, cancel));
                    AppUpdate { view_changed: true, ..AppUpdate::default() }
                }
                Err(_) => error(Some(document), MessageKey::CommandRejected),
            }
        })
    }

    fn commit_field(&mut self, document: DocumentId, field: FieldKey, text: &str) -> AppUpdate {
        let request = self.next_id::<RequestId>();
        self.with_session(document, |s| {
            let Some(target) = s.selection.single() else { return AppUpdate::default() };
            if field == FieldKey::Rotation {
                return rotate_selection(s, document, target, text, request);
            }
            let parsed = match field {
                FieldKey::X | FieldKey::Y => parse_position(text),
                _ => parse_length(text),
            };
            let value = match parsed {
                Ok(v) => v,
                Err(e) => {
                    s.field_errors.insert(field, MessageKey::InvalidLength(e));
                    return AppUpdate { view_changed: true, ..AppUpdate::default() };
                }
            };
            s.field_errors.remove(&field);
            let snapshot = s.doc.snapshot();
            let Some(bounds) = snapshot.object(target).and_then(|o| o.bounds()) else { return AppUpdate::default() };
            let payload = match field {
                FieldKey::X | FieldKey::Y => {
                    let d = if field == FieldKey::X {
                        Vector::new(value.mm() - bounds.min().x, 0.0)
                    } else {
                        Vector::new(0.0, value.mm() - bounds.min().y)
                    };
                    // An unchanged value sends no command (phase-B decision for field commits).
                    if d.x == 0.0 && d.y == 0.0 {
                        return AppUpdate { view_changed: true, ..AppUpdate::default() };
                    }
                    let Ok(transform) = Affine::translate(d) else {
                        return error(Some(document), MessageKey::CommandRejected);
                    };
                    CommandPayload::Transform { targets: vec![target], transform }
                }
                _ => {
                    let axis = if field == FieldKey::Height { Axis::Height } else { Axis::Width };
                    CommandPayload::SetDimension { target, axis, value, anchor: Anchor::Min }
                }
            };
            let Some(request) = request else { return error(Some(document), MessageKey::CommandRejected) };
            let cmd = DocumentCommand { document, expected: s.doc.revision(), request, payload };
            let result = apply_command(&s.fonts, &mut s.doc, cmd);
            after_change(s, document, result)
        })
    }

    fn commit_tool_size(&mut self, document: DocumentId, width: &str, height: &str) -> AppUpdate {
        let request = self.next_id::<RequestId>();
        self.with_session(document, |s| {
            if !s.editor.is_gesture_active() {
                return AppUpdate::default();
            }
            let w = parse_length(width);
            let h = if height.trim().is_empty() { w } else { parse_length(height) };
            let (w, h) = match (w, h) {
                (Ok(w), Ok(h)) => (w, h),
                (Err(e), _) | (_, Err(e)) => {
                    // The gesture stays in preview; the drawing HUD shows the error.
                    s.tool_size_error = Some(MessageKey::InvalidLength(e));
                    return AppUpdate { view_changed: true, ..AppUpdate::default() };
                }
            };
            s.tool_size_error = None;
            let step = s.editor.exact_size(w, h);
            match request {
                Some(request) => handle_step(s, document, step, request),
                None => error(Some(document), MessageKey::CommandRejected),
            }
        })
    }

    fn pointer(&mut self, document: DocumentId, event: ViewPointerEvent) -> AppUpdate {
        let request = self.next_id::<RequestId>();
        self.with_session(document, |s| {
            // DR-04: the one place where canvas points become world millimetres.
            let position = s.camera.points_to_world(&s.viewport, event.position_pt);
            let ev = PointerEvent {
                pointer: event.pointer,
                phase: event.phase,
                button: event.button,
                position,
                modifiers: event.modifiers,
                click_count: event.click_count,
                time: event.time,
            };
            let Some(request) = request else { return error(Some(document), MessageKey::CommandRejected) };
            if s.trim_mode {
                return trim_at(s, document, &ev, request);
            }
            let alt_press = ev.phase == PointerPhase::Down && ev.modifiers.alt && s.editor.tool() == ToolKind::Select;
            if ev.phase == PointerPhase::Down && !alt_press {
                s.cycle = None; // any other press ends the "2 van 3" display
            }
            let update = run_tool(s, document, &CanvasInput::Pointer(ev), request);
            if alt_press {
                // "2 van 3": where the Alt-click cycle stands among the candidates under the pointer.
                let hits = hit_test(&s.doc.snapshot(), position, s.camera.mm_per_point() * CLICK_TOLERANCE_PT);
                s.cycle = s.selection.single().and_then(|sel| {
                    hits.iter().position(|h| *h == sel).map(|i| SelectionCycle { position: i + 1, count: hits.len() })
                });
            }
            update
        })
    }

    fn key(&mut self, document: DocumentId, key: CanvasKey, modifiers: Modifiers) -> AppUpdate {
        let drawing_points = self.sessions.get(&document).is_some_and(|s| {
            s.editor.is_gesture_active() && matches!(s.editor.tool(), ToolKind::Polyline | ToolKind::Pen)
        });
        let key = match key {
            // Delete removes the last placed point while drawing a polyline or pen path.
            CanvasKey::Delete if drawing_points => CanvasKey::Backspace,
            CanvasKey::Delete => return self.invoke(Some(document), CommandKey::Delete),
            other => other,
        };
        if key == CanvasKey::Escape
            && let Some(s) = self.sessions.get_mut(&document)
            && s.trim_mode
        {
            s.trim_mode = false;
            return AppUpdate::redraw(document);
        }
        let request = self.next_id::<RequestId>();
        self.with_session(document, |s| match request {
            Some(request) => {
                run_tool(s, document, &CanvasInput::Key { key, modifiers, phase: KeyPhase::Down }, request)
            }
            None => error(Some(document), MessageKey::CommandRejected),
        })
    }

    fn camera(&mut self, document: DocumentId, intent: CameraIntent) -> AppUpdate {
        self.with_session(document, |s| match intent {
            CameraIntent::Resized { viewport } => {
                s.viewport = viewport;
                AppUpdate::redraw(document)
            }
            // Camera pan/zoom is window state: it never becomes a document command.
            CameraIntent::Scroll { delta_pt } => {
                s.camera.pan_by_points(delta_pt);
                AppUpdate::redraw(document)
            }
            CameraIntent::Magnify { magnification, anchor_pt } => {
                match s.camera.zoom_at(&s.viewport, anchor_pt, 1.0 + magnification) {
                    Ok(()) => AppUpdate::redraw(document),
                    Err(_) => AppUpdate::default(),
                }
            }
        })
    }
}

/// Every document command goes through the evaluators, so derived geometry (sketches) follows
/// its definition in the same transaction and history entry (contract §11).
fn apply_command(
    fonts: &Option<SharedFonts>,
    doc: &mut DocumentSession,
    cmd: DocumentCommand,
) -> Result<CommandOutcome, CommandError> {
    let sketches = SketchEvaluator::new(SketchSolverAdapter);
    match fonts {
        Some(f) => doc.apply_with(cmd, &[&sketches, &TextEvaluator::new(f.clone())]),
        None => doc.apply_with(cmd, &[&sketches]),
    }
}

/// The attached font source, shared by every session's text evaluator.
#[derive(Clone)]
struct SharedFonts(Arc<dyn FontSource + Send + Sync>);

impl FontSource for SharedFonts {
    fn load(&self, font: &FontRef) -> Result<Arc<FontData>, FontError> {
        self.0.load(font)
    }
}

fn compile_options() -> Option<CompileOptions> {
    let tolerance = ProductionTolerance::new(Length::from_mm(COMPILE_TOLERANCE_MM).ok()?).ok()?;
    Some(CompileOptions { tolerance, placement: Placement::Absolute, start: StartPoint::MachineOrigin })
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
            let mut focus: Vec<_> = outcome.changes.created.clone();
            focus.extend(outcome.changes.modified.iter().copied());
            if !focus.is_empty() {
                s.selection.apply(&SelectionChange::Replace(focus));
            }
            let snapshot = s.doc.snapshot();
            s.selection.retain_existing(&snapshot);
            s.issues = geometry_issues(&snapshot);
            s.cycle = None;
            s.messages.clear();
            s.problem = None;
            layers::sync(s);
            AppUpdate::redraw(document)
        }
        Err(_) => {
            s.messages.push(StatusMessage { key: MessageKey::CommandRejected });
            let mut update = error(Some(document), MessageKey::CommandRejected);
            update.view_changed = true;
            update
        }
    }
}

fn parse_speed(text: &str) -> Result<Speed, MessageKey> {
    let invalid = MessageKey::InvalidNumber(OperationField::Speed);
    parse_number(text, &["mm/s"]).ok().and_then(|v| Speed::mm_per_s(v).ok()).ok_or(invalid)
}

fn parse_power(text: &str) -> Result<PowerRange, MessageKey> {
    let invalid = MessageKey::InvalidNumber(OperationField::Power);
    let p = parse_number(text, &["%"]).ok().and_then(|v| PowerPercent::new(v).ok()).ok_or(invalid)?;
    PowerRange::new(p, p).map_err(|_| invalid)
}

fn refused(s: &mut Session, key: MessageKey) -> AppUpdate {
    s.operation_error = Some(key);
    AppUpdate { view_changed: true, ..AppUpdate::default() }
}

/// Operations panel: a new Line or Fill operation for the selection (one command).
fn add_operation_text(
    s: &mut Session,
    document: DocumentId,
    name: String,
    kind: OperationKind,
    speed: &str,
    power: &str,
    request: Option<RequestId>,
) -> AppUpdate {
    let speed = match parse_speed(speed) {
        Ok(v) => v,
        Err(k) => return refused(s, k),
    };
    let power = match parse_power(power) {
        Ok(v) => v,
        Err(k) => return refused(s, k),
    };
    let mode = match kind {
        OperationKind::Line => OperationMode::Line { kerf: Kerf::NONE },
        OperationKind::Fill => {
            let fill = Length::from_mm(DEFAULT_FILL_INTERVAL_MM).ok().zip(Length::from_mm(0.0).ok()).and_then(
                |(interval, overscan)| {
                    FillSettings::new(interval, ScanAxis::X, ScanDirection::Bidirectional, overscan).ok()
                },
            );
            match fill {
                Some(f) => OperationMode::Fill(f),
                None => return refused(s, MessageKey::CommandRejected),
            }
        }
        _ => return refused(s, MessageKey::CommandRejected),
    };
    let Ok(passes) = Passes::new(1) else { return refused(s, MessageKey::CommandRejected) };
    let draft = OperationDraft {
        name,
        mode,
        settings: ProcessSettings { speed, power, passes, air_assist: true },
        output_enabled: true,
    };
    s.operation_error = None;
    let targets = s.selection.ids().to_vec();
    apply_edit(s, document, CommandPayload::Process(ProcessCommand::AddOperation { draft, targets }), request)
}

/// Operations panel: one field of an operation (one `UpdateOperation`; unchanged = nothing).
fn set_operation_field(
    s: &mut Session,
    document: DocumentId,
    operation: OperationId,
    field: OperationField,
    text: &str,
    request: Option<RequestId>,
) -> AppUpdate {
    let snapshot = s.doc.snapshot();
    let Some(op) = snapshot.process().operation(operation) else { return AppUpdate::default() };
    let mut draft = op.draft();
    let changed = match field {
        OperationField::Name => {
            let name = text.trim();
            let changed = !name.is_empty() && name != draft.name;
            if changed {
                draft.name = name.to_owned();
            }
            Ok(changed)
        }
        OperationField::Speed => parse_speed(text).map(|v| {
            let changed = v != draft.settings.speed;
            draft.settings.speed = v;
            changed
        }),
        OperationField::Power => parse_power(text).map(|v| {
            let changed = v != draft.settings.power;
            draft.settings.power = v;
            changed
        }),
        OperationField::PowerMin | OperationField::PowerMax => {
            let min = field == OperationField::PowerMin;
            layers::parse_percent(text, field).and_then(|p| {
                let (lo, hi) = if min { (p, draft.settings.power.max()) } else { (draft.settings.power.min(), p) };
                let range = PowerRange::new(lo, hi).map_err(|_| MessageKey::InvalidNumber(field))?;
                let changed = range != draft.settings.power;
                draft.settings.power = range;
                Ok(changed)
            })
        }
        OperationField::Passes => layers::parse_passes(text).map(|v| {
            let changed = v != draft.settings.passes;
            draft.settings.passes = v;
            changed
        }),
        OperationField::Kerf => match draft.mode {
            OperationMode::Line { kerf } => layers::parse_kerf(text).map(|v| {
                draft.mode = OperationMode::Line { kerf: v };
                v != kerf
            }),
            _ => Ok(false),
        },
    };
    match changed {
        Err(k) => refused(s, k),
        Ok(false) => {
            s.operation_error = None;
            AppUpdate { view_changed: true, ..AppUpdate::default() }
        }
        Ok(true) => {
            s.operation_error = None;
            let edit = ProcessCommand::UpdateOperation { operation, draft };
            apply_edit(s, document, CommandPayload::Process(edit), request)
        }
    }
}

/// Rotation field: turns `target` about its bounds centre by the typed angle (one Transform).
fn rotate_selection(
    s: &mut Session,
    document: DocumentId,
    target: GeometryId,
    text: &str,
    request: Option<RequestId>,
) -> AppUpdate {
    let degrees = match parse_angle(text) {
        Ok(d) => d,
        Err(e) => {
            s.field_errors.insert(FieldKey::Rotation, MessageKey::InvalidLength(e));
            return AppUpdate { view_changed: true, ..AppUpdate::default() };
        }
    };
    s.field_errors.remove(&FieldKey::Rotation);
    if degrees.rem_euclid(360.0) == 0.0 {
        return AppUpdate { view_changed: true, ..AppUpdate::default() };
    }
    let snapshot = s.doc.snapshot();
    let Some(centre) = snapshot.object(target).and_then(|o| o.bounds()).map(|b| b.center()) else {
        return AppUpdate::default();
    };
    let Ok(angle) = Angle::from_degrees(degrees) else { return error(Some(document), MessageKey::CommandRejected) };
    let to_origin = Affine::translate(Vector::new(-centre.x, -centre.y));
    let back = Affine::translate(Vector::new(centre.x, centre.y));
    let (Ok(to_origin), Ok(back)) = (to_origin, back) else {
        return error(Some(document), MessageKey::CommandRejected);
    };
    let transform = to_origin.then(&Affine::rotate(angle)).then(&back);
    apply_edit(s, document, CommandPayload::Transform { targets: vec![target], transform }, request)
}

/// Applies one payload as one command against the current revision.
fn apply_edit(s: &mut Session, document: DocumentId, payload: CommandPayload, request: Option<RequestId>) -> AppUpdate {
    let Some(request) = request else { return error(Some(document), MessageKey::CommandRejected) };
    let cmd = DocumentCommand { document, expected: s.doc.revision(), request, payload };
    let result = apply_command(&s.fonts, &mut s.doc, cmd);
    after_change(s, document, result)
}

fn invoke_in_session(
    s: &mut Session,
    document: DocumentId,
    command: CommandKey,
    request: Option<RequestId>,
) -> AppUpdate {
    let align_edge = match command {
        CommandKey::AlignLeft => Some(AlignEdge::Left),
        CommandKey::AlignRight => Some(AlignEdge::Right),
        CommandKey::AlignTop => Some(AlignEdge::Top),
        CommandKey::AlignBottom => Some(AlignEdge::Bottom),
        CommandKey::AlignCenterX => Some(AlignEdge::CenterX),
        CommandKey::AlignCenterY => Some(AlignEdge::CenterY),
        _ => None,
    };
    let direction = match command {
        CommandKey::DistributeHorizontal => Some(Direction::Horizontal),
        CommandKey::DistributeVertical => Some(Direction::Vertical),
        _ => None,
    };
    let targets = s.selection.ids().to_vec();
    let arrange = match (align_edge, direction, s.selection.reference()) {
        // E04: the reference (first selected) stays in place and is visible in the view model.
        (Some(edge), _, Some(reference)) if targets.len() >= 2 => {
            Some(GeometryCommand::Align { targets, edge, to: AlignTarget::Object(reference) })
        }
        (_, Some(direction), _) if targets.len() >= 3 => {
            Some(GeometryCommand::Distribute { targets, direction, spacing: Spacing::Gaps })
        }
        (Some(_), ..) | (_, Some(_), _) => return AppUpdate::default(),
        _ => None,
    };
    if let Some(edit) = arrange {
        // Selection order (and so the reference) is kept across the change.
        let keep = s.selection.clone();
        let update = apply_edit(s, document, CommandPayload::Edit(edit), request);
        s.selection = keep;
        s.selection.retain_existing(&s.doc.snapshot());
        return update;
    }
    match command {
        CommandKey::Undo | CommandKey::Redo => {
            // A gesture in progress is cancelled first; it never reaches history (A4).
            s.editor.cancel();
            let expected = s.doc.revision();
            let result = if command == CommandKey::Undo { s.doc.undo(expected) } else { s.doc.redo(expected) };
            match result {
                Err(CommandError::NothingToUndo | CommandError::NothingToRedo) => AppUpdate::redraw(document),
                other => after_change(s, document, other),
            }
        }
        CommandKey::Delete => {
            if s.selection.is_empty() {
                return AppUpdate::default();
            }
            let payload = CommandPayload::Delete { targets: s.selection.ids().to_vec() };
            apply_edit(s, document, payload, request)
        }
        CommandKey::SelectAll => {
            let all = s.doc.snapshot().objects().map(|(id, _)| id).collect();
            apply_selection(s, &SelectionChange::Replace(all));
            AppUpdate::redraw(document)
        }
        CommandKey::SnapToObjects => {
            s.snap.objects = !s.snap.objects;
            AppUpdate::redraw(document)
        }
        CommandKey::SnapToGrid => {
            s.snap.grid_mm = if s.snap.grid_mm.is_some() { None } else { Some(DEFAULT_SNAP_GRID_MM) };
            AppUpdate::redraw(document)
        }
        CommandKey::ApplyTrace => {
            let Some(t) = s.tracer.as_ref() else { return AppUpdate::default() };
            let Some(result) = t.result.clone() else { return AppUpdate::default() };
            // The source image travels with the project; objects keep it as provenance.
            let asset = s.assets.add(t.media, &t.bytes);
            let items: Vec<ImportItem> = (0..result.components.len())
                .filter_map(|i| {
                    let path = result.component_path(i)?;
                    Some(ImportItem {
                        name: format!("Trace {}", i + 1),
                        entity: Some(format!("trace/component/{i}")),
                        path,
                    })
                })
                .collect();
            apply_edit(s, document, CommandPayload::Edit(GeometryCommand::Import { asset, items }), request)
        }
        CommandKey::CommitImport => {
            let Some(pending) = s.import.as_ref() else { return AppUpdate::default() };
            let Ok(draft) = pending.draft() else { return AppUpdate::default() };
            let asset = s.assets.add(pending.media, &pending.bytes);
            let Ok(edit) = draft.into_command(asset) else { return error(Some(document), MessageKey::ImportFailed) };
            let update = apply_edit(s, document, CommandPayload::Edit(edit), request);
            if s.doc.undo_label() == Some(burnburn_document::UndoLabel::Edit(burnburn_document::EditLabel::Import)) {
                s.import = None;
            }
            update
        }
        CommandKey::CancelImport => {
            s.import = None;
            AppUpdate { view_changed: true, ..AppUpdate::default() }
        }
        CommandKey::CloseTracer => {
            for (kind, src) in s.pending.values() {
                if matches!(kind, TaskKind::Trace | TaskKind::LoadImage) {
                    src.cancel();
                }
            }
            s.tracer = None;
            AppUpdate::redraw(document)
        }
        CommandKey::ApplyPreview => {
            let Some(preview) = current_boolean_preview(s).cloned() else { return AppUpdate::default() };
            let Some(options) = boolean_options() else { return error(Some(document), MessageKey::BooleanFailed) };
            let edit =
                GeometryCommand::Boolean { op: preview.op, targets: preview.operands, options, keep_sources: false };
            s.boolean_preview = None;
            apply_edit(s, document, CommandPayload::Edit(edit), request)
        }
        CommandKey::ToolTrim => {
            s.editor.cancel();
            s.editor.select_tool(ToolKind::Select);
            s.trim_mode = true;
            AppUpdate::redraw(document)
        }
        other => match tool_for(other) {
            Some(tool) => {
                s.trim_mode = false;
                s.editor.select_tool(tool);
                s.tool_size_error = None;
                AppUpdate::redraw(document)
            }
            None => AppUpdate::default(),
        },
    }
}

fn tool_for(command: CommandKey) -> Option<ToolKind> {
    Some(match command {
        CommandKey::ToolSelect => ToolKind::Select,
        CommandKey::ToolNode => ToolKind::Node,
        CommandKey::ToolRectangle => ToolKind::Rectangle,
        CommandKey::ToolEllipse => ToolKind::Ellipse,
        CommandKey::ToolCircle => ToolKind::Circle,
        CommandKey::ToolLine => ToolKind::Line,
        CommandKey::ToolPolyline => ToolKind::Polyline,
        CommandKey::ToolPen => ToolKind::Pen,
        _ => return None,
    })
}

fn current_boolean_preview(s: &Session) -> Option<&BooleanPreview> {
    s.boolean_preview.as_ref().filter(|b| b.revision == s.doc.revision())
}

/// Builds the tool context for one input and processes the resulting step.
fn run_tool(s: &mut Session, document: DocumentId, input: &CanvasInput, request: RequestId) -> AppUpdate {
    let snapshot = s.doc.snapshot();
    let ctx = ToolContext {
        document: &snapshot,
        selection: &s.selection,
        mm_per_point: s.camera.mm_per_point(),
        snap: &s.snap,
    };
    let step = s.editor.input(input, &ctx);
    handle_step(s, document, step, request)
}

fn handle_step(s: &mut Session, document: DocumentId, step: ToolStep, request: RequestId) -> AppUpdate {
    match step {
        ToolStep::Nothing => AppUpdate::default(),
        ToolStep::Select(change) => {
            apply_selection(s, &change);
            s.cycle = None;
            AppUpdate::redraw(document)
        }
        ToolStep::Commit(edit) => {
            let kind = name_kind(&edit);
            let name = kind.map(|k| s.next_name(k)).unwrap_or_default();
            let Some(payload) = payload(edit, name) else { return AppUpdate::redraw(document) };
            let cmd = DocumentCommand { document, expected: s.doc.revision(), request, payload };
            let result = apply_command(&s.fonts, &mut s.doc, cmd);
            if let (Some(k), true) = (kind, result.is_ok()) {
                *s.created.entry(k).or_insert(0) += 1;
            }
            after_change(s, document, result)
        }
        _ => AppUpdate::redraw(document),
    }
}

fn document_view(id: DocumentId, s: &Session, machine: Option<&Machine>) -> DocumentViewModel {
    let snapshot = s.doc.snapshot();
    let objects = snapshot
        .objects()
        .map(|(gid, obj)| ObjectRow {
            id: gid,
            label: obj.name().to_owned(),
            depth: 0,
            selected: s.selection.contains(gid),
            visible: true,
        })
        .collect();
    let inspector = s.selection.single().and_then(|gid| snapshot.object(gid).map(|o| (gid, o))).and_then(|(gid, o)| {
        let b = o.bounds()?;
        let field = |key: FieldKey, mm: f64| FieldViewModel {
            key,
            label: MessageKey::Field(key),
            text: format_mm(mm),
            unit: Unit::Millimeter,
            editable: true,
            error: s.field_errors.get(&key).copied(),
        };
        Some(InspectorViewModel {
            object: gid,
            fields: vec![
                field(FieldKey::X, b.min().x),
                field(FieldKey::Y, b.min().y),
                field(FieldKey::Width, b.width()),
                field(FieldKey::Height, b.height()),
                FieldViewModel { text: "0".into(), ..field(FieldKey::Rotation, 0.0) },
            ],
        })
    });
    DocumentViewModel {
        id,
        revision: s.doc.revision(),
        title: s.title.clone(),
        dirty: s.doc.is_dirty(),
        tool: s.editor.tool(),
        preview: s.editor.preview(),
        objects,
        selection: s.selection.ids().to_vec(),
        inspector,
        undo: s.doc.undo_label(),
        redo: s.doc.redo_label(),
        messages: s.messages.clone(),
        tasks: s.pending.iter().map(|(tid, (kind, _))| TaskBadge { id: *tid, kind: *kind }).collect(),
        measure: s.measure.clone(),
        camera: s.camera,
        viewport: s.viewport,
        snap: s.editor.snap(),
        snap_settings: s.snap,
        selection_cycle: s.cycle,
        align_reference: (s.selection.len() >= 2).then(|| s.selection.reference()).flatten(),
        issues: s.issues.clone(),
        tool_size_error: s.tool_size_error,
        boolean_preview: current_boolean_preview(s).cloned(),
        sketches: sketch_rows(&snapshot),
        plan: s.compiled.as_ref().map(|c| plan_view(c, s, machine)),
        export: s.export.clone(),
        tracer: s.tracer.as_ref().map(|t| tracer_view(t, s)),
        job: None,
        problem: s.problem.clone(),
        operations: snapshot.process().operations().map(|op| layers::operation_row(s, op)).collect(),
        operation_error: s.operation_error,
        cut_settings: layers::cut_settings_view(s, machine.map(|m| m.profile.capabilities())),
        import: s.import.as_ref().map(import_view),
    }
}

fn import_view(i: &PendingImport) -> ImportViewModel {
    let draft = i.draft();
    let size_mm = draft.as_ref().ok().and_then(|d| {
        d.items.iter().filter_map(|it| it.path.bounds()).reduce(|a, b| a.union(b)).map(|b| (b.width(), b.height()))
    });
    let (skipped, open_contours, units, items) = match (&draft, &i.parsed) {
        (Ok(d), _) => (d.report.skipped.clone(), d.report.open_contours.len(), Some(d.report.units), d.items.len()),
        // Unitless DXF: the report is known only in drawing units; entity counts still apply.
        (Err(_), ParsedImport::Dxf(doc)) => match doc.to_draft(DxfScale::mm_per_unit(1.0)) {
            Ok(d) => (d.report.skipped, d.report.open_contours.len(), None, d.items.len()),
            Err(_) => (Vec::new(), 0, None, 0),
        },
        (Err(_), _) => (Vec::new(), 0, None, 0),
    };
    ImportViewModel {
        file: i.file.clone(),
        needs_unit: matches!(draft, Err(ImportError::ScaleRequired)),
        unit: i.unit,
        units,
        size_mm,
        items,
        skipped,
        open_contours,
    }
}

fn tracer_view(t: &Tracer, s: &Session) -> TracerViewModel {
    let computing = s.pending.values().any(|(kind, _)| *kind == TaskKind::Trace);
    let result = t.result.as_ref().map(|r| {
        let path = r.path();
        let b = path.bounds();
        TraceSummary {
            components: r.stats.closed_shapes,
            holes: r.stats.holes,
            width_mm: b.map_or(0.0, |b| b.width()),
            height_mm: b.map_or(0.0, |b| b.height()),
            mm_per_pixel: r.stats.source_mm_per_pixel,
            fit_tolerance_mm: t.recipe.as_ref().map_or(0.0, |rc| rc.fit.tolerance_mm),
            preview: Arc::new(path),
        }
    });
    TracerViewModel {
        width_px: t.source.width(),
        height_px: t.source.height(),
        needs_scale: t.recipe.as_ref().is_none_or(|r| r.image.scale.is_none()),
        computing,
        result,
    }
}

fn plan_view(c: &Compiled, s: &Session, machine: Option<&Machine>) -> PlanViewModel {
    let source = c.plan.source();
    let profile_changed = machine.is_none_or(|m| *m.profile.revision() != source.machine_profile);
    let errors = c.report.findings.iter().filter(|f| f.severity() == Severity::Error).count();
    PlanViewModel {
        id: c.plan.id(),
        revision: source.document_revision,
        stale: source.document_revision != s.doc.revision() || profile_changed,
        preview: Arc::clone(&c.preview),
        findings: c.report.findings.clone(),
        errors,
        warnings: c.report.findings.len() - errors,
    }
}

fn sketch_rows(snapshot: &burnburn_document::DocumentSnapshot) -> Vec<SketchViewModel> {
    snapshot
        .sketch_layer()
        .sketches()
        .map(|(id, sketch)| {
            let record = snapshot.derived_record(DerivedOwner::Sketch(id)).map(|r| &r.status);
            let status = match record {
                Some(EvalStatus::Valid) => SketchStatus::Valid,
                Some(EvalStatus::Failed { .. }) => SketchStatus::Failed,
                _ => SketchStatus::Pending,
            };
            let problem_constraints = match record {
                Some(EvalStatus::Failed {
                    diagnosis:
                        EvalDiagnosis::Conflict { constraints }
                        | EvalDiagnosis::InvalidDimensions { constraints }
                        | EvalDiagnosis::NotConverged { unmet: constraints },
                }) => constraints.clone(),
                _ => Vec::new(),
            };
            let parameters = sketch.parameters().iter().map(|(n, t)| (n.to_owned(), t.to_owned())).collect();
            SketchViewModel {
                id,
                name: sketch.name().to_owned(),
                status,
                parameters,
                constraints: sketch.constraints().len(),
                dof: sketch.solution().filter(|_| status == SketchStatus::Valid).map(|sol| sol.dof),
                problem_constraints,
                last_constraint: sketch.constraints().map(|(c, _)| c).max(),
            }
        })
        .collect()
}

fn command_state(key: CommandKey, s: Option<&Session>, machine: Option<&Machine>) -> CommandState {
    let has_machine = machine.is_some();
    let label = match (key, s) {
        (CommandKey::Undo, Some(s)) => s.doc.undo_label().map_or(MessageKey::Command(key), MessageKey::UndoAction),
        (CommandKey::Redo, Some(s)) => s.doc.redo_label().map_or(MessageKey::Command(key), MessageKey::RedoAction),
        _ => MessageKey::Command(key),
    };
    let (enabled, checked) = match (key, s) {
        (CommandKey::New, _) => (true, false),
        (_, None) => (false, false),
        (CommandKey::Undo, Some(s)) => (s.doc.can_undo(), false),
        (CommandKey::Redo, Some(s)) => (s.doc.can_redo(), false),
        (CommandKey::Delete, Some(s)) => (!s.selection.is_empty(), false),
        (CommandKey::SelectAll, Some(s)) => (!s.doc.snapshot().is_empty(), false),
        (CommandKey::SnapToObjects, Some(s)) => (true, s.snap.objects),
        (CommandKey::SnapToGrid, Some(s)) => (true, s.snap.grid_mm.is_some()),
        (
            CommandKey::AlignLeft
            | CommandKey::AlignRight
            | CommandKey::AlignTop
            | CommandKey::AlignBottom
            | CommandKey::AlignCenterX
            | CommandKey::AlignCenterY,
            Some(s),
        ) => (s.selection.len() >= 2, false),
        (CommandKey::DistributeHorizontal | CommandKey::DistributeVertical, Some(s)) => (s.selection.len() >= 3, false),
        (CommandKey::BooleanUnion | CommandKey::BooleanDifference | CommandKey::BooleanIntersection, Some(s)) => {
            (boolean_operands(&s.doc.snapshot(), &s.selection).is_ok(), false)
        }
        (CommandKey::ApplyPreview, Some(s)) => (current_boolean_preview(s).is_some(), false),
        (CommandKey::Compile, Some(s)) => (has_machine && !s.doc.snapshot().process().is_empty(), false),
        (CommandKey::ExportRd, Some(s)) => (has_machine && s.compiled.is_some(), false),
        (CommandKey::ApplyTrace, Some(s)) => (s.tracer.as_ref().is_some_and(|t| t.result.is_some()), false),
        (CommandKey::CloseTracer, Some(s)) => (s.tracer.is_some(), false),
        (CommandKey::PrepareJob, Some(s)) => (preparable(s, machine), false),
        (CommandKey::ApproveJob, Some(s)) => (s.prepared.as_ref().is_some_and(|p| !p.approved), false),
        (CommandKey::ToolTrim, Some(s)) => (true, s.trim_mode),
        (CommandKey::CommitImport, Some(s)) => (s.import.as_ref().is_some_and(|i| i.draft().is_ok()), false),
        (CommandKey::CancelImport, Some(s)) => (s.import.is_some(), false),
        (other, Some(s)) => match tool_for(other) {
            Some(tool) => (true, !s.trim_mode && s.editor.tool() == tool),
            None => (true, false),
        },
    };
    CommandState { enabled, checked, label }
}

/// A plan can be frozen when it is fresh for this revision and profile and has no errors.
fn preparable(s: &Session, machine: Option<&Machine>) -> bool {
    s.compiled.as_ref().is_some_and(|c| {
        let view = plan_view(c, s, machine);
        !view.stale && view.errors == 0
    })
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
        | UiIntent::CommitField { document, .. }
        | UiIntent::CommitToolSize { document, .. }
        | UiIntent::AddOperation { document, .. }
        | UiIntent::ExportRd { document, .. }
        | UiIntent::OpenTraceSource { document, .. }
        | UiIntent::SetTraceRecipe { document, .. }
        | UiIntent::SetSketchParameter { document, .. }
        | UiIntent::Pointer { document, .. }
        | UiIntent::Key { document, .. }
        | UiIntent::Camera { document, .. } => Some(*document),
        UiIntent::Invoke { document, .. } | UiIntent::Machine { document, .. } => *document,
        _ => None,
    }
}

/// Machine panel from the service status. Status and controls follow evidence only.
fn machine_view_of(status: &MachineStatus) -> MachineViewModel {
    let current = status.current.as_ref();
    let mut buttons = Vec::new();
    let mut seen = None;
    if let Some(record) = current {
        let e = &record.execution;
        let subject = ActionSubject::Execution(e.id);
        seen = Some(e.state.clone());
        let live = !e.state.is_terminal();
        let mut add = |kind: MachineAction, enabled: bool| {
            buttons.push(MachineButton {
                action: status.action_id(kind, subject),
                enabled,
                seen: Some(e.state.clone()),
                label: MessageKey::MachineAction(kind),
            });
        };
        add(MachineAction::ConfirmStored, e.state == ExecutionState::UploadAcked);
        add(MachineAction::Start, e.state == ExecutionState::Stored && !e.start_sent);
        add(MachineAction::QueryStatus, live);
        add(MachineAction::Cancel, live);
        if matches!(e.state, ExecutionState::Unknown(_)) {
            for outcome in Outcome::ALL {
                add(MachineAction::RecordOutcome(outcome), reconcilable(e));
            }
        }
    }
    buttons.push(MachineButton {
        action: status.action_id(MachineAction::Reconnect, ActionSubject::Machine),
        enabled: true,
        seen,
        label: MessageKey::MachineAction(MachineAction::Reconnect),
    });
    let (status_key, message, progress) = match current.map(|r| &r.execution) {
        None => (MachineStatusKey::Idle, None, None),
        Some(e) => match &e.state {
            ExecutionState::VerifyingIdentity => (MachineStatusKey::VerifyingIdentity, None, None),
            ExecutionState::Reverifying => (MachineStatusKey::Reverifying, None, None),
            ExecutionState::Uploading { acked, total } => (MachineStatusKey::Uploading, None, Some((*acked, *total))),
            ExecutionState::UploadAcked => {
                let total = current.map_or(0, |r| {
                    r.execution.evidence.iter().filter(|x| matches!(x.evidence, Evidence::ChunkAcked { .. })).count()
                });
                (
                    MachineStatusKey::UploadedNotConfirmed,
                    Some(MessageKey::UploadedNotStoredConfirmed),
                    Some((total, total)),
                )
            }
            ExecutionState::Stored => (MachineStatusKey::Stored, None, None),
            ExecutionState::StartRequested => {
                (MachineStatusKey::StartRequested, Some(MessageKey::StartRequestedAwaitingStatus), None)
            }
            ExecutionState::Running => (MachineStatusKey::Running, None, None),
            ExecutionState::Completed => (MachineStatusKey::Completed, None, None),
            ExecutionState::Unknown(reason) => (
                MachineStatusKey::Unknown,
                Some(match reason {
                    UnknownReason::StopRequested => MessageKey::StopSentOutcomeUnknown,
                    UnknownReason::UploadChunkUnconfirmed { .. } => MessageKey::UploadUnconfirmedCheckController,
                    _ => MessageKey::StatusUnknownCheckMachine,
                }),
                None,
            ),
            ExecutionState::Failed(reason) => (
                MachineStatusKey::Failed,
                Some(match reason {
                    FailReason::UploadRejected { .. } => MessageKey::UploadRejected,
                    FailReason::NotStored => MessageKey::NotStored,
                    FailReason::IdentityMismatch => MessageKey::IdentityMismatch,
                    FailReason::IdentityUnconfirmed => MessageKey::IdentityUnconfirmed,
                    _ => MessageKey::ControllerFault,
                }),
                None,
            ),
            ExecutionState::Cancelled => {
                (MachineStatusKey::Cancelled, file_may_remain(e).then_some(MessageKey::ControllerFileMayRemain), None)
            }
        },
    };
    MachineViewModel {
        machine: status.machine.clone(),
        status: status_key,
        message,
        execution: current.map(|r| r.execution.id),
        job: current.map(|r| r.job.plan),
        job_name: current.map(|r| r.job.name.clone()),
        progress,
        buttons,
        last_action: status.last_action.as_ref().map(|h| (h.action, h.result.is_ok())),
    }
}

/// Chord tolerance for offsets applied from the panel (production geometry).
const OFFSET_TOLERANCE_MM: f64 = 0.01;

fn located(
    s: &mut Session,
    document: DocumentId,
    message: MessageKey,
    object: Option<GeometryId>,
    at: Point,
) -> AppUpdate {
    s.problem = Some(ProblemViewModel { message, object, at });
    let mut update = error(Some(document), message);
    update.view_changed = true;
    update.scenes_changed.push(document);
    update
}

fn centre_of(snapshot: &burnburn_document::DocumentSnapshot, id: GeometryId) -> Point {
    snapshot
        .object(id)
        .and_then(|o| o.bounds())
        .map_or(Point::new(0.0, 0.0), |b| Point::new((b.min().x + b.max().x) / 2.0, (b.min().y + b.max().y) / 2.0))
}

/// E05 offset panel: one command for the whole selection (one undo entry, sources kept). When it
/// fails, each target is tried alone on a scratch copy to name the object that cannot be offset.
fn offset_selection(s: &mut Session, document: DocumentId, text: &str, request: Option<RequestId>) -> AppUpdate {
    let targets = s.selection.ids().to_vec();
    let Some(first) = targets.first().copied() else { return AppUpdate::default() };
    let snapshot = s.doc.snapshot();
    let distance = match parse_position(text) {
        Ok(d) if d.mm() != 0.0 => d,
        Ok(_) => return AppUpdate::default(),
        Err(e) => {
            let at = centre_of(&snapshot, first);
            s.problem = Some(ProblemViewModel { message: MessageKey::InvalidLength(e), object: None, at });
            return AppUpdate { view_changed: true, ..AppUpdate::default() };
        }
    };
    let Some(tolerance) = Length::from_mm(OFFSET_TOLERANCE_MM).ok().and_then(|l| ProductionTolerance::new(l).ok())
    else {
        return error(Some(document), MessageKey::CommandRejected);
    };
    let edit = |targets: Vec<GeometryId>| {
        CommandPayload::Edit(GeometryCommand::Offset {
            targets,
            distance,
            join: OffsetJoin::Round,
            tolerance,
            keep_sources: true,
        })
    };
    let Some(request) = request else { return error(Some(document), MessageKey::CommandRejected) };
    let cmd = DocumentCommand { document, expected: s.doc.revision(), request, payload: edit(targets.clone()) };
    match apply_command(&s.fonts, &mut s.doc, cmd) {
        Err(CommandError::Edit(EditError::Offset(_))) => {
            let culprit = targets.iter().copied().find(|id| {
                let mut scratch = DocumentSession::from_snapshot(document, (*snapshot).clone());
                let probe =
                    DocumentCommand { document, expected: scratch.revision(), request, payload: edit(vec![*id]) };
                apply_command(&s.fonts, &mut scratch, probe).is_err()
            });
            let at = centre_of(&snapshot, culprit.unwrap_or(first));
            located(s, document, MessageKey::OffsetImpossible, culprit, at)
        }
        other => after_change(s, document, other),
    }
}

/// E09: close the gaps of one chosen problem with its own tolerance (one undo entry).
fn repair_issue(
    s: &mut Session,
    document: DocumentId,
    issue: usize,
    text: &str,
    request: Option<RequestId>,
) -> AppUpdate {
    let Some(chosen) = s.issues.get(issue).cloned() else { return AppUpdate::default() };
    let tolerance = match parse_length(text) {
        Ok(t) => t,
        Err(e) => return located(s, document, MessageKey::InvalidLength(e), None, chosen.at),
    };
    let Ok(tolerance) = DiagnosticTolerance::new(tolerance) else {
        return error(Some(document), MessageKey::CommandRejected);
    };
    let Some(request) = request else { return error(Some(document), MessageKey::CommandRejected) };
    let payload = CommandPayload::Edit(GeometryCommand::CloseGaps { targets: chosen.objects.clone(), tolerance });
    let cmd = DocumentCommand { document, expected: s.doc.revision(), request, payload };
    match apply_command(&s.fonts, &mut s.doc, cmd) {
        Err(CommandError::Edit(EditError::NothingChanged)) => {
            s.problem = Some(ProblemViewModel {
                message: MessageKey::NothingToRepair,
                object: chosen.objects.first().copied(),
                at: chosen.at,
            });
            AppUpdate::redraw(document)
        }
        other => after_change(s, document, other),
    }
}

/// Trim mode: a click (release) on a line trims it against every other object.
fn trim_at(s: &mut Session, document: DocumentId, ev: &PointerEvent, request: RequestId) -> AppUpdate {
    if ev.phase != PointerPhase::Up {
        return AppUpdate::default();
    }
    let snapshot = s.doc.snapshot();
    let hits = hit_test(&snapshot, ev.position, s.camera.mm_per_point() * CLICK_TOLERANCE_PT);
    let Some(target) = hits.first().copied() else { return AppUpdate::default() };
    let cutters = snapshot.objects().map(|(id, _)| id).filter(|id| *id != target).collect();
    let payload = CommandPayload::Edit(GeometryCommand::TrimLine { target, pick: ev.position, cutters });
    let cmd = DocumentCommand { document, expected: s.doc.revision(), request, payload };
    match apply_command(&s.fonts, &mut s.doc, cmd) {
        Err(CommandError::Edit(EditError::Trim(_))) => {
            located(s, document, MessageKey::TrimImpossible, Some(target), ev.position)
        }
        other => after_change(s, document, other),
    }
}
