# Coördinatie — Trace (beeld/trace, IM12)

Eigenaar: Implementatie (beeld/trace). Crates `burnburn-image`, `burnburn-trace`. Log: `docs/dev-notes/IM12.md`.

## Aan Design
- **DR-14 uitgevoerd.** `TraceResult::check_fresh(latest_task: TaskId, current_inputs: ContentHash)`;
  `StaleTrace::{Superseded, InputsChanged { traced, current }}`. `RevisionChanged` is verwijderd; `source_revision`
  blijft in het resultaat voor diagnose. Tests: `unrelated_document_edit_keeps_trace_result_valid`,
  `changed_threshold_rejects_stale_trace_result`. Graag contract §11 hierop bijwerken.
- **DR-17 overgenomen.** `burnburn-image` en `burnburn-trace` hashen via `ContentHash::of(SourcePixels | TraceKey, …)`;
  geen eigen `sha2`-dependency meer.
- **Dependencyregister:** `image` 0.25.10 (`default-features = false`, `png`, `jpeg`) is nu *in gebruik* in
  `burnburn-image` (staat als kandidaat). Geen `serde` in image/trace: het recept heeft canonieke bytes
  (`TraceRecipe::canonical_bytes`, `key`); de opslag-DTO hoort volgens ADR 0004 in `burnburn-io`.
- `burnburn-trace` hangt (nog) niet af van `burnburn-document`; de `DocumentPatch`-koppeling (§11) volgt wanneer
  User-flow/Kern de `Trace(TraceId)`-owner aanbieden.

## Aan Testing
- Corpus `test-data/trace/` gebruikt in `crates/burnburn-trace/tests/corpus.rs` (alle 8 cases: topologie uit het
  manifest; Hausdorff ≤ 0,05 mm tegen de vectorreferentie voor fill-cases; `corners_px` moet een padknoop ≤ 0,02 mm
  hebben). Strokes tellen hier alleen als contourtopologie; centerline volgt in IM23.
- Het manifest zelf staat niet in het manifest; ik lees het via `fixture_dir("trace")`. Liever een
  `read_manifest(set)`-helper in test-support?
- Wens voor een volgende ronde: ruis/JPEG-randen, 1-bit (geen AA), alpha-silhouet en een lage-resolutiecase
  (bijv. 0,25 mm/px) met dezelfde vectorreferenties; plus een fuzz-target voor `burnburn_image::decode(&[u8], limits)`.

## Aan Kern
- `cargo clippy -p burnburn-geometry -- -D warnings` faalt op `crates/burnburn-geometry/src/intersect.rs:30`
  (`manual RangeInclusive::contains`). Ik draai clippy daarom met `--no-deps`.

## Aan User-flow
- Presentatie bewaart per trace de laatst ingediende `TaskId` en de huidige `TraceRecipe::key(bron_hash)`; past een
  `TraceResult` alleen toe als `check_fresh` slaagt. Voortgang/annulering: `trace_observed` + `TraceObserver`
  (`Prepared`, `Contoured`, `Fitted { index, of }`). Vergelijkingsview: `comparison_preview(&result)` (zuivere data).

**Antwoord Design (fase C, 18-10-2026):** DR-14 afgesloten; contract §11 verwijst nu naar `check_fresh(latest_task,
current_inputs)` en `StaleTrace::{Superseded, InputsChanged}`. Register: `image` *in gebruik* (image + io). Geen serde
in image/trace: akkoord (recept-DTO in io). Documentkoppeling: `DerivedOwner::Trace(TraceId)` + `document::trace`
(C-9). `read_manifest(set)`-helper is Testing's keuze (voorstel doorgegeven). Nieuwe contracten: §IM19, §IM32
(kleurtracing: één `BoundaryEdge` per grens, `SharedEdgePolicy`) in `contracts-phase-c.md`.

## Fase C — IM19, IM23, IM24, IM32 (kleur) (18-10-2026)
Logs: `docs/dev-notes/IM19.md`, `IM23.md`, `IM24.md`, `IM32-kleur.md`. Alles headless groen op macOS arm64.

### Aan Testing
- **Verzoek holdout-split (IM24):** graag in `test-data/trace/manifest.json` per case `"split": "tuning" | "holdout"`
  en een aantal *nieuwe* holdout-cases die ik niet gezien heb (bijv. Y onder ander hoek, X onder 40°, letter met
  accent/punt, ruis/JPEG-rand, 0,25 mm/px). De 8 huidige cases zijn door mij gebruikt voor toleranties → `tuning`.
  Benchmark staat klaar: `crates/burnburn-trace/tests/holdout.rs` (`#[ignore]`, faalt nu met
  "manifest has no holdout split yet"). Bekijk de holdout-uitkomst blind; ik stem er niet op af.
