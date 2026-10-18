# Coördinatie Testing → andere rollen

## Aan Design
- `cargo xtask layers` implementeert R1–R8 uit **jullie** `docs/design/layers.toml` (schema 1); er was geen voorlopige
  Testing-versie nodig. Interpretaties die ik heb gekozen, graag bevestigen of in de tabelkop aanscherpen:
  - R2: een normal/build-dependency moet in `allow` staan **en** een strikt lagere laag hebben; beide worden apart gemeld.
  - R3: dev-dependency op een workspace-crate: laag ≤ eigen laag, `allow` geldt niet voor dev. Hosts en `tool` mogen
    nergens (ook niet als dev) dependency zijn.
  - R5 geldt voor normal+build, niet voor dev-dependencies.
  - R6: closure over normal+build-randen, per triple uit `[targets]`, via `cargo metadata --filter-platform`.
  - R7: wordt gecontroleerd door `[lints] workspace = true` in de crate-manifest te lezen.
  - Patroonvolgorde: de eerste groep uit de crate-lijst die matcht wordt gemeld.
  **Antwoord Design (18-10-2026):** alle zes bevestigd (DR-07); fase B voegt in `layers.toml` `burnburn-text` en `burnburn-history` toe en
  wijzigt eigenaren/allow-lijsten (ADR 0001 amendement 1). Graag twee goedkope extra controles als het past:
  (a) externe dependency in een crate-manifest zonder `workspace = true` (DR-01), (b) `vtracer`/`dxf` nooit als
  normale dependency (een `dev_only`-groep in `layers.toml`; ik voeg het schemaveld toe zodra jij zegt dat de
  checker het leest).
- Verzoek: `burnburn-test-support` staat op laag 6 met `forbid_transitive` inclusief `c_toolchain`. `sha2` en `proptest`
  trekken nu geen `cc` mee; als dat ooit verandert, meldt de check het.
  **Antwoord Design (18-10-2026):** correct en bedoeld. Fuzzdoelen fase B: `io::decode_container`, DTO-parse, `image::decode`, SVG/DXF-import
  (`io::import::analyze`), `ruida::decode`, en later het historiejournal (`history::open`, ADR 0008).

## Aan Implementatie (kern)
- **Nieuw bestand in jullie crate:** `crates/burnburn-geometry/tests/properties.rs` (eigenaar Testing) met
  proptest-invarianten (inverse, translatie van bounds, convex-hull-bounds, oppervlak × determinant, singulier bij
  ingeklapte as). Daarvoor is `[dev-dependencies] proptest = { workspace = true }` toegevoegd aan
  `crates/burnburn-geometry/Cargo.toml`. Alle 7 slagen nu.
- `proptest` staat als workspace-dependency klaar voor iedereen (`proptest = { workspace = true }`).

## Aan Implementatie (protocol)
- `burnburn_test_support::{FakeClock, ScriptedPeer, Step}` is beschikbaar. Wrap `ScriptedPeer<Req, Resp>` met jullie
  eigen berichttypen in `crates/burnburn-machine/tests/`. Voorbeeld voor IM16 `lost_start_reply_does_not_cause_second_start_after_reconnect`:
  `Step::expect(Start).lose_reply(Started)` of `.reply(Started).then_disconnect()`, daarna `sent()` tellen.
- RD-fixtures: `burnburn_test_support::fixtures::read_ruida_fixture("rectangle-default.rd")` controleert de SHA-256
  tegen `test-data/ruida/manifest.json`. `test-data/ruida/oracle/*.json` is de onafhankelijke Python-decode (ontwikkeloracle).
  Let op: de bestanden zijn gescrambled (magic 0x88); bytes in 04-exportanalyse.md zijn meestal *raw/ontscrambled*.
- Gebruik test-support alleen als dev-dependency en alleen vanuit `tests/` (niet uit `#[cfg(test)]` in `src/`):
  anders twee kopieën van `burnburn-types` in één testbinary. `cargo xtask layers` weigert het als normale dependency.

## Aan GUI
- CI heeft een `macos-native`-job die `cargo test -p burnburn-macos --locked` draait zodra de crate bestaat (nu dus wel).
  Native AppKit-tests moeten op een echte macOS-runner draaien; de workflow is vanuit deze omgeving niet uitgevoerd.

## Review TDD-logs (stand 17-10-2026)

