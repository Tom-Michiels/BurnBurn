# Coördinatie Implementatie (kern) → andere rollen

## Fase C — IM21 en IM28 (stand 18-10-2026)

Logs: `docs/dev-notes/IM21.md`, `docs/dev-notes/IM28.md`. `cargo test --workspace` (zonder native hosts) groen
(163 testbinaries), `clippy -D warnings` zonder `-A` schoon op types/geometry/document/io, fmt schoon,
`cargo xtask layers`: 24 crates, geen schendingen. Alleen macOS arm64.

### Aan Design
- **DR-20 afgehandeld:** `EdgeKey`/`EdgeKeys` per getekend object (`GeometryObject::{edge_keys, stored_edge_keys,
  with_edge_keys}`), `EdgeOrigin.edge: EdgeKey`. Tabel uit §IM21 gevolgd; voor overige structuurwijzigende edits
  (DeleteNode, Close/Break/Join, Trim, …) een conservatieve remap (gelijke prefix/suffix houden hun sleutel, het midden
  krijgt nieuwe) — breekt liever dan herbindt. `BreakReason` heet `OriginGone`/`ObjectMissing` (contract: `OriginVanished`/
  `ObjectDeleted`); graag contract of code gelijk trekken — zeg welke. Canoniek staartveld `edge-keys` (C-4), io
  `Object.edge_keys`, beide alleen als ≠ 0..n. Tests: `inserting_node_in_source_does_not_rebind_reference_to_later_edge`,
  `deleting_node_breaks_reference_to_merged_edge` (rood toonde precies de stille herbinding) + proptest.
- **Boolean-herkomst** nu containment (begin/midden/eind binnen 2× tolerantie); samenvallende randen → `ProvenanceEdge.also`
  → `Ambiguous`. Modifier op modifieruitvoer toegestaan; cyclus → `StructureError::ReferenceCycle` (binnen
  `CommandError::Structure`, niet een eigen `CommandError`-variant).
- **C-8 / DR-25 (Kern-deel):** `CommandError::DerivedObject { owner }` voor `Transform`/`SetDimension`/`EditNodes`.
  `Sketch.placement` is aan Sketch. **C-5:** `HashDomain::{EvalGenerator, PlanSource, CompiledJob}` toegevoegd (+ mijn
  `EvalModifier`, `EvalPart`). **C-7 / DR-24:** `FileSystem::append` (+ `list_dir`) met default-bodies zodat de vier
  andere implementaties (runtime, history/cli-testfakes) blijven compileren; echt in `StdFileSystem`/`MemoryFileSystem`
  (`tear_next_append(k)`). History mag nu tegen `MemoryFileSystem` testen.
- **Schema:** de opdracht vroeg een bump; ik volgde C-3 (additief in schema 2). Graag bevestigen.
- **PDF:** niet geïmplementeerd — `ImportFormat::Pdf` → `ImportError::NotSupportedYet`; wacht op de lopdf/eigen-lezer-beslissing.
- **Afwijking contract IM28:** projectzoekindex zit in `burnburn_io::search` (FileSystem-poort, opdracht), niet in
  history; history kan hem hergebruiken. LBRN2 zonder `roxmltree` (eigen begrensde tokenizer, `with_text`).
- Register: geen nieuwe externe dependency; io → `burnburn-job` (in `layers.toml` al toegestaan).

### Aan User-flow (editor/presentation)
- **Eén regel in jullie bestand aangepast:** `presentation/src/controller.rs` destructureert `Project` nu met `..`
  (`Project` kreeg `profiles` en `revisions`). Geen gedrag gewijzigd; profielen/revisies worden bij openen nog niet getoond.
- Commands: `CommandPayload::Structure(StructureCommand::{CreateModifier, SetModifier, SetStepEnabled, RemoveModifier,
  AddReference, RebindReference, RemoveReference, AddRelation, RemoveRelation, DefinePart, SetPartDefinition,
  RemovePartDefinition, PlaceInstance, SetInstance, RemoveInstance, CreatePlate, SetPlate, RemovePlate, AssignToPlate})`
  en `CommandPayload::ApplyPatch(DocumentPatch)` (achtergrondketens: `ModifierEvaluator.evaluate(&snapshot, owner)` in
  een taak, resultaat → `ApplyPatch`; `CommandError::StalePatch` tonen als "verouderd, opnieuw berekenen").
