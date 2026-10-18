# Publieke contracten — API-schetsen

Status: **ontwerp v1.2 (Design), 18 oktober 2026** — fase B: §4/§6/§7/§10 bijgewerkt na review (DR-11..),
nieuw §11 evaluatorpatch, §12 sketch, §13 beeld en trace, §14 import, §15 proces/CAM/JobPlan. Fase C/D:
[`contracts-phase-c.md`](contracts-phase-c.md). v1.1 (17-10): §3, §4, §6, §7 na review van de bestaande crates
en `docs/user-flows/presentation-api.md`. Bindend zijn de *eigenaar*, de *regels* en de vorm van de
types; veldnamen en hulpfuncties mogen tijdens TDD wijzigen zolang de regels gelden. Een afwijking die een regel
raakt gaat via [`docs/coordination/design-review.md`](../coordination/design-review.md). Lagen en toegestane
afhankelijkheden: [ADR 0001](../adr/0001-workspace-en-lagen.md).

Algemene afspraken voor alle contracten:

- Geen `unwrap/expect/panic`; fouten zijn getypeerde enums met `Display` + `std::error::Error`.
- Alle `f64` die een contract binnenkomen zijn eindig; constructors geven een fout (`LengthError::NotFinite`,
  `GeometryError::NotFinite`). Deserialisatie gaat via dezelfde constructors (ADR 0004).
- Contracten bevatten geen native types (`NSView`, `HWND`), geen `wgpu`-types en geen closures die domeinwerk
  doen. `Arc<T>` voor immutable snapshots is toegestaan.
- Revisies stijgen strikt monotoon per document, **ook bij undo/redo**. Een oud taakresultaat herkent men aan
  een oudere `RevisionId`, nooit aan inhoudsgelijkheid.
- `#[non_exhaustive]` op publieke enums die zullen groeien (payloads, acties, events).
- **Platformdeterminisme (fase B, DR-13):** code waarvan de uitvoer gehasht of tussen Mac en Windows vergeleken wordt
  (geometry, sketch, trace, cam, image-raster, job) gebruikt voor transcendente functies (`sin`, `cos`, `tan`, `atan2`,
  `acos`, `hypot`, `exp`, `ln`, `powf`) de pure-Rust `libm`-crate, niet `f64::sin` e.d. (die gaan naar de libm van het
  OS en kunnen per platform 1 ulp verschillen). `+ − × ÷ sqrt` zijn IEEE-exact en vrij. Geen `mul_add` tenzij overal.

## Overzicht eigenaarschap

| Contract | Crate | Module (voorstel) | Werkpakket |
|---|---|---|---|
| `GeometryId`, `OperationId`, `IdAllocator`, `RevisionId` | `burnburn-types` | `id`, `revision` | IM02 ✔ bestaat |
| `DocumentId`, `RequestId`, `TaskId` | `burnburn-types` | `id` | IM03 ✔ / IM05 |
| `CancellationToken`, `CancellationSource`, `Cancelled` | `burnburn-types` | `cancel` | IM05 (taak 7) |
| `ContentHash` | `burnburn-types` | `hash` | IM04 |
| `Clock`, `MonotonicTime` (poort) | `burnburn-types` | `time` | IM01 ✔ bestaat |
| `DocumentCommand`, `CommandPayload`, `CommandOutcome`, `CommandError`, `DocumentSession`, `DocumentSnapshot` | `burnburn-document` | `command`, `session` | IM03 |
| `FileSystem` (poort), `FsError` | `burnburn-io` | `fs` | IM04 |
| `PointerEvent`, `Modifiers` | `burnburn-editor` | `input` | IM05 ✔ bestaat |
| `Camera`, `Viewport` | `burnburn-editor` | `view` | IM07 (nu in render, DR-02) |
| `ToolState`, `ToolOutput`, `EditCommit`, `Editor`, `HitTest` | `burnburn-editor` | `tool` | IM05 ✔ bestaat |
| `SceneSnapshot`, `SceneItem`, `Overlay`, `StyleRole` | `burnburn-editor` | `scene` | IM07 |
| `RenderPacket` | `burnburn-render` | `packet` | IM07 ✔ spike bestaat |
| `WindowId`, `AppIntent`, `UiIntent`, `ViewPointerEvent`, `AppViewModel`, `AppEffect`, `AppController`, `ProjectStore` (poort) | `burnburn-presentation` | zie presentation-api.md | IM05 |
| `TaskRequest`, `TaskOutcome`, `TaskResult`, `Delivery`, `run_task` | `burnburn-presentation` | `task` | IM05 |
| `CanonicalEncode`, `CanonicalWriter` | `burnburn-types` | `canonical` | IM04 (ADR 0004) |
| `DocumentPatch`, `DerivedKey`, `DerivedOwner`, `Evaluator` (poort) | `burnburn-document` | `patch` | IM09/IM11 (§11) |
| `Sketch`, `Entity`, `Constraint`, `ParameterTable` (data) | `burnburn-document` | `sketch` | IM11 (§12) |
| `Solver` (trait), `SolveProblem`, `SolveOutcome`, `SolveDiagnosis`, `SketchEvaluator`, parameterparser | `burnburn-sketch` | `solver`, `param`, `eval` | IM11 (§12) |
| `RasterImage`, `SourceImage`, `DecodeLimits`, `decode` | `burnburn-image` | — | IM10/IM12 (§13) |
| `TraceRecipe`, `TraceResult`, `ContourTree`, `trace` | `burnburn-trace` | — | IM12 (§13) |
| `ImportAnalysis`, `ImportDecision`, `ImportReport`, `UnitDecision` | `burnburn-io` | `import` | IM10 (§14) |
| `Operation`, `OperationRef`, `OperationKind`, `ProcessCommand` (data) | `burnburn-document` | `process` (eigenaar CAM) | IM13 (§15) |
| `JobPlan`, `PlanEvent`, `PlanSource`, `JobPlanId`, `MachineFrame`, `ProfileRevision` | `burnburn-job` | — | IM13 (§8, §15) |
| `compile`, `CompileRequest`, `PreflightReport`, `Diagnostic`, `PreparedJob` | `burnburn-cam` | `compile`, `preflight` | IM13/IM15 (§15) |
| `lower`, `RuidaProfile`, `LowerError` | `burnburn-ruida` | `lower` | IM13/IM14 (§15) |
| `ExecutionId`, `MachineId`, `ExecutionState`, `Evidence`, `ExecutionActor`, `ControllerPort` (poort), `MachineIntent` | `burnburn-machine` | `ids`, `state`, `actor`, `port` | IM16 ✔ bezig |
| `RdProgram`, codec | `burnburn-ruida` | (bestaand) | IM14 ✔ bezig |

Poortimplementaties: productie in `burnburn-runtime` (`SystemClock`, `StdFileSystem` — gedefinieerd in io, zie §10 —, `RuidaUdpPort`,
`IoProjectStore`, worker-threads), fakes in `burnburn-test-support` (`FakeClock` ✔, `FakeFileSystem`, `ScriptedPeer` ✔
als `ControllerPort`); de harness voert taken synchroon uit.

---

## 1. Identiteiten en revisies — `burnburn-types`

Bestaand (IM02): `GeometryId`, `OperationId` (gedeelde teller via `IdAllocator`, `u64`, nooit hergebruikt),
`RevisionId(u64)` met `RevisionId::INITIAL`. Aanvullen in dezelfde stijl (`stable_id!`):

