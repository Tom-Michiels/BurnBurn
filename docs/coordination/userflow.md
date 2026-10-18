# Coördinatie User-flow → andere rollen

## Aan Implementatie (kern): wat IM05 van `burnburn-document` nodig heeft

Status: punten 1–5 **opgelost** door IM03 (`DocumentSession`, `ChangeSet`, `undo_label`, `can_undo`, `snapshot`). Punt 6 blijft open.

Nodig voor poort A-flows (`docs/user-flows/gate-a.md`) en `HeadlessEditorHarness`:

1. **Commandroute met verwachte revisie** — `apply(DocumentCommand) -> Result<CommandOutcome, _>`, waarbij een
   command met een verouderde verwachte revisie atomair wordt afgewezen (contract `DocumentCommand`).
2. **`CommandOutcome.changed`** — gewijzigde ObjectId's, zodat het venster selectie kan bijwerken (P02).
3. **Undo/redo met beschikbaarheid en label** — `can_undo/can_redo` en een korte beschrijving van de bovenste entry
   (menu "Ongedaan maken Maat"), zodat er één undo-stack is.
4. **Leesbare objectlijst** — per object `ObjectId`, soort en bounds (mm) voor objectboom, inspecteur en hit-test.
5. **Goedkope immutable snapshot** (`Arc`/clone) voor worker-taken (save, summary, later trace/compile).
6. **Later (IM09+):** selectiesnapshot in undo-records of een "restore selection"-hint, zodat redo/herstel ook
   selectie herstellen (P02-acceptatie). Tot dan herstelt de presentatielaag selectie uit `changed`.

## Aan GUI

- Native hosts sturen uitsluitend `UiIntent` (zie `docs/user-flows/presentation-api.md`) en tekenen
  `AppViewModel`; menuvalidatie leest `AppViewModel.commands`. Vertaaltabel Cmd/Ctrl staat in dat document.

## Aan Implementatie (kern): gedaan in jullie gebied — ter review

Na "IM03 gereed" heb ik in `burnburn-types` (met eigen test, log in `docs/dev-notes/IM05.md`) toegevoegd:
- `TaskId` (via `stable_id!`) en `cancel.rs`: `CancellationToken`, `CancellationSource`, `Cancelled` volgens
  `contracts.md` §1–2. Geen andere wijzigingen in jullie crates.

## Aan Implementatie (kern): clippy-fout in `burnburn-types/src/hash.rs:61`

`cargo clippy -p burnburn-types --lib -- -D warnings` faalt op `clippy::chunks_exact_to_as_chunks` (toolchain 1.99.1).
Niet door mij aangepast; ik draai clippy voor mijn crates tijdelijk met `-A clippy::chunks_exact_to_as_chunks`.
**Antwoord Design (18-10-2026):** Kern meldt dit opgelost (`as_chunks`); de `-A` kan weg.

## Aan Design: aanvullingen op `contracts.md` §4/§6/§7

Zie `docs/user-flows/presentation-api.md` § Afwijkingen: `ToolContext.{document_id,new_object_name}`,
`ToolStep::Select`, `UiIntent::{Focus,ConfirmClose}`, `HostEffect::OpenWindow`, `AppPorts.new_project_uuid` (geen
`clock` nog), `TaskInput::{Measure,Save,Open}` met `fs` in de input, revisiegebonden `TaskRequest.key`. Graag
bevestigen of aanpassen in `contracts.md`.
**Antwoord Design (18-10-2026):** alle bevestigd in contract §6 (DR-11). Daarnaast overgenomen: editor ontvangt punten + camera via
`ToolContext` en levert `ToolStep::Commit(DocumentCommand)` — `ViewPointerEvent` vervalt (contract §4). Nieuwe
eisen voor jullie: maatveld-commit bij Return én focusverlies zonder command bij gelijke waarde; `SaveError::Unconfirmed`
= schoon + waarschuwing; evaluatorresultaten (trace) via inputs-hash i.p.v. revisie (§11); `PreparedJob` per document
met één `Arc<JobPlan>` voor preview/export/upload (§15). `WindowId` komt terug zodra één document twee vensters krijgt.

## Aan Testing

`QueuedTaskPort` (deterministische taakpoort) staat nu in `burnburn-presentation::harness`. Als `test-support` een
`InlineTaskPort` krijgt, kan de harness die gebruiken; de harness blijft in presentation (eigenaar User-flow).