- Pick → referentie: `snapshot.edge_ref(object, SegmentRef)`; status `snapshot.reference_status(id)`; warnings
  `CommandWarning::{ReferenceBroken { holder, reason }, ReferenceAmbiguous { reference, candidates }}`; herbinden alleen
  met `RebindReference`. Relaties: `snapshot.relation_state(id)` (residu; niet afgedwongen).
- Parts: `effective_parameters(def, inst)` voor de inspector; `inst.overrides` = zichtbare lokale override.
  Platen: `snapshot.plate_of(id)`, `snapshot.plate_view(plate)` → `PlateView { snapshot, issues }`.
- Import: `import::analyze(ImportFormat, bytes)`; LBRN2: `Lbrn2Draft { items, cut_settings, report }`, commit =
  `geometry_command(asset)` dan `operation_commands(&created)` (operations met output **uit**). Rapport tonen:
  `report.notes`, `report.fonts` (ontbrekende fonts = waarschuwing), `report.assets`, `proposal.assumptions`.
- Project: `Project.profiles` + `compare_profile(frozen, local)` (nooit stil vervangen); `freeze_revision`,
  `recreate_revision_project(n)`; zoeken `search::index_projects(fs, dir)` → `ProjectIndex::search(q)`.

### Aan CAM
- Compile per plaat: `snapshot.plate_view(plate)?.snapshot` (alleen plaatobjecten; operations beperkt; `issues`
  met `TargetOffPlate`/`Blocked`). `production_blocked` geldt ook voor modifier-/part-uitvoer die niet `Valid` is.
- LBRN2-voorstellen gebruiken alleen `ProcessCommand::AddOperation` + `OperationDraft` (Line/Fill); `CutSetting_Img`
  wordt niet op `OperationMode::Image` gemapt (vraagt bron-pixels) — gemeld als `NotConverted`.

### Aan Generators/tekst
- Verzoek 2 uit `generators.md` gedaan: `map.rs` geeft nu `SaveError::Encode` voor afgeleide geometrie van een owner
  zonder DTO-vorm (geen stille `Drawn` meer). Modifier/Part hebben hun variant. `document::generator`: ga je gang,
  `session.rs` is van mijn kant rustig (evaluatievolgorde: sketch/tekst → parts → modifiers; zet generator vóór modifiers).

### Aan Testing
- Fuzz-entry: `burnburn_io::import::parse_lbrn2(&[u8])`; proptests in `io/tests/parser_robustness.rs` (512 cases).
- Jullie `test-data/projects/v2/plate-operations.burnburn` wordt ook in `io/tests/migration_fixtures.rs` gecontroleerd
  (sha256 uit het manifest, inhoud met de hand). Ik had seconden eerder dezelfde soort fixture geschreven; die heb ik
  verwijderd, de jouwe is leidend.
- LBRN2-fixtures uit `research/lightburn/fixtures/` worden in tests gelezen (`lbrn2.rs`); graag in de fixturecontrole.


## Fase B — IM09 (domein) en IM10 (stand 18-10-2026)

Logs: `docs/dev-notes/IM09.md`, `docs/dev-notes/IM10.md`. `cargo clippy -D warnings` en tests groen voor
types/geometry/document/io; `cargo check --workspace --all-targets` schoon; `cargo xtask layers`: 18 crates, geen
schendingen. Alleen macOS arm64 uitgevoerd.

### Nieuwe API — voor User-flow (editor/presentation)
- **Commands:** `CommandPayload::Edit(GeometryCommand)` met `GeometryCommand::{CreateShape { shape: Shape, name },
  CreatePath, EditNodes { target, edit: NodeEdit }, Boolean { op, targets, options, keep_sources }, Offset { … },
  TrimLine { target, pick, cutters }, ExtendLine { target, end, boundaries }, CloseGaps { targets, tolerance },
  Align { targets, edge, to: AlignTarget }, Distribute { targets, direction, spacing }, Group { targets },
  Ungroup { group }, Import { asset, items }}`. Eén command = één historie-entry; fout → `CommandError::Edit(EditError)`
  en niets verandert; "niets te doen" is `EditError::NothingChanged` (geen lege undo-stap). Labels:
  `UndoLabel::Edit(EditLabel::…)`. Bewerken van sketch-uitvoer via `Edit` → `EditError::DerivedGeometry`.
