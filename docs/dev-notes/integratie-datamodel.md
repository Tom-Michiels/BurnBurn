# Integratie-datamodel (fase C/D, losse eindjes tussen rollen)

Rol: Implementatie (integratie-datamodel). Alle commando's met `export PATH="$HOME/.cargo/bin:$PATH"` en
`CARGO_TARGET_DIR=target/integration-a`, vanuit de repo-root, macOS arm64. Formaat: `docs/dev-notes/README.md`.

Uitgangspunt (vóór wijzigingen): `cargo check --workspace --all-targets` groen; `cargo clippy -p
burnburn-{types,geometry,document,io,sketch,generators,cam,trace,scenarios} --all-targets --no-deps -- -D warnings`
schoon; `cargo fmt --all -- --check` schoon; `cargo test` op document/io/sketch/generators/cam/trace/scenarios groen.

## 1. `DerivedOwner::Trace` en `DerivedOwner::Generator` (C-2, C-8, C-9)

Ontwerp: `document::engine::{EngineRecipe, ParamValue, EngineLayer, EngineError}` (engine = `naam/versie` + getypeerde
parameters; io kan dit opslaan zonder trace/generators te kennen), `document::trace::{TraceId, TraceDefinition {
name, source (asset), recipe, key (= inputs-hash, `TraceResult.recipe_key`), placement }, TraceOutput, TraceCommand::{
Create, Replace, SetPlacement, Delete}}`, `document::generator::{GeneratorId, GeneratorDefinition, GeneratorCommand,
generator_inputs}`. Trace-uitvoer komt trace-lokaal binnen en wordt door het document geplaatst; `ApplyPatch` voor een
trace wordt gecontroleerd tegen `key`, voor een generator tegen `generator_inputs` (domein `eval-generator`).
Stub voor de rode fase: datatypes + `CommandPayload::{Trace, Generator}` waarvan de sessie-arm niets deed (`Ok(())`).

| Test | Verwachte fout (red) | Red-commando en waargenomen resultaat | Green-commando en resultaat | Refactor |
|---|---|---|---|---|
| `document/tests/trace_and_generator.rs` (8): `applying_a_trace_creates_placed_derived_outputs_with_recipe_in_one_undo_step`, `traced_output_refuses_direct_transform_and_moves_with_set_placement`, `retrace_keeps_geometry_id_per_contour_key_and_unbinds_vanished_outputs`, `apply_patch_for_a_trace_is_checked_against_its_recipe_key`, `trace_with_a_non_finite_recipe_value_is_refused_and_changes_nothing`, `generator_without_an_evaluator_is_pending_and_produces_nothing`, `generator_outputs_follow_parameter_edits_in_the_same_transaction_with_stable_ids`, `failed_generator_evaluation_keeps_last_output_and_blocks_production` | stub slaat niets op | `cargo test -p burnburn-document --test trace_and_generator` → 0 passed, 8 failed (`trace stored`/`generator stored` expect-panics; NaN-recept niet geweigerd) | zelfde → 8 passed | — |
| `io/tests/engine_roundtrip.rs` (3): `trace_and_generator_survive_save_and_reopen_with_records_and_outputs`, `project_hash_covers_trace_recipe_and_generator_parameters`, `trace_whose_source_asset_is_missing_is_refused_on_load` | io weigert de nieuwe owners | `cargo test -p burnburn-io --test engine_roundtrip` → 0 passed, 3 failed (`Encode("no file form for derived owner Trace(TraceId(1))")`) | zelfde → 3 passed; daarna `cargo test -p burnburn-io -p burnburn-document` alles groen (v1/v2-fixtures en handgecodeerde hash ongewijzigd) | — |
| `trace/tests/document_recipe.rs` (3): `every_recipe_field_survives_the_engine_parameter_form_with_the_same_key`, `parameters_of_another_engine_or_with_a_bad_value_are_refused`, `a_trace_result_becomes_one_trace_command_keyed_by_contour_ids` | stub (`recipe_to_engine` leeg, `recipe_from_engine` = default, `trace_command` = `Err`) | `cargo test -p burnburn-trace --test document_recipe` → 0 passed, 3 failed (recept ≠ default-recept; andere engine niet geweigerd; `Err(NonFinite)`) | zelfde → 3 passed | — |
| `generators/tests/evaluator.rs` (4): `qr_generator_evaluates_to_placed_modules_in_the_same_transaction`, `invalid_generator_parameter_keeps_the_last_output_and_names_the_parameter`, `finger_box_panels_keep_their_ids_when_the_thickness_changes`, `unknown_generator_or_other_engine_version_is_an_invalid_definition` | stub-evaluator geeft altijd `Failed { InvalidDefinition }` | `cargo test -p burnburn-generators --test evaluator` → 1 passed, 3 failed. **Let op:** `unknown_generator_…` was al groen tegen de stub (die faalt altijd); hij bewijst pas iets samen met de drie andere | zelfde → 4 passed | — |

