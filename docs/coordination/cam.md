# Coördinatie Implementatie (CAM/job) → andere rollen

Stand 18-10-2026 (fase B, IM13/IM15 + IM14-koppeling; fase C hieronder). Logs: `docs/dev-notes/IM13.md`, `docs/dev-notes/IM15.md`.

## Aan Kern (document/io)

1. **Proceslaag in `burnburn-document`.** Module `src/process/` (eigendom CAM) + test `tests/process.rs`. Gedeelde,
   additieve edits in jullie bestanden:
   - `lib.rs`: `pub mod process;`.
   - `command.rs`: `CommandPayload::Process(ProcessCommand)`, `UndoLabel::Process(ProcessLabel)`,
     `CommandError::Process(ProcessError)`, `ChangeSet { operations, recipes }`.
   - `snapshot.rs`: veld `process`, `process()`, `with_process(..)` (valideert ids en targets), `same_content` incl.
     proceslaag, `SnapshotError::Process`.
   - `session.rs`: `Txn.process`, dispatch naar `process::apply`, `Delete` haalt targets weg (zelfde transactie),
     `is_dirty`/`mark_saved` incl. proceslaag. Jullie `Edit`-tak volgt dat patroon al (dank).
   - Clippy: `session.rs:294` (`for id in self.changes.deleted[first_deleted..].to_vec()`) geeft
     `unnecessary_to_owned` onder `-D warnings`; `.iter().copied()` werkt (disjuncte velden). Niet door mij aangepast.
2. **`burnburn-io` schema 2 (gedaan, graag review).** `OperationPlaceholder` en `Project.operations` zijn vervallen;
   operations wonen in `DocumentSnapshot::process()`. Nieuw bestand `src/process_map.rs` (CAM); kleine edits in `dto.rs`
   (velden `Operation.{targets: [Target], process}`, `Document.{constraints, recipes}`), `map.rs` (`document_to_dto`
   geeft `Result`, `document_from_dto(dto, schema, ..)`), `container.rs` (`SCHEMA_VERSION = 2`), `project.rs`, `hash.rs`
   (operations met instellingen; constraints/recipes alleen indien aanwezig → hash van projecten zonder operations
   ongewijzigd, jullie handgecodeerde test is groen), `lib.rs`.
   - Migratie schema 1 → 2: placeholder → Line-operation, **output uit**, 10 mm/s, 0 %, 1 pass, geen air
     (`MIGRATED_V1_*`). `v1/rectangle-120x80.burnburn` laadt ongewijzigd; verwachting in `migration_fixtures.rs` aangepast;
     de schrijver van de v1-fixture is verwijderd (deze build schrijft geen schema 1 meer).
   - `load_validation::inconsistent_project_is_not_written`: dangling target/id-clash kan niet meer gebouwd worden
     (`with_process` weigert); de test toetst dat nu. `kinds_and_groups.rs`: alleen het vervallen veld weggehaald.
   - Moduleneem `dto::v1` dekt nu schema 1 en 2 (additief); hernoemen mag van mij.
   - **Verzoek:** een schema-2-fixture in `test-data/projects/` (met operations, constraint, recept) — Testing beheert
     het manifest; ik lever graag de inhoud.
3. **Kerf.** Gedaan in fase C via `geometry::offset` (zie onder); `KerfNotSupportedYet` is vervallen.
4. **DR-13.** Plan-id hangt via flattening van bogen af van `acos`/`sin_cos`; graag seintje als `libm` erin zit.
5. **DR-17.** `burnburn-job` gebruikt `ContentHash::of(HashDomain::Plan)` (plan) en `HashDomain::Profile`; eigen
   `job/src/hash.rs` is weg. De cam-bronhash (`burnburn_cam::source_hash`) gebruikt ook `Plan`, met een payload die met
   `tag("cam-source")` begint (plans beginnen met `tag("units-mm-s")`). Een eigen `HashDomain::PlanSource` mag; dan
   pas ik het aan.

## Aan Protocol (`burnburn-ruida`)
- Nieuw bestand `src/lower.rs` (CAM) en `tests/lower.rs`. Kleine edits: `lib.rs` (`mod lower;` + re-exports
  `lower, LowerError, OverscanPolicy, RdProgram, RuidaProfile`, één doc-regel), `Cargo.toml` (`burnburn-job` normaal;
  dev: `burnburn-cam`, `burnburn-document`, `burnburn-geometry`, `burnburn-types`). Codec ongewijzigd.
- `RdProgram::plan()` is de `JobPlanId` voor DR-10 (`Execution` legt plan + RD-hash vast).

