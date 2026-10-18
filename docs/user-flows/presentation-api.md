# Presentatie-API voor IM05 (`burnburn-editor`, `burnburn-presentation`)

Status: **poort A + IM09-interactie + poort-B-routes geïmplementeerd** (zie `docs/dev-notes/IM05.md` en
`docs/dev-notes/IM09-interactie.md` voor wat getest is). Eigenaar: User-flow. Identiteiten zijn de kerntypes
`DocumentId`, `RevisionId`, `GeometryId` (DR-03; geen `SessionId`).
Bindend kader: `docs/design/contracts.md` §4 (editor), §6 (presentatie), §7 (taken). Afwijkingen/aanvullingen
staan onderaan en zijn gemeld in `docs/coordination/userflow.md`. Signaturen hieronder zijn ingekort; de code is leidend.

## Uitgangspunten

1. **Eén editorimplementatie.** AppKit-host, Windows-host, CLI en `HeadlessEditorHarness` roepen dezelfde
   `AppController::handle` / `task_finished` aan. De harness bevat geen selectie-, gereedschap- of commandlogica.
2. **Native callback → semantische intentie.** Hosts vertalen OS-events naar `UiIntent` (pointer in canvaspunten); Cmd (Mac) en Ctrl
   (Windows) worden `Modifiers::command`. Menu, toolbar, sneltoets en palet sturen `UiIntent::Invoke(CommandKey)` (X02).
3. **Alleen commits muteren.** Een gereedschap levert per gebaar hooguit één `ToolStep::Commit(DocumentCommand)`;
   preview is tijdelijke editorstate. Cancel herstelt het begintoestandbeeld zonder historie-entry (P02, E01).
4. **Viewmodels zijn data.** Geen native objecten, geen closures; tekst als `MessageKey` behalve gebruikersdata.
5. **Taken zijn revisiegebonden.** Revisiegevoelige resultaten voor een oude revisie of gesloten sessie worden nooit
   toegepast; annuleren is best effort, de revisiecheck beslist.
6. **Machine-acties zijn geen documentcommands** en komen nooit in de undo-historie (P02).

## `burnburn-editor` (laag 4)

```rust
// input: wereld-mm (DR-04); de host stuurt punten naar presentation, die de venstercamera toepast
pub struct PointerEvent { pub pointer: PointerId, pub phase: PointerPhase /* Down|Move|Up|Cancel */,
    pub button: Option<PointerButton> /* None = hover */, pub position: Point, pub modifiers: Modifiers,
    pub click_count: u8, pub time: MonotonicTime }
#[non_exhaustive] pub enum CanvasInput { Pointer(PointerEvent), Key { key: CanvasKey /* Escape|Delete|Return|Backspace */, modifiers, phase } }

// view (DR-02: de enige camera; render gebruikt dezelfde)
impl Viewport { pub fn new(w_pt, h_pt, backing_scale) -> Result<_, ViewError>; pub fn size_px(&self) -> [u32; 2] }
impl Camera { world_to_points, points_to_world, world_to_px, mm_per_point, pan_by_points, zoom_at(vp, anchor_pt, factor) }

// gereedschap
pub struct ToolContext<'a> { pub document: &'a DocumentSnapshot, pub selection: &'a Selection,
    pub mm_per_point: f64, pub snap: &'a SnapSettings }
pub enum ToolKind { Select, Node, Rectangle, Ellipse, Circle, Line, Polyline, Pen }
#[non_exhaustive] pub enum ToolStep { Nothing, PreviewChanged, Commit(EditCommit), Cancelled, Select(SelectionChange) }
#[non_exhaustive] pub enum EditCommit { CreateRectangle { rect }, CreateShape { shape: geometry::Shape },
    CreatePath { path }, Transform { targets, transform, kind: Move|Scale|Rotate },
    EditNodes { target, edit: geometry::NodeEdit, path } }
#[non_exhaustive] pub enum ToolPreview { Rectangle(..), Shape { path, shape }, RubberBand { bounds },
    Transform { targets, transform, kind }, Node { target, path, dragged: NodeTarget } }
pub trait Tool { fn input(..) -> ToolStep; fn cancel(&mut self) -> ToolStep; fn preview(&self) -> Option<ToolPreview>;
    fn snap(&self) -> Option<SnapCandidate>; fn exact_size(&mut self, w: Length, h: Length) -> ToolStep }
pub struct Editor; // select_tool(kind) annuleert een lopend gebaar; input/cancel/exact_size/preview/snap

// selectie, snap, problemen, scène
pub enum SelectionChange { Replace(ids), Toggle(id), Extend(ids), Clear }   // eerste id = referentie (E04)
pub fn hit_test(&DocumentSnapshot, at: Point, tolerance_mm) -> Vec<GeometryId>   // domein, outline-nauwkeurig
pub fn next_in_cycle(hits, &Selection) -> Option<(GeometryId, usize)>; pub fn objects_inside(snapshot, Bounds)
pub struct SnapSettings { pub objects: bool, pub grid_mm: Option<f64>, pub radius_pt: f64 }
pub fn snap_point(snapshot, &SnapSettings, mm_per_point, at, suppress /* Cmd/Ctrl */, exclude) -> Option<SnapCandidate>
pub enum SnapKind { Endpoint, Intersection, Midpoint, Center, Nearest, Grid }
pub fn geometry_issues(snapshot) -> Vec<GeometryIssue { kind: OpenContour|SelfIntersection, at, objects, size_mm }>
pub fn boolean_operands(snapshot, &Selection) -> Result<Vec<GeometryId>, ArrangeError>   // enablement
pub fn build_scene(&SceneRequest) -> SceneSnapshot   // items (Arc<Path>, Affine, StyleRole) + overlays
#[non_exhaustive] pub enum Overlay { Grid, SelectionBounds, Handle, Node, SnapCandidate, RubberBand, DragPreview,
    Issue, ResultPreview }
```

