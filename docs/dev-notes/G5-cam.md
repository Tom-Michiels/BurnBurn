# G5-cam (golf 5): CAM-stories C01-C12, compile-schaal, tool-lagen, pad-arrays, feeder

Basis: commit be65aea. Alle commando's met `export PATH="$HOME/.cargo/bin:$PATH"` en
`CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/g5-cam`, toolchain 1.99.1, `--locked`. Alleen werkelijk
uitgevoerde commando's en hun werkelijke uitkomst staan hieronder; wat niet gedraaid is staat onder "Open".

## TDD-log

| Test | Red (verwachte fout, echt gezien) | Red-commando | Green |
|---|---|---|---|
| `cam line::tests::choosing_the_next_contour_examines_a_few_contours_not_all_of_them` (kostenteller: `PlanStats::entry_evaluations`) | stub `plan_counted` = oude lineaire scan met teller: "examined 2001000 entries for 2000 contours" | `cargo test -p burnburn-cam --lib --locked line::tests` -> 1 FAILED | zelfde commando na `nearest.rs` + `Chooser`: ok |
| `cam line::tests::containment_and_hole_depth_examine_neighbours_not_every_pair` (teller `containment_counted`) | "examined 4000000 pairs for 2000 contours" | idem -> 2 FAILED | ok na `SpatialIndex`-query per buitencontour |
| `cam line::tests::the_indexed_planner_gives_exactly_the_steps_of_the_original` (proptest, 400 gevallen, orakel = de oude `plan`/`containment`/`hole_depth` letterlijk in `mod reference`), `containment_and_hole_depth_equal_the_original`, `a_cancelled_plan_returns_cancelled` | karakteriseringstests: direct groen (oude code is het orakel); blijven groen na de omschrijving | idem | ok |
| `cam nearest::tests::the_cheapest_live_item_is_the_first_minimum_of_a_scan` (proptest tegen een scan in indexvolgorde, met aan/uit-zetten), `an_empty_or_fully_switched_off_index_finds_nothing` | tegelijk met de implementatie geschreven (nieuwe module) | `cargo test -p burnburn-cam --lib --locked` | ok |
| `geometry tests/path_array.rs` (7 tests: gelijke spatiëring open pad met beide uiteinden, 1 kopie, pad niet in de oorsprong, oriëntatie langs een knik, gesloten pad deelt door aantal, cirkel op gelijke hoeken, geweigerde invoer) | stub gaf `count` maal identiteit: 6 van 7 FAILED | `cargo test -p burnburn-geometry --locked --test path_array` | 7 ok |
| `document tests/path_array_modifier.rs` (4 tests: volgt bron én pad, georiënteerd, geweigerd bij ontbrekend pad/aantal 0/punt als pad, undo) | `Modifier::PathArray` evalueerde als niets: 3 van 4 FAILED | `cargo test -p burnburn-document --locked --test path_array_modifier` | 4 ok |
| `io tests/path_array_roundtrip.rs::a_path_array_step_survives_save_and_reopen` | `Encode("no file form for modifier PathArray {…}")` | `cargo test -p burnburn-io --locked --test path_array_roundtrip` | ok na DTO `ModifierKind::PathArray` |
| `document tests/tool_layers.rs` (6 tests: referentie+nooit uitvoer, uitvoer aanzetten geweigerd, geen snij-opties, undo in één stap, hash, rol in de hash) | `AddToolLayer` maakte een gewone laag: 3 FAILED; `the_role_itself_is_in_the_hash_not_just_the_name` apart rood gezien door de canonieke tag tijdelijk weg te halen | `cargo test -p burnburn-document --locked --test tool_layers` | 6 ok |
| `io tests/tool_layer_roundtrip.rs` | lezen gaf `(None, false)` i.p.v. `(Some(T1), false)` (stub `tool: None`) | `cargo test -p burnburn-io --locked --test tool_layer_roundtrip` | ok |
| `cam tests/tool_layers.rs::a_tool_layer_adds_nothing_to_the_plan_and_is_not_reported_as_forgotten` | preflight meldde `UnassignedGeometry` voor de hulplijn | `cargo test -p burnburn-cam --locked --test tool_layers` | ok |
| `job tests/feeder.rs` (3 tests: Feed/Pause gecontroleerd en verplaatsen de kop niet, inhoud van het plan (id) maar niet de grenzen, tijdschatting) | stub nam alles aan en telde niets: 3 FAILED | `cargo test -p burnburn-job --locked --test feeder` | 3 ok |
| `cam tests/feeder.rs` (7 tests: herhalen/voor/na/pauze analytisch, schatting, neutrale instelling = zelfde plan-id, 1 herhaling zonder pauze, bereiken, te groot plan, annulering) | stub gaf het plan terug: 6 van 7 FAILED | `cargo test -p burnburn-cam --locked --test feeder` | 7 ok |
| `scenarios tests/g5_cam_feeder.rs::the_ruida_backend_refuses_every_feeder_plan_by_name` | `Unsupported { what: "event" }` i.p.v. `"feeder"` | `cargo test -p burnburn-scenarios --locked --test g5_cam_feeder` | ok |
| `presentation tests/g5_cam_array.rs` (7 tests) | `cam_dialog(Array)` bestond niet: 7 FAILED (stub gaf `None`) | `cargo test -p burnburn-presentation --locked --test g5_cam_array` | 7 ok |