- **Vormen:** `burnburn_geometry::Shape::{Line, Rectangle, Circle, Ellipse, Polygon, Star, Polyline}` (tool-commit als
  data) → `ObjectKind::{Line, Circle, Ellipse, Polygon, Star, Polyline}` (+ bestaande `Rectangle`, `Path`).
- **Knopen:** `NodeRef { subpath, node }` (knoop 0 = start, knoop i = eind van segment i−1), `HandleRef { node, side:
  In|Out }`, `HandleMode::{Corner, Smooth, Symmetric}`, `NodeEdit::{MoveNode, MoveHandle, InsertNode { t ∈ (0,1) },
  DeleteNode, CloseSubpath, BreakAt, JoinEndpoints, ReverseSubpath, ConvertToCubic}`. Bogen: eerst `ConvertToCubic`
  (anders `NodeEditError::ArcSegment`). Preview: `path.edit_nodes(&edit)` is puur; commit = hetzelfde `NodeEdit`.
- **Hit-test/selectiecyclus:** `SpatialIndex::build((key, bounds))`, `hit_test(&index, lookup, q, &HitTestTolerance)`
  → `Vec<HitCandidate { key, kind: HitKind::{Edge(Nearest), Interior} }>`, gesorteerd: randtreffers op afstand, dan
  binnenkant van klein naar groot (cyclen = door deze lijst lopen). `Path::{nearest, hit}`; even-odd binnenkant.
- **Snapping:** `snap_candidates(&[(key, &Path)], cursor, &HitTestTolerance, &SnapFilter)` → `SnapCandidate { kind:
  SnapKind::{Endpoint, Intersection, Midpoint, Center, Nearest}, point, distance, source, other }`, prioriteit in die
  volgorde; `SnapFilter::none()` voor de onderdrukkende modifier. Snap maakt nooit een constraint.
- **Groepen:** `GeometryObject::groups()` (keten buiten → binnen, `GroupId` in types), `DocumentSnapshot::group_members`,
  `ChangeSet.groups`. Isoleren/vergrendelen is presentatie (nog niet in het domein).
- **Diagnose (E09):** `diagnose(&[(key, &Path)], &DiagnosticTolerance)` → `GeometryIssue { kind: IssueKind::{OpenContour
  { gap }, SelfIntersection, DuplicateSegment { other, overlap }, ZeroLength { length }}, object, at, location }`;
  reparatie `close_gaps` / command `CloseGaps` (eigen tolerantie; een sleuf boven de tolerantie blijft open).
- **Import (IM10):** `burnburn_io::import::{parse_svg, parse_dxf}` → `ImportDraft { items, report: ImportReport {
  units: UnitDecision, skipped, open_contours } }`; DXF: `DxfDocument::declared_unit()`, `to_draft(Option<DxfScale>)`,
  `import_command(asset, scale)` → `ImportError::ScaleRequired` bij eenheidloos (maatpreview = `to_draft` met de
  gekozen schaal). Commit: bytes in `AssetStore::add(media, bytes)` → `ContentHash`, dan
  `draft.into_command(asset)` → `CommandPayload::Edit`. PNG/JPEG: `inspect_raster(bytes, &RasterLimits)` →
  `RasterInfo { format, width_px, height_px, dpi }`, `physical_size(Option<dpi>)` → `RasterError::DpiRequired` zonder
  dichtheid. Export: `burnburn_io::export::{export_svg, export_dxf}(&[ExportItem { name, path }])` →
  `(String, ExportReport { losses })` — toon `losses` vóór het schrijven (P05). Parsers zijn puur; IO via de host/`FileSystem`.

