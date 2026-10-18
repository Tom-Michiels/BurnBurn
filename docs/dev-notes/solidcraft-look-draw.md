# Lane S-draw — Maken-tools (18-10-2026)

Plan: `docs/plan/solidcraft-look.md`. Commando's: `export PATH="$HOME/.cargo/bin:$PATH"; CARGO_TARGET_DIR=target/draw`.
Gedrag nagebouwd uit SolidCraft `ui-egui/src/sketch_tools.rs` (klikvolgorde per tool); BurnBurn-API gebruikt.

## Wat er is

- `burnburn-geometry/src/construct.rs` (zuivere functies, typed `ConstructError`): `circle_through_three`,
  `circle_tangent_two_lines` (middelpunt = projectie van de aanwijzer op de bissectrice van de sector waarin hij ligt;
  evenwijdig: de strook), `circle_tangent_two_lines_radius`, `circle_tangent_three_lines` (in-/aanschrijvende cirkel het
  dichtst bij de aanwijzer), `arc_through_three`, `arc_center_start_end`, `arc_tangent`, `rectangle_three_point`,
  `rounded_rectangle` (r > halve korte zijde → `RadiusTooLarge`), `ellipse_from_axes`, `polygon_{inscribed,circumscribed,edge}`,
  `slot_{center_to_center,overall}`, `slot_along_arc`. Transcendentalen via `math` (libm, determinism-test blijft groen).
- `burnburn-editor/src/tool/`: `click.rs` (gedeelde punt-voor-punt-engine `DrawTool`: slepen of klik-klik, preview als
  `ToolPreview::Shape`, één `EditCommit` bij het laatste punt, Esc/Cancel/toolwissel → `Cancelled`), familiebestanden
  `line, rect, circle, arc, ellipse, polygon, slot, point`; `TangentCircleTool` (circle.rs) en `TangentArcTool` (arc.rs)
  pikken segmenten/uiteinden uit het document. `ToolKind` +22 varianten; `ToolParam`/`Editor::set_param`;
  `Tool::is_active`; `EditCommit::{CreateConstruction, CreatePoint}`.
- Bewuste keuzes: afgeronde rechthoek klemt de straal vast (geometry geeft de typed error); mislukte laatste klik (collineair,
  geen oppervlak) → `Cancelled`, geen halfobject; center-point-boog en bogen-sleuf lopen linksom (omgekeerde klikvolgorde =
  rechtsom bij 3-punts); raakcirkels/-boog werken op rechte segmenten resp. vrije uiteinden, niet op gebogen doelen.

## Rode/groene fase (echt uitgevoerd)

| Onderdeel | Rood | Groen |
|---|---|---|
| geometry `tests/construct.rs` (31 tests) | tegen stubs die overal `Err(NoSolution)` gaven: `cargo test -p burnburn-geometry --test construct` → **30 failed, 1 passed** (bv. `left: Err(NoSolution) right: Err(Coincident)`) | na implementatie 28/30; twee waren fouten in mijn *test* (middenfractie bij de lange boog; boogsleufoppervlak 6π i.p.v. 11π) — test gecorrigeerd met handberekening, daarna **30 passed** |
| editor (`click.rs`, `line/rect/circle/arc/ellipse/polygon/slot/point.rs`, 48 nieuwe tests) | engine echt, bouwfuncties/tangenttools stubs (`None`/`Nothing`): `cargo test -p burnburn-editor --lib` → **48 failed, 74 passed** | **121 passed, 1 failed** → `is_gesture_active` was afgeleid van de preview (valse negatieve na een eerste klik zonder tekenbare preview): `Tool::is_active` toegevoegd → **122 passed** |

Tests: o.a. `three_point_circle_passes_through_all_clicks` (middelpunt (5,5), r=√50), `two_tangent_circle_touches_both_picked_lines_and_follows_the_pointer`,
`tangent_arc_continues_the_end_of_a_picked_line`, `rounded_rectangle_radius_is_clamped_to_half_the_short_side`,
`every_tool_commits_exactly_once_on_its_last_point_and_returns_to_idle`, `cancelling_mid_gesture_commits_nothing_and_leaves_no_preview`,
`first_points_snap_to_object_targets_unless_suppressed`, geometry: `rounded_rectangle_rejects_oversized_or_non_positive_radius`.

## Niet gedaan / open

- Geen documentcommando voor construction-geometrie en punten: `CreateConstruction`/`CreatePoint` worden door presentation
  nog niet toegepast (zie `docs/coordination/userflow.md`).
- Geen handmatige GUI-controle; alles headless. Raakcirkel/-boog op bogen en cirkels als doel ontbreekt.