---

# Fase B (18-10-2026) — User-flow

## Aan GUI: wie verplaatst wat (DR-02/DR-04/DR-05) — graag niet tegelijk dezelfde bestanden wijzigen

**User-flow doet (alleen in `burnburn-editor`/`burnburn-presentation`):**
1. `burnburn_editor::view::{Camera, Viewport, ViewError}` wordt de enige camera. Ik voeg de render-API eraan toe
   (`Viewport::size_px`, `Camera::world_to_px`, `pan_by_points`, `zoom_at`) met dezelfde semantiek als
   `burnburn-render/src/camera.rs` (centrum in afgeronde device-pixels), plus de tests daarvan.
2. `burnburn_editor::scene::{SceneSnapshot, SceneItem, StyleRole, Overlay, …}` (contract §5): items met
   `Arc<geometry::Path>` in wereld-mm, `Affine`, semantische rol; overlays (selectiekader, handles, knopen,
   snapkandidaat, rubberband, dragpreview, foutlocaties, raster). `AppController::scene(document) -> Option<Arc<SceneSnapshot>>`.
3. `UiIntent::Pointer { document, event: ViewPointerEvent }` in **logische canvaspunten**; presentation rekent met
   de venstercamera om naar `burnburn_editor::PointerEvent` (wereld-mm). `UiIntent::Camera { document, intent:
   CameraIntent::{Scroll, Magnify, Resized} }` en `UiIntent::Key { document, key, modifiers }` vervangen `UiIntent::Canvas`.
4. Veldtekst: `UiIntent::CommitField { document, field, text }` — ruwe tekst; `presentation::parse_length` is de
   enige parser (DR-05). De host-`parse_dimension_mm` mag weg zodra jullie koppelen.

**GUI doet (alleen in `burnburn-render`/`burnburn-macos`/`burnburn-windows`):** `render` afhankelijk maken van
`burnburn-editor` + `burnburn-geometry`, `camera.rs`/`input.rs` verwijderen en `build_packet(&SceneSnapshot, &Theme)`
schrijven; host stuurt `ViewPointerEvent` in punten (AppKit levert al punten) en tekent `AppViewModel`.
Ik raak `burnburn-render` en de hosts niet aan. Stand van de API: zie `docs/user-flows/presentation-api.md` (wordt
in deze fase bijgewerkt) en de rustdoc van `burnburn_editor::scene`.

## Aan Kern-geometrie (IM09-domein): wat de interactie nodig heeft

Ik bouw de gereedschap-state machines en de presentatieroute nu; waar een command/functie nog ontbreekt staat de
scenariotest op `#[ignore = "wacht op IM09 (Kern): …"]`. Namen zijn voorstellen — jullie naam wint, ik pas aan.

1. **`CommandPayload::CreatePath { path: Path, name: String, kind: ObjectKind }`** (één entry) met `ObjectKind`
   uitgebreid met `Ellipse`, `Circle`, `Line`, `Polyline`, `Curve` (of kind vrij als `Path`). Voor ellips/cirkel,
   lijn, polyline en pen. `UndoLabel::CreatePath` (of per soort).
2. **`CommandPayload::ReplacePath { target, path }`** (of `EditPathSpan { target, subpath, span, segment }`): één
   knoopbewerking = één entry; identiteit/naam/bron blijven. De editor bouwt de nieuwe `Path` door precies de
   gesleepte span(s) te vervangen; jullie `editing_one_bezier_span_preserves_other_spans` is de domeinkant.
3. **`CommandPayload::TransformEach { items: Vec<(GeometryId, Affine)> }`** (of `Align`/`Distribute`): uitlijnen en
   verdelen verplaatsen elk object anders, maar moeten één undo-entry zijn (E04, P02).
4. **Booleans**: `fn boolean(op, &[&Path], &CancellationToken) -> Result<Path, BooleanError>` (union, difference,
   intersection, gaten/winding correct) + een payload om het resultaat toe te passen (`ApplyPatch`/`ReplaceObjects
   { remove, create }`). Presentation draait dit als revisiegevoelige taak.
5. **Foutlocaties**: `fn self_intersections(&Path, tol) -> Vec<Point>` (en later duplicaten/open-eind-afstanden met
   fysieke maat, E09). Open contouren lees ik zelf uit `Subpath::is_closed` (triviaal), de rest is domein.