`trace` hangt nu af van `burnburn-document` (toegestaan in `layers.toml`). Nieuw: `burnburn_trace::{recipe_to_engine,
recipe_from_engine, trace_command}`; `burnburn_generators::{GeneratorEvaluator, GENERATOR_ENGINE}` (generatoren `qr`,
`code128`, `finger-box`; overige generatoren volgen door Generators/tekst).

## 5. QR-encoder (eigen, pure Rust; `burnburn-generators::qr`)

Bytemodus, ECC L/M/Q/H, versies 1–10, Reed–Solomon over GF(256) (0x11D), BCH-format/versie-informatie, maskers met
strafregels N1–N4. Oracles (niet de eigen encoder): gepubliceerde RS-vector "HELLO WORLD" 1-M (16 data → 10 EC-codewoorden),
gepubliceerde tabellen format-informatie (32) en versie-informatie (v7–v10), en tien volledige modulematrices van Apple
CoreImage (`CIQRCodeGenerator`), gegenereerd met het referentiehulpmiddel `research/qr-reference/reference.swift` (geen
runtime-afhankelijkheid) naar `crates/burnburn-generators/tests/fixtures/qr-coreimage.txt` (v1, v2, v3, v4, v6, v7, v10;
alle vier niveaus; multi-block met twee groepen; 16-bit lengteveld in v10).

| Test | Verwachte fout (red) | Red-commando en waargenomen resultaat | Green-commando en resultaat | Refactor |
|---|---|---|---|---|
| `generators/tests/qr.rs` (7): `reed_solomon_matches_the_published_hello_world_1m_vector`, `format_information_matches_the_published_table`, `version_information_matches_the_published_table`, `matrices_match_the_coreimage_reference_for_its_mask`, `automatic_mask_choice_agrees_with_the_reference_encoder`, `longest_input_per_level_fits_version_10_and_one_more_byte_is_refused`, `qr_geometry_is_one_square_per_dark_module_inside_the_quiet_zone` | stub (nullen, lege matrix) | `cargo test -p burnburn-generators --test qr` → 0 passed, 7 failed | eerste implementatie: 5 passed, 2 failed (v7-H-fixture: rij 0 verschilt; masker 6 i.p.v. 3). Diagnose met tijdelijke unit-test (codewoorden teruglezen uit de referentiematrix): CoreImage schakelde voor `"serienummer bb-2026-000123; …"` midden in naar een alfanumeriek segment (bytetelling 14, dan `-2026-000123`) — geldig maar geen bytemodus. Fixture vervangen door een tekst die CoreImage in bytemodus houdt; daarna alle 10 matrices codewoord- en modulegelijk → 7 passed | tijdelijke debugtest verwijderd |

Extra onafhankelijke controle (referentiehulpmiddel, geen cargo-test): alle tien fixture-matrices met Apple Vision
(`VNDetectBarcodesRequest`) gedecodeerd → `decoded 10/10` gelijk aan de bron-bytes. Omdat onze matrices (inclusief
automatische maskerkeuze) bitgelijk zijn aan deze, geldt dat ook voor onze uitvoer. Niet gedaan: leesproef van een
gelaserde code (fysiek) — **handmatig, onbevestigd**.