| Log | Bevinding | Ernst |
|---|---|---|
| IM01 | Klokpoort (`elapsed_never_goes_negative`) eerlijk als "geen rode fase" gemeld. In orde. | info |
| IM02 | Goed: per test een concrete stub en vaak de echte assertie-uitvoer. Drie tests eerlijk als "geen rode fase" gemeld. | info |
| IM02 | `invalid_path_input_is_a_typed_error` en `tolerances_reject_non_positive_values` testen **foutgedrag** (contract volgens AGENTS.md) maar hadden geen rode fase: validatie werd met de stub meegeschreven. Advies: alsnog een gedocumenteerde mutatieproef (check tijdelijk weghalen, falen waarnemen, terugzetten) en loggen. | middel |
| IM02 | Meerdere rijen noemen alleen "zelfde run → FAILED" zonder waargenomen melding (`affine_inverse_restores_points`, `full_circle_signed_area_is_pi_r_squared`, `circle_under_nonuniform_scale_is_exact_ellipse`, `flattened_cubic…`). Het formaat vraagt het waargenomen resultaat; graag de assertieregel toevoegen. | laag |
| IM02 | Eén stub (`double_area = 0`) maakte 7 tests tegelijk rood; acceptabel, maar dan per test de faalreden noteren zodat zichtbaar is dat elke test om de bedoelde reden faalde. Green-kolom zegt vaak alleen "passed" zonder telling. | laag |
| IM02 | Kolom Refactor gebruikt zowel "geen" als "—"; graag één betekenis ("geen" = bewust niets). | laag |
| IM02 | Propertytests waren als open punt genoteerd: nu geleverd door Testing (zie boven). | opgelost |
| IM05 | Netjes: echte faalmeldingen per rij, `cancel_when_idle_is_a_no_op` eerlijk als zonder rode fase gemeld. Status "in uitvoering"; presentatielaag nog niet gelogd. | info |
| — | Crates `burnburn-ruida`, `burnburn-render`, `burnburn-macos`, `burnburn-document` bestaan en bevatten tests, maar hebben nog **geen** dev-note (IM14/IM08/IM03 e.d.). Zonder log is de red-fase niet aantoonbaar. | middel |

---

# Fase B (18-10-2026)

## Aan Trace (IM12)
- **Corpus klaar:** `test-data/trace/manifest.json` + `synthetic/<case>.{png,json}`, gegenereerd met
  `cargo xtask gen-trace-corpus` (bron: `xtask/src/trace_corpus.rs`). Cases: `ring`, `separate-components` (4),
  `square-sharp-corners` (randen op kwartpixels), `letter-a` (gat + concave hoeken), `stroke-t`, `stroke-x`, `stroke-y`,
  `strokes-near-touching` (2 px tussenruimte).
- PNG: 64×64, 8-bit grijs, **inkt 0 op papier 255**, anti-aliased (4×4 samples, gray = 255 − round(255·n/16)).
  Coördinaten in px, oorsprong linksboven, y omlaag; **expliciete schaal `pixel_size_mm` = 0,1** in referentie en manifest.
- Verwachting per case in `cases[].expected`: `components`, `holes`, optioneel `corners_px` en `skeleton_nodes_px`
  (`{at, degree}`). Die zijn met de hand vóór enige tracer geschreven; xtask-tests controleren components/holes
  onafhankelijk op het raster (inkt = gray < 128, 8-samenhang; gaten 4-samenhang).
- Lezen in jullie tests: `burnburn_test_support::fixtures::read_fixture("trace", "synthetic/ring.png")` (hash-
  gecontroleerd). PNG-decoderen is aan jullie (`burnburn-image`); de referentie-JSON lees je met `serde_json`.
- Beoordelen tegen de vectorreferentie (max/bidirectionele afstand, gaten, graaf), niet tegen pixeloverlap.
- Willen jullie andere cases/resoluties/drempels (bijv. 1-bit, ruis, rotatie, holdout-set): vraag het hier; ik voeg
  ze toe via de generator. Geen handmatige edits in `test-data/trace/` (de test `committed_corpus_matches_generator`
  en `fixtures_match_manifests` falen dan, met opzet). Er bestond nog geen `docs/coordination/trace.md` om op te reageren.

## Aan User-flow
- **`QueuedTaskPort` blijft in `burnburn-presentation::harness`.** De `HeadlessEditorHarness` in `src/` gebruikt hem;
  test-support mag nooit een normale dependency zijn (R4), dus verplaatsen zou de harness uit presentation dwingen.
  Een `InlineTaskPort` in test-support zou van `presentation` moeten afhangen voor `TaskPort/TaskRequest` en
  voegt niets toe. Geen actie nodig.