## Aan User-flow / GUI
- Preview: `burnburn_job::preview(&plan, &profile)` → segmenten met `event`, `operation`, `SourceRef` (klik → bron).
  Preflight: `burnburn_cam::preflight(..)` → `Finding` met `SourceRef`/`GeometryId` voor zoom-naar-oorzaak.
- `burnburn_io::Project` heeft geen `operations` meer; jullie controller gebruikte het al niet meer bij mijn build.

## Aan Design
- DR-15: targets in io als `[{ "geometry": id }]` (schema 2) — gedaan. `plan_has_no_public_mutation_after_finish` als
  `compile_fail`-doctest op `JobPlan`; `same_snapshot_and_profile_yield_same_plan_id` heet
  `cam::compile::same_snapshot_compiles_to_identical_plan_hash`.
- DR-18: `ProfileRevision { id, revision, content }`, `MachineProfile::new(id, revision, ..)` berekent `content`;
  test `profile_content_change_changes_plan_id`.
- Afwijking van contract §8: `PlanSource.source_hash` (cam-bronhash) i.p.v. `project_hash` (cam kan io niet zien);
  `JobPlanId` bevat de sessierevisie bewust niet (revisies zijn sessielokaal, staleness via `PlanSource` + preflight).
  `MachinePoint` heeft `f64`-mm-getters i.p.v. `Length`-velden. `bounds()` geeft `geometry::Bounds`.
- `layers.toml`: geen wijziging nodig (`job`, `cam`, `ruida → job` stonden er al).

**Antwoord Design (fase C, 18-10-2026):** DR-15 en DR-18 afgesloten. Afwijkingen van §8 overgenomen (design-review C-6):
`source_hash`, geen sessierevisie in `JobPlanId`, `f64`-getters, `geometry::Bounds`. `HashDomain::PlanSource` mag
(C-5): Kern voegt hem toe, jij zet `source_hash` om; de planhash-goldens wijzigen dan — herzie ze bewust (geen
auto-accept) en log het. Nieuwe contracten voor jou: §IM25 (capability-matrix), §IM26 (nesting), §IM32 (common-line)
in `contracts-phase-c.md`.

---

# Fase C (18-10-2026): IM18-raster, IM25, IM26, IM32-CAM

Logs: `docs/dev-notes/IM18-raster.md`, `IM25.md`, `IM26.md`, `IM32-cam.md`; bugfix bounds + mutaties in `IM15.md`.
fmt + `clippy --all-targets --no-deps -D warnings` schoon op cam/job/image/ruida en mijn bestanden in document/io;
`cargo xtask layers` → 23 crates, geen schendingen. Alleen macOS arm64.

## Aan Design (graag bevestigen)
1. **IM32-contract:** `SharedEdgePolicy::{CutOnce (default), CutPerRegion}` per Line-operation in
   `AdvancedOptions`. Default: gedeeld segment (bitgelijk, andere geometrie, zelfde pass) één keer snijden +
   preflight-`Warning SharedBoundaryCutOnce`; geen tolerantie. Common-line cutting (C12) = aparte `Feature::CommonLine`,
   standaard `Unverified`, niet gebouwd.
2. **Capability-matrix:** `burnburn_job::{Feature, Support, MachineCapabilities}`; `Support::Unverified { reason }` is
   het "unproven"-niveau en gedraagt zich als Unsupported. Standaard Supported alleen voor features die met dezelfde
   events als Line/Fill worden uitgedrukt; ScanAngle/Grayscale/CommonLine Unverified; ZStep Unsupported. Matrix zit in
   de profielhash (DR-18). `MachineProfile::new` ongewijzigd; `with_capabilities` toegevoegd.
3. **Plan-uitbreiding:** `PlanMode::{AngledScan, GrayscaleScan}`, `PlanEvent::{PowerRun, FocusOffset}`; `lower`
   weigert alle drie met een benoemde `Unsupported`. `JobPlan::bounds` neemt nu het beginpunt van elke cut mee.
4. **Afwijkingen van de schets:** Line-opties niet in `OperationMode::Line { .. }` maar in `Operation.advanced`
   (`ProcessCommand::SetAdvanced`), zodat bestaande struct-literals in io/presentation niet breken; subbewerkingen als
   `AdvancedOptions.sub_operations` (geen eigen ids). Image als `OperationMode::Image` met bron = pixel-`ContentHash`
   over de bounds van de target. `nest` gebruikt `PartKey(u32)` i.p.v. `PartInstanceRef` (IM21 nog in aanbouw), geen
   `seed`/`mirror_allowed`; `Utilisation` heeft `denominator_mm2` erbij.
5. **Register:** `burnburn-image` gebruikt nu `libm` (DR-19, tooncurve); `burnburn-cam` hangt af van `burnburn-image`
   (stond al in `layers.toml`).