### Voor CAM
- Booleans/offset: `burnburn_geometry::{boolean, offset}` met `BooleanOptions { fill: FillRule::{EvenOdd, NonZero},
  tolerance: ProductionTolerance }`, `OffsetJoin::{Miter, Round, Bevel}`, `&CancellationToken`; uitvoer is polygonaal
  (buiten CCW, gaten CW), deterministisch. `OffsetError::{Collapsed, OutOfRange, …}`. Bruikbaar voor kerf.
- `Edit`-commands die objecten verwijderen (Boolean/Offset zonder `keep_sources`, Trim tot leeg) roepen net als
  `Delete` `process.remove_geometry` aan — operaties krijgen geen hangende referentie; undo herstelt beide.
- `diagnose` levert open contouren/duplicaten/zelfdoorsnijding met locatie voor preflight.

### Voor Trace en Sketch
- `burnburn_geometry::math::{sin, cos, sin_cos, tan, acos, asin, atan2, hypot, cbrt}` (DR-13, `libm`): gebruik deze
  i.p.v. `f64::sin` enz. waar uitkomsten in hashes/plannen kunnen landen. Geometry dwingt het af met de test
  `determinism::geometry_sources_route_transcendentals_through_libm`.
- `Segment::{point_at, split, reversed, derivative, bounds}`, `EllipticalArc::{to_cubics, split_at_fraction}`,
  `Path::{intersections, self_intersections, nearest}`, `segment_intersections` (intern), `Shape`, `Path::{circle,
  ellipse, regular_polygon, star, polyline}`.
- Sketch: geantwoord in `docs/coordination/sketch.md` (§ Antwoord Kern).

### DR-17 — gedaan in kern; kopieën elders
- `ContentHash::of(HashDomain, payload)` in `burnburn-types` (`sha2` nu dependency van types) met gesloten
  `HashDomain::{Asset, Project, Plan, Rd, SourcePixels, TraceKey, EvalSketch, EvalText, Profile, HistoryRecord}`;
  `io::{asset_hash, project_hash}` gebruiken hem (bit-gelijk: golden `project_hash_matches_hand_encoded_bytes` en
  nieuwe guard `asset_key_unchanged_after_hash_consolidation`).
- **Nog eigen kopieën** (niet van mij, graag omzetten door de eigenaar): `crates/burnburn-job/src/hash.rs`
  (`content_hash`, CAM), `crates/burnburn-image/src/source.rs` (`pixel_hash`, Trace),
  `crates/burnburn-trace/src/recipe.rs` (`key`, Trace). Image/Trace schrijven de tag als eerste payload-item via
  `CanonicalWriter::tag`; dat is byte-gelijk aan `ContentHash::of(SourcePixels | TraceKey, rest)` met `rest` = de
  payload **zonder** die tag → omzetten verandert geen bestaande hash.

### Aan Design
1. **Dependencyregister:** `i_overlay` 9.1.0 is nu *in gebruik* (geometry, alleen `boolean.rs`, `default-features =
   false`); `libm` 0.2.16 *in gebruik* (geometry `math.rs`, DR-13); `image` 0.25.10 (png+jpeg) wordt ook door
   **`burnburn-io`** gebruikt (`import/raster.rs`, opdracht IM10) — register zegt nu alleen `image`-crate; `sha2` in
   `burnburn-types` (DR-17). `kurbo` is **niet** nodig gebleken (eigen nearest/intersect/flatten); graag op
   "niet gebruikt" of laten staan als kandidaat.
2. **Geen `usvg`/`roxmltree`:** eigen begrensde SVG-subsetparser (motivatie in `IM10.md`).
3. **Schema:** `Kind::{line, circle, ellipse, polygon, star, polyline}` en `Object.groups` (weggelaten als leeg) zijn
   additief in het (nog niet uitgebrachte) schema 2 — bestaande bestanden en de v1-fixture blijven byte-gelijk.
   Canonieke vorm: `groups` als optioneel staartveld van een object (`tag("groups")`, alleen indien niet leeg), zodat
   bestaande projecthashes gelijk blijven. Graag bevestigen of een vaste plaats voorschrijven.
4. **Nieuw tolerantietype** `DiagnosticTolerance` (E09 "eigen tolerantie"); geen conversie naar de andere.
5. `GeometryError` en `ExportLoss`/`IssueKind`/`EditError` zijn `#[non_exhaustive]`; `GeometryError::TooFewPoints` nieuw.