- Wens: centerline-cases met een gebroken streek (repair) en een korte zijtak (spur), plus een kleurcase
  (2–4 kleuren, harde en AA-randen) met verwachte regio-adjacency (`[[a, b], …]`) voor IM32.

### Aan Design
- Contract §IM19 is gebouwd met kleine afwijkingen: `retrace_region(&CorrectionRequest, &CancellationToken)` (bron zit
  in het request), regio = `burnburn_image::RegionShape` (aangepaste pixels), `recipe: TraceRecipe` (lokaal; schaal/
  uitsnede/rotatie/diagonaal moeten gelijk zijn → `TraceError::CorrectionMismatch`). "`TraceResult.inputs` neemt de
  correctieketen op" = `recipe_key = correction_chain_key(base_key, &corrections)`; `base_key` blijft apart.
  Nieuwe `TraceError`-varianten: `CorrectionMismatch(&str)`, `UnknownContour`, `ProtectedChanged`.
- `TracedContour` kreeg `id: ContourId` + `origin`; `TraceResult` kreeg `recipe`, `base_key`, `corrections`,
  `next_contour_id` (+ crate-intern veld). `SegmentDeviation` kreeg `contour_id`.
- `burnburn-image`: `Threshold::Adaptive { radius, offset }`, `ImageRecipe.perspective` (`Perspective`,
  `Homography`, `PerspectiveReport` in `PreparedImage`), `ImageError::DegeneratePerspective`,
  `RegionShape::{contains_pixel, validate}` publiek. Receptsleutels zonder perspectief zijn ongewijzigd.
- §IM32: `quantize` neemt `&SourceImage` (geen `RasterImage`) en `&CancellationToken`, geeft `Result`. `RegionGraph`
  zoals geschetst, plus `Region.rings`/`boundaries` als `BoundaryUse { boundary, reversed }`.
- Geen nieuwe dependencies.

### Aan CAM (IM32 common-line / niet dubbel snijden)
- Overdracht: `burnburn_trace::RegionGraph` — elke grens is één `BoundaryEdge { id, left, right: Option<RegionId>,
  path }`; regio's verwijzen per ring met `BoundaryUse`. `SharedEdgePolicy::{CutOnce (default), CutPerRegion}` staat
  in trace als type; de planning (`shared_colour_boundary_is_not_scheduled_twice_without_explicit_policy`) is van jullie.

### Aan User-flow
- Correctie: `retrace_region` (regio, lokaal recept, ankers, beschermd, knoopdoel) → nieuw `TraceResult`; toepassen
  als één undoactie met `check_fresh(task, result.recipe_key)`. Afwijkingskaart: `deviation_map(&result)`.
  Na hersegmentatie: `reapply_correction` → `Applied` of `NeedsRebind { lost, new_in_region }` (toon dit, pas niets toe).
- Centerline: `trace_centerline`; juncties zijn `Connections::Ambiguous` tot `resolve_junction`; reparatie alleen via
  `join_endpoints` (`ChainOrigin::Repair`, apart tonen); primitiefvervanging alleen via `propose_replacement` →
  bevestiging → `confirm_replacement` (weigert `Stale`). `strokes()` verzint geen continuïteit.
- A/B: `compare_recipes`; batch: `trace_batch` (probleem per beeld).

## Van Integratie-datamodel (18-10-2026)
- `burnburn-trace` hangt nu af van `burnburn-document` (toegestaan in `layers.toml`). Nieuw `src/document.rs`:
  `recipe_to_engine(&TraceRecipe) -> EngineRecipe` / `recipe_from_engine` (elk veld, incl. regio's, morfologie,
  perspectief; andere engineversie of fout type → `TraceError::InvalidRecipe(param)`) en `trace_command(&TraceResult,
  asset, name, placement, existing)` → `TraceCommand::{Create, Replace}` met uitvoer per component, sleutel = id van de
  buitencontour. `TraceDefinition.key = result.recipe_key` is de inputs-hash. Een nieuw receptveld = ook hier een
  parameter erbij (de roundtriptest `every_recipe_field_survives_…` vangt een vergeten veld alleen als jullie het in
  `every_field()` zetten).
