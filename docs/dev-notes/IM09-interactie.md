# IM09 Direct tekenen en repareren — interactiedeel (User-flow)

Status: interactie headless groen op macOS arm64 (18-10-2026). Domeinkant (geometrie, `GeometryCommand`) is van
Kern (`docs/dev-notes/IM09.md`); dit log dekt `burnburn-editor` en de presentatieroute. Windows: onbevestigd.
Commando's met `export PATH="$HOME/.cargo/bin:$PATH"; CARGO_TARGET_DIR=target/userflow`.

## Werkwijze en eerlijke kanttekeningen

- Editor: alle nieuwe tests zijn eerst geschreven tegen stubs (gereedschap `input → Nothing`, `snap_point → None`,
  `hit`/`objects_inside → []`, `build_scene → leeg`). Eén rode run: `cargo test -p burnburn-editor` →
  **49 failed, 14 passed** (o.a. `left: Err(Empty)`, `left: None right: Some(…)`, `Select(Clear)`-asserts). Daarna per
  module groen gemaakt. Tests die tegen de stub al slaagden staan hieronder als "geen rode fase".
- Halverwege verscheen Kerns IM09-domein (`snap_candidates`, `hit_test`+`SpatialIndex`, `Path::edit_nodes`, `Shape`,
  `self_intersections`, `boolean`, `GeometryCommand`). Eigen afleidingen (eindpunt/midden/centrum, bbox-hit,
  knoopverplaatsing, uitlijnwiskunde) zijn toen vervangen door die domeinfuncties; de editortests bleven de oracle
  (refactor). Waar de domeinsemantiek afwijkt is de test bewust aangepast (zie "Aangepast").
- Presentation: de controllerroute is deels vóór de scenario's geschreven. Dat is gecompenseerd met mutaties (tabel
  "Mutaties"); dat vervangt geen test-first volgorde.

## Cycli — burnburn-editor

| Test | Verwachte fout (red) | Red-commando en waargenomen resultaat | Green-commando en resultaat | Refactor |
|---|---|---|---|---|
| `view::tests::world_maps_to_device_pixels_at_retina_scale`, `pan_moves_content_with_pointer`, `zoom_keeps_anchor_world_point_fixed` (uit render, DR-02) | stubs `[0,0]` / no-op | `cargo test -p burnburn-editor` → FAILED (view.rs:160/174/187) | `cargo test -p burnburn-editor view::` → 6 passed | centrum in afgeronde device-pixels gedeeld door projectie en pointer |
| `selection::tests::rubber_band_selects_only_objects_fully_inside` | stub `[]` | zelfde run → FAILED "object 3 sticks out" | `cargo test -p burnburn-editor -- snap:: selection::` → 10 passed | — |
| `selection::tests::alt_click_cycles_through_overlapping_candidates_and_wraps` | stub `None` | zelfde run → FAILED "nothing selected: topmost first" | idem | — |
| `snap::tests::{snap_names_endpoint_midpoint_and_centre, snap_radius_is_in_screen_points_not_millimetres, grid_applies_only_when_no_object_target_is_near}` | stub `None` | zelfde run → FAILED | idem | eigen doelen → `burnburn_geometry::snap_candidates` |
| `snap::tests::nearest_target_wins` → hernoemd `named_targets_beat_the_nearest_outline_point` | stub | zelfde run → FAILED | groen na domeinadapter met aangepaste verwachting | zie "Aangepast" |
| `snap::tests::{primary_modifier_suppresses_snapping, excluded_objects_offer_no_targets}` | — | **geen rode fase** (slaagden tegen stub `None`) | idem | — |
| `issues::tests::nearly_closed_contour_is_located_with_its_gap` | stub `[]` | zelfde run → FAILED `left: 0 right: 2` | `cargo test -p burnburn-editor -- arrange:: issues::` → ok | — |
| `issues::tests::self_intersection_is_located_at_the_crossing` | stub `[]` | `cargo test -p burnburn-editor -- issues::` → FAILED `left: 0 right: 1` | `Path::self_intersections` → 66 passed | — |
| `arrange::tests::booleans_need_two_closed_objects` | stub `Ok([])` | eerste run → FAILED | ok | align/distribute-tests verwijderd: wiskunde is domein (`GeometryCommand::Align/Distribute`), gedekt door scenario's |
| `tool::rectangle::tests::{rectangle_corners_snap_to_endpoints_unless_suppressed, hover_reports_snap_candidate_without_preview}` | snap-stub | eerste run → FAILED (rectangle.rs:212/229) | `cargo test -p burnburn-editor -- rectangle::` → 7 passed | — |
| `tool::rectangle::tests::typed_size_while_dragging_commits_exact_rectangle` (E01) | `exact_size → Nothing` | eerste run → FAILED (unwrap op `None`) | idem | — |
| `tool::rectangle::tests::{rectangle_drag_…, rectangle_cancel_…, release_without_area…, shift_…_alt_…}` | — | **geen rode fase**: gemigreerd van IM05 (zelfde gedrag, nieuwe API `EditCommit`) | idem | — |
| `tool::draw::tests::*` (12: ellips, cirkel, lijn, polyline, pen) | stubs `Nothing` | eerste run → 12 FAILED | `cargo test -p burnburn-editor -- draw::` → 12 passed | eigen ellips-pad → domein `Shape` + `EditCommit::CreateShape` |
| `tool::node::tests::{path_nodes_lists_…, dragging_one_handle_changes_only_that_span, dragging_an_anchor_…, escape_…, press_away_…}` | stubs | eerste run → 5 FAILED | `cargo test -p burnburn-editor -- node::` → 6 passed | eigen `move_node` → `Path::edit_nodes(&NodeEdit)`; commit `EditNodes { edit, path }` |
| `tool::node::tests::release_without_movement_commits_nothing` | — | **geen rode fase** (stub gaf al `Nothing`) | idem | — |
| `tool::select::tests::*` (13: cyclus, rubberband, verplaatsen, schalen, roteren, handles, snap, threshold, cancel) | stubs | eerste run → 12 FAILED, `click_tolerance_…` ok | `cargo test -p burnburn-editor -- select::` → eerst 3 FAILED (release gebruikte laatste Move i.p.v. Up-positie; Shift-druk togglede i.p.v. slepen), na fix 12 passed | `release(ev)` loopt eerst `drag_to`; `toggle_on_click` |
| `tool::select::tests::shift_click_on_a_selected_object_toggles_it_off` | — | **geen test-first rode fase**; mutatie `toggle_on_click … && false` → FAILED `left: Nothing right: Select(Toggle(..))`; teruggezet → ok | idem | — |
| `scene::tests::*` (5) | stub leeg | eerste run → 5 FAILED | `cargo test -p burnburn-editor` → 63 passed | — |
| `selection::tests::hits_are_outline_accurate_and_inner_shapes_come_first` | — | **geen rode fase** (na domeinadapter geschreven); mutatie: oude bbox-hit → FAILED; teruggezet → ok | 65 passed | `Reverse(GeometryId)` als sleutel: bij gelijke kandidaten wint het bovenste object |