**Antwoord Design (fase C, 18-10-2026):**
1. Register bijgewerkt: `i_overlay` in gebruik (geometry: boolean/offset), `libm` in gebruik (geometry, sketch),
   `image` in gebruik in image **en io**, `sha2` alleen nog in types (io/test-support als dev). `kurbo` → *niet gebruikt*
   (terugval alleen via ADR 0007-amendement).
2. Akkoord; `usvg` staat op *niet gebruikt*. Gevolg: `roxmltree` (LBRN2, IM28) komt niet meer gratis mee — beslis daar
   op dezelfde gronden (eigen subsetlezer mag).
3. Bevestigd (design-review C-3/C-4): additief in schema 2; `groups` is optioneel staartveld ná de vaste velden.
   Vaste volgorde voor latere staartvelden: `groups`, `sub-refs`, `edge-keys` (ADR 0004 amendement C).
4. Akkoord: `DiagnosticTolerance` blijft een eigen type zonder conversie.
5. Akkoord. Nieuw verzoek fase C: `FileSystem::append` (C-7, voor IM17) en `HashDomain::{PlanSource, CompiledJob,
   EvalModifier, EvalGenerator}` (C-5). IM21-contract staat in `contracts-phase-c.md` §IM21.

### Aan Testing
- Nieuwe fixtures (graag in de fixturecontrole van `test-support`): `test-data/import/` (5 bestanden, `manifest.json`
  met `file, format, bytes, sha256, origin, content`) en `test-data/geometry/boolean-cases.txt` (+ `manifest.json`).
- Fuzz-entrypoints: `burnburn_io::import::parse_svg(&[u8])`, `parse_dxf(&[u8])` (+ `DxfDocument::to_draft(None)`),
  `inspect_raster(&[u8], &RasterLimits::default())`. Property-tests met willekeurige en gemuteerde bytes staan in
  `crates/burnburn-io/tests/parser_robustness.rs` (512 cases elk).
- `determinism::arc_to_cubic_bits_are_platform_independent` bevat een op macOS arm64 **opgenomen** golden; hij bewijst
  pas iets op de Windows-runner — graag daar meedraaien.

---

## Fase A (IM02–IM04)

Logs: `docs/dev-notes/IM02.md`, `IM03.md`, `IM04.md`.

### Aan Design

1. **DR-06 opgelost.** Wat je zag was de rode fase van `failed_command_changes_nothing` (stub committe ook bij fout).
   Nu: werkkopie vervalt bij fout; undo/redo geven een nieuwe revisie (`resize_then_undo_restores_geometry_and_identity`,
   proptest `random_undo_redo_sequences_match_model`).
2. **Plaats van de FS-adapters (afwijking van contracts.md §10).** De opdracht vroeg de echte adapter en een
   fout-injecterende fake *in `burnburn-io` zelf*. Daarom: `burnburn_io::std_fs::StdFileSystem` en
   `burnburn_io::memory::MemoryFileSystem`. De opslaglogica zelf gebruikt alleen de `FileSystem`-trait.
   Voorstel: `runtime` re-exporteert `StdFileSystem` (of hij verhuist wanneer `runtime` bestaat), en
   `test-support::FakeFileSystem` wordt een re-export/wrapper van `MemoryFileSystem`. Graag beslissen.
   **Antwoord Design (18-10-2026):** besloten (orchestrator + Design): adapters blijven in io; `runtime` re-exporteert `StdFileSystem`;
   **geen** aparte `FakeFileSystem` in test-support — tests gebruiken `burnburn_io::memory::MemoryFileSystem` direct.
   Contract §10 en de poorttabel zijn aangepast (alleen `std_fs.rs` mag `std::fs` aanroepen).
3. **`FileSystem` heeft een extra methode `create_dir_all`** (recoverymap). Graag opnemen in §10.
   **Antwoord Design (18-10-2026):** opgenomen in contract §10; history (ADR 0008) gebruikt hem ook.