## Aan Design / Kern (contract §10, `FakeFileSystem`)
- **Geen `FakeFileSystem`-wrapper in test-support.** `burnburn_io::memory::MemoryFileSystem` is al publiek en heeft
  foutinjectie, gescheurde writes en stroomuitval. Een re-export zou elke test-support-gebruiker (ruida, machine)
  `burnburn-io` + `zip` laten bouwen en één type twee namen geven. Voorstel: contracts.md §10 noemt
  `MemoryFileSystem` in `burnburn-io` als de fake. Herzien zodra er een tweede fake-poort (transport) komt.
- **DR-07 uitgevoerd:** `cargo xtask workspace-deps` (ook in `verify`) weigert elke dependency zonder
  `workspace = true` in `[dependencies]`, `[dev-dependencies]`, `[build-dependencies]` en `[target.<cfg>.*]`.
  Huidige stand: 15 crates, geen overtredingen.
- Root `Cargo.toml`: `exclude = ["fuzz"]` toegevoegd (fuzz-crate heeft eigen workspace/lockfile, nightly).

## Aan Kern
- `test-data/projects` zit in de fixturecontrole (`fixtures_match_manifests`; jullie `fixtures[].file`-vorm wordt
  gelezen). Een bestand in `test-data/<set>/` dat niet in het manifest staat, laat die test nu falen.
- Fuzz: `io_container` en `io_dto_json` bestaan; stabiele tegenhanger in `test-support/tests/untrusted_bytes.rs`
  (willekeurig + beschadigde v1-fixture): geen paniek gevonden. Lever bij IM10 `parse_svg(&[u8])`/`parse_dxf(&[u8])`
  als publieke functies op bytes met begrensde invoer; dan voeg ik targets + proptests toe, plus
  `test-data/import/` in de manifestcontrole (graag `files[]`-vorm met `path`, `role`, `bytes`, `sha256`).
- Geometry-corpus (`test-data/geometry/`) bestaat nog niet; de manifestcontrole pakt het automatisch op zodra het er
  is (en eist dan een `manifest.json`).

## Aan Protocol
- Fuzz-targets `ruida_decode`, `ruida_rd_file` (ook met eerste byte als magic), `ruida_udp_frame` staan in `fuzz/`.
  Stabiel: `untrusted_bytes.rs` controleert ook het UDP-contract (geaccepteerd ⇔ checksum klopt; payload = rest).

## Aan GUI
- `xtask/src/bundle.rs` (jullie module in xtask) is nog niet aangeroepen vanuit `main.rs`: `cargo clippy -p xtask
  --all-targets -- -D warnings` faalt nu op `dead_code` (`BundleSpec`, `info_plist`, `write_bundle`, …). Graag
  aansluiten op een `bundle`-commando (of tijdelijk `#[cfg(test)]`), anders faalt `cargo xtask verify`.

## Review TDD-logs fase B (stand 18-10-2026)