Gedrag (getest, zie `docs/dev-notes/IM09-interactie.md`): rechthoek/ellips (Shift = vierkant/cirkel, Alt = vanuit
centrum), cirkel (middelpunt + straal), lijn (Shift = 45°), polyline (klik per punt, Return/dubbelklik = open,
klik op eerste punt = gesloten, Backspace = laatste punt weg), pen (klik = hoek, slepen = gladde knoop), getypte
maat tijdens slepen; Esc/Cancel/gereedschapwissel laten geen half object achter. Select: klik, Shift/Cmd-toggle,
Alt-klik cyclus, rubberband (Shift voegt toe), verplaatsen (Shift = as), schaalhandles (Shift = proportioneel,
Alt = vanuit centrum), rotatiehandle (Shift = 15°); één `Transform` per sleepbeweging. Node: anker of handvat
slepen = één `NodeEdit` (geen = hoek, Alt = glad, Shift = symmetrisch). Snap op elk gereedschapspunt; Cmd/Ctrl
onderdrukt; de verplaatste objecten bieden zelf geen doelen.

## `burnburn-presentation` (laag 5)

```rust
#[non_exhaustive] pub enum UiIntent {
    NewDocument, Open { path }, Focus { document }, Save { document, path: Option<PathBuf> }, Close { document },
    ConfirmClose { document, choice }, DialogCancelled { document, dialog },
    Select { document, ids, mode },                                   // objectboom
    CommitField { document, field: FieldKey /* X|Y|Width|Height */, text },   // ruwe tekst (DR-05)
    CommitToolSize { document, width: String, height: String },       // tekenmaat tijdens slepen (E01)
    Invoke { document: Option<DocumentId>, command: CommandKey },
    Pointer { document, event: ViewPointerEvent },                    // logische canvaspunten (DR-04)
    Key { document, key: CanvasKey, modifiers },
    Camera { document, intent: CameraIntent::{Scroll{delta_pt}, Magnify{magnification, anchor_pt}, Resized{viewport}} },
    SetMachineProfile { profile: Box<MachineProfile>, ruida: RuidaProfile },   // voorkeuren (B11)
    AddOperation { document, draft: OperationDraft },                 // proceslaag voor de selectie (B10)
    ExportRd { document, path },                                      // na HostEffect::ShowExportPanel
    OpenTraceSource { document, path }, SetTraceRecipe { document, recipe: Box<TraceRecipe> },   // tracer (B9)
    SetSketchParameter { document, sketch, name, text },              // parameterpaneel (B8)
}
pub enum CommandKey { New, Save, SaveAs, Close, Undo, Redo, Delete, SelectAll,
    ToolSelect, ToolNode, ToolRectangle, ToolEllipse, ToolCircle, ToolLine, ToolPolyline, ToolPen,
    SnapToObjects, SnapToGrid, AlignLeft, AlignRight, AlignTop, AlignBottom, AlignCenterX, AlignCenterY,
    DistributeHorizontal, DistributeVertical, BooleanUnion, BooleanDifference, BooleanIntersection, ApplyPreview,
    Measure, Compile, ExportRd, ApplyTrace, CloseTracer }

impl AppController {
    pub fn new(ports: AppPorts) -> Self;
    pub fn handle(&mut self, intent: UiIntent) -> AppUpdate;
    pub fn task_finished(&mut self, outcome: TaskOutcome) -> AppUpdate;
    pub fn view(&self) -> Arc<AppViewModel>;
    pub fn scene(&self, document: DocumentId) -> Option<Arc<SceneSnapshot>>;   // contract §5
}
#[non_exhaustive] pub enum HostEffect { OpenWindow, ShowSavePanel, ShowExportPanel, ConfirmClose, ShowError,
    CloseWindow, Redraw }
```