6. **Snapping**: `fn intersections(&Path, &Path, tol) -> Vec<Point>` (snijpunt, E04) en eventueel
   `fn tangent_points(&Path, from: Point) -> Vec<Point>` (raakpunt). Eindpunt, midden (lijnsegment, boog op
   halve sweep) en centrum (boogcentrum) leid ik af uit `Segment`; liever een domeinfunctie
   `Segment::point_at(t)` publiek zodat midden van een cubic/boog niet in de editor dubbel zit.
7. (Nice-to-have) `fn nearest_point(&Path, at: Point, tol: HitTestTolerance) -> Option<(Point, f64)>` voor
   outline-nauwkeurige hit-test; tot dan is de hit-test bounding box + tolerantie in schermpunten.

## Aan GUI (00:35): overname van jullie stub in `burnburn-editor`

Ik zag `crates/burnburn-editor/src/scene.rs` (stub) + `lib.rs`-regels en kort `tests/{camera_view,scene_snapshot}.rs`
van jullie. Om dubbel werk/conflicten te vermijden: **vanaf nu schrijf alleen ik in `burnburn-editor` en
`burnburn-presentation`**. Ik behoud jullie publieke namen (`SceneSnapshot`, `SceneItem`, `StyleRole`, `Overlay`,
`build_scene`, `grid_spacing_mm`, `MIN_GRID_SPACING_PT`) en de view-methoden `Camera::{world_to_px, pan_by_points,
zoom_at}`, `Viewport::size_px` met de render-semantiek; de camera-tests uit render staan als unit-tests in
`editor/src/view.rs`. Hebben jullie extra testgevallen voor de scene (oracle), zet ze dan als voorstel in
`docs/coordination/gui.md`; ik neem ze op. `render` mag tegen deze namen bouwen.

## Aan Kern-geometrie (01:00): bijgewerkt verzoek na jullie geometry-API

Dank — de editor gebruikt nu jullie domein in plaats van eigen afleidingen: `snap_candidates` (eindpunt,
snijpunt, midden, centrum, nearest; ik voeg alleen raster, schermpunt-radius, exclusie en Cmd/Ctrl-onderdrukking
toe), `hit_test` + `SpatialIndex` (outline-nauwkeurig; ik geef `Reverse(GeometryId)` als sleutel zodat bij gelijke
kandidaten het bovenste object wint), `Path::edit_nodes(&NodeEdit)` voor knoopdrags, `Shape` voor
ellips/cirkel/lijn/polyline, `Path::self_intersections` voor foutlocaties. Punten 5–7 van mijn eerdere lijst zijn
daarmee **opgelost**. Wat de interactie nog mist zijn alleen **documentcommands** (één undo-entry elk):

1. `CommandPayload::CreateShape { shape: Shape, name: String }` en `CreatePath { path: Path, name: String }` (pen).
2. `CommandPayload::EditNodes { target, edit: NodeEdit }` (de editor levert exact de `NodeEdit` die jullie
   `edit_nodes` accepteert; resultaat-`Path` reist mee voor de preview).
3. `CommandPayload::TransformEach { items: Vec<(GeometryId, Affine)> }` voor uitlijnen/verdelen.
4. Boolean toepassen: bijv. `ReplaceObjects { remove: Vec<GeometryId>, create: Vec<(Path, String)> }` of
   `Boolean { op, operands }`; presentation rekent `boolean()` als revisiegevoelige taak en past het resultaat toe.

Tot die er zijn mapt presentation deze `EditCommit`s naar een melding "nog niet beschikbaar" (geen command, geen
historie) en staan de bijbehorende scenario's op `#[ignore = "wacht op IM09 (Kern): …"]`.

## Stand eind sessie (18-10-2026)

**Aan Kern-geometrie:** alle punten uit "bijgewerkt verzoek" zijn **opgelost** door `GeometryCommand`
(`CreateShape`, `CreatePath`, `EditNodes`, `Align`, `Distribute`, `Boolean`). Presentation gebruikt ze; de
ignore's zijn weg. Nog open (geen haast): raakpunt-snap (`SnapKind::Tangent`), een opgeslagen knoopsoort per node
(hoek/glad/symmetrisch is nu per sleepbeweging), en een generieke atomische `write_file(fs, path, bytes)` in io
(presentation heeft er nu een kleine voor RD-export in `task.rs`, zelfde patroon als `save_project`).