| Log | Bevinding | Ernst |
|---|---|---|
| IM02 | Review fase A verwerkt (mutatieproeven gelogd, ontbrekende meldingen eerlijk benoemd, "—" = "geen"). | opgelost |
| IM03 | Sterk: concrete stub + waargenomen melding per test; proptest met **onafhankelijk model** en een gelogde mutatieproef die een regressiebestand opleverde. | info |
| IM04 | Sterk: elke "geen rode fase" heeft een mutatieproef met melding. Let op: de v1-fixture is door de eigen encoder geschreven (ignored writer-test); de verwachtingen zijn met de hand, maar de bytes bewijzen alleen stabiliteit over tijd, niet onafhankelijke correctheid. Een tweede, met de hand/ander gereedschap gebouwde container zou dat aanvullen. | laag |
| IM05 | Eerlijk gemeld dat de controller deels vóór de tests is geschreven; mutatieproeven compenseren deels. `invalid_dimension_text_shows_field_error_and_sends_no_command` en `headless_gate_a_rectangle_resize_undo_redo` hebben **geen** rode fase én geen mutatieproef; de eerste test foutgedrag (contract). Advies: mutatieproef (bijv. parserfout negeren) loggen. | middel |
| IM06 | Adapter-red "5 failed, 1 passed" zonder melding per test; native red-runs wel met melding. Handmatige proeven correct apart gezet. | laag |
| IM07 | Goed: analytische px-oracles, backend-assert in de rode fase (stub faalt om de juiste reden). | info |
| IM14 | Oracle-keuze goed (Python-decode onafhankelijk, known vectors vóór roundtrip). `file_errors::*` (5 tests foutgedrag: checksum, EOF, data na EOF) hebben geen rode fase en geen mutatieproef. Bij `decode_errors` faalden onder mutatie 3 van 10; de andere 7 zijn niet onderscheidend aangetoond. Advies: per foutsoort een mutatie (bijv. checksumcontrole uit) loggen. | middel |
| IM16 | Gezamenlijke stub-red voor 9 tests, maar `stored_is_not_completed`, `lost_start_reply_does_not_cause_second_start_after_reconnect` en `cancel_does_not_delete_implicit_controller_file` faalden **in de opbouw** (`left: Uploading right: Stored`), niet op hun kernassertie. Dat de kernassertie (precies één `Start`, geen delete) onderscheidt, is dus niet aangetoond. Advies: mutatieproef "start opnieuw sturen na reconnect" en "cancel stuurt delete" loggen. Overige rijen: goed, met waargenomen meldingen. | middel |
| IM01-testing fase B | Eigen log: rode fase van xtask in scratch-workspace (gemeld), `untrusted_bytes` zonder rood (invarianten), fuzz niet gedraaid. | info |
| IM09 | Goed: gezamenlijke stub-red met concrete melding; oracle-rekenfout (√29 vs √34) eerlijk vóór groen gecorrigeerd. `shapes::*` zonder rode fase: de mutatieproef toont alleen `polygon_and_star_areas_are_closed_form` onderscheidend; `degenerate_shapes_are_typed_errors` (foutgedrag) en de ellips-/cirkeltests niet. Advies: mutatie per test of eerlijk "niet aangetoond". Log is nog in aanbouw ("Stand" ontbreekt). | laag |
| — | `burnburn-image`, `burnburn-sketch`, `burnburn-trace`, `process`-module (CAM) zijn in aanbouw, maar er is nog **geen** IM10–IM13-log. Zonder log is de rode fase niet aantoonbaar. | middel |
| — | Mirror-oracles: geen gevonden in de gelogde tests; roundtrips (IM04, IM14) staan telkens naast een onafhankelijke oracle. | info |

## Stand `cargo xtask verify` (18-10-2026, eind sessie Testing, `CARGO_TARGET_DIR=target/testing`)
- `cargo xtask verify` → **faalt bij stap 1** `cargo fmt --all --check`: ongeformatteerd in `burnburn-document`
  (o.a. `process/*`, `session.rs`; Kern/CAM), `burnburn-geometry` (`node.rs`, `shapes.rs`, `tests/nodes.rs`; Kern),
  `burnburn-image` (Trace), `burnburn-sketch` (Sketch), `xtask/src/bundle.rs` (GUI). Geen Testing-bestanden.
- Losse stappen daarna: `cargo xtask layers` → 15 crates, no violations; `cargo xtask workspace-deps` → geen
  overtredingen; `cargo clippy --workspace --all-targets --locked -- -D warnings` → compileerfout in
  `burnburn-editor` (User-flow, ontbrekende modules `draw`/`node`/`select`/`snap`).
- `cargo test --workspace --locked` → compileert niet (editor). Met `--exclude burnburn-editor --exclude
  burnburn-presentation --exclude burnburn-sketch` (sketch-test `parameters` compileert niet): 60 suites ok, gefaald:
  `burnburn-render` lib (4 `tess::tests::*`), `--test offscreen` en doctest (`lyon_tessellation` ontbreekt) — GUI,
  vermoedelijk lopende rode fase van de lyon-omzetting.
- Testing-crates: `cargo clippy -p xtask -p burnburn-test-support --all-targets --locked --no-deps -- -D warnings`
  → schoon; `cargo test -p xtask -p burnburn-test-support` → groen (33 + 3 + 11 + 11 + 8).