Elke documentwijziging gaat via `DocumentSession::apply_with(cmd, &[&SketchEvaluator])`: één transactie en één
historie-entry, ook als afgeleide schetsgeometrie meeverandert. Machine-artefacten (gecompileerd plan, RD-export,
machineprofiel) zijn sessie-/app-state, nooit documentdata en nooit in undo.

### Taken

```rust
#[non_exhaustive] pub enum TaskInput { Measure(snapshot), Save { project, path, fs }, Open { path, fs },
    Boolean { snapshot, op, operands }, Compile { snapshot, profile, options },
    ExportRd { plan: Arc<JobPlan>, ruida, path, fs }, LoadImage { path, fs }, Trace(Box<TraceRequest>) }
#[non_exhaustive] pub enum TaskOutput { Measured, Saved, Opened, Boolean { op, operands, result },
    Compiled { plan, preview, report }, ExportedRd { plan, path, rd_hash, bytes }, ImageLoaded { .. }, Traced(..) }
pub fn run_task(input: TaskInput, cancel: &CancellationToken) -> Result<TaskOutput, TaskError>;
```

Regels: (1) gesloten sessie → niets. (2) Revisiegevoelig (Measure, Boolean, Compile): alleen toegepast bij
`source == revision`; een edit annuleert ze. (3) Save/ExportRd schrijven precies de revisie/het plan waarmee ze
begonnen (atomisch: temp → sync → replace → dir-sync); `SaveError::Unconfirmed` = schoon + waarschuwing.
(4) Trace: versheid via `TraceResult::check_fresh(laatste TaskId, recept-sleutel)` (DR-14), niet via revisie.

### Viewmodel

`DocumentViewModel` bevat naast poort A: `snap` (benoemd snapdoel), `snap_settings`, `selection_cycle` ("2 van 3"),
`align_reference`, `issues`, `tool_size_error`, `boolean_preview`, `sketches` (status Valid/Pending/Failed +
parameters), `plan` (`PlanViewModel { id, revision, stale, preview, findings, errors, warnings }`), `export`
(`ExportViewModel { plan, plan_revision, path, rd_hash, bytes }`), `tracer` (`needs_scale`, `computing`, resultaat).
Inspecteur: X, Y, breedte, hoogte. `commands` volgt selectie (uitlijnen ≥ 2, verdelen ≥ 3, boolean ≥ 2 gesloten,
compileren met profiel en operation, export met plan).

## `HeadlessEditorHarness`

`new`, `fs`, `send`, `new_document`, `invoke`, `view`, `document`, `scene`, `pointer(doc, phase, at_mm, mods)`
(mm → punten via de venstercamera, dan `UiIntent::Pointer`), `hover`, `click`, `double_click`, `begin_drag(_with)`,
`drag(_with)`, `key`, `escape`, `take_task_requests`, `complete`, `complete_despite_cancel`, `run_tasks`.

## Platformvertaling (host-verantwoordelijkheid; adapterlogica headless testen)