## 2. IM25-procesopties additief in schema 2 (`burnburn-io`)

`OperationProcess.advanced` (tabs, lead-in/out, overcut, perforatie, z-step, sub-operations, `shared_edges`; elk veld
weggelaten als het uit staat), `Mode::Fill.angle_deg` (weggelaten bij 0°), `Mode::OffsetFill`, `Mode::Image` (bron =
pixelhash in hex, render, toon, richting, serpentine, overscan). Laden via de domeinconstructors (`TabSpec::new`,
`ZStep::new`, `Operation::with_advanced`, …) met JSON-pad. Canonieke hash was al door CAM gedefinieerd
(`operations_without_new_options_keep_their_im13_canonical_layout`).

| Test | Verwachte fout (red) | Red-commando en waargenomen resultaat | Green-commando en resultaat | Refactor |
|---|---|---|---|---|
| `io/tests/process_options_roundtrip.rs`: `every_process_option_survives_save_and_reopen_with_the_same_hash`, `project_hash_changes_with_each_advanced_option`, `invalid_option_values_in_a_file_are_refused_with_their_path`, `operations_without_new_options_keep_their_file_form` | io weigert advanced/scanhoek/nieuwe modi | `cargo test -p burnburn-io --test process_options_roundtrip --test process_roundtrip` → 1 passed, 3 failed (`Encode("advanced process options have no file schema yet")`); `…_keep_their_file_form` was al groen (bewaakt alleen weglating) | na implementatie: 3 passed, 1 failed — **gevonden bug:** `"angle_deg": null` (zo schrijft serde_json een NaN) werd als "afwezig" = 0° gelezen; opgelost met `dto::v1::present_num` (expliciete `null` → NaN → geweigerd met pad) voor alle optionele getallen → 4 passed | — |
| `io/tests/process_roundtrip.rs::process_options_are_kept_not_dropped` (was CAM's `process_options_without_a_file_schema_are_refused_not_dropped`) | gedrag bewust gewijzigd: niet meer weigeren maar bewaren | (zelfde run, binary niet bereikt na de eerste falende) | `cargo test … --test process_roundtrip` → 4 passed | test herschreven naar roundtrip-gelijkheid |
| `io/tests/migration_fixtures.rs::v2_fixture_document_reencodes_byte_identically` (bewaking: v2-fixture opnieuw opslaan = byte-gelijke `document.json`) | — (bewakingstest) | eerste run groen | mutatieproef: `skip_serializing_if` op `advanced` weggehaald → test FAILED; teruggezet → 3 passed | — |

## 3. Sketch 'external entity' — niet in één nette stap: getest "not supported yet"

Motivatie en ontwerp voor later: `docs/coordination/sketch.md` (model, evaluator, inputs-hash met opgeloste externe
geometrie, tweede evaluatieronde na `evaluate_structure` + cyclusdetectie).

| Test | Verwachte fout (red) | Red-commando en waargenomen resultaat | Green-commando en resultaat | Refactor |
|---|---|---|---|---|
| `document/tests/sketch_external.rs::adding_an_external_entity_to_a_sketch_is_refused_as_not_supported_yet` | stub accepteerde `AddExternal` zonder effect | `cargo test -p burnburn-document --test sketch_external` → eerst compileerfout (variant `ExternalNotSupported` ontbrak; geen rode fase), na toevoegen van de variant: 0 passed, 1 failed (`Ok` met een **lege undo-stap**) | zelfde → 1 passed | — |

## 4. Arrays als modifier en nesting op een plaat

Refactor zonder gedragswijziging: `grid_array`/`circular_array` van `burnburn-cam` naar `burnburn_geometry::array`
(cam re-exporteert); bewaakt door de bestaande `cam/tests/nest.rs::grid_and_circular_arrays_generate_placements`
(`cargo test -p burnburn-cam --test nest` → 5 passed na de verhuizing).

| Test | Verwachte fout (red) | Red-commando en waargenomen resultaat | Green-commando en resultaat | Refactor |
|---|---|---|---|---|
| `document/tests/array_modifiers.rs` (3): `grid_array_modifier_outputs_every_copy_and_follows_source_edits`, `circular_array_modifier_turns_copies_about_the_centre`, `invalid_array_parameters_are_refused_and_change_nothing` | stub: array-stap faalt altijd (`InvalidValue`), geen validatie | `cargo test -p burnburn-document --test array_modifiers` → 0 passed, 3 failed | zelfde → 3 passed | `LinearArray` deelt nu dezelfde kopie-logica (`array_transforms`); `cargo test -p burnburn-document` groen (o.a. `modifiers.rs`) |
| `io/tests/array_modifier_roundtrip.rs::grid_and_circular_array_steps_survive_save_and_reopen` | **gevonden bug:** onbekende modifierstappen werden bij opslaan stil weggelaten | `cargo test -p burnburn-io --test array_modifier_roundtrip` → 0 passed, 1 failed (`steps: []` na heropenen) | zelfde → 1 passed; `structure_to_dto` geeft nu `SaveError::Encode` voor een stap zonder bestandsvorm | — |
| `document/tests/nest_on_plate.rs` (2): `applying_a_nest_moves_instances_onto_the_plate_in_one_undo_step`, `apply_nest_refuses_unknown_or_duplicate_instances_and_unknown_plates` | stub `ApplyNest` → `NothingChanged` | `cargo test -p burnburn-document --test nest_on_plate` → 0 passed, 2 failed | zelfde → 2 passed. Daarna extra eis (geen lege undo-stap bij identieke nest) eerst als assertie: 1 failed (`Ok` met alleen `Plate`-change), dan opgelost → 2 passed | — |
| `cam/tests/nest_apply.rs::nest_result_becomes_one_apply_nest_command_for_its_stock` | stub `apply_nest_command` → `Err(InvalidInput { "stub" })` | `cargo test -p burnburn-cam --test nest_apply` → 0 passed, 1 failed | zelfde → 1 passed | — |

Niet gebouwd (contract IM26): stale-controle van een nest via een inputs-hash (`Delivery::Stale`); nu alleen de verwachte
revisie van het command.

## 6. Clippy en fmt

- `cargo fmt --all` twee keer gedraaid tijdens het werk. **Let op:** dat kan bestanden van Integratie-productie in
  `burnburn-presentation` die op dat moment niet geformatteerd waren mee geformatteerd hebben (alleen opmaak). Daarna
  alleen per pakket: `cargo fmt -p burnburn-{types,geometry,document,io,sketch,generators,cam,trace,scenarios} -- --check`
  → 0 afwijkingen. `cargo fmt --all -- --check` meldt nu alleen `burnburn-presentation` (lopend werk van de collega).
- `cargo clippy -p burnburn-{types,geometry,document,io,sketch,generators,cam,trace,scenarios} --all-targets --no-deps
  -- -D warnings` (zonder `-A`): één bevinding in mijn nieuwe code (`chunks_exact_to_as_chunks` in `trace/src/document.rs`)
  opgelost → schoon. Ter informatie ook schoon: text, image, job, ruida, machine, history, editor, test-support.

## Eindverificatie (18-10-2026)

- `cargo test -p burnburn-{document,io,sketch,generators,cam,trace,scenarios,geometry}` → 97 testbinaries `ok`, 0 failed
  (ignored: bestaande `#[ignore]`s, o.a. trace-holdout).
- `cargo xtask layers` → `24 crates, 2 triples, no violations`.
- `cargo check --workspace --all-targets` → groen bij mijn runs (hosts compileren). `cargo test -p burnburn-presentation`
  compileerde bij twee runs niet door lopend werk in presentation zelf (eerst `crate::camera`/`OperationId`/`UiIntent`,
  later `crate::field::parse_number`) — niet door deze wijzigingen.

## Onbevestigd / handmatig
- Alleen macOS arm64. QR: geen leesproef van een fysiek gelaserde code. Trace-/generatoruitvoer is niet in de GUI bekeken.