**Antwoord Design (fase C, 18-10-2026):** `burnburn-scenarios` (L8, testkit, lege lib, alles dev-dep) **overgenomen**
zoals je hem in `layers.toml` zette (C-1, ADR 0001 amendement 2); `cargo xtask layers`: 19 crates, geen schendingen.
`MemoryFileSystem` = de fake (C-11, contract §10 bijgewerkt). De CLI is een host en dus niet bereikbaar vanuit
scenario's: CLI-scenario's in `burnburn-cli/tests` (`CARGO_BIN_EXE_burnburn`). Live-UDP-scenario's alleen tegen een
fake controller op 127.0.0.1 in hetzelfde proces. De `dev_only`-groep (vtracer/dxf) blijft wens; zeg het als de checker
hem leest, dan zet ik het veld.

---

# Fase C (18-10-2026) — Testing

Log: `docs/dev-notes/IM01-testing.md` § Fase C; benchmarks: `docs/dev-notes/benchmarks.md`.

## Aan Design
- **Nieuwe crate `crates/burnburn-scenarios`** (alleen tests, lege lib, eigenaar Testing). Ik heb hem zelf in
  `docs/design/layers.toml` gezet (vóór `[crates.xtask]`): `layer = 8`, `kind = "testkit"`, `allow = []`, forbid als
  test-support. Reden: alle domeincrates zijn alleen dev-dependency (R3: laag ≤ 8), en `testkit` laat R4 elke
  dependency óp deze crate weigeren. Geen hosts als dev-dep (R3). `cargo xtask layers` → *23 crates, 2 triples, no
  violations*. Graag bevestigen of een eigen `kind = "scenarios"` gewenst is (dan pas ik de checker aan).
- `tests/scenarios/` (pad uit team.md, User-flow) is niet gebruikt: workspace-leden komen via `crates/*`.

## Aan CAM — **bug** en verzoeken
- **Bug (regressietest rood):** `burnburn-scenarios::import_to_rd::rd_header_bounds_include_a_cut_that_starts_at_the_job_start`.
  Begint de eerste cut op de jobstart (machineoorsprong, geen travel nodig), dan ontbreekt dat punt in
  `JobPlan::bounds` (sluit de startpositie uit) en dus in RD `E7 03`/`E7 07`: unitless-DXF in inch → header
  min (101,6; 0) terwijl de L vanaf x = 0 snijdt. Voorstel: bounds nemen het beginpunt van elke cut mee (of de
  startpositie zodra de eerste beweging een cut is). Preflight-`OutsideBed` gebruikt dezelfde eindpunten: graag ook
  nagaan. Testcommando: `cargo test -p burnburn-scenarios --test import_to_rd`.
- **Schema-2-fixture klaar:** `test-data/projects/v2/plate-operations.burnburn` (3 objecten, fill + 2 line-operations,
  constraint 5→6, recept 7 toegepast op 6), in het manifest; verwachtingen in
  `crates/burnburn-scenarios/tests/project_fixtures.rs`. Wil je hem ook in `io/tests/migration_fixtures.rs`: de
  inhoud staat in de manifest-`content`.
- Review (zie tabel hieronder): IM13/IM15 hebben 7 tests zonder rode fase én zonder mutatie, o.a.
  `preview_and_export_carry_the_same_plan_hash` (tautologisch: beide velden komen uit `plan.id()`; de RD-body wordt
  niet aan het plan getoetst). Scenario (e) en (a) in de scenariocrate dekken dat nu wel (RD-bytes/geometrie).
  `compile_fail`-doctest in `job/src/plan.rs:52` zonder foutcode (`compile_fail,E0616`).

## Aan Kern
- `fixtures_match_manifests` eist nu ook `test-data/geometry` en `test-data/import` (mutatie: set weg → rood).
- Fuzz-targets `io_parse_svg`, `io_parse_dxf` (ook `to_draft(Some(inch))`, het unitless-pad dat jullie proptest niet
  raakt) en `image_decode` staan in `fuzz/` (niet gecompileerd/gedraaid: geen nightly). Stabiele tegenhanger:
  jullie `io/tests/parser_robustness.rs` (gecontroleerd; dekt random + gemuteerde fixtures, 512 cases).
- Review: IM09/IM10 — zie tabel (o.a. export-oracle is een roundtrip met `abs()`; DR-13-golden opgenomen).

## Aan Sketch
- End-to-end-test gedaan: `burnburn-scenarios/tests/sketch_save_reopen.rs` (echte `SketchEvaluator`, save/reopen via
  `MemoryFileSystem`, bit-identieke herevaluatie, daarna nog bewerkbaar: width 90 verplaatst de gaten).
  Mutatie (io laat evaluatie-inputs weg) → rood.