## Compile-schaal (X04 / W4-prestaties)

`line::plan` kiest het volgende contour via `nearest::NearestIndex` (boom van dozen rond de mogelijke instappunten,
met per knoop het aantal levende contouren); een contour wordt levend als alle contouren erin gesneden zijn.
Dezelfde keuze als de oude scan: laagste kosten, bij gelijke kosten de laagste invoerindex; zonder
reistijdverkorting de eerste klare in invoervolgorde; bij een (degenerate) cyclus een lineaire terugval.
`containment` en `hole_depth` gebruiken `geometry::SpatialIndex`.

Meting (release, `BB_COMPILE_ALL=1 cargo test -p burnburn-scenarios --release --locked --test benchmarks_100k -- --ignored --nocapture --test-threads=1`,
dezelfde machine, load average 114 tijdens de meting):

| Fixture | Voor (W4-prestaties) | Na |
|---|---|---|
| F100K-A (1 000 x 100-hoek) | 136 ms | 25,1 ms |
| F100K-B (10 000 x 10-hoek) | 3 985 ms | 55,1 ms |
| F100K-C (100 000 losse lijnen) | ~92 000 ms | 1 450,8 ms |

## Flaky nest-annulering (coordinator-opdracht)

Oorzaak: `cancel_midrun::a_nest_cancelled_while_running_…` annuleert via een tweede thread na 15 ms. Raakt de annulering
binnen `geometry::offset`/`boolean` van de nest (spacing-offset, keep-out, randmarge), dan werd `OffsetError::Cancelled`
in `nest.rs` omgezet in `NestError::Geometry { what: "spacing offset" }`: een echte productiefout (annulering gemeld als
geometriefout), die alleen onder belasting zichtbaar werd. De timing-test zelf blijft (hij meet "echt midden in de run"), maar
kan nu niet meer op de verkeerde fout falen.

| Test | Red | Red-commando | Green |
|---|---|---|---|
| `cam nest::cancel_tests::{a_cancel_during_the_spacing_offset_is_cancelled_not_a_geometry_error, a_cancel_during_keep_out_and_edge_margin_is_cancelled_too}` (deterministisch: token is vóór de stap geannuleerd) | beide FAILED: `Err(Geometry { what: … })` i.p.v. `Cancelled` | `cargo test -p burnburn-cam --lib --locked cancel_tests` | 12 ok na `offset_error()` en `BooleanError::Cancelled`-arm |