```rust
/// One open document session in this process. Not persisted; a file's own identity is ProjectUuid in io.
pub struct DocumentId(u64);
/// Correlates a UiIntent / DocumentCommand with the AppEvent it produced.
pub struct RequestId(u64);
/// One background task submission. A resubmission gets a new TaskId.
pub struct TaskId(u64);

impl RevisionId {
    /// Strictly next revision; `None` on overflow (caller maps to a typed error).
    pub fn next(self) -> Option<RevisionId>;
}

/// SHA-256 over a canonical projection (ADR 0004). Comparison only; computing lives in io/document/job.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentHash([u8; 32]);
impl ContentHash {
    pub fn from_bytes(b: [u8; 32]) -> Self;
    pub fn as_bytes(&self) -> &[u8; 32];
    pub fn to_hex(&self) -> String;               // lowercase, 64 chars
    pub fn from_hex(s: &str) -> Result<Self, HashParseError>;
    /// Fase B (DR-17): the one domain-separated hash, SHA-256("burnburn-v1\0" ‖ tag(domain) ‖ payload).
    pub fn of(domain: HashDomain, payload: &[u8]) -> Self;
}
#[non_exhaustive]
pub enum HashDomain { Asset, Project, Plan, Rd, SourcePixels, TraceKey, EvalSketch, EvalText, Profile, HistoryRecord }
```

Regel: `ExecutionId` (eigendom `burnburn-machine`) wordt persistent toegekend door de historie (runtime), niet door
een procesteller die na herstart opnieuw bij 1 begint.

## 2. Annulering — `burnburn-types`

```rust
/// Cheap, cloneable, Send + Sync. Checked cooperatively by heavy algorithms.
#[derive(Clone)]
pub struct CancellationToken { /* Arc<AtomicBool> */ }
pub struct CancellationSource { /* owns the flag */ }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled;

impl CancellationSource {
    pub fn new() -> Self;
    pub fn token(&self) -> CancellationToken;
    pub fn cancel(&self);                          // idempotent
}
impl CancellationToken {
    pub fn never() -> Self;                        // for synchronous callers/tests
    pub fn is_cancelled(&self) -> bool;
    pub fn check(&self) -> Result<(), Cancelled>;  // use with `?` inside loops
}
```

Regel: algoritmen in geometry/trace/cam/sketch die onbegrensd kunnen lopen nemen `&CancellationToken` en
controleren minstens per buitenste iteratie. Annulering geeft `Err(Cancelled)` (of een fout die het bevat), nooit
een half resultaat als `Ok`.

## 3. Documentcommands — `burnburn-document`

```rust
pub struct DocumentCommand {
    pub document: DocumentId,
    pub expected: RevisionId,     // optimistic concurrency: must equal session.revision()
    pub request: RequestId,
    pub payload: CommandPayload,
}

#[non_exhaustive]
pub enum CommandPayload {
    CreateRectangle { rect: burnburn_geometry::Rect, name: String }, // IM03 ✔ bestaat
    SetDimension { target: GeometryId, axis: Axis, value: Length, anchor: Anchor },
    Transform { targets: Vec<GeometryId>, transform: Affine },       // één historie-entry per drag-commit
    Delete { targets: Vec<GeometryId> },
    Rename { target: GeometryId, name: String },
    /// Later (IM09/IM11/IM12): result of an evaluator (solve, trace, boolean) computed against `expected`.
    ApplyPatch(DocumentPatch),
}
pub enum Axis { Width, Height }
pub enum Anchor { Min, Center, Max }            // which side stays fixed when resizing

pub struct CommandOutcome {
    pub previous: RevisionId,
    pub revision: RevisionId,     // == previous.next()
    pub changes: ChangeSet,
    pub warnings: Vec<CommandWarning>,
    pub undo_label: UndoLabel,    // enum used as localisation key for "Undo Resize"; inverse data stays internal
}
pub struct ChangeSet { pub created: Vec<GeometryId>, pub modified: Vec<GeometryId>, pub deleted: Vec<GeometryId> }

#[non_exhaustive]
pub enum CommandError {
    StaleRevision { expected: RevisionId, actual: RevisionId },
    WrongDocument { expected: DocumentId, actual: DocumentId },
    UnknownGeometry(GeometryId),
    InvalidValue { field: &'static str, reason: ValueError },
    IdsExhausted,
    RevisionsExhausted,
    NothingToUndo,
    NothingToRedo,
}

pub struct DocumentSession { /* owns current snapshot + history; UI thread only, not Sync */ }
impl DocumentSession {
    pub fn new(id: DocumentId) -> Self;
    pub fn from_snapshot(id: DocumentId, snapshot: DocumentSnapshot) -> Self;  // after open/recovery
    pub fn id(&self) -> DocumentId;
    pub fn revision(&self) -> RevisionId;
    pub fn snapshot(&self) -> Arc<DocumentSnapshot>;          // immutable, cheap to clone into workers
    pub fn apply(&mut self, cmd: DocumentCommand) -> Result<CommandOutcome, CommandError>;
    pub fn undo(&mut self, expected: RevisionId) -> Result<CommandOutcome, CommandError>;
    pub fn redo(&mut self, expected: RevisionId) -> Result<CommandOutcome, CommandError>;
    pub fn undo_label(&self) -> Option<UndoLabel>;
    pub fn redo_label(&self) -> Option<UndoLabel>;
    pub fn can_undo(&self) -> bool;
    pub fn can_redo(&self) -> bool;
    // dirty/saved-revision bookkeeping lives in presentation's session wrapper (presentation-api.md)
}

/// Immutable view handed to evaluators, io and the scene builder.
pub struct DocumentSnapshot { /* objects: BTreeMap<GeometryId, GeometryObject>, operations, … */ }
impl DocumentSnapshot {
    pub fn revision(&self) -> RevisionId;
    pub fn object(&self, id: GeometryId) -> Option<&GeometryObject>;
    pub fn objects(&self) -> impl Iterator<Item = (GeometryId, &GeometryObject)>;  // ordered by id
}
```

Regels:
- Eén `apply` = één transactie = één historie-entry. Een `Err` laat snapshot, revisie en historie ongewijzigd.
- `undo`/`redo` herstellen *inhoud en identiteit* (zelfde `GeometryId`) en geven een **nieuwe** revisie.
- Een `apply` na `undo` wist de redo-tak.
- Machine-acties zijn nooit een `CommandPayload` (AGENTS.md: undo speelt nooit machineacties af).
- Evaluators (`sketch`, `trace`, `cam`) lezen `DocumentSnapshot` en leveren een `DocumentPatch`; zij importeren
  `DocumentSession` niet.

## 4. Invoer, camera en gereedschap — `burnburn-editor`

*Herzien na lezen van `crates/burnburn-editor` en [`docs/user-flows/presentation-api.md`](../user-flows/presentation-api.md):
het ontwerp van User-flow (semantische `EditCommit`, `ToolOutput`, `Modifiers::primary`, `HitTest`-poort) is
eenvoudiger dan de eerste Design-schets en is overgenomen. Eén aanvulling: de coördinatengrens host ↔ presentatie.*

Twee coördinatenstelsels, twee typen:

| Grens | Type | Positie |
|---|---|---|
| host → presentation | `ViewPointerEvent` (presentation) | logische **canvaspunten** (`[f64; 2]`, oorsprong linksboven, y omlaag) |
| presentation → editor | `PointerEvent` (editor, ✔ bestaat) | **wereld-mm** (`Point`), na toepassen van de camera van dat venster |

De host kent de camera niet en rekent niet om; `presentation` bezit per venster de `Camera` (in `WindowState`),
rekent met `Camera::points_to_world` om en leidt hit-/snaptoleranties in mm af uit een tolerantie in punten.
Zo zitten pan/zoom en omrekening één keer in gedeelde code, niet in elke host.