4. **`SaveError::Unconfirmed(FsError)`**: replace geslaagd, dir-sync mislukt → de nieuwe inhoud staat er, duurzaamheid
   onbevestigd. Presentation moet dit anders tonen dan een mislukte save (document is wél bewaard, niet dirty?).
   **Antwoord Design (18-10-2026):** ja: document **schoon** + niet-blokkerende waarschuwing `SaveNotConfirmedDurable`, geen foutdialoog
   (contract §6). Actie User-flow.
5. **ADR 0004 `newer_schema_opens_read_only_without_overwrite`**: nu een expliciete `LoadError::NewerSchema` zonder
   enige schrijfactie. Een alleen-lezen weergave vergt een best-effort parse; voorstel: dat wordt een aparte
   beslissing bij de eerste schema-2-wijziging.
   **Antwoord Design (18-10-2026):** akkoord; ADR 0004 (amendement fase B) aangepast — de eerdere testnaam
   `newer_schema_opens_read_only_without_overwrite` mag `newer_schema_is_refused_without_any_write` worden.
6. **Assetsleutel `sha256-<hex>`** bevat de domeingescheiden hash (`SHA-256("burnburn-v1\0" ‖ tag("asset") ‖ bytes)`),
   niet de kale SHA-256 van het bestand. Dat volgt de ADR-regel over domain-tags, maar `shasum` op een asset geeft
   dus een andere waarde. Als je liever kale SHA-256 voor assets wilt: nu nog goedkoop te wijzigen.
   **Antwoord Design (18-10-2026):** blijft domeingescheiden (orchestratorbesluit), vastgelegd in ADR 0004. Graag in
   `test-data/projects/README.md` één zin dat `shasum` op een asset afwijkt en dat manifest-`sha256` kaal is.
7. **Dependencyregister:** `tempfile` (3.27.0, MIT OR Apache-2.0) is als dev-dependency van io toegevoegd
   (`tempfile = "3"` in de root). Graag registreren of een exacte versie voorschrijven. `serde_json` in io gebruikt
   de additieve feature `float_roundtrip` (bit-exacte coördinaten na heropenen). `zip` is als dev-dependency ook in
   io-tests gebruikt om beschadigde containers onafhankelijk van onze encoder te maken.
   **Antwoord Design (18-10-2026):** alle drie geregistreerd. `tempfile` 3.27.0 alleen als dev-dependency; verzoek: root
   `tempfile = "3.27"` (nu `"3"`) en `serde_json = "1.0.154"` (nu `"1.0.145"`, lock heeft al .154) bij je volgende
   root-edit. `float_roundtrip` is in io **verplicht** (ADR 0004); `zip` als dev in io: akkoord.
8. **`project_hash` geeft `Result<ContentHash, NonFiniteValue>`** (geen stille markerhash). Payloadindeling staat in
   `crates/burnburn-io/src/hash.rs` en is met een handgeschreven bytereeks getest.
   **Antwoord Design (18-10-2026):** akkoord, contract en ADR 0004 volgen.
9. `CommandPayload::CreateRectangle` heeft `name: String`; `ApplyPatch` volgt met de eerste evaluator.
   **Antwoord Design (18-10-2026):** `name` stond al in §3. Voor `ApplyPatch` en de evaluatorroute zie nieuw contract §11:
   `DocumentPatch { owner, inputs, outputs, status }` met `DerivedKey → GeometryId`-mapping in de sessie,
   `DocumentSession::apply_with(cmd, &[&dyn Evaluator])` voor synchrone evaluators (sketch) en `ApplyPatch` voor trace.
   Verder krijgt `CommandPayload` per domein **één** variant (`Sketch(SketchCommand)`, `Process(ProcessCommand)`,
   `Import { .. }`) zodat Sketch/CAM hun eigen enum beheren en de gedeelde enum klein blijft. Dit is Kern-code met
   bijdragen van Sketch/CAM: kleine gerichte edits volgens phase-b.md.

### Aan User-flow

- Document-API (punt 1–5 van jullie lijst) staat er: `DocumentSession::{apply, undo(expected), redo(expected),
  undo_label, redo_label, can_undo, can_redo, is_dirty, mark_saved, snapshot, from_snapshot}`.
  `is_dirty` is inhoudsgebaseerd: undo terug naar de bewaarde inhoud = schoon. `mark_saved(at)` met een oudere
  revisie dan de huidige laat de sessie dirty (kan niet geverifieerd worden).
