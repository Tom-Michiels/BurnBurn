# LF2 - Canvas: anti-aliasing, donker thema, bed en raster, geen grijze vulling

Plan: `docs/plan/look-and-feel.md` (canvasbevindingen 2, 3, 6). Lane LF2-canvas, golf LF-A.

## Wat er gebouwd is

- **MSAA 4x** (`burnburn-render/src/gpu.rs`): `CanvasRenderer::new(device, format, samples)`; `GpuContext::sample_count(format)`
  geeft 4 als de adapter het kan resolven, anders 1 met een melding op stderr (terugval). Beide pipelines (vlakken/lijnen en
  beelden) en `render_offscreen` plus `SurfaceCanvas` gebruiken het; het MSAA-doel wordt per formaat bewaard en
  opgelost in de drawable/offscreen-textuur.
- **Thema** (`render/src/scene.rs`): `Theme::light()` herzien (grijs canvas, wit bed, geen vulling, hover oranje, preview paars,
  selectie-accent) en nieuw `Theme::dark()`. Nieuwe velden `grid_major`, `bed_fill`, `bed_border`, `origin`. Hulpfuncties
  `contrast_ratio` (WCAG) en `delta_e` (CIE76), plus `lift_to_contrast` voor het donkere laagpalet.
- **Bed, oorsprong, raster** (`editor/src/scene.rs`, `render/src/scene.rs`): `Overlay::Bed {width_mm, height_mm, origin: BedOrigin}`,
  `Overlay::Grid {spacing_mm, major_every}` (`grid_major_every`: majors op decaden: 1 -> 10, 2 -> 5, 5 -> 2). Het raster wordt
  binnen het bed geclipt, het bed is een vlak met rand, de oorsprong een L van 24 pt op de juiste hoek.
- **Machineprofiel -> bed**: `presentation/src/controller/view.rs` voegt `Overlay::Bed` (maat + `OriginCorner`) vooraan in de scene
  in als er een machine is (buiten mijn lane, klein en additief; zonder machine geen bed).
- **Geen grijze vulling**: ontwerp/laag/hover/uitgeschakeld hebben `fill: None`; alleen `Selected` heeft een 8% (licht) / 12% (donker)
  accenttint als selectiehint.
- **macOS** (`appkit/canvas.rs`, `classes.rs`, `app.rs`): `canvas_theme`/`canvas_packet_themed(scene, wireframe, tol, dark)`;
  `is_dark(view)` uit `effectiveAppearance`; `CanvasView.viewDidChangeEffectiveAppearance` tekent opnieuw; `redraw` kiest het
  thema. `tests/native/macos/gui_snapshots.rs` rendert het canvas nu in het gekozen thema (BURNBURN_GUI_SNAPSHOT_THEME).

## Red/green (TDD, alle commando's met `CARGO_TARGET_DIR=.../target/lf2 CARGO_INCREMENTAL=0`)

Rood (met minimale stubs: `Theme::dark()` = light, `contrast_ratio`=1, `delta_e`=0, MSAA 1, Bed genegeerd), commando
`cargo test -p burnburn-render --test lf2_canvas --locked`: `1 passed; 9 failed`, o.a.
`slanted_line_has_intermediate_tones_on_its_edge` ("only 0 intermediate-tone pixels"),
`selection_hover_and_preview_differ_by_at_least_20_delta_e` ("light: selection vs hover only 0.0"),
`grid_lines_stay_inside_the_bed_...` ("grid point 0.5,0 outside the bed"),
`a_closed_shape_gets_no_fill_unless_it_means_something` ("Design must not be filled", left 1 right 0).
Editor: `major_grid_lines_fall_on_decades_for_every_1_2_5_spacing` (stub gaf altijd 10, rood vooraf niet apart vastgelegd: de
stub-assert voor spacing 2.0 verwachtte 5). Presentation: `the_scene_has_the_bed_of_the_machine_profile_beneath_the_grid`
rood (left `[]`), daarna groen na de view.rs-wijziging (rood opnieuw bevestigd met de wijziging teruggedraaid).
macOS: `canvas_theme` (2 tests) rood: "left 0.88.. right 0.09..".

Groen: `lf2_canvas` 10/10, `burnburn-render` en `burnburn-editor` volledig groen, `bed_overlay` 2/2, macos `canvas_theme` 2/2
en `wireframe` 1/1.

Tests (render/tests/lf2_canvas.rs): `slanted_line_has_intermediate_tones_on_its_edge`, `pixel_aligned_rectangle_stays_crisp_with_multisampling`,
`contrast_and_delta_e_follow_their_definitions` (analytisch: 21:1, 4.48:1, dE zwart-wit 100), `dark_theme_is_dark_and_its_ink_has_high_contrast`,
`lines_marks_and_layer_colours_are_readable_against_the_bed_in_both_themes` (inkt >= 7:1, selectie/hover/oorsprong/preview >= 3:1,
donkere laagkleuren >= 3:1), `selection_hover_and_preview_differ_by_at_least_20_delta_e`, `grid_lines_stay_inside_the_bed_with_majors_apart_from_minors`,
`the_origin_mark_sits_on_the_right_bed_corner` (4 hoeken), `rendered_bed_has_background_outside_and_bed_colour_inside` (GPU-pixels, geen rasterpixel buiten het bed),
`a_closed_shape_gets_no_fill_unless_it_means_something`.

## Herziene bestaande exact-pixeltests

`bed_image.rs`, `image_preview.rs` (2 tests) en `offscreen.rs::scene_snapshot_renders_offscreen_to_exact_pixels` verwachtten een
wit canvas en/of een gevuld object. Het lichte thema heeft nu een grijs canvas en geen vulling; die tests zetten expliciet
`theme.background = wit` en (waar nodig) `design.fill = grijs`. De aanspraken (pixelpositie, volgorde, opacity) zijn onveranderd.
`scene_projection.rs`: `Overlay::Grid` krijgt `major_every: 0` (geen majors).

## Voor/na

Map `scratchpad/lf-lf2`: `voor/light-nl-03-selection-naamloos.png` (grijze vulling, geen bed, trappetjes) tegen
`na/light-nl-03-selection-naamloos.png` en `na-dark/dark-nl-03-selection-naamloos.png` (bed met oorsprongshoek, hoofd/hulp-raster,
selectietint, donker canvas). Gemaakt met `gui_snapshots` offscreen. Een screenshot is geen test.

## Bewust anders / open

- "Fill-bewerking" als vullingsreden: de scene kent de bewerkingsmodus per object niet; daarvoor is een `SceneItem`-veld (fill-modus) nodig
  van presentation (aanvraag voor LF-C). Nu alleen het selectiehint.
- Handgrepen (7 pt, draaigreep met steel), snap-markers, maatlabelfont en linialen zijn LF-C, niet hier.
- De standaardcamera kadert het bed niet (bed 1500x900 mm staat deels buiten beeld); een "zoom naar bed"-actie hoort bij LF-C.
- Windows-host: gebruikt `Theme::light()`; het kiezen van `Theme::dark()` naar Windows-appearance staat open (geen Windows-runner).
- Natieve detectie van de appearance (`is_dark`, `viewDidChangeEffectiveAppearance`) is alleen via de donkere offscreen-snapshots gezien: handmatig.