```rust
// burnburn-editor (bestaand, overgenomen)
pub struct Modifiers { pub shift: bool, pub alt: bool, pub primary: bool }   // primary = Cmd (Mac) / Ctrl (Win)
pub enum PointerButton { Primary, Secondary }                                 // Mac Ctrl-klik → Secondary (host)
pub enum PointerPhase { Press, Move, Release, Cancel }                        // Cancel = capture verloren
pub struct PointerEvent { pub position: Point, pub button: PointerButton, pub phase: PointerPhase,
                          pub modifiers: Modifiers, pub time: MonotonicTime }

pub enum ToolState { Idle, Preview(ToolPreview) }          // preview is nooit documentdata
pub enum ToolOutput { None, PreviewChanged, Commit(EditCommit), Cancelled, Click { at: Point, extend: bool } }
pub enum EditCommit { CreateRectangle { bounds: Bounds } /* MoveObjects{..}, … */ }   // presentation → 1 DocumentCommand
pub struct Editor { /* actief gereedschap + ToolState; geen documentreferentie */ }
pub trait HitTest { fn hit(&self, at: Point, tolerance_mm: f64) -> Vec<GeometryId>; }   // leesgrens naar snapshot

// burnburn-editor (nieuw, verplaatst uit burnburn-render — zie design-review DR-02)
/// Pure f64 view math, shared by presentation (pointer → world, tolerances) and render (projection).
pub struct Viewport { /* width_pt, height_pt, backing_scale; finite, > 0 */ }
pub struct Camera { /* center_mm: Point, points_per_mm > 0 */ }
impl Camera {
    pub fn world_to_points(&self, vp: &Viewport, p: Point) -> [f64; 2];
    pub fn points_to_world(&self, vp: &Viewport, pt: [f64; 2]) -> Point;
    pub fn pan_by_points(&mut self, delta_pt: [f64; 2]);
    pub fn zoom_at(&mut self, vp: &Viewport, anchor_pt: [f64; 2], factor: f64) -> Result<(), ViewError>;
}

// burnburn-presentation
pub struct ViewPointerEvent { pub position_pt: [f64; 2], pub button: PointerButton, pub phase: PointerPhase,
                              pub modifiers: Modifiers, pub time: MonotonicTime }
```

Regels:
- Press → Move* → Release **of** Cancel; alleen een geslaagde Release levert `Commit`; nul-oppervlak, `Cancel`-fase,
  Esc of gereedschapwissel → `Cancelled`, geen command, geen historie-entry.
- `ToolState` heeft geen `Committing`: `AppController` past de command synchroon toe op de UI-thread, dus die toestand
  is niet waarneembaar. Wordt een commit ooit asynchroon, dan komt `Committing { request }` terug.
- Camera pan/zoom is vensterstate en levert nooit een command. Selectie is per venster.
- De editor kent geen `RequestId`/`RevisionId`: `presentation` maakt van `EditCommit` een `DocumentCommand` met
  verwachte revisie en request-id.

**Fase B (18-10-2026, DR-11): de gebouwde vorm is leidend.** `burnburn-editor` ontvangt `CanvasInput` met
`PointerEvent.position_pt` (logische punten) en krijgt `Camera`/`Viewport` via `ToolContext`; de editor rekent zelf om
naar mm en levert `ToolStep::{Nothing, PreviewChanged, Commit(DocumentCommand), Cancelled, Select(..)}`, met
`ToolContext.{document_id, next_request, new_object_name}`. Dat haalt de bedoeling van DR-04 (hosts doen geen
camera-wiskunde, één plek voor omrekening) en is eenvoudiger dan een aparte `ViewPointerEvent`; `ViewPointerEvent`
vervalt. De regel die blijft: `expected` in de command is altijd `ctx.document.revision()` (de snapshot die het
gereedschap las), zodat een verouderd gebaar als `StaleRevision` faalt en niet stil op een nieuwere revisie landt.

## 5. Scene — `burnburn-editor` (bouw) → `burnburn-render` (vertaling)

```rust
/// Immutable view of one document revision plus overlays, for one window (selection and camera are per window).
/// Not the document, not the JobPlan.
pub struct SceneSnapshot {
    pub document: DocumentId,
    pub revision: RevisionId,
    pub camera: Camera,
    pub viewport: Viewport,
    pub items: Vec<SceneItem>,           // back-to-front
    pub overlays: Vec<Overlay>,          // drawn above items
}
pub struct SceneItem {
    pub source: GeometryId,
    pub path: Arc<burnburn_geometry::Path>,   // world mm, f64, already resolved (no evaluator here)
    pub transform: Affine,
    pub role: StyleRole,
}
/// Semantic styling; colours, dash patterns and line widths are a render/theme decision (X03: not colour-only).
pub enum StyleRole { Design, ProcessLayer(u16), Selected, Hovered, Disabled }
#[non_exhaustive]
pub enum Overlay {
    SelectionBounds { bounds: Bounds },
    Node { at: Point, kind: NodeKind, selected: bool },
    SnapCandidate { at: Point, kind: SnapKind },
    DragPreview { path: Arc<Path>, transform: Affine },
    DimensionLabel { anchor: Point, text: String },
    PlanPreview { plan: Arc<JobPlan>, upto_event: Option<usize> },
    Grid { spacing_mm: f64 },
}
```

```rust
// burnburn-render
pub fn build_packet(scene: &SceneSnapshot, theme: &Theme) -> RenderPacket;   // pure, no GPU
pub struct RenderPacket { pub size_px: [u32; 2], pub clear: Rgba, pub primitives: Vec<Primitive> /* f32 px */ }
```

Regel: `build_packet` trekt de camera-oorsprong in `f64` af vóór de cast naar `f32` (viewportrelatief, ADR 0003).
Schermtessellatie gebruikt een tolerantie in pixels; productieflattening (cam) en hit-testing (editor) hebben eigen
toleranties. GPU-pixels zijn nooit invoer voor CAM of Ruida.

## 6. Presentatie — `burnburn-presentation`

*Gedetailleerde, bindende schets: [`docs/user-flows/presentation-api.md`](../user-flows/presentation-api.md)
(eigenaar User-flow). Design heeft die gereviewd en neemt hem over; hieronder alleen de architectuurregels en de
afwijkingen die Design vraagt (zie [design-review](../coordination/design-review.md) DR-03..DR-05).*

Kernvorm (uit presentation-api.md):

```rust
pub struct WindowId(u64);                       // eigen selectie, camera, focus
// SessionId → gebruik burnburn_types::DocumentId (DR-03): één identiteit voor "open document"
pub enum AppIntent { NewDocument, OpenDocument { path: ProjectPath }, Quit }
pub enum UiIntent {                             // altijd gericht aan één WindowId
    Command(CommandId), SelectTool(ToolKind),
    Pointer(ViewPointerEvent),                  // canvaspunten, niet mm (DR-04)
    CancelInteraction,
    SetDimension { object: GeometryId, dimension: DimensionField, text: String },   // ruwe veldtekst
    SetSelection(Vec<GeometryId>), SaveAsChosen { path: ProjectPath }, SaveSheetCancelled,
    CloseDecision(CloseDecision), Camera(CameraIntent), WindowClosed,
}
impl AppController {
    pub fn new(store: Box<dyn ProjectStore>) -> Self;
    pub fn app(&mut self, intent: AppIntent) -> Result<Dispatch, PresentationError>;
    pub fn dispatch(&mut self, window: WindowId, intent: UiIntent) -> Result<Dispatch, PresentationError>;
    pub fn deliver(&mut self, outcome: TaskOutcome) -> Delivery;      // Applied | Stale | SessionClosed | UnknownTask
    pub fn view_model(&self, window: WindowId) -> Option<AppViewModel>;
    pub fn scene(&self, window: WindowId) -> Option<Arc<SceneSnapshot>>;   // aanvulling Design (IM07)
    pub fn take_task_requests(&mut self) -> Vec<TaskRequest>;
}
pub struct Dispatch { pub request: RequestId, pub events: Vec<AppEvent>, pub effects: Vec<AppEffect> }
```

Architectuurregels:
- Hosts vertalen OS-gebeurtenissen naar `AppIntent`/`UiIntent` en tonen `AppViewModel`; zij wijzigen nooit
  documentvelden en parseren geen maten (de presentatielaag parseert `150`, `5,9 in`; DR-05).