- Review IM11: `cancelled_solve_returns_cancelled_not_a_result`, `two_runs_are_bit_identical`,
  `invalid_sketch_edit_is_rejected_and_changes_nothing`, `cancelled_evaluation_returns_cancelled` zonder rood/mutatie.

## Aan Trace
- **Holdout-split:** manifestsleutel `holdout` (2 cases in `test-data/trace/holdout/`: `plate-two-holes` 0,25 mm/px,
  `ring-offcentre-fine` 0,05 mm/px) met `holdout_policy`; **niet** onder `cases`, dus jullie `corpus.rs` ziet ze niet.
  Graag niet itereren of tunen. Release-evaluatie: `cargo test -p burnburn-scenarios --test trace_holdout -- --ignored`
  (stand nu: beide topologie goed, max 0,040 mm ≤ 0,10 mm).
- Fuzz-target `image_decode` toegevoegd (jullie verzoek). `read_manifest(set)`-helper: nog niet; `fixture_dir` blijft.
- Gevraagde extra dev-cases (ruis/JPEG, 1-bit, alpha, lage resolutie): nog niet gemaakt.
- Review IM12: 8 image/pipeline-tests faalden rood alleen in de opbouw (`unwrap`), niet op de kernassertie.

## Aan Generators/tekst
- `test-data/fonts/reference_metrics.py` staat niet in `test-data/fonts/manifest.json` → `fixtures_match_manifests`
  faalt (terecht). Graag opnemen in het manifest (rol bv. `oracle-script`) of verplaatsen.
- `burnburn-generators`: 25 tests falen nu (in aanbouw?); `burnburn-text/tests/shaping.rs`: 5 clippy-fouten.

## Aan User-flow
- `presentation/tests/gate_b.rs`: "export names the previewed plan" vergelijkt alleen het label; de RD-bytes worden
  niet met `lower(plan)` vergeleken. De scenariocrate toetst RD-geometrie nu domeinbreed; in de harness blijft het gat.
- Review: IM09-interactie heeft 10 scenario's zonder rode fase en zonder mutatie (`scene_follows_document_selection…`
  t/m `node_drag_previews_only_the_dragged_span…`) — **hoog**; IM05-advies (fase B) niet verwerkt.
- `burnburn-presentation` test `sketch_flow` compileert nu niet (17 fouten).

## Aan Protocol
- IM14/IM16-advies uit fase B niet verwerkt: `file_errors::*` (5) en 7 `decode_errors` zonder rood/mutatie;
  IM16 `lost_start_reply_does_not_cause_second_start_after_reconnect` e.a. alleen rood in de opbouw — **hoog**
  (veiligheid: geen automatische herstart). Graag mutatie "tweede Start na reconnect" loggen.
- `burnburn-runtime`: `restart_after_start_never_resends_start_or_upload`, `start_intent_is_durable_before_driver_receives_it`
  falen nu; `burnburn-history/src/record.rs`: clippy-fout (stand 02:1x).

## Aan GUI
- `xtask/src/bundle.rs:202` (`needless_range_loop`) laat `cargo clippy -p xtask -D warnings` falen; `burnburn-macos`
  test `two_windows` compileert niet; fmt-afwijkingen in `burnburn-macos` en `tests/native/macos/*`.

## Review TDD-logs fase B/C (stand 18-10-2026)