### Aangepast aan de domeinsemantiek
- Snapvolgorde: domein sorteert eerst op soort (eindpunt > snijpunt > midden > centrum > nearest), dan afstand. De
  eerdere "dichtstbijzijnde wint"-test is vervangen door `named_targets_beat_the_nearest_outline_point`.
- Hit-test: domein geeft randtreffers op afstand, dan interieurs van klein naar groot. Gate-A/selectietests bleven groen.

## Cycli — presentatieroute (scenario's `crates/burnburn-presentation/tests/im09_interaction.rs`)

| Test | Verwachte fout (red) | Red-commando en waargenomen resultaat | Green-commando en resultaat | Refactor |
|---|---|---|---|---|
| `alt_click_cycles_overlapping_objects_and_reports_the_position` | — (route al geschreven) | eerste run `cargo test -p burnburn-presentation --test im09_interaction` → **FAILED** `left: Some(SelectionCycle{3,3}) right: None`: **bug** — klik op al geselecteerd object liet "3 van 3" staan | elke niet-Alt-druk wist de cyclus → groen | — |
| `boolean_preview_is_revision_checked_and_never_changes_the_document` | — | eerste run → FAILED: **testfout** (veldcommit met 2 geselecteerd stuurt niets) | edit vervangen door `AlignLeft` → groen | — |
| `issues_overlay_marks_open_contour_and_self_intersection` | — | eerste run → FAILED: fixture-eind lag óp het eerste segment (domein meldt terecht een raakpunt) | fixture-eind op (0, 0.08) → groen | — |
| `ellipse_circle_line_polyline_and_pen_each_create_one_object_with_one_undo`, `node_edit_is_one_undo_entry_and_other_spans_stay_identical`, `align_and_distribute_of_many_objects_is_one_entry`, `applying_a_boolean_is_one_undo_entry` | eerst `#[ignore = "wacht op IM09 (Kern)"]` | `cargo test … --test im09_interaction -- --ignored` → 4 FAILED (`left: 0 right: 5`, `left: 2 right: 1`, `left: 4 right: 5`, node) | `GeometryCommand::{CreateShape, CreatePath, EditNodes, Align, Distribute, Boolean}` gekoppeld, ignore weg → 19 passed | tijdelijke "niet beschikbaar"-route + 3 bijbehorende tests verwijderd |
| overige 14 scenario's (DR-04 camera, scene, boom↔canvas, verplaatsen/Esc, schalen/roteren, snap+Cmd, raster, X/Y, getypte maat, uitlijnen, enablement, …) | — | **geen test-first rode fase** (route eerst geschreven) | 19 passed | — |