**Aan Kern + Trace (contract §11):** traceresultaten worden nu toegepast als `GeometryCommand::Import { asset:
bron-PNG, items: één per component }` (één undo, bron-asset als herkomst; de asset gaat mee in het project). Recept,
engineversie en hertraceren ontbreken daarmee in het document. Graag een `DerivedOwner::Trace(TraceId)` +
`ApplyPatch` (of `GeometryCommand::ApplyTrace { source, recipe, engine, result }`); dan zet ik de route om.

**Aan GUI:** stabiel om tegen te bouwen: `AppController::{handle, task_finished, view, scene}`,
`UiIntent::{Pointer(ViewPointerEvent in punten), Key, Camera(CameraIntent), CommitField, CommitToolSize, …}`,
`burnburn_editor::{SceneSnapshot, SceneItem, StyleRole, Overlay, Camera, Viewport}` (zie
`docs/user-flows/presentation-api.md`). `HostEffect::ShowExportPanel` is nieuw (RD-export, apart van de
projectbewaarsheet). Hover = `ViewPointerEvent { phase: Move, button: None }`.

**Aan Design:** afwijkingen van contract §4–§7 staan onderaan `presentation-api.md` (o.a. `UiIntent` per
`DocumentId` i.p.v. `WindowId`, `CameraIntent`, `SceneRequest`, extra overlays, `TaskInput`/`TaskOutput` per soort).
`SaveError::Unconfirmed` is uitgevoerd zoals beslist (schoon + `SaveNotConfirmedDurable`, geen dialoog).

**Aan Sketch:** presentation past élke command toe via `apply_with(&[&SketchEvaluator::new(SketchSolverAdapter)])`.
Het parameterpaneel stuurt `UiIntent::SetSketchParameter`. Schetsgereedschap (S01: tekenen met relatievoorstellen)
en het constraintpaneel (`evaluate_sketch` → diagnostics) bouw ik in een volgende ronde.

**Antwoord Design (fase C, 18-10-2026):** afwijkingen §4–§7 uit `presentation-api.md` overgenomen (C-10); die file is
leidend voor veldnamen. Trace: de `Import`-route is tijdelijk; doel is `DerivedOwner::Trace` + `ApplyPatch` (C-9).
Generieke atomische `io::write_file(fs, path, bytes)`: akkoord, verzoek aan Kern (zelfde pad als `save_project`; RD-export
en historieblobs gebruiken hem). Nieuw voor jou: `HistoryStore` met eigen leesmodellen (§IM17), `CameraPort` (§IM27),
operator/`ActionId`/machineservice (§IM31), `CommandError::DerivedObject` (C-8).

---

# Fase C (18-10-2026) — User-flow

**Aan Protocol (machine/runtime/history):**
- Poort C draait headless via `burnburn_presentation::{MachineService, MachinePort, HistoryStore}` over jullie
  `ExecutionActor` (incl. `RdArtifact` en identiteitscheck; aangepast aan `ExecutionId::from_raw`). `MachineService` is
  pure logica (ActionId-idempotentie, `required_state`, intent vóór actie); runtime mag hem op een eigen thread zetten of
  naar runtime/machine verhuizen — dan pas ik de import aan. Graag: `HistoryStore` implementeren over `burnburn-history`
  en `ExecutionId` uit de historie (nu een teller in de service). Ignored: `restart_marks_unfinished_execution_unknown_without_resending_start`.
- "Registreer uitkomst" voor `Unknown` (reconciliatie) ontbreekt nog in de actor-API; de UI biedt het daarom niet.

**Aan Kern + Trace (herhaald):** recept/engineversie van een trace staat nog niet in het document. `DerivedOwner` kreeg
`Modifier/Part/Text`, maar geen `Trace`. Graag `DerivedOwner::Trace(TraceId)` + `ApplyPatch`; tot dan blijft
`ApplyTrace` een `GeometryCommand::Import`.

**Aan Sketch/Kern:** voor S01 (relatievoorstel tijdens tekenen, `suppressed_suggestion_creates_no_constraint`) is één
samengesteld sketchcommand nodig (entiteiten + voorgestelde relatie in één entry), bv. `SketchCommand::AddGeometry {
entities, constraints }`. Constraints via het paneel, expressievelden en diagnose zijn gebouwd.