- `AppViewModel` bevat alleen data (strings, getallen, enums, ID's), `PartialEq`; geen native objecten of closures.
  Labels zijn lokalisatiesleutels of al vertaalde strings, nooit host-specifiek.
- Eén undo-stack: menu-undo is `Command(Undo)`; `NSUndoManager` spiegelt hooguit labels.
- De CLI en `HeadlessEditorHarness` gebruiken exact dezelfde `AppController`.
- `ProjectStore` is een poort van `presentation` (openen/bewaren als semantische actie); `runtime` implementeert hem
  met `burnburn-io` + `FileSystem` (§10). `presentation` doet zelf geen IO.

**Bevestigd in fase B (aanvullingen User-flow, `presentation-api.md` § Afwijkingen):** `ToolContext.{document_id,
new_object_name}`, `ToolStep::Select`, `UiIntent::{Focus, ConfirmClose}`, `HostEffect::OpenWindow`,
`AppPorts.new_project_uuid`, `TaskInput::{Measure, Save, Open}` met `fs: Arc<dyn FileSystem>` in de input en
`AppController::{handle, task_finished, view}` (één venster per `DocumentId` tot een tweede venster per document
nodig is; dan komt `WindowId` terug). Eisen erbij:

- **Maatveld (orchestratorbesluit):** commit bij Return **en** bij focusverlies (Tab, klik elders). De host stuurt
  in beide gevallen `UiIntent` met de ruwe tekst; `presentation` vergelijkt de *geparste* waarde met de huidige
  (exact `f64` na parse, in mm) en maakt **geen** command bij gelijke waarde of bij een parsefout (dan veldfout in het
  viewmodel, document ongewijzigd). Esc herstelt de veldtekst zonder command. Dubbele commit (Return gevolgd door
  focusverlies) is daarmee vanzelf één command. Test: `tab_after_return_commits_once`, `unchanged_value_creates_no_command`.
- **`SaveError::Unconfirmed`:** het bestand is vervangen, alleen de map-sync faalde. Viewmodel: document **schoon**
  (de inhoud staat er), plus een niet-blokkerende waarschuwing `MessageKey::SaveNotConfirmedDurable`; geen
  foutdialoog als bij een mislukte save. Een volgende save probeert gewoon opnieuw.
- **`LoadError::NewerSchema`:** openen weigert zonder schrijfactie; alleen-lezen weergave is een aparte beslissing bij
  de eerste schema-2-wijziging (Design akkoord met voorstel Kern).
- **Taakresultaten van evaluators** (trace, later boolean/tekst) zijn niet revisie- maar *invoergebonden*: zie §11.

## 7. Achtergrondtaken — `burnburn-presentation`

Overgenomen uit presentation-api.md, met `DocumentId` i.p.v. `SessionId` en `RevisionId` als revisietype:

```rust
pub struct TaskRequest {
    pub task: TaskId,
    pub document: DocumentId,
    pub source_revision: RevisionId,
    pub kind: TaskKind,                  // Save{..}, Summarize{..}; later Trace/Boolean/Compile — snapshot in, resultaat uit
    pub cancel: CancellationToken,       // burnburn-types (§2)
}
pub struct TaskOutcome { pub task: TaskId, pub document: DocumentId, pub source_revision: RevisionId, pub result: TaskResult }
pub enum TaskResult { Saved, Summary(DocumentSummary), Failed(TaskError), Cancelled /* , … */ }
pub enum Delivery { Applied, Stale, SessionClosed, UnknownTask }

/// Pure worker entry point, used by runtime threads and by the harness alike.
pub fn run_task(kind: TaskKind, cancel: &CancellationToken) -> TaskResult;
```

Regels:
- Pull-model: de controller zet verzoeken klaar (`take_task_requests`); `runtime` (threads) of de harness (synchroon)
  voert `run_task` uit en roept `deliver` aan op de UI-thread (wake-up, ADR 0002/0005). De controller bezit geen
  threads.
- `deliver` past een revisiegevoelig resultaat **alleen** toe als het document nog open is en
  `source_revision == session.revision()`; anders `Stale`/`SessionClosed`, zonder event of viewmodelwijziging.
- Sluiten of een nieuwe edit annuleert lopende tokens (best-effort); de revisiecontrole blijft doorslaggevend.
- Een cachekey (`ContentHash` over canonieke invoer, ADR 0004) komt erbij zodra de eerste cachebare taak (trace) er is.

## 8. JobPlan — `burnburn-job`

```rust
/// Immutable, content-addressed compiled plan. Preview, RD export and upload all read the same Arc<JobPlan>.
pub struct JobPlan { /* private fields */ }
pub struct JobPlanId(ContentHash);                 // hash of canonical plan content (no addresses, no log time)

impl JobPlan {
    pub fn id(&self) -> JobPlanId;
    pub fn source(&self) -> &PlanSource;
    pub fn frame(&self) -> &MachineFrame;          // bed size, origin corner, axis directions
    pub fn settings(&self) -> &[OperationSettings];
    pub fn events(&self) -> &[PlanEvent];
    pub fn bounds(&self) -> Option<Bounds>;        // machine frame, includes travel
}
pub struct JobPlanBuilder { /* … */ }
impl JobPlanBuilder {
    pub fn new(source: PlanSource, frame: MachineFrame) -> Self;
    pub fn push(&mut self, event: PlanEvent) -> Result<(), PlanError>;   // validates finite/in-range
    pub fn finish(self) -> Result<JobPlan, PlanError>;                  // computes JobPlanId
}

pub struct PlanSource {
    pub document_revision: RevisionId,
    pub project_hash: ContentHash,
    pub machine_profile: ProfileRevision,
    pub compiler_version: String,
}
#[non_exhaustive]
pub enum PlanEvent {
    OperationBoundary { operation: OperationId, settings: SettingsIndex },
    Travel { to: MachinePoint },
    Cut { to: MachinePoint, origin: Option<SourceRef> },
    ScanRun { line: ScanLine, origin: Option<SourceRef> },
    Dwell { duration: Duration },
    Air { on: bool },
    AxisAction { action: AxisAction },
}
pub struct MachinePoint { pub x: Length, pub y: Length }            // machine frame, mm, f64; quantised only by a backend
pub struct SourceRef { pub geometry: GeometryId, pub operation: OperationId }   // preflight → source (IM15)
pub struct OperationSettings { pub speed_mm_s: f64, pub min_power: Ratio, pub max_power: Ratio, /* … */ }
```

*Fase C (C-6, gebouwd door CAM, overgenomen):* `PlanSource` bevat `source_hash` (cam-bronhash over de opgeloste
geometrie + operations, domein `plan-source`) i.p.v. `project_hash` — cam kan io niet zien; de koppeling naar het
projectbestand staat in `CompiledJob.project_hash` (§IM17). `JobPlanId` bevat de sessierevisie **niet** (revisies zijn
sessielokaal; CLI = GUI vereist dat); staleness via `PreparedJob.source_revision` + preflight. `MachinePoint` heeft
`f64`-mm-getters, `bounds()` geeft `geometry::Bounds`. `ProfileRevision { id, revision, content }` (DR-18).

Regels: geen `pub`-velden op `JobPlan` (onveranderlijk na `finish`). Een backend die een event of instelling niet
ondersteunt, geeft een fout (`unsupported_modulation_mode_is_rejected_instead_of_replaced`); hij vervangt niet stil.
Ruida: `burnburn_ruida::lower(&JobPlan, &RuidaProfile) -> Result<RdProgram, LowerError>` en daarna de bestaande
`encode_rd_file`. Een document-undo raakt een bevroren `JobPlan` of `Execution` nooit.

## 9. Machine — `burnburn-machine`

*Herzien na lezen van `crates/burnburn-machine` (Protocol): een **getypeerde, niet-blokkerende poort** met
`RequestTag { execution, epoch, seq }` en een poll-gestuurde actor met geïnjecteerde `Clock` is pragmatisch beter dan
de eerste Design-schets (ruwe bytes + effect-lijst): actortests hoeven geen RD-bytes te kennen en de epoch maakt
late antwoorden van vóór een reconnect structureel onschadelijk. Overgenomen.*

```rust
// bestaand (IM16, in uitvoering)
pub struct MachineId(String);
pub struct MachineIdentity { pub id: MachineId, pub model: Option<String>, /* firmware, … */ }
pub struct ExecutionId(u64);                       // eigendom van machine (niet types)
pub enum Request { UploadChunk { index: usize, bytes: Vec<u8> }, QueryStored, Start, QueryStatus, Stop }
impl Request { pub fn is_idempotent(&self) -> bool; }   // alleen queries
pub struct RequestTag { pub execution: ExecutionId, pub epoch: u32, pub seq: u32 }
pub struct Envelope { pub to: MachineId, pub tag: RequestTag, pub request: Request }
pub struct Incoming { pub from: MachineId, pub tag: RequestTag, pub reply: Reply }
pub enum Reply { Ack, Nak, Err, Stored(bool), Status(ControllerStatus) }

/// Port: non-blocking. Production adapter (runtime) encodes with burnburn-ruida and owns the UDP socket;
/// tests wrap test-support's ScriptedPeer.
pub trait ControllerPort {
    fn send(&mut self, envelope: Envelope) -> Result<(), PortError>;
    fn try_recv(&mut self) -> Result<Option<Incoming>, PortError>;
    fn reconnect(&mut self) -> Result<(), PortError>;
}

pub struct ExecutionActor<P: ControllerPort> { /* one execution at a time on one machine */ }
impl<P: ControllerPort> ExecutionActor<P> {
    pub fn new(machine: MachineIdentity, port: P, clock: Arc<dyn Clock>, policy: Policy) -> Self;
    pub fn begin_upload(&mut self, id: ExecutionId, chunks: Vec<Vec<u8>>) -> Result<(), ActorError>;
    pub fn confirm_stored(&mut self) -> Result<(), ActorError>;
    pub fn start(&mut self) -> Result<(), ActorError>;
    pub fn query_status(&mut self) -> Result<(), ActorError>;
    pub fn cancel(&mut self) -> Result<(), ActorError>;
    pub fn poll(&mut self) -> Result<(), ActorError>;          // replies + timeouts, using the Clock
    pub fn execution(&self) -> Option<&Execution>;            // state + evidence log
}
```

`MachineIntent` (plancontract) is de vorm waarin `presentation` een actie aanvraagt; `runtime` vertaalt hem naar
de actor-methoden hierboven op de driver-thread:

```rust
// burnburn-machine
pub struct MachineIntent {
    pub execution: ExecutionId,
    pub machine: MachineIdentity,
    pub action: MachineAction,
    pub requires: ExecutionState,        // state the actor must have *proven*; otherwise rejected
}
#[non_exhaustive]
pub enum MachineAction { Upload { plan: JobPlanId, rd_hash: ContentHash, file_name: String }, ConfirmStored,
                         Start, Stop, QueryStatus /* Pause, Resume, Frame{laser off}, Jog — later */ }
```

Regels (verplichte tests uit 01-test-driven-design): ACK ≠ `Stored`; `Stored` ≠ `Completed`; verloren
startantwoord → `Unknown`; reconnect stuurt geen tweede `Start`; `Start` wordt nooit automatisch herhaald
(alleen `is_idempotent()`-requests); antwoorden voor een andere `MachineId` of oude tag/epoch worden genegeerd
(en gelogd in `ignored`); `cancel` verwijdert geen controllerbestand impliciet. Een `Execution` legt vast welk
`JobPlanId` en welke RD-`ContentHash` geüpload zijn (DR-10), zodat preview, export en upload aantoonbaar dezelfde
revisie zijn.

## 10. Poorten

| Poort | Eigenaar | Productie (`runtime`) | Fake (`test-support`) |
|---|---|---|---|
| `Clock` | types ✔ | `SystemClock` (`std::time::Instant`) | `FakeClock` ✔ |
| `FileSystem` | io | `StdFileSystem` (io, re-export in runtime) | `MemoryFileSystem` (io) met foutinjectie per stap (C-11) |
| `ControllerPort` | machine | `RuidaUdpPort` (ruida-codec + UDP; later USB) | `ScriptedPeer` ✔ (wrapper in machine-tests) |
| `ProjectStore` | presentation | `IoProjectStore` (io + `StdFileSystem`) | tijdelijke map / `FakeFileSystem` |
| (taken) | presentation (pull-model) | worker-threads + host-wake-up | harness `run_tasks()` synchroon |

```rust
// burnburn-io
/// Every step is separate so tests can inject a failure on write, flush or replace (IM04).
pub trait FileSystem: Send + Sync {
    fn read(&self, path: &Path, limit: u64) -> Result<Vec<u8>, FsError>;    // TooLarge above limit
    fn exists(&self, path: &Path) -> Result<bool, FsError>;
    fn create_temp_in(&self, dir: &Path, stem: &str) -> Result<PathBuf, FsError>;  // exclusive create
    fn write_all(&self, path: &Path, bytes: &[u8]) -> Result<(), FsError>;
    fn sync_file(&self, path: &Path) -> Result<(), FsError>;               // macOS: F_FULLFSYNC
    fn replace(&self, from: &Path, to: &Path) -> Result<(), FsError>;      // same volume, atomic rename
    fn sync_dir(&self, dir: &Path) -> Result<(), FsError>;                 // no-op where unsupported (documented)
    fn remove_file(&self, path: &Path) -> Result<(), FsError>;
}
pub struct FsError { pub op: FsOp, pub path: PathBuf, pub kind: FsErrorKind }
#[non_exhaustive]
pub enum FsErrorKind { NotFound, PermissionDenied, AlreadyExists, NoSpace, TooLarge, Interrupted, Other }
```

Fase B (orchestratorbesluit, 18-10-2026): `FileSystem` heeft ook `create_dir_all(&self, dir)` (recoverymap,
historie). **De adapters wonen in `burnburn-io`:** `burnburn_io::std_fs::StdFileSystem` (enige module van io die
`std::fs` aanroept) en `burnburn_io::memory::MemoryFileSystem` (fout-injecterend, ook bruikbaar buiten tests, bv. de
harness). `runtime` re-exporteert `StdFileSystem`; `test-support` krijgt géén eigen `FakeFileSystem` maar gebruikt
`MemoryFileSystem`. Reden: de adapter is klein, zuiver `std` en wordt in io-integratietests (`std_fs_integration.rs`)
tegen een tijdelijke map getest; verhuizen zou een extra crate-grens zonder winst zijn.

Regel: io-*logica* doet nooit zelf `std::fs`; alleen via `FileSystem` (alleen `std_fs.rs` is uitgezonderd). Tests schrijven alleen in tijdelijke mappen of in
`FakeFileSystem` (AGENTS.md: geen gebruikersprojecten wijzigen).

---

# Fase B — evaluators, import en CAM (18 oktober 2026)

Gemeenschappelijk principe (plan § Cargo workspace): **het document bevat de data** (sketchdefinitie, tracerecept,
operations) **én het laatst geldige afgeleide resultaat**; evaluators (`sketch`, `trace`, later `text`, modifiers)
lezen een `DocumentSnapshot` en leveren een `DocumentPatch`. Document en evaluator importeren elkaar niet;
`presentation` verbindt ze. CAM leest uitsluitend *opgeloste* geometrie uit de snapshot en kent geen evaluator.

## 11. Evaluatorpatch — `burnburn-document::patch`

```rust
/// Who owns a piece of derived geometry. One owner regenerates only its own outputs.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum DerivedOwner { Sketch(SketchId), Trace(TraceId), Text(TextId), Modifier(ModifierId), Generator(GeneratorId),
                    Part(PartInstanceId) }   // fase C (C-2); elke owner één variant, toegevoegd door wie hem bouwt

/// Stable key of one output *within* its owner (sketch entity, contour id, glyph run…). The session maps
/// DerivedKey → GeometryId and reuses the GeometryId as long as the key survives, so OperationRefs stay bound.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DerivedKey { pub owner: DerivedOwner, pub local: u64 }

pub struct DocumentPatch {
    pub owner: DerivedOwner,
    /// ContentHash over the canonical evaluator *inputs* (definition + referenced assets + evaluator version).
    pub inputs: ContentHash,
    pub outputs: Vec<DerivedOutput>,      // complete set for this owner; keys not listed are removed
    pub status: EvalStatus,
}
pub struct DerivedOutput { pub key: DerivedKey, pub kind: ObjectKind, pub path: Path, pub name: Option<String> }

#[non_exhaustive]
pub enum EvalStatus {
    Valid,
    /// Evaluation failed: the document keeps the *last valid* outputs, marked stale, plus the diagnosis.
    Failed { diagnosis: EvalDiagnosis },
}

/// Synchronous evaluator hook, implemented by burnburn-sketch (and later text/modifiers) and passed in by presentation.
pub trait Evaluator {
    fn owner_kind(&self) -> DerivedOwnerKind;
    /// Pure and deterministic; bounded by an *iteration* budget (never wall-clock: determinism on Mac and Windows).
    fn evaluate(&self, snapshot: &DocumentSnapshot, owner: DerivedOwner) -> DocumentPatch;
}

impl DocumentSession {
    /// Applies `cmd` and then re-evaluates every owner whose inputs hash changed, *inside the same transaction*:
    /// one history entry, one new revision. Undo restores definition and derived outputs together.
    pub fn apply_with(&mut self, cmd: DocumentCommand, evaluators: &[&dyn Evaluator])
        -> Result<CommandOutcome, CommandError>;
}
// CommandPayload::ApplyPatch(DocumentPatch) — for *asynchronous* evaluators (trace): an explicit user action
// ("Pas trace toe"), its own history entry.
```

Regels:
- **Snel en synchroon** (sketch, tekst, modifiers): via `apply_with` in dezelfde transactie. Een evaluator die zijn
  budget overschrijdt, levert `Failed { NotConverged }` — de transactie slaagt (de definitie is wat de gebruiker
  vroeg), de geometrie blijft de laatst geldige en het viewmodel toont de diagnose. Zo is er nooit een
  "halve" commit en nooit twee undo-stappen voor één maatwijziging.
- *Fase C (DR-14 afgesloten):* trace bouwde dit als `TraceResult::check_fresh(latest_task: TaskId, current_inputs:
  ContentHash)` met `StaleTrace::{Superseded, InputsChanged { traced, current }}`; dat is de vorm.
- **Zwaar en asynchroon** (trace, IM12; later nesting): taak met `CancellationToken`; het resultaat draagt
  `inputs: ContentHash`. `deliver` past het toe als de huidige `inputs` van die owner in het document nog gelijk is —
  *niet* op revisiegelijkheid, want een edit aan een ander object maakt een trace niet ongeldig. Bij verschil:
  `Delivery::Stale`. De `DocumentCommand` voor `ApplyPatch` heeft gewoon `expected = huidige revisie` (UI-thread).
- `ApplyPatch` met een owner wiens `inputs` intussen anders is → `CommandError::StalePatch { owner }`.
- Een key die verdwijnt terwijl een `OperationRef` (§15) of associatieve referentie (IM21) ernaar wijst, verwijdert
  het object en geeft `CommandWarning::ReferenceBroken { operation, geometry }`; er wordt **nooit** automatisch aan een
  buur herbonden.
- Afgeleide objecten zijn niet direct bewerkbaar (`SetDimension` op een sketch-uitvoer → `CommandError::DerivedObject`);
  de editor stuurt dan een command op de definitie (sketchparameter) of de gebruiker maakt eerst "Losmaken".
- `inputs`-hash: `ContentHash` met domain-tag per owner-soort (ADR 0004 §hash), inclusief evaluatorversie.

## 12. Sketch — data in `burnburn-document::sketch`, evaluator in `burnburn-sketch`

```rust
// burnburn-document::sketch (data; eigenaar Sketch-rol, module in Kern-crate)
pub struct SketchId(u64);                 // stable_id!, uit dezelfde IdAllocator
pub struct EntityId(u32); pub struct ConstraintId(u32); pub struct ParameterId(u32);   // lokaal per sketch, nooit hergebruikt
pub struct Sketch {
    pub placement: Affine,                // sketch frame → document mm
    entities: BTreeMap<EntityId, Entity>,
    constraints: BTreeMap<ConstraintId, Constraint>,
    parameters: ParameterTable,           // naam → expressietekst, bv. "breedte = 150 mm", "gat = breedte / 2 - 10"
    solution: SketchSolution,             // last valid: variabelewaarden + dof + status (deel van de afgeleide toestand)
}
#[non_exhaustive] pub enum Entity { Point { at: Point }, Line { a: EntityId, b: EntityId },
    Arc { center: EntityId, start: EntityId, end: EntityId, ccw: bool }, Circle { center: EntityId, radius: Length } }
#[non_exhaustive] pub enum Constraint {
    Coincident(EntityId, EntityId), Horizontal(EntityId), Vertical(EntityId), Parallel(EntityId, EntityId),
    Perpendicular(EntityId, EntityId), Tangent(EntityId, EntityId), Equal(EntityId, EntityId),
    Symmetric { a: EntityId, b: EntityId, axis: EntityId }, PointOnLine { point: EntityId, line: EntityId },
    Fix(EntityId),
    Distance { a: EntityId, b: EntityId, value: DimValue }, Radius { circle: EntityId, value: DimValue },
    Angle { a: EntityId, b: EntityId, value: DimValue },
}
pub enum DimValue { Driving(Expr), Driven }   // driven: gemeten, nooit opgelegd; toont waarde, geen residu
pub struct Expr(String);                       // brontekst blijft bewaard (heropenen toont wat de gebruiker typte)
// Commands: CommandPayload::Sketch(SketchCommand) — Create, AddEntity, AddConstraint, Remove, SetParameter{name, text},
// SetDimMode{constraint, Driving|Driven}, Drag{entity, to}. Eén variant in CommandPayload, enum in de sketchmodule.
```

**Review fase B (DR-12): de gebouwde solvervorm van Sketch is leidend** en vervangt de schets hieronder waar ze
verschillen: `Solver::solve(&Problem, &CancellationToken) -> Result<SolveReport, SolveError>`, met de startwaarden
(*guess* = laatst geldige oplossing) in het `Problem` zelf, een `SolveReport { status: Solved|Conflict|NotConverged,
dof, free, constraints: Vec<ConstraintState>, entities, flipped: Vec<Flip>, residual, tolerance }` met per constraint
`Satisfied|Redundant{with}|Conflicting{with}|Unmet{residual}` en `SolveError` voor ongeldige invoer. Beter dan de schets:
diagnose per constraint, expliciete branch-flips en "conflict alleen als bewezen". In `apply_with` (synchroon) roept de
evaluator de solver aan met `CancellationToken::never()` en een interne iteratiegrens van de adapter.

```rust
// burnburn-sketch — oorspronkelijke schets (zie review hierboven)
/// Own model types; sketch-solver (ADR 0007) sits behind `solver::sketch_solver` and never leaks.
pub trait Solver: Send + Sync {
    fn solve(&self, problem: &SolveProblem, start: &[f64], budget: SolveBudget) -> SolveOutcome;
}
pub struct SolveProblem { pub variables: Vec<VarInfo>, pub residuals: Vec<Residual> }   // Residual verwijst naar ConstraintId
pub struct SolveBudget { pub max_iterations: u32, pub tolerance_mm: f64 }
pub enum SolveOutcome {
    Solved { values: Vec<f64>, max_residual_mm: f64, dof: u32, redundant: Vec<ConstraintId> },  // dof > 0 = onderbepaald
    Failed(SolveDiagnosis),
}
#[non_exhaustive]
pub enum SolveDiagnosis {
    Conflicting { constraints: Vec<ConstraintId> },                  // overbepaald + inconsistent: kleinste gevonden set
    NotConverged { iterations: u32, max_residual_mm: f64, worst: Vec<ConstraintId> },
    InvalidParameter { parameter: ParameterId, error: ParamError },  // syntax, eenheid, onbekende naam, ≤ 0 waar verboden
    CyclicParameters { cycle: Vec<ParameterId> },
    Degenerate { entities: Vec<EntityId> },                          // bv. lijn met lengte 0
}
pub fn parse_expr(text: &str, scope: &ParameterScope) -> Result<Length, ParamError>;   // "5,9 in", "breedte/2"
pub struct SketchEvaluator<S: Solver> { /* implements document::Evaluator */ }
```

Regels:
- **Continuïteit:** `start` = laatst geldige oplossing; de solver mag geen andere tak kiezen als die oplossing nog
  binnen tolerantie convergeert (test `changing_plate_width_preserves_symmetric_hole_offsets`,
  `small_width_change_does_not_flip_arc_branch`).
- **Determinisme:** zelfde invoer → bit-identieke `values` op aarch64-macOS en x86_64-Windows; geen `HashMap`-iteratie,
  geen tijdsbudget, geen threads in de solver.
- `Conflicting` ≠ `NotConverged`: verschillende diagnose, verschillende UI-tekst. Conflict noemt constraint-ID's die de
  editor kan markeren.
- Last-valid-result: bij `Failed` blijven de uitvoer-objecten (laatst geldig) bestaan met status *verouderd*; CAM-preflight
  geeft dan `Diagnostic::SketchNotSolved` (fout, blokkeert export; preview mag).
- Editor-drag van een sketchpunt: de editor roept de solver direct aan voor de preview (editor mag `sketch`), de commit is
  `SketchCommand::Drag` en wordt in `apply_with` opnieuw opgelost — de preview is nooit documentdata.

## 13. Bronbeeld en tracing — `burnburn-image`, `burnburn-trace`

**Review fase B (DR-14): de gebouwde vorm van Trace/Image is leidend** voor namen: `SourceImage` (met
`content_hash()`), `PixelScale` (mm/px per as; `from_dpi`, `from_known_distance`) i.p.v. een `Affine`, `QuarterTurn` voor
EXIF, `ImageRecipe` in image, `TraceRecipe { image, contour, fit }` met `key(source_hash)`, `TraceResult { contours
(arena met parent-index, ContourKind::{Outer, Hole}), recipe_key, engine, source_hash, .. }`, `ENGINE_VERSION`,
`comparison_preview`. Ontbrekende schaal is een fout, geen DPI-default. Eén wijziging gevraagd: zie §11 —
geldigheid op `recipe_key` (inputs), niet op documentrevisie.

```rust
// burnburn-image — oorspronkelijke schets
pub struct RasterImage { width: u32, height: u32, format: PixelFormat, pixels: Arc<[u8]> }   // originele pixels, immutable
#[non_exhaustive] pub enum PixelFormat { Gray8, Rgba8 }
pub struct ImageMetadata { pub dpi: Option<[f64; 2]>, pub orientation: ExifOrientation, pub has_icc: bool }
pub struct DecodeLimits { pub max_bytes: u64, pub max_pixels: u64, pub max_dimension: u32 }
pub fn decode(bytes: &[u8], limits: &DecodeLimits) -> Result<(RasterImage, ImageMetadata), DecodeError>;
// burnburn-document: ImageObject { asset: ContentHash, pixel_to_mm: Affine, crop: Option<PixelRect> } — de fysieke
// schaal staat in het document, nooit impliciet uit DPI-metadata (die is alleen een voorstel in het importverslag).

// burnburn-trace
pub struct TraceId(u64);                                   // stable_id!; ook DerivedOwner::Trace
pub struct TraceRecipe {
    pub version: u16,                                      // receptschema, mee in de inputs-hash
    pub crop: Option<PixelRect>,
    pub preprocess: Vec<Preprocess>,                       // Grayscale, Threshold{level}|Otsu, Invert, Despeckle{min_area_px}, …
    pub fit: FitSettings,                                  // max_deviation_px, corner_angle, Lines|Curves
}
pub struct TraceRequest { pub trace: TraceId, pub image: Arc<RasterImage>, pub image_asset: ContentHash,
                          pub recipe: TraceRecipe, pub pixel_to_mm: Affine }
pub fn trace(req: &TraceRequest, cancel: &CancellationToken) -> Result<TraceResult, TraceError>;   // TraceError::Cancelled

pub struct TraceResult {
    pub trace: TraceId,
    pub inputs: ContentHash,             // recept + image_asset + pixel_to_mm + tracerversie (§11)
    pub source_revision: RevisionId,     // revisie waarop de aanvraag gebaseerd was (log/diagnose; niet de geldigheidstoets)
    pub tree: ContourTree,
    pub metrics: FitMetrics,             // max/gem. afwijking px en mm, aantal knopen, componenten
}
pub struct ContourTree { nodes: Vec<ContourNode> }         // arena; roots = buitencontouren
pub struct ContourNode {
    pub id: ContourId,                   // deterministisch: volgorde van eerste pixel in rasterscan (y, dan x)
    pub parent: Option<ContourId>, pub children: Vec<ContourId>,
    pub role: ContourRole,               // Outer | Hole (afwisselend per diepte)
    pub pixel_boundary: Vec<[i32; 2]>,   // crack-grens in pixelhoeken, exact
    pub fitted: Path,                    // in mm via pixel_to_mm, gesloten
    pub area_px: u64,
}
impl TraceResult { pub fn to_patch(&self) -> DocumentPatch; }   // DerivedKey.local = ContourId; gat → deel van het
                                                                 // pad van de ouder (even-odd), niet een los object
```

Regels:
- Tracing werkt op de **originele** pixels; uitsnede en schaal zijn parameters, nooit een geresamplede kopie in het
  document. `traced_ring_preserves_hole_at_declared_physical_scale`: ring op 300 dpi → buitendiameter en gat in mm
  binnen `max_deviation`, gat is kind van de ring.
- Contourboom is topologisch exact op pixelniveau (8-/4-connectiviteit vast gekozen en gedocumenteerd: voorgrond
  8-, achtergrond 4-verbonden). Fitting mag de topologie niet veranderen (geen zelfdoorsnijding, gat blijft binnen ouder) —
  property-test.
- Vergelijkingspreview: `Overlay::TraceComparison { trace, show: Pixels|Fitted|Deviation }` in de scene; pixels uit
  `RasterImage`, geen GPU-readback.
- vtracer alleen als benchmarkreferentie (dev-dependency/bench), nooit in `[dependencies]`.
- Receptopslag: `TraceRecipe` is documentdata (serde-DTO in io, schema-versie); presets later als bibliotheek.

## 14. Import — `burnburn-io::import` (+ `burnburn-image` voor PNG/JPEG)

```rust
#[non_exhaustive] pub enum ImportFormat { Svg, Dxf, Png, Jpeg }
pub struct ImportLimits { pub max_bytes: u64, pub max_entities: u32, pub max_nesting: u16, pub max_path_segments: u64 }

/// Step 1, no side effects: what is in the file and which decisions are needed.
pub fn analyze(bytes: &[u8], format: ImportFormat, limits: &ImportLimits) -> Result<ImportAnalysis, ImportError>;
pub struct ImportAnalysis {
    pub declared_unit: DeclaredUnit,            // Known(Unit) | Unitless | Pixels{ css_dpi: f64 } | Conflicting{..}
    pub extents_file_units: Option<Bounds>,
    pub questions: Vec<ImportQuestion>,         // bv. NeedsUnit{ suggestions: Vec<(Unit, Bounds_mm)> }
    pub preview_report: ImportReport,
}
/// Step 2: decisions explicit; produces geometry, not yet a command.
pub fn import(bytes: &[u8], format: ImportFormat, decision: &ImportDecision, limits: &ImportLimits)
    -> Result<ImportedGeometry, ImportError>;
pub struct ImportDecision { pub unit: UnitDecision, pub placement: Affine /* , layers: …, */ }
pub enum UnitDecision { UseDeclared, Explicit(Unit), PixelsPerInch(f64) }
pub struct ImportedGeometry { pub asset: ContentHash, pub objects: Vec<ImportedObject>, pub report: ImportReport }
pub struct ImportReport {
    pub format: ImportFormat, pub asset: ContentHash,
    pub unit: AppliedUnit,                      // { unit, how: Declared | UserChosen }
    pub imported: u32,
    pub notes: Vec<ImportNote>,                 // { severity: Info|Warning|Dropped, code: ImportNoteCode, entity: Option<String>, count: u32 }
}
// Commit: CommandPayload::Import { asset_bytes: Arc<[u8]>, objects: Vec<ImportedObject>, report: ImportReport }
// → één historie-entry; asset in de container; objecten met GeometrySource::Imported { asset, entity }.
```

Regels:
- **Eenheidsbeslissing:** een eenheidloos bestand (DXF zonder of met `$INSUNITS = 0`, SVG zonder fysieke `width/height`)
  geeft `ImportError::UnitDecisionRequired` bij `UnitDecision::UseDeclared`; er is **geen stille default**
  (`unitless_dxf_needs_explicit_scale_before_import_commit`). SVG `px` = CSS-px (96/inch) alleen als *voorstel* in
  `questions`, behalve als de gebruiker dat expliciet kiest.
- Alles wat niet geïmporteerd wordt (onbekende entiteit, tekst zonder font, clip, gradient, 3D) komt als `Dropped` in
  het rapport met telling; niets verdwijnt stil. Het rapport wordt in het document bewaard bij het asset.
- Parsers: SVG via `usvg` (`default-features = false`: geen tekst/fontdb), DXF via een **eigen** begrensde
  ASCII-DXF-lezer (subset LINE, LWPOLYLINE, POLYLINE, ARC, CIRCLE, ELLIPSE, SPLINE, INSERT zonder diepe nesting), PNG/JPEG
  via `burnburn-image::decode`. Alle drie fuzz-doelen (Testing). Het `dxf`-crate is geen runtime-dependency
  (groot, geen limieten); hooguit testreferentie.
- Geometrie-export (beperkte SVG/DXF): eigen writers in `io::export`, roundtriptest + onafhankelijke parse met usvg.

## 15. Proces, CAM en JobPlan — `document::process`, `burnburn-cam`, `burnburn-job`, `ruida::lower`

```rust
// Review fase B (DR-15): gebouwd als `ProcessLayer { operations, constraints, recipes }` met `Operation::targets()
// -> &[GeometryId]`, `OperationMode`, `ProcessSettings`, `MaterialRecipe` + `RecipeRef { recipe, revision }`. Leidend.
// `OperationRef` hieronder = voorlopig gewoon `GeometryId`; IM21 maakt er `GeometryRef` van (migratie v1→v2).
// burnburn-document::process (data; eigenaar CAM-rol). Ontwerplaag en proceslaag zijn gescheiden:
// een Operation verwijst naar geometrie, kopieert haar nooit.
pub struct Operation {
    pub id: OperationId,                 // bestaande gedeelde IdAllocator
    pub name: String,
    pub kind: OperationKind,
    pub settings: OperationSettings,     // of MaterialRecipeRef + overrides (C08); waarden eindig en begrensd
    pub targets: Vec<OperationRef>,
    pub enabled: bool,
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OperationRef { pub geometry: GeometryId /* , sub: Option<SubRef> — IM21 */ }
#[non_exhaustive] pub enum OperationKind { Line, Fill { interval: Length, angle: Angle, bidirectional: bool },
                                           Image { /* IM18 */ } }
pub struct ProcessPlan { operations: Vec<Operation> /* volgorde = uitvoervolgorde */, constraints: Vec<OrderConstraint> }
// Commands: CommandPayload::Process(ProcessCommand) — CreateOperation, Assign{operation, refs}, Unassign, SetSettings,
// Reorder, SetEnabled. Eén variant in CommandPayload; de enum leeft in de processmodule.

// burnburn-job (aanvulling op §8)
pub struct ProfileRevision(ContentHash);      // hash van het canonieke machineprofiel (bed, origin, capabilities, calibratie)
pub struct MachineFrame { pub bed: Bounds, pub origin: OriginCorner, pub y_down: bool }

// burnburn-cam
pub struct CompileRequest {
    pub snapshot: Arc<DocumentSnapshot>,
    pub project_hash: ContentHash,           // canonieke projecthash (io::project_hash) van precies deze snapshot
    pub machine: Arc<MachineProfile>,        // frame + MachineCapabilities + ProfileRevision
    pub options: CompileOptions,             // flatten-tolerantie (productie, eigen type), job origin, optimalisatie
}
pub fn compile(req: &CompileRequest, cancel: &CancellationToken) -> Result<CompileOutcome, CompileError>;
pub struct CompileOutcome { pub plan: Option<Arc<JobPlan>>, pub report: PreflightReport }
pub struct PreflightReport { pub plan: Option<JobPlanId>, pub source_revision: RevisionId, pub diagnostics: Vec<Diagnostic> }
pub struct Diagnostic {
    pub severity: Severity,                  // Error (blokkeert export/upload) | Warning | Info
    pub code: DiagnosticCode,                // OutsideBed, EmptyOperation, OpenPathInFill, UnsupportedFeature{..},
                                             // ValueOutOfRange{..}, SketchNotSolved{..}, ReferenceBroken{..}, StaleDerived{..}
    pub sources: Vec<SourceRef>,             // (GeometryId, OperationId) — altijd minstens één als de oorzaak geometrie is
    pub at: Option<Bounds>,                  // documentframe, mm (voor highlight)
    pub message: MessageKey,
}
impl PreflightReport { pub fn is_blocking(&self) -> bool; }

/// What presentation holds per document after a compile; preview, export and upload read *this*.
pub struct PreparedJob { pub plan: Arc<JobPlan>, pub report: Arc<PreflightReport>, pub source_revision: RevisionId }

// burnburn-ruida::lower
pub struct RuidaProfile { /* model, firmware-range, capabilities, quantisatie, snelheids-/vermogensgrenzen */ }
pub fn lower(plan: &JobPlan, profile: &RuidaProfile) -> Result<RdProgram, LowerError>;
#[non_exhaustive] pub enum LowerError { Unsupported { event: usize, feature: Feature }, OutOfRange { event: usize, field: &'static str } }
```

Regels:
- **Eén bron:** `two_operations_share_one_geometry_without_duplicate_source` — twee operations met dezelfde
  `OperationRef` laten het object één keer in het document; de JobPlan bevat beide passes, elk met eigen `SourceRef`.
- **Preflight noemt de bron:** elk geometrisch probleem verwijst naar `SourceRef { geometry, operation }`
  (`outside_bed_operation_links_error_to_source_geometry`); de UI selecteert/markeert via die ID's.
- **Plan ook bij fouten**, zolang het geometrisch kan: de preview toont dan de foute moves. Export en upload eisen
  `!report.is_blocking()` en een `PreparedJob` waarvan `report.plan == Some(plan.id())`.
- **Preview = export = upload:** preview-overlay (`Overlay::PlanPreview { plan: Arc<JobPlan> }`), RD-export-taak
  (`TaskInput::ExportRd { plan: Arc<JobPlan>, profile, path, fs }`) en upload (`MachineAction::Upload { plan, rd_hash }`)
  krijgen **dezelfde** `Arc<JobPlan>` uit `PreparedJob`; niemand compileert stil opnieuw. Test:
  `preview_export_and_upload_name_the_same_plan_id`. Een documentwijziging na compile maakt de `PreparedJob`
  *verouderd* (viewmodel), niet ongeldig voor een al gestarte export; een nieuwe export eist een nieuwe compile.
- `JobPlanId` = domeingescheiden SHA-256 over de canonieke planinhoud incl. `PlanSource`
  (`CanonicalEncode`, ADR 0004); deterministisch: zelfde snapshot + profiel + compilerversie → zelfde ID (CLI = GUI, IM30).
- **Kerf en flattening op één plek:** in `cam`, met een eigen `ProductionTolerance`; `lower` kwantiseert alleen
  (µm/controllereenheden) en interpreteert geen geometrie. Kerfcompensatie wordt per operation één keer toegepast en in
  `OperationSettings` vastgelegd (IM22: niet dubbel).
- Machineframe: `cam` zet documentframe → machineframe om (origin, y-richting, job origin). `PlanEvent` bevat alleen
  machinecoördinaten.
- Onbekende/niet-ondersteunde instelling: preflight-`Error UnsupportedFeature` op basis van `MachineCapabilities`;
  `lower` faalt nogmaals als tweede verdediging — nooit stil vervangen.