## Aan Trace (`burnburn-image`)
- Nieuwe module `src/raster.rs` (eigenaar CAM, contract §IM18 `burnburn-image::raster`) + `tests/raster.rs`; edits in
  jullie bestanden: `lib.rs` (`mod raster;` + re-exports), `Cargo.toml` (`libm`). Geen bestaande code gewijzigd.

## Aan Kern
1. Process-edits (eigen module): `process/advanced.rs` (nieuw), `settings.rs` (OffsetFill/Image/Tone/Dither,
   `FillSettings::with_angle`), `operation.rs` (`advanced`), `command.rs` (`SetAdvanced`), `canonical.rs`
   (zonder nieuwe opties exact de IM13-layout: projecthashes blijven geldig; test
   `operations_without_new_options_keep_their_im13_canonical_layout`).
2. **io schema:** `process_map.rs` weigert nu bij opslaan advanced opties, scanhoek en de modi OffsetFill/Image
   (`SaveError::Encode`) i.p.v. ze te laten vallen (test `process_options_without_a_file_schema_are_refused_not_dropped`).
   Verzoek: schema 3 met deze velden; ik lever de mapping graag.
3. **Arrays:** `burnburn_cam::{grid_array, circular_array}` geven `Vec<Affine>`. Voorstel: `Modifier::{GridArray,
   CircularArray}` naast jullie `LinearArray` die deze generatoren gebruiken — dan horen ze in `burnburn-geometry`
   (document mag cam niet zien); verplaatsen mag van mij.
4. **Nesting ↔ IM21:** `nest` levert `Placement { part: PartKey, stock, rotation_deg, transform }`; de vertaling naar
   `PartInstance`/`Plate` en een `ApplyPatch` op de plaat is aan jullie of samen.
5. Jullie lopende `io/tests/structure_roundtrip.rs` (2 tests) is rood bij mijn run; niet door mij veroorzaakt.

## Aan Protocol (`burnburn-ruida`)
- `src/lower.rs` (CAM): benoemde weigeringen + bounds-fix. `Cargo.toml`: dev-dependency `burnburn-image` voor de
  image-lowertest.

## Aan Testing
- Bounds-bug opgelost (`rd_header_bounds_include_a_cut_that_starts_at_the_job_start` groen); mutatieproeven voor de 7
  tests + zinvolle `preview_and_export_carry_the_same_plan_hash` in `IM15.md`.
- Graag fixtures (hardware/LightBurn) voor tabs, leads, overcut, perforatie, scanhoek en dithered image; zonder die
  blijft C11 ("benoemde status, uitvoerbetekenis en fixture") open.

## Aan User-flow / GUI
- `compile_with(.., &dyn RasterSource, ..)` voor beeldoperaties (project-assets als bron); `compile` = zonder bronnen.
- Fouten om te tonen: `CompileError::{UnsupportedFeature{feature, support}, InvalidTabs, MissingImageSource,
  LineOptionsOnNonLineMode, KerfOnOpenContour}`; preflight `UnsupportedFeature`, `SharedBoundaryCutOnce` (warning).
- Preview toont `PowerRun` als Scan; `FocusOffset` heeft geen segment.

## Van Integratie-datamodel (18-10-2026)
- **io schema (jullie verzoek 2):** IM25-opties additief in schema 2: `OperationProcess.advanced` (tabs, leads, overcut,
  perforatie, z-step, sub-operations, shared edges), `Mode::Fill.angle_deg`, `Mode::OffsetFill`, `Mode::Image`. Alles
  weggelaten als het uit staat (v2-fixture herschrijft byte-gelijk). Jullie test `process_options_without_a_file_schema_
  are_refused_not_dropped` is herschreven naar `process_options_are_kept_not_dropped` (gedrag bewust gewijzigd).
  Gevonden en opgelost: `null` in een optioneel getal (NaN) werd "afwezig"; nu geweigerd met pad.
- **Arrays (verzoek 3):** `GridArray`/`CircularArray`/`grid_array`/`circular_array` verhuisd naar
  `burnburn_geometry::array`; `burnburn_cam` re-exporteert ze ongewijzigd (jullie `nest.rs` gebruikt
  `geometry::array::rotation_about`). `Modifier::{GridArray, CircularArray}` in `document::structure` + io.
- **Nesting ↔ plaat (verzoek 4):** `burnburn_cam::nest::apply_nest_command(&result, stock, plate, &instances)` →
  `StructureCommand::ApplyNest { plate, placements: Vec<(PartInstanceId, Affine)> }` (één undo; `Placement.transform`
  wordt de instance-transform; ontbrekende instance = fout, geen gok). Stale-controle via een nest-inputs-hash (contract
  `Delivery::Stale`) is **niet** gebouwd: nu alleen de verwachte revisie van het command.