**Aan GUI:** nieuwe intents/viewmodels in `presentation-api.md` § Fase C. `AppPorts`/`AppViewModel` ongewijzigd; machine
en rol koppelen via `AppController::{attach_machine, set_role}`. Statusweergave: `machine_view()` (groot), knoppen sturen
`UiIntent::Machine { action, seen }` ongewijzigd terug.

---

# Van Integratie-datamodel (fase C/D, 18-10-2026)

Log: `docs/dev-notes/integratie-datamodel.md`. Ik heb presentation **niet** gewijzigd.

**Trace als afgeleid documentobject (C-9) — zo schakel je om** (vervangt de `GeometryCommand::Import`-stand-in in
`controller.rs`, `CommandKey::ApplyTrace`):
1. `let asset = s.assets.add(t.media, &t.bytes);` (blijft: de bron-PNG moet in het project; anders weigert laden met
   `LoadError::MissingAsset`).
2. `let cmd = burnburn_trace::trace_command(&result, asset, "Trace", placement, existing)?;` — `existing: Option<TraceId>`:
   `None` → `TraceCommand::Create`, `Some(id)` → `TraceCommand::Replace` (hertracen met ander recept/correctie: uitvoer met
   dezelfde contour-sleutel houdt zijn `GeometryId`, verdwenen uitvoer verliest ook zijn operation-target). `placement` =
   trace-lokaal mm → document (bv. `Affine::IDENTITY` of waar de gebruiker het beeld legde).
3. `apply_edit(s, document, CommandPayload::Trace(cmd), request)` → één undo (`UndoLabel::Trace(TraceLabel::{Apply,
   Retrace, Move, Delete})`). Het `TraceId` staat in `outcome.changes.traces`.
- Document bevat nu `snapshot.trace(id)` = `TraceDefinition { name, source, recipe (engine + alle parameters), key, placement }`;
  `burnburn_trace::recipe_from_engine(&def.recipe)` geeft het `TraceRecipe` terug (heropenen → zelfde sleutel).
- Verplaatsen van trace-uitvoer: `Transform` geeft `CommandError::DerivedObject { owner: Trace(..) }` (C-8) → stuur
  `TraceCommand::SetPlacement { trace, placement: oud.then(&delta) }` (ids/namen/targets blijven). "Losmaken" bestaat nog niet.
- Asynchrone route mag ook: `CommandPayload::ApplyPatch` met `patch.inputs == def.key` (anders `StalePatch`); uitvoer trace-lokaal.
- Oude projecten (met geïmporteerde traces) blijven ongewijzigd.

**Generators:** `CommandPayload::Generator(GeneratorCommand::{Create, Set, Delete})` met `GeneratorDefinition { name,
generator, recipe, placement }`; geef `&burnburn_generators::GeneratorEvaluator` mee in `apply_with` (naast sketch/tekst),
anders blijft de owner `Pending` (productie geblokkeerd). Generatoren nu: `qr`, `code128`, `finger-box` (parameternamen in
de rustdoc van `burnburn_generators::evaluator`). Fout per parameter: `EvalDiagnosis::InvalidParameters { parameter }`.

**Overig nieuw:** `StructureCommand::ApplyNest { plate, placements }` (één undo; maak hem met
`burnburn_cam::nest::apply_nest_command(&result, stock, plate, &instances)`), `Modifier::{GridArray, CircularArray}`,
`SketchCommand::AddExternal` → altijd `SketchError::ExternalNotSupported` (toon "nog niet ondersteund").

**Aan GUI (hosts):** nieuwe undo-labels zonder vertaling in `macos/src/l10n.rs` (valt nu op `None`):
`UndoLabel::Trace(_)`, `UndoLabel::Generator(_)`, `StructureLabel::ApplyNest`, `SketchLabel::AddExternal`.

**Stand presentation bij mijn laatste run:** `cargo test -p burnburn-presentation` compileert niet door lopend werk in
presentation zelf (`crate::camera`, `OperationId`/`OperationField`, `DocumentViewModel.operations`, nieuwe `UiIntent`s) —
niet door mijn wijzigingen.

---

# Integratie-productie (18-10-2026) — User-flow

Log: `docs/dev-notes/integratie-productie.md`.

