# Fase C-besluiten (Design, 18 oktober 2026)

Gelezen: `docs/coordination/*.md` (open vragen aan Design), `phase-c.md`, `layers.toml`, ADR 0001–0008, contracten.
`cargo xtask layers` na C-1: *19 crates, 2 triples, no violations*. Contracten fase C aangescherpt in
[`contracts-phase-c.md`](../design/contracts-phase-c.md); beslissingen voor de eigenaar in
[`open-beslissingen.md`](../design/open-beslissingen.md).

| # | Besluit | Gevolg / wie |
|---|---|---|
| **C-1** | **Crates fase C** (`layers.toml`, ADR 0001 amendement 2). `burnburn-runtime` (L6), `burnburn-history` (L5), `burnburn-cli` (L7 host) blijven **eigen crates** zoals al klaargezet. `burnburn-text` (L3) gaat naar Generators/tekst. **`burnburn-generators` = eigen crate op L4** (types, geometry, document, text) — géén module in sketch: generators hebben `text` nodig (zelfde laag als sketch) en mogen `cam` niet zien (kerf één keer). Modifiers (IM21) krijgen **geen** crate: `document::structure` (data + evaluatie, Kern). `burnburn-scenarios` (Testing, L8, testkit, lege lib, alles dev-dep) overgenomen zoals Testing hem maakte. runtime: `async_runtime`/`network`/`c_toolchain` nu ook verboden; runtime → text (FontSource). Hosts → image (CameraFrame, IM27). presentation, cli, test-support → generators. | Protocol, Generators, Testing kunnen nu crates aanmaken; root `[workspace.dependencies]` krijgt per nieuwe crate één regel (wie hem maakt). |
| **C-2** | **Datamodules in document** (patroon DR-12): `document::text` (`TextObject`, `FontRef`; eigenaar Generators/tekst), `document::generator` (generatordefinities; Generators), `document::structure::{modifier, part, plate, reference}` (Kern, zo gebouwd). Evaluators leveren `DocumentPatch`; `DerivedOwner` krijgt `Text`, `Generator`, `Modifier`, `Part` (instance), `Trace`. | Kern beheert `DerivedOwner`/`CommandPayload`; elke rol één variant (`CommandPayload::Text`, `::Generator`, …). |
| **C-3** | **Schema 2 = alles tot de eerste release.** Schema 2 is niet uitgebracht; alle fase-C-uitbreidingen (sketches, kinds/groups, modifiers, componenten, tekst, `GeometryRef.sub`) blijven **additief in schema 2** (optionele staartvelden, weggelaten als leeg → bestaande hashes gelijk). Bij de eerste release wordt schema 2 bevroren; daarna is elke uitbreiding schema 3 + migratie. Een veld dat niet additief kan, vraagt nu al schema 3 (melden aan Design). | Kern (io), allen die DTO's toevoegen. Zie open-beslissingen B-4 (releasemoment). |
| **C-4** | **Canonieke vorm `groups`** (Kern-vraag 3): bevestigd als optioneel staartveld van een object, `tag("groups")` ná alle vaste velden en vóór latere staartvelden; staartvelden in vaste, in ADR 0004 vastgelegde volgorde: `groups`, `sub-refs`, `edge-keys`. | ADR 0004 amendement C. |
| **C-5** | **Hashdomeinen:** `HashDomain::PlanSource` ("plan-source", CAM-vraag) en `HashDomain::CompiledJob` ("compiled-job", IM17) erbij; `EvalModifier`, `EvalGenerator` ("eval-modifier", "eval-generator") voor evaluatorinputs. Blobs in de historie gebruiken `Asset` (opaque bytes, herberekenbaar). | Kern voegt varianten toe (één change); CAM zet `source_hash` om (plan-id's wijzigen: goldens bewust herzien, geen auto-accept). |
| **C-6** | **CAM-afwijkingen §8 overgenomen**: `PlanSource.source_hash` i.p.v. `project_hash`; `JobPlanId` zonder sessierevisie; `MachinePoint` met `f64`-getters; `bounds() -> geometry::Bounds`. De koppeling naar het projectbestand gebeurt in `CompiledJob.project_hash` (history), niet in het plan. | contracts.md §8/§15 aangepast. |
| **C-7** | **`FileSystem::append`** (nieuw portlid, io): `append(&self, path, bytes) -> Result<u64, FsError>` (nieuwe lengte), plus `MemoryFileSystem`-injectie "gescheurde append" (k van n bytes). Nodig voor het journal (ADR 0008); `write_all` blijft "hele inhoud". Alternatief "record = eigen bestand" afgewezen (tienduizenden bestanden). | Kern (io) — **blokkeert IM17-append**; tot dan mag history testen tegen een lokale testfake. |
| **C-8** | **Afgeleide objecten** (Sketch/Kern-vraag 3): `SetDimension`, `EditNodes`, `Transform` op afgeleide geometrie → `CommandError::DerivedObject { owner }`. Een sketch/tekst/generator verplaatsen = command op de definitie (`SketchCommand::SetPlacement`, `TextCommand::SetPlacement`, …). "Losmaken" (`Detach`) maakt er gewone geometrie van (één undo). | Kern (`session.rs`), Sketch (`placement`). |
| **C-9** | **Trace naar het document** (User-flow-vraag): `DerivedOwner::Trace(TraceId)` + `document::trace` (bron-asset, recept, engineversie) + `CommandPayload::ApplyPatch` (contract §11) — de `Import`-route is een tijdelijke stand-in. | Kern (patch/ApplyPatch), Trace (`TraceId`, recept-DTO in io), User-flow (route omzetten). |
| **C-10** | **presentation-api-afwijkingen** (§4–§7) overgenomen: `UiIntent` per `DocumentId`, `CameraIntent`, `SceneRequest`, extra overlays, `TaskInput`/`TaskOutput` per soort, `AppPorts` zonder klok, `QueuedTaskPort` in de harness. Contract verwijst naar presentation-api.md als leidend voor veldnamen. | — |
| **C-11** | **Test-fakes:** `MemoryFileSystem` (io) is de fake voor `FileSystem` (contract §10); geen wrapper in test-support. Fake controller voor live UDP = `test-support::FakeRuidaController` op 127.0.0.1 (binnen het testproces), te bouwen door Testing met Protocol. | Testing, Protocol. |

## Antwoorden fase C op open vragen (18-10-2026)

Inline beantwoord met "Antwoord Design (fase C)": `core.md` (register, kurbo, schema 2, canonieke groups,
`DiagnosticTolerance`, `non_exhaustive`), `cam.md` (§8-afwijkingen, `PlanSource`-domein), `sketch.md` (register,
schemavraag, `DerivedObject`, `placement`), `trace.md` (§11, image-status, `read_manifest`), `userflow.md` (trace-route,
`write_file`, presentation-api), `gui.md` (register, hosts → io), `testing.md` (scenariocrate, `dev_only`-groep).

## Fase C — review tijdens het werk (18-10-2026, ronde 1)

Gelezen (in aanbouw, dus voorlopig): `document/src/structure/*` (IM21, Kern), `document/src/process/advanced.rs` (IM25, CAM),
`image/src/raster.rs` (IM18, Trace/CAM), `burnburn-text` (Generators), `burnburn-generators` (leeg), `burnburn-runtime`
(`clock.rs`, `udp.rs`, Protocol), `burnburn-scenarios/tests/*` (Testing). `cargo xtask layers`: *22 crates, 2 triples, no violations*.

### Overgenomen (contract aangepast)

| Bron | Wat | Waarom beter |
|---|---|---|
| Kern `structure::reference` | `EdgeOrigin { source, edge, copy }`, `EdgeName { origin, index, of }`, `SubRef::Vertex { edge, end }`, `ProvenanceEdge`-runs | Vlakker dan Design's geneste stap-origins; vertex als eind van een benoemde edge is eenduidig. §IM21 |
| Kern `structure::modifier` | `EvalMode::{Inline, Background}` per keten | Zware booleans als taak met `StalePatch` zonder de UI te blokkeren |
| Kern `structure::{part, plate}` | `PartInstance.plate`, `DerivedOwner::Part`, rechthoekige `Plate` | Eén richting van verwijzing; minder invarianten |
| CAM `process::advanced` | `SharedEdgePolicy` op *exact gedeelde* segmenten, tweede voorkomen = laser-uit travel + waarschuwing | Werkt zonder herkomst (trace-grenzen zijn bit-gelijk); §IM32 punt 1 aangepast |
| Protocol `runtime::udp` | `check_peer` (geen broadcast/multicast/unspecified/poort 0), `UdpStats` voor vreemde afzenders, `WireDialect` + `NoVerifiedDialect` | Maakt "geen discovery" en "RD-dialect onbevestigd" structureel |
| Protocol `machine::frame` | `FramePlan` (alleen `Travel`, vermogen 0, `source()` = productieplan) in machine i.p.v. `cam::frame_plan` | Pure afleiding van een bevroren plan; geen cam-afhankelijkheid voor machine/runtime. §IM16 |
| Trace/CAM `image::raster` | Tooncurve als 256-LUT via `libm::pow`, afronding half-up, integer-dither | DR-19 nagekomen; determinisme per constructie |

### DR-20 — stabiele `EdgeKey` voor getekende bronnen (Kern, **hoog**)
- **Probleem:** `identity_origins` noemt elk segment van een getekend object naar zijn *index* (`SegmentRef`). De bron van
  een modifier blijft bewerkbaar (E06): `InsertNode` in segment 1 verschuift segment 3 → 4, en een referentie naar
  "edge 3" wijst daarna stil naar de buur. Dat is precies de verboden herbinding.
- **Voorstel:** per getekend object een `edge_keys: Arc<[EdgeKey]>` parallel aan de segmenten (per-object teller),
  onderhouden door `NodeEdit` volgens de tabel in §IM21; `EdgeOrigin.edge` wordt die sleutel. Opslag als optioneel
  staartveld `edge-keys` (C-4). Test eerst: `inserting_node_in_source_does_not_rebind_reference_to_later_edge` en
  `deleting_node_breaks_reference_to_merged_edge`.

### DR-21 — hosts → io (GUI) — gepland
- Nu runtime bestaat: zodra `burnburn_runtime` `StdFileSystem` re-exporteert (Protocol), haalt GUI `burnburn-io` uit de
  hosts en Design uit `layers.toml`. Geen haast; geen gedragswijziging.

### DR-22 — tekstdata hoort in het document (Generators, **hoog**)
- **Probleem:** `TextObject`/`FontRef`/`FontSize` wonen in `burnburn-text::model`; `text` hangt niet van `document` af en
  levert een eigen `TextOutput`/`TextStatus` i.p.v. `DocumentPatch`/`EvalStatus`. Dan kan tekst niet in snapshot, undo
  of projectbestand (zelfde probleem als DR-12 bij sketch).
- **Voorstel:** pure data naar `burnburn-document::text` (module-eigenaar Generators), `CommandPayload::Text(TextCommand)`;
  `burnburn-text` krijgt dependency `burnburn-document` (toegestaan) en implementeert `document::Evaluator`; shaping en
  outlines blijven puur in `text`. `TextStatus::MissingGlyphs` → `EvalStatus::Valid` + diagnose (geen fout: outlines zijn
  actueel), `FontMissing`/`Failed` → `EvalStatus::Failed`. Generatordata idem in `document::generator` vóór de eerste generator.

### DR-23 — feature-afleiding in preflight (CAM)
- `AdvancedOptions` (tabs, leads, overcut, perforatie, z-step, subbewerkingen) bestaat; preflight controleert nog geen
  `MachineCapabilities`. Graag `required_features(&Operation)` (één plek) + `Finding::UnsupportedFeature { feature, support }`.
  Test: `unverified_feature_blocks_export_with_reason`, `default_options_require_no_feature`. `lower` als tweede verdediging.

### DR-24 — `FileSystem::append` (Kern) — blokkeert IM17
- Zie C-7. Test: `append_extends_file_and_sync_makes_it_durable` en in `MemoryFileSystem` `torn_append_keeps_prefix_only`.

### DR-25 — `CommandError::DerivedObject` en `Sketch.placement` (Kern + Sketch)
- Zie C-8. `ReferenceBroken`, `StalePatch`, `ApplyPatch` staan er nu (Kern, dank); `DerivedObject` nog niet.

### Stand DR's na ronde 1 fase C

| DR | Eigenaar | Status |
|---|---|---|
| DR-10, DR-14, DR-15, DR-17, DR-18 | Protocol / Trace / CAM / Kern | **afgehandeld** (gezien in code) |
| DR-12 (+aanvulling), DR-13 | Sketch + Kern | afgehandeld (document::sketch, geometry::math); `placement` → DR-25 |
| DR-02, DR-11 | GUI + User-flow | volgens userflow.md/gui.md afgehandeld; **niet** door Design nagelezen — eindcontrole bij IM20 |
| DR-16 | wie de root raakt | open (`serde_json` "1.0.145", `tempfile` "3" in root) |
| DR-20 | Kern | open, **hoog** |
| DR-21 | GUI (+Protocol re-export) | gepland |
| DR-22 | Generators | open, **hoog** |
| DR-23 | CAM | open (in aanbouw) |
| DR-24 | Kern | open, blokkeert IM17-append |
| DR-25 | Kern + Sketch | open |

---

# Design-review van bestaande crates (17 oktober 2026)

Gelezen: `crates/burnburn-{types,geometry,document,editor,render,ruida,macos,test-support}`, `xtask/src/layers.rs`,
`docs/user-flows/presentation-api.md`, `docs/coordination/{testing,userflow}.md`. Uitkomst `cargo xtask layers`:
*9 crates, 2 triples, no violations*.

Waar een rol een pragmatisch beter ontwerp had, is [`docs/design/contracts.md`](../design/contracts.md) aangepast
(niet de code). Wat hieronder staat, is een **wijzigingsverzoek** aan de eigenaar; Design wijzigt hun code niet.

## Overgenomen van andere rollen (contract aangepast, geen actie nodig)

| Bron | Wat | Waarom beter |
|---|---|---|
| editor (`tool.rs`, `input.rs`) | `EditCommit` + `ToolOutput` i.p.v. dat de editor zelf een `DocumentCommand` maakt | Editor hoeft `RequestId`/`RevisionId` niet te kennen |
| editor | `Modifiers { shift, alt, primary }`, `PointerPhase::{Press, Move, Release, Cancel}` | Duidelijker dan `command/control`; Ctrl-klik wordt al in de host `Secondary` |
| editor | `ToolState { Idle, Preview }` zonder `Committing` | Commit is synchroon op de UI-thread; extra toestand is niet waarneembaar |
| presentation-api.md | `WindowId`, `AppIntent` vs `UiIntent`, `Dispatch{events, effects}`, `Delivery`, pull-model `take_task_requests` | Selectie/camera per venster; controller bezit geen threads; deterministisch testbaar |
| presentation-api.md | `ProjectStore`-poort in presentation | Openen/bewaren als semantische actie; `io` + `FileSystem` erachter in runtime |
| document | `RevisionsExhausted`, `can_undo/can_redo`, `UndoLabel` als enum | Getypeerd en lokaliseerbaar |
| testing | Interpretaties R2–R7 van `layers.toml` | Bevestigd, zie DR-07 |

## Wijzigingsverzoeken

### DR-01 — `crates/burnburn-macos/Cargo.toml` (GUI)
- **Probleem:** `objc2 = "0.6"`, `objc2-foundation = { version = "0.3", … }`, `objc2-app-kit = { version = "0.3", … }`
  en `raw-window-handle = "0.6"` staan inline met losse versies. Teamregel: versies alleen in root
  `[workspace.dependencies]`; ADR 0002 pint exacte versies omdat `wgpu` 30 dezelfde objc2-familie gebruikt.
- **Voorstel:** in root `Cargo.toml` (alfabetisch, één regel per crate)
  `objc2 = "0.6.5"`, `objc2-app-kit = { version = "0.3.2", default-features = false }`,
  `objc2-foundation = { version = "0.3.2", default-features = false }`, `raw-window-handle = "0.6.2"`;
  in de host `objc2-app-kit = { workspace = true, features = [ …bestaande lijst… ] }`. Root-edit valt onder de
  gedeelde regel voor `[workspace.dependencies]`.

### DR-02 — `crates/burnburn-render/src/{camera.rs,input.rs}` (GUI, met User-flow)
- **Probleem:** `Camera`/`Viewport` en de scene-invoer (`SceneInput`, `SceneItem`, `SceneShape`) wonen in `render`.
  `presentation` bezit de camera per venster (pan/zoom, pointer → mm, hit-tolerantie) maar mag niet van `render`
  (wgpu) afhangen. Scene-items gebruiken `[f64; 2]` i.p.v. `burnburn_geometry::Point`/`Path`.
- **Voorstel (bij IM07-integratie, niet nu blokkerend):** verplaats `Camera`/`Viewport` (pure `f64`-wiskunde,
  inclusief hun tests) naar `burnburn-editor::view`; vervang `SceneInput` door `burnburn_editor::SceneSnapshot`
  (contract §5) met `geometry::Path` en `StyleRole`; `render` krijgt dependencies `burnburn-editor`,
  `burnburn-geometry` (toegestaan in `layers.toml`) en houdt `RenderPacket`, `Theme`, tessellatie en GPU.
  `project_scene` wordt `build_packet(&SceneSnapshot, &Theme)`. Tot die tijd mag de spike blijven.

### DR-03 — `docs/user-flows/presentation-api.md` (User-flow)
- **Probleem:** eigen namen `SessionId`, `Revision`, `ObjectId` naast de bestaande kerntypes
  `burnburn_types::{DocumentId, RevisionId, GeometryId}`.
- **Voorstel:** gebruik de kerntypes; `SessionId` vervalt (een open document = `DocumentId`). Het document zegt zelf al
  "wijkt de kern af, dan geldt de kern".

### DR-04 — coördinatengrens pointer (User-flow + GUI)
- **Probleem:** presentation-api.md zegt bij `PointerEvent` "host heeft camera/DPI al verrekend", maar de camera
  staat in `WindowState` (presentation). Dan moet elke host (Mac, Windows) camera-wiskunde dupliceren.
- **Voorstel:** `UiIntent::Pointer(ViewPointerEvent)` met `position_pt` in logische canvaspunten; `presentation`
  rekent met de venstercamera om naar `burnburn_editor::PointerEvent` (mm, zoals nu). Hosts doen alleen
  backing-scale → punten (AppKit levert al punten). Contract §4.

### DR-05 — `crates/burnburn-macos/src/adapter.rs` (GUI)
- **Probleem:** `parse_dimension_mm` en `UiIntent::CommitDimension { width_mm }` parseren de veldtekst in de host.
  Contract en presentation-api.md: de host stuurt ruwe tekst (`SetDimension { object, dimension, text }`), de
  presentatielaag parseert eenheden/decimaalteken en levert veldfouten in het viewmodel — anders parseert Windows
  anders dan Mac.
- **Voorstel:** zodra `burnburn-presentation` bestaat, de host-`UiIntent` laten vervallen en `NativeEvent::FieldCommitted`
  vertalen naar `presentation::UiIntent::SetDimension { text }`; parsertests verhuizen naar presentation (User-flow).
  Als tijdelijke stand-in is de huidige code acceptabel.

### DR-06 — `crates/burnburn-document/src/session.rs` (Kern) — ter controle
- **Waarneming:** in `apply` wordt `self.current` vervangen vóór `result?`; bij een fout zou de sessie dan toch de
  gewijzigde snapshot en revisie hebben. `undo`/`redo` zijn nog stubs (geven dezelfde revisie terug).
- **Verzoek:** waarschijnlijk een lopende red-fase; zorg dat `failed_command_changes_nothing` (snapshot, revisie én
  historie ongewijzigd) en `undo_yields_new_revision_with_restored_identity` bestaan (contract §3: revisies stijgen
  ook bij undo). Graag ook een `docs/dev-notes/IM03.md` (Testing meldde dat die ontbreekt).

### DR-07 — `xtask/src/layers.rs` (Testing) — bevestiging
- R2 (allow **en** strikt lager, apart gemeld), R3 (allow geldt niet voor dev; hosts/tool nergens als dependency),
  R5 alleen normal+build, R6 closure per triple, R7 via `[lints] workspace = true`, "eerste matchende groep melden":
  **bevestigd**. De tabelkop van `layers.toml` beschrijft dit al zo.
- **Optioneel voorstel:** een extra controle "externe dependency in een crate-manifest zonder `workspace = true`"
  zou DR-01 automatisch vangen. Alleen als het goedkoop is.

### DR-08 — `crates/burnburn-render/Cargo.toml` (GUI) — info
- `pollster` als normale dependency is prima voor offscreen/headless; ADR 0002: in de host alleen bij opstart blokkeren,
  nooit tijdens de eventloop. `pollster` 0.4.0 is gelockt; 1.0.1 bestaat (upgrade los, geen haast).

### DR-09 — `crates/burnburn-ruida` (Protocol) — info
- Codec is zuiver (ook `udp.rs` is alleen framing): in orde. De latere encoder-ingang wordt
  `lower(&burnburn_job::JobPlan, &RuidaProfile) -> Result<RdProgram, LowerError>` (ADR 0001, contract §8);
  `burnburn-job` bestaat pas bij IM13, dus nu geen actie.

## Tweede leesronde (eind van de sessie)

`crates/burnburn-machine` is erbij gekomen (IM16, Protocol). Het ontwerp (getypeerde `ControllerPort`,
`RequestTag { execution, epoch, seq }`, poll-gestuurde `ExecutionActor` met geïnjecteerde `Clock`) is overgenomen in
contract §9 en ADR 0005; het vervangt Design's eerste schets met ruwe bytes en een effect-lijst.
`cargo xtask layers`: *10 crates, 2 triples, no violations*.

### DR-10 — `crates/burnburn-machine/src/{ids.rs,actor.rs}` (Protocol)
- **Probleem 1:** `ExecutionId(pub u64)` heeft een publiek veld; de andere ID's (`burnburn-types`) zijn opaak met
  `from_raw`/`raw`, zodat niemand ad hoc ID's verzint.
  **Voorstel:** veld privé maken en `from_raw`/`raw` aanbieden (of `StableId` implementeren).
- **Probleem 2:** `begin_upload(id, chunks)` legt niet vast *welk* plan en welk RD-bestand geüpload wordt. AGENTS.md:
  preview, RD-export en upload verwijzen naar dezelfde immutable gecompileerde revisie.
  **Voorstel:** `Execution` krijgt `rd_hash: ContentHash` (en later `plan: JobPlanId` zodra `burnburn-job` bestaat);
  het actor-evidencelog noemt die hash bij `Stored`. Voor nu volstaat de RD-hash (SHA-256 van de bytes); de
  `ContentHash`-newtype komt in `burnburn-types` (contract §1). Test-idee:
  `stored_evidence_names_the_uploaded_rd_hash`.

---

# Fase B — review halverwege (18 oktober 2026)

Gelezen: `crates/burnburn-{image,sketch,trace,job}` (nieuw), `burnburn-document/src/process/`, `burnburn-editor`,
`burnburn-presentation`, `docs/coordination/{cam,core,gui,userflow,testing}.md`. `cargo xtask layers`:
*16 crates, 2 triples, no violations* (na de fase-B-wijziging van `layers.toml`, ADR 0001 amendement 1).
Nieuwe contracten: [`contracts.md`](../design/contracts.md) §11–§15, [`contracts-phase-c.md`](../design/contracts-phase-c.md),
[ADR 0008](../adr/0008-productiehistorie.md).

## Overgenomen (contract aangepast, geen actie)

| Bron | Wat | Waarom beter |
|---|---|---|
| editor/presentation (User-flow) | Editor krijgt punten + `Camera`/`Viewport` via `ToolContext`, levert `ToolStep::Commit(DocumentCommand)`; `ViewPointerEvent` vervalt | Zelfde doel als DR-04 (geen camera-wiskunde in hosts), één type minder |
| sketch (Sketch) | `Solver::solve(&Problem, &CancellationToken) -> Result<SolveReport, SolveError>` met guess in `Problem`, per-constraint `ConstraintState`, `Flip` | Diagnose per constraint en expliciete branch-flips; "conflict alleen als bewezen" |
| image/trace (Trace) | `SourceImage`, `PixelScale`, `ImageRecipe`, `TraceRecipe::key`, `ContourKind`, `ENGINE_VERSION` | Schaal expliciet en verplicht; recept-key met engineversie maakt caching en reproductie toetsbaar |
| process (CAM) | `ProcessLayer` met `MaterialRecipe`/`RecipeRef { revision }`, `Delete` verwijdert ook operation-targets in dezelfde transactie | Recepten met revisie passen bij C08; één transactie houdt undo correct |

## Wijzigingsverzoeken

### DR-11 — `burnburn-presentation` maatveld en save-waarschuwing (User-flow)
- Orchestratorbesluit: commit bij Return **en** focusverlies; **geen command als de geparste waarde gelijk is**
  (contract §6). Tests: `tab_after_return_commits_once`, `unchanged_value_creates_no_command`,
  `parse_error_keeps_document_and_shows_field_error`.
- `SaveError::Unconfirmed` → document schoon + waarschuwing `SaveNotConfirmedDurable` (geen foutdialoog).

### DR-12 — `burnburn-sketch` → document-evaluator (Sketch, met Kern)
- Nog nodig voor S01–S04: `document::sketch` (data, met `Expr`-brontekst en `DimValue::{Driving, Driven}`),
  `CommandPayload::Sketch(SketchCommand)` en de evaluatorhook `DocumentSession::apply_with(cmd, &[&dyn Evaluator])`
  (contract §11): definitie + afgeleide geometrie in **één** transactie, last-valid-result bij `Conflict`/`NotConverged`.
- `DerivedKey { owner: Sketch(id), local: entity }` → stabiele `GeometryId`; test
  `changing_plate_width_preserves_symmetric_hole_offsets` moet óók de `GeometryId` van de gaten gelijk houden
  (anders breken OperationRefs).
- `SolveError::Rejected { reason: String }`: graag een getypeerde variant per bekende oorzaak zodra die bekend is; een
  vrije string is geen lokaliseerbare diagnose.

### DR-13 — platformdeterminisme van transcendente functies (Kern; ook Sketch/Trace/CAM)
- **Probleem:** `burnburn-geometry` gebruikt `f64::{sin_cos, atan2, tan, acos, hypot}` (`affine.rs:54`, `segment.rs`,
  `flatten.rs:74`, `point.rs:61`, `bounds.rs:75`, `shapes.rs`). Die roepen de libm van het OS aan; macOS en Windows
  kunnen 1 ulp verschillen → andere projecthash/JobPlanId per platform en flakey cross-platformfixtures.
- **Voorstel:** `libm` 0.2.16 (pure Rust, al in de lock) als workspace-dependency; vervang de aanroepen door
  `libm::{sin, cos, atan2, tan, acos, hypot}` (of een interne `geometry::math`-module). Test-idee: vaste vectoren met
  vooraf berekende bitpatronen (`arc_to_cubic_bits_are_platform_independent`) die op beide CI-runners draaien.
  `sqrt` en de vier basisbewerkingen mogen blijven. Registerregel en contractregel staan er al.

### DR-14 — `burnburn-trace` geldigheid van een resultaat (Trace, met User-flow)
- **Probleem:** `StaleTrace::RevisionChanged` verwerpt een trace zodra het document *ergens* gewijzigd is. Een trace
  duurt seconden; een gebruiker die intussen een ander object verschuift verliest zijn resultaat.
- **Voorstel:** geldigheid op invoer: `StaleTrace::InputsChanged { traced: recipe_key, current: recipe_key }` — de
  presentatielaag vergelijkt met de `recipe_key` die het document *nu* voor die trace heeft (bron-asset + recept +
  engine). `Superseded` (nieuwere taak voor dezelfde trace) blijft. `source_revision` blijft in het resultaat voor
  log/diagnose. Contract §11.

### DR-15 — `document::process` (CAM)
- `Operation::targets() -> &[GeometryId]`: akkoord voor nu; IM21 vervangt dit door `GeometryRef` (met
  sub-referentie) via een schemamigratie — houd daarom de DTO in io een **lijst van objecten**
  (`[{ "geometry": 7 }]`), niet een kale lijst getallen, dan is de migratie additief.
- Kerf (`CompileError::KerfNotSupportedYet`): goed dat het een fout is i.p.v. stil 0. Offset komt via
  `geometry::offset` op i_overlay (ADR 0007 amendement); Kern levert die in IM09. Kerf blijft één keer in `cam`.
- `burnburn-job` is nog leeg: graag als eerste test `same_snapshot_and_profile_yield_same_plan_id` en
  `plan_has_no_public_mutation_after_finish` (contract §8/§15).

### DR-16 — root `Cargo.toml` (wie het eerst raakt)
- `tempfile = "3"` → `"3.27"`, `serde_json = "1.0.145"` → `"1.0.154"` (lock heeft .154 al), `libm = "0.2.16"` toevoegen
  bij DR-13. Geen gedragswijziging.

# Fase B — eindreview Design (18 oktober 2026)

Tweede leesronde: `burnburn-job` (nu gevuld: `JobPlan`, `PlanEvent`, `MachineFrame`, `MachineProfile`, `preview`),
`burnburn-sketch/src/model.rs`, `burnburn-trace/src/*`, `burnburn-editor/src/{arrange,issues}.rs`. `cargo xtask layers`:
*16 crates, 2 triples, no violations*.

## Overgenomen

| Bron | Wat | Waarom beter |
|---|---|---|
| job (CAM) | `PlanEvent::OperationBoundary { pass }`, `ScanLine { overscan_mm }`, `PowerRatio`, `PlanUnits`, `job::preview(&JobPlan, &MachineProfile) -> PlanPreview` met `event`-index en `SourceRef` per move | Preview is een pure functie van het plan: preview en export lezen aantoonbaar hetzelfde `JobPlan` (§15) |
| editor (User-flow/Kern) | `arrange::{align, distribute, boolean_operands}`, `issues` als overlays | Commands blijven semantisch; probleemlocaties zijn scene-overlays, geen documentdata |

## Wijzigingsverzoeken

### DR-12 (aanvulling, **hoog**) — plaats van het sketchmodel (Sketch, met Kern)
- **Probleem:** `Sketch` (entiteiten, constraints, dimensies, `Parameters`) woont in `burnburn-sketch::model`. Een
  sketch moet in `DocumentSnapshot` zitten (undo, bewaren, CAM leest de uitvoer), maar `document` (laag 2) en `io`
  (laag 4, geen `sketch` in allow) kunnen `burnburn-sketch` (laag 3) niet zien. Zo komt de sketch niet in het
  document of in het bestand.
- **Voorstel (contract §12):** de **pure data** (`SketchEntity`, `SketchConstraint`, `Relation`, `Dimension` met
  `DimensionRole::{Driving, Driven}` en de *brontekst* van de expressie, parameter-naam → tekst) verhuist naar
  `burnburn-document::sketch` (module-eigenaar Sketch). In `burnburn-sketch` blijven parser (`Expression::parse`,
  `evaluate`), `Parameters`-evaluatie, `Problem`-opbouw, solver en de `Evaluator`-implementatie. Validatie van
  expressies gebeurt in de command (`SketchCommand::SetDimensionValue { text }` → evaluator meldt `InvalidParameter`
  in de diagnose; het document bewaart de tekst ook als hij ongeldig is, zodat de gebruiker hem kan corrigeren).
  Alternatief "io → sketch toestaan" lost het document-/undoprobleem niet op en is daarom afgewezen.
- Graag nu, vóór er meer code op `model.rs` steunt; tests in `crates/burnburn-sketch/tests/` blijven geldig.

### DR-17 — één domeingescheiden hashfunctie (Kern; raakt image, trace, job, io)
- **Probleem:** vier eigen implementaties van `SHA-256("burnburn-v1\0" ‖ tag ‖ payload)`: `io/src/hash.rs`,
  `job/src/hash.rs`, `image/src/source.rs:166`, `trace/src/recipe.rs:72`. De framing van de tag verschilt al subtiel
  (`CanonicalWriter::tag` vs. rechtstreeks), waardoor twee domeinen dezelfde bytes kunnen geven en niemand de tags
  overziet.
- **Voorstel (sluit het open punt van ADR 0004):** `sha2` mag in `burnburn-types`; `ContentHash::of(domain: HashDomain,
  payload: &[u8])` met een gesloten `#[non_exhaustive] enum HashDomain { Asset, Project, Plan, Rd, SourcePixels,
  TraceKey, EvalSketch, EvalText, Profile, HistoryRecord }` (tag-string per variant, vastgelegd in ADR 0004).
  De vier crates roepen die aan; een golden-test per domein met een handberekende bytereeks blijft in io. Bestaande
  hashes van assets/projecten (`asset`, `project`) moeten bit-gelijk blijven — test eerst
  `asset_key_unchanged_after_hash_consolidation` tegen de fixture in `test-data/projects/`.

### DR-18 — `burnburn-job::ProfileRevision` (CAM)
- **Probleem:** `ProfileRevision { id: String, revision: u32 }` is een teller. Twee profielen met hetzelfde id en
  dezelfde teller maar andere inhoud (ander bed, andere calibratie, op een andere computer bewerkt) geven hetzelfde
  `JobPlanId`; IM29 (`scan_calibration_update_invalidates_dependent_prepared_plan`) kan dan stil falen.
- **Voorstel:** `ProfileRevision { id, revision, content: ContentHash }` met `content` = domeingescheiden hash
  (`HashDomain::Profile`) over het canonieke profiel (frame, snelheden, vermogens, capabilities, calibratie); het
  `JobPlanId` neemt `content` op. De teller blijft voor weergave. Test: `profile_content_change_changes_plan_id`.

### DR-19 — `burnburn-image` grijswaarden in `f32` (Trace) — info
- `GrayImage` gebruikt `f32`. Basisbewerkingen zijn IEEE-exact, dus deterministisch zolang er geen
  transcendente `std`-functies (gamma: `powf`) of `mul_add` in komen; bij tooncorrectie (IM18) `libm::powf` gebruiken
  (DR-13). Dither (IM18) moet integer-foutdiffusie gebruiken (contracts-phase-c §IM18).

## Stand open DR's

| DR | Eigenaar | Status |
|---|---|---|
| DR-01, DR-05, DR-06, DR-07, DR-08, DR-09 | GUI / Kern / Testing / Protocol | afgehandeld of info |
| DR-02 | GUI + User-flow | open (IM07-integratie fase B) |
| DR-03, DR-04 | User-flow | vervallen/vervangen door de gebouwde vorm (contract §4/§6) |
| DR-10 | Protocol | open (`rd_hash`, `JobPlanId` in `Execution` — `burnburn-job` bestaat nu) |
| DR-11 | User-flow | open |
| DR-12 (+aanvulling) | Sketch + Kern | open, **hoog** |
| DR-13 | Kern (+ Sketch/Trace/CAM) | open |
| DR-14 | Trace + User-flow | open |
| DR-15 | CAM (+Kern voor offset) | open |
| DR-16 | wie het eerst de root raakt | open |
| DR-17 | Kern | open |
| DR-18 | CAM | open |
| DR-19 | Trace | info |