### Mutaties (presentatie)
| Mutatie | Test | Resultaat |
|---|---|---|
| pointer zonder camera (`position_pt` als mm) | `pointer_points_are_mapped_through_the_window_camera` | FAILED; teruggezet → ok |
| Cmd onderdrukt snap niet (`snapped(…, false, …)`) | `snap_names_endpoint_and_primary_modifier_suppresses` | FAILED; teruggezet → ok |
| Boolean niet revisiegevoelig | `boolean_preview_is_revision_checked…` | **bleef groen**: tweede wacht (viewmodel filtert op revisie). Beide wachten weg → FAILED. Dubbele verdediging, elk afzonderlijk niet detecteerbaar. |
| Difference-fold als Union (`task::boolean_of`) | `task::tests::boolean_task_folds_the_domain_boolean_over_the_operands` | FAILED; teruggezet → ok (unit-test zelf zonder test-first rode fase) |

## Poort-B-scenario's (`crates/burnburn-presentation/tests/gate_b.rs`)

| Test | Verwachte fout (red) | Red-commando en waargenomen resultaat | Green-commando en resultaat | Refactor |
|---|---|---|---|---|
| `headless_gate_b_mounting_plate_width_change` | stub `SetSketchParameter → niets` | `cargo test -p burnburn-presentation --test gate_b` → FAILED `left: None right: Some(Sketch(SetParameter))` | alle commands via `apply_with(&[SketchEvaluator])` → ok | — |
| `cam_flow::headless_gate_b_same_revision_preview_and_rd` | stubs (machine/operation/export doen niets) | zelfde → FAILED gate_b.rs:195 (geen plan) | `TaskInput::{Compile, ExportRd}`, `plan_view` → 3 passed | — |
| `cam_flow::compile_result_for_an_older_revision_is_dropped` | — | **geen rode fase** (slaagde tegen stub); mutatie Compile niet revisiegevoelig → FAILED; teruggezet → ok | ok | — |
| `trace_flow::headless_gate_b_ring_trace_keeps_hole`, `trace_result_for_an_older_recipe_is_dropped` | stubs | zelfde → 2 FAILED (gate_b.rs:287/332, `tracer` is `None`) | `TaskInput::{LoadImage, Trace}`, `check_fresh`, `Import` → 6 passed | — |
| `trace_flow::closing_the_tracer_changes_nothing` | — | **geen rode fase**; mutatie annuleren weg → FAILED; teruggezet → ok | ok | — |
| `trace_flow` mutatie freshness-check weg | `trace_result_for_an_older_recipe_is_dropped` | FAILED; teruggezet → ok | — | — |

## Verificatie (eindstand)
- `cargo fmt -p burnburn-editor -p burnburn-presentation --check` → ok.
- `cargo clippy -p burnburn-editor -p burnburn-presentation --all-targets -- -D warnings` → ok.
- `cargo test -p burnburn-editor -p burnburn-presentation` → editor 63; presentation 7 unit + 6 gate_b + 19 im09 + 12
  scenario's; 0 ignored, alle groen.
- `cargo xtask layers` → "18 crates, 2 triples, no violations".

## Onbevestigd / open
- Windows: geen runner. Native: hover/drag/Alt-klik/Return/Delete-vertaling in de hosts is GUI (tests/native).
- Raakpunt-snap (tangent) levert het domein nog niet. Knoopsoort (hoek/glad/symmetrisch) is per drag gekozen
  (geen/Alt/Shift); een opgeslagen knoopsoort per node bestaat nog niet.
- Trace: toepassen gebeurt als `GeometryCommand::Import` (bron-asset als herkomst, één entry). Recept + engineversie
  worden nog niet in het document bewaard (contract §11 `Trace`-owner/`ApplyPatch` — gevraagd aan Kern/Trace).
- Sketch: schetsen maken met tekengereedschap en relatievoorstellen (S01) is niet gebouwd; het scenario opent een
  door het domein opgebouwde plaat en wijzigt de parameter via het parameterpaneel.
- Foutlocaties worden synchroon per revisie herberekend (`geometry_issues`); voor grote documenten hoort dit een taak
  te worden (B5 `GeometryCheck`).