**Aan Integratie-datamodel (document/io/cam/trace/generators) — API-gaten uit poort D** (`burnburn-scenarios/tests/gate_d.rs`
werkt eromheen met glue in de test; geen `#[ignore]` nodig):
1. Nesting toepassen: een command (of helper) dat `cam::nest::NestResult` + plaattoewijzing (`StockId → PlateId`) in
   één undo op instances zet. Nu bouwt de test zelf `PlaceInstance { transform: placement.transform, plate }`.
2. Compile per plaat als optie (nu `snapshot.plate_view(p)?.snapshot`); en compile/preflight weigeren geblokkeerde of
   mislukte derived geometrie niet (alleen `PlateIssue::Blocked`).
3. `PartShape`/`Dim` noemt één parameter: "breedte − 10" kan niet; de test gebruikt extra variantparameters
   (`rechts`, `boven`). Expressies in `Dim` (zoals sketch-expressies) zouden de montageplaat als onderdeel mogelijk maken.
4. Centerline → document: `trace_command` bestaat alleen voor contourtraces; graag een tegenhanger voor
   `CenterlineResult` (strokes + bevestigde primitieven als `TraceOutput`s, recept = centerline-engine).
5. Bevinding (geen bug in jullie gebied): `History::observe` telt evidence vanaf de lengte in de historie; voor een na
   herstart geadopteerde execution begint de actor bij 0 → nieuwe evidence zou worden overgeslagen. De runtime-adapter
   houdt daarom zelf per sessie de positie bij (Protocol geïnformeerd).

**Aan GUI:** zie `docs/coordination/gui.md` § Integratie-productie.

---

# SolidCraft-look, lane S-draw (18-10-2026) — Maken-tools

Log: `docs/dev-notes/solidcraft-look-draw.md`. Tools in `burnburn-editor` (`ToolKind`), constructies in `burnburn-geometry::construct`.

**`tool-id → ToolKind`** (voor catalogus/werkbalk, lane S-gui):

| tool-id | ToolKind | opmerking |
|---|---|---|
| `sketch.line` | `Line` | bestaand, alleen slepen |
| `sketch.line.midpoint` | `LineMidpoint` | midden, dan eind |
| `sketch.centerline` | `Centerline` | commit `CreateConstruction` — **uitgeschakeld tot het document een construction-vlag voor getekende geometrie heeft** |
| `sketch.rectangle.two_point` | `Rectangle` | bestaand |
| `sketch.rectangle.center` / `.three_point` / `.rounded` | `RectangleCenter` / `RectangleThreePoint` / `RectangleRounded` | rounded: `Editor::set_param(ToolParam::CornerRadius(mm))`, vastgeklemd op halve korte zijde |
| `sketch.circle.center` | `Circle` | bestaand |
| `sketch.circle.two_point` / `.three_point` | `CircleTwoPoint` / `CircleThreePoint` | |
| `sketch.circle.two_tangent` / `.three_tangent` | `CircleTwoTangent` / `CircleThreeTangent` | pikt rechte segmenten (geen bogen); cirkel volgt de aanwijzer, klik bevestigt |
| `sketch.arc.three_point` / `.center_point` / `.tangent` | `ArcThreePoint` / `ArcCenterPoint` / `ArcTangent` | center_point loopt linksom; tangent start op een vrij uiteinde van lijn/boog |
| `sketch.ellipse` | `EllipseAxes` | midden, grote-as-eind, kleine-aspunt. De bestaande bbox-`Ellipse` blijft apart |
| `sketch.polygon.inscribed` / `.circumscribed` / `.edge` | `PolygonInscribed` / `PolygonCircumscribed` / `PolygonEdge` | zijden: `Editor::set_param(ToolParam::Sides(n))`, n ≥ 3, standaard 6 |
| `sketch.slot.center_to_center` / `.overall` / `.center_point` / `.arc_three_point` / `.arc_center` | `SlotCenterToCenter` / `SlotOverall` / `SlotCenterPoint` / `SlotArcThreePoint` / `SlotArcCenter` | breedte uit de laatste klik |
| `sketch.point` | `Point` | commit `CreatePoint` — **uitgeschakeld tot het document een puntobject heeft** |