| Semantiek | macOS (AppKit) | Windows |
|---|---|---|
| `Invoke(Undo)` / `Invoke(Redo)` | Edit-menu, Cmd+Z / Shift+Cmd+Z | Ctrl+Z / Ctrl+Y en Ctrl+Shift+Z |
| `Invoke(Save)` / `Invoke(SaveAs)` | Cmd+S / Shift+Cmd+S; `NSSavePanel` als sheet → `Save{path}` of `DialogCancelled` | Ctrl+S / Ctrl+Shift+S; `IFileSaveDialog` |
| `Invoke(Close)` | Cmd+W, rode sluitknop; bevestigingssheet → `ConfirmClose{choice}` | Ctrl+W / Ctrl+F4, titelbalk-X; TaskDialog |
| `Invoke(Delete)` | Delete (⌫) en Fn-Delete | Del |
| `Key(Escape)` / `Key(Return)` / `Key(Backspace)` | Esc (`cancelOperation:`), Return, ⌫ tijdens tekenen | Esc, Enter, Backspace |
| `Pointer` (hover: `button: None`) | `mouseMoved:`/`mouseDragged:` in punten | `WM_MOUSEMOVE` (DIP → punten) |
| Alt-klik (selectiecyclus) | Option-klik | Alt-klik |
| `Modifiers::command` | Cmd | Ctrl |
| `PointerButton::Secondary` | rechtsklik of Ctrl-klik | rechtsklik |
| `Camera(Scroll/Magnify)` | trackpadscroll = pan, pinch = zoom | wiel = pan, Ctrl+wiel = zoom (host vertaalt naar Magnify) |
| `CommitField` | Return of focusverlies; Esc herstelt veld zonder intentie | Enter of focusverlies; Esc herstelt |
| `dirty` | `setDocumentEdited:` | `*` in titel |

Native lifecycle (sheet getoond, focus, menuvalidatie) hoort in `tests/native/` (GUI-rol); hier wordt getest dat
dezelfde intentie hetzelfde viewmodel en document oplevert.

## Afwijkingen van / aanvullingen op `contracts.md` (gemeld aan Design)

- `ToolContext { document, selection, mm_per_point, snap }` (geen camera, geen request-id); `ToolStep::Select`.
- `UiIntent` blijft per document (`DocumentId`, geen `WindowId`/`AppIntent`-splitsing); `Pointer`, `Key` en
  `Camera(CameraIntent)` vervangen §6 `Pointer`/`Camera`/`CancelInteraction`; `ViewPointerEvent` heeft extra
  `pointer`, `button: Option` (hover) en `click_count`.
- `SceneRequest` als invoer van `build_scene`; extra overlays `Handle`, `Node`, `RubberBand`, `Issue`,
  `ResultPreview`. `PlanPreview` als overlay volgt (plan staat nu in het viewmodel).
- `AppPorts` zonder `clock`; machineprofiel is app-state via `UiIntent::SetMachineProfile`.
- `TaskInput`/`TaskOutput` per soort i.p.v. `TaskKind`/`TaskResult` uit §7; `TaskRequest.key` is revisiegebonden.
- `QueuedTaskPort` staat in de harness (presentation).

## Fase C — productie, operator en open B-flows (18-10-2026)

Nieuw, zonder wijziging van `AppPorts` of `AppViewModel` (hosts bouwen die letterlijk op):
- `AppController::{attach_machine(Arc<dyn MachinePort>), set_role(Role), machine_view(), operator_view()}`.
- `production`: `PreparedJob`, `MachineAction`, `ActionId { kind, subject, generation }`, `MachineIntent { action, machine,
  job, required_state }`, `MachinePort { dispatch, status }`, `MachineService<P: ControllerPort>` (één per machine,
  `poll()` door runtime/test), `HistoryStore::record_intent` (vóór verzenden, ADR 0008), `Role`, `OrderDraft`, `OrderId`.
- Intents: `Machine { document, action, seen }` (host kopieert `MachineButton.action/seen`), `AddOrder`,
  `ChooseOrderJob`, `UploadForOrder`, `Import`, `SetImportUnit`, `Offset`, `RepairIssue`, `AddSketchConstraint`,
  `RemoveSketchConstraint`, `SetSketchDimension`. Commands: `PrepareJob`, `ApproveJob`, `ToolTrim`, `CommitImport`,
  `CancelImport`.
- Viewmodels: `DocumentViewModel.{job, problem, import}`, `SketchViewModel.{constraints, dof, problem_constraints,
  last_constraint}`, `MachineViewModel` (status uit bewijs + knoppen), `OperatorViewModel` (geen editorvelden).
- Taken: `TaskInput::{PrepareJob, ParseImport}` (beide niet revisiegevoelig: de job noemt zijn plan; een bestand is
  onafhankelijk van de revisie).
Afwijking van contract IM31: `ActionId` is een gestructureerde sleutel i.p.v. `u128`, zodat vensters dezelfde id afleiden.