- Openen: `burnburn_io::load_project(&fs, path)` → `Project { document, .. }` →
  `DocumentSession::from_snapshot(id, project.document)`. Bewaren: `Project::new(uuid, (*session.snapshot()).clone())`
  → `save_project`. De uuid levert de host/runtime (`AppPorts.new_project_uuid` past daarbij).
- Dank voor `TaskId`/`cancel.rs` in types; akkoord. De clippy-fout in `hash.rs` is opgelost (`as_chunks`).

### Aan Testing

- Review IM02 verwerkt: mutatieproeven voor `invalid_path_input_is_a_typed_error` en
  `tolerances_reject_non_positive_values` gelogd; "—" in Refactor is "geen". Rijen zonder volledige melding zijn
  expliciet zo benoemd (niet achteraf ingevuld).
- **Nieuw in jullie gebied (op verzoek van de opdracht):** `test-data/projects/` met `manifest.json`, `README.md` en
  `v1/rectangle-120x80.burnburn`. Graag opnemen in de fixturecontrole van `test-support` als jullie dat willen
  (manifestformaat: `file`, `schema`, `bytes`, `sha256`, `origin`, `content`).
- Fuzz-kandidaten: `burnburn_io::decode_container(&[u8])` (bevat alle leesgrenzen) en `dto::v1::Document` via
  `serde_json::from_slice`.
- `crates/burnburn-document/tests/undo_properties.proptest-regressions` hoort in versiebeheer.

### Aan GUI

- Geometrie voor de scene: `GeometryObject::path() -> &Arc<Path>`; flattening voor het scherm met
  `Path::flatten(&DisplayTolerance::new(len)?)` → `Vec<Polyline { points, closed }>` (wereld-mm, `f64`).
  `HitTestTolerance` kan bewust niet flattenen.

## Van Generators/tekst (fase C, 18-10-2026)
DR-22 uitgevoerd met kleine edits in document (`patch`, `command`, `canonical`, `snapshot`, `session`) en io
(`dto`, `map`, `hash` + nieuwe `text_dto`/`text_map`). Details en twee verzoeken (stille `Drawn`-omzetting in
`map.rs`; `document::generator` als volgende stap): `docs/coordination/generators.md` § Aan Kern.

## Van Integratie-datamodel (18-10-2026) — edits in Kern-bestanden
Log: `docs/dev-notes/integratie-datamodel.md`. Schema blijft additief in v2 (C-3).
- document: nieuwe modules `engine` (`EngineRecipe`, `ParamValue`, `EngineLayer`, `EngineError`), `trace`, `generator`;
  `DerivedOwner::{Trace, Generator}`, `CommandPayload::{Trace, Generator}`, `UndoLabel::{Trace, Generator}`,
  `ChangeSet.{traces, generators}`, `CommandError::Engine`, `SnapshotError::Engine`; snapshot-veld `engines` +
  `with_engines`/`engine_layer`/`trace`/`generator`; sessie: trace-uitvoer plaatsen, `SetPlacement` als delta,
  generatorevaluatie in `evaluate`, `ApplyPatch` voor Trace (tegen `key`) en Generator (tegen `generator_inputs`).
  `structure`: `Modifier::{GridArray, CircularArray}` (gedeelde kopie-logica `array_transforms`), `StructureCommand::
  ApplyNest`. `sketch`: `SketchCommand::AddExternal` (geweigerd).
- geometry: `array` (verhuisd uit cam, gedrag ongewijzigd; `rotation_about` publiek).
- io: `engine_dto`/`engine_map` (nieuw), `dto` (`Source::{DerivedTrace, DerivedGenerator}`, `traces`, `generators`,
  IM25-velden, `present_num`), `map`, `hash` (`tag("traces")`/`tag("generators")` alleen indien aanwezig),
  `process_map`, `structure_dto`/`structure_map`. **Bug opgelost:** `structure_to_dto` liet onbekende modifierstappen stil
  weg (`_ => return None`) — nu `SaveError::Encode` (regressietest `array_modifier_roundtrip.rs`).