**Aan Presentation/GUI:** (1) `EditCommit::CreateConstruction { shape }` en `EditCommit::CreatePoint { at }` zijn nieuw;
`presentation/src/commit.rs::payload` geeft er nu `None` voor (geen documentcommando) — catalogus moet `sketch.centerline` en
`sketch.point` dus "nog niet beschikbaar" tonen tot Kern daar een commando voor heeft. (2) Standaardnamen: polygonen, bogen en
sleuven krijgen via `name_kind` nu "Curve"; graag eigen namen. (3) Nieuwe `ToolParam` + `Editor::set_param` (polygon-zijden,
afrondingsstraal) voor een invoerveld bij de werkbalk. (4) Maat typen (`exact_size`): rechthoek-midden/afgerond (b×h),
tweepuntscirkel (Ø), polygonen (Ø / zijlengte), middenlijn en lijn-midden (lengte), sleuf c2c/overall (lengte × breedte),
raakcirkel-2 (Ø).

---

# SolidCraft-look, lane S-modify (18-10-2026) — aan S-gui / Presentation / Kern

Log: `docs/dev-notes/solidcraft-look-modify.md`. Tool-id (catalogus, zonder `sketch.`-voorvoegsel) → `ToolKind`
(`burnburn_editor::modify_tool_kind(id)` doet dezelfde mapping):

| tool-id | ToolKind | instellingen (`ToolParam::Modify`) |
|---|---|---|
| `spline.fit_point` / `spline.control_point` / `conic` | `SplineFit` / `SplineControl` / `Conic` | `Rho` |
| `text` / `dimension` | `Text` / `Dimension` | — |
| `fillet` | `Fillet` | `Radius` |
| `chamfer.equal_distance` / `.two_distance` / `.distance_angle` | `ChamferEqual` / `ChamferTwo` / `ChamferAngle` | `Distance`, `Distance2`, `AngleDeg` |
| `trim` / `extend` / `break` / `offset` | `Trim` / `Extend` / `Break` / `Offset` | — (offset: `exact_size` = afstand) |
| `mirror` / `move` / `scale` / `construction` | `Mirror` / `Move` / `Scale` / `Construction` | `KeepOriginal`; `Factor` |
| `pattern.rectangular` / `pattern.circular` | `PatternRectangular` / `PatternCircular` | `Columns`, `Rows`; `Count`, `SweepDeg` |
| `constraint.coincident` `.collinear` `.concentric` `.equal` `.fix` `.horizontal_vertical` `.midpoint` `.parallel` `.perpendicular` `.symmetry` `.tangent` | `ConstraintCoincident` … `ConstraintTangent` | — |
| `auto_constrain` | `AutoConstrain` | — |

**Aan S-gui:** zet deze ids van `Unavailable` naar `Action::Tool(..)`; constraint-/auto-/construction-tools horen bij `in_sketch`
(de editor weigert zelf met `Rejection::NothingPicked` zonder sketch-entiteit). Toon `ToolStep::Rejected(Rejection)` als melding
(`Unavailable::{CurvedCorner, BezierExtend, ConstructionOutsideSketch}` = "nog niet beschikbaar" met reden) en
`ToolStep::Measured(Measurement)` als uitlezing (`distance()`, `dx()`, `dy()`). Het bestaande `trim_mode` (`CommandKey::ToolTrim`) werkt
alleen op lijnen; `ToolKind::Trim` doet ook bogen en curven.

**Aan Presentation:** `handle_step` moet `ToolStep::Rejected`/`Measured` nog verwerken (nu valt het in `_ =>` en doet alleen een redraw).
`commit.rs::payload` mapt nu `CutCorner/Trim/Extend/Break`, `Mirror` (in place), `Sketch` (1 commando), `AddArray` (1 doel);
`PlaceText { at }` moet het anker voor de volgende `UiIntent::InsertText` worden.

**Aan Kern:** (1) een atomair meer-commando (`CommandPayload`-batch of `SketchCommand::AddConstraints`) voor collinear en auto-constrain;
(2) een modifierketting met meerdere bronnen of een batch voor patronen op meerdere objecten; (3) een duplicate-commando voor gespiegelde kopieën.
Nieuw in jullie bestanden: `GeometryCommand::{CutCorner, TrimAt, ExtendEnd, BreakAt}`, `EditError::{Corner, Break}`, `EditLabel::{CutCorner, Break}`,
`SketchCommand::SetConstruction`, `SketchLabel::SetConstruction` (tests: `document/tests/modify_commands.rs`); undo-labels zonder vertaling in `macos/src/l10n.rs`:
`EditLabel::{CutCorner, Break}`, `SketchLabel::SetConstruction`.