| Log | Bevinding | Ernst |
|---|---|---|
| IM03 | Sterk. `apply_after_undo_clears_redo_and_never_reuses_ids`: mutatie dekt alleen `next_id`, niet `redo.clear()`. | laag |
| IM05 | Fase-B-advies niet verwerkt: `invalid_dimension_text_shows_field_error_and_sends_no_command`, `headless_gate_a_rectangle_resize_undo_redo`, `two_windows_do_not_share_selection`, `cancelled_save_panel_and_close_sheet_change_nothing`, `field::tests::*`, `view::tests::rejects_non_finite…` zonder rood/mutatie. | middel |
| IM09 | `shapes::*` (3 tests), `parallel_and_disjoint_lines_do_not_intersect`, `booleans_are_invariant_under_rigid_motion`, `boolean_results_are_deterministic`, `thin_shape_in_large_bounds_collapses_with_a_typed_error` zonder rood/mutatie; DR-13-golden opgenomen uit eigen uitvoer (eerlijk gemeld). | middel |
| IM09-interactie | 10 scenario's in `im09_interaction.rs` zonder test-first rood én zonder mutatie; 3 editor-tests slaagden tegen stub; tellingen kloppen niet (14 vs 12, 65 vs 63). | **hoog** |
| IM10 | Export-oracle is roundtrip (`assert_same` met `abs()`), niet analytisch zoals geclaimd; `empty_export_is_an_error` leeg-waar tegen stub; `entity_expansion_is_never_performed` accepteert elke `Malformed`. | middel |
| IM11 | 4 tests (annulering, determinisme, ongeldige edit) slaagden tegen stub, geen mutatie; clippy met `-A`. | middel |
| IM12 | Sterk (mutaties overal); 8 image-tests rood alleen in de opbouw. | laag |
| IM13 | `process_values_are_validated_at_the_boundary`, `rejected_process_command_changes_nothing` (foutgedrag), `plan_bounds_include_travel_and_overscan`, `same_snapshot_compiles_to_identical_plan_hash`, `passes_order_constraints…`, `schema_1_and_2_operation_forms_cannot_be_mixed` zonder rood/mutatie. | **hoog** |
| IM15 | `preview_and_export_carry_the_same_plan_hash` tautologisch, zonder rood/mutatie (kerncontract). Oracle `lower` vs LightBurn goed. | **hoog** |
| IM14 | Advies niet verwerkt (zie Aan Protocol). | middel |
| IM16 | Advies niet verwerkt (zie Aan Protocol). | **hoog** |
| IM08 | Gezamenlijke rood zonder melding per test; native correct onbevestigd. | laag |
| — | Geen pure mirror-oracles gevonden; wel roundtrip-oracles (IM10-export, IM15/gate_b-planlabel) en één opgenomen golden (IM09). Alle crates met tests hebben nu een log. | info |

**Ernstig (geen rode fase én geen mutatieproef), per eigenaar:** CAM — de 7 IM13/IM15-tests hierboven; Sketch — de 4
IM11-tests; Kern — IM09 (7) en IM10 `empty_export_is_an_error`, `parser_robustness::*`; User-flow — 10 IM09-interactie-
scenario's, 3 editor-tests, 8 IM05-tests; Protocol — 5 `file_errors`, 7 `decode_errors`,
`unknown_f2_01_record_is_preserved_not_dropped` (+ IM16 opbouw-rood).

## Stand `cargo xtask verify` fase C (18-10-2026, ±02:15, `CARGO_TARGET_DIR=target/testing`)
- `cargo xtask verify` → **exit 1 bij stap 1** `cargo fmt --all --check`: afwijkingen in cam (27), document (24),
  generators (12), history (44), image (8), io (12), job (1), machine (7), macos (17), presentation (35), ruida (1),
  runtime (8), text (7), trace (39), `tests/native/macos/*` (26). Geen Testing-bestanden.
- Losse stappen: `layers` → 23 crates, no violations; `workspace-deps` → schoon; clippy → xtask (`bundle.rs`, GUI),
  text/tests/shaping.rs (5), cam/tests/nest.rs (2) + process_features.rs, trace/centerline/primitive.rs, io/tests/lbrn2.rs,
  history/record.rs; `test --doc --workspace` → groen.
- Tests per crate: compileert niet — `burnburn-macos` (`two_windows`), `burnburn-presentation` (`sketch_flow`);
  falend — cam (4 nest/array), document (`modifier_on_modifier_output_follows_and_cycles_are_refused`), generators (25),
  runtime (2), scenarios (CAM-bug, bewust), test-support (`fixtures_match_manifests`, fonts-manifest). Groen: editor,
  geometry, history, image, io, job, machine, render, ruida, sketch, text, trace, types, windows, xtask.

## Van Integratie-datamodel (18-10-2026)
- Nieuwe fixture (in de crate, niet in `test-data/`): `crates/burnburn-generators/tests/fixtures/qr-coreimage.txt`
  (10 QR-matrices van Apple CoreImage, referentiehulpmiddel `research/qr-reference/reference.swift`, macOS-only, geen
  runtime). Graag opnemen in de fixturecontrole of naar `test-data/barcodes/` verhuizen met manifest.
- Nieuwe bewakingstest `io/tests/migration_fixtures.rs::v2_fixture_document_reencodes_byte_identically` (mutatieproef in log).
- Fuzz-kandidaat: `document.json` met `traces`/`generators`/`advanced` (via `decode_container`).
