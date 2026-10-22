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

## Bedienbaar in het Mac-venster (presentation, macOS-panelen, Windows-menu)

Alle nieuwe dialogen gebruiken het generieke CAM-dialoogkader (`DialogViewModel`): één formulier per dialoog, door macOS
(`appkit/panels/cam_dialog.rs`) en Windows (`forms/cam.rs`) uit dezelfde rijen gebouwd. Additief in dat kader:
`DialogRow.labels` (pop-ups met inhoud uit het document), `DialogViewModel.lines`, `DialogLabel`, `FindingKind`, de
actieknoppen `ShowSource/AddRow/RemoveRow/MoveRowUp/MoveRowDown`.

| Feature | Plaats | Tests (rood gezien tenzij anders vermeld) |
|---|---|---|
| Reeks (AR02): raster, cirkel, langs pad, voorbeeld als ghosts op het canvas, Apply = één modifier = één undo | Schikken ▸ Reeks… | `presentation g5_cam_array` (7), `macos g5_cam_panels`, native `g5_cam_dialogs::array_dialog_previews_and_makes_one_modifier` |
| Feeder (FE01): herhalen, voeden voor/na, pauze; compile-taak past toe (`TaskInput::Fed`), plan wordt verouderd bij andere instelling, RD weigert `RdRefusal::Feeder` | Laser ▸ Feeder… | `presentation g5_cam_feeder` (5) |
| Preflight-paneel (C09): lijst fouten eerst, "Toon bron" selecteert + zoomt, verouderd plan als eerste regel | Laser ▸ Preflight… | `presentation g5_cam_preflight` (5; rood: stub gaf geen dialoog, cargo stopte bij de eerste falende test-binary, de preflight-binary is dus pas groen gezien), native `preflight_dialog_lists_findings_and_selects_the_cause` |
| Snijvolgorde (C06): "laag A volledig vóór laag B", kring geweigerd met de lagen genoemd, één undo-stap | Bewerken ▸ Snijvolgorde… | `presentation g5_cam_order` (4; een eerste verwachting klopte niet: de stabiele topologische volgorde zet een vrije laag ertussen, test aangepast aan het echte contract) |
| Toollagen T1/T2 (LY01): knoppen 30/31 in het palet, nooit uitvoer, uitvoer aanzetten geeft melding | palet | `presentation g5_cam_tool_layers` (4; mutatieproef: `TOOL_SLOT_T1 => None` laat `a_click_on_t1_makes_a_tool_layer_that_is_never_output` falen), macOS `layer_panels` aangepast (32 knoppen), native `layer_palette` |
| Aanduiding "×n" in de objectlijst voor een object dat op meer lagen staat (C02) | objectlijst | `macos g5_cam_panels::the_palette_ends_with_the_tool_layers_and_the_object_list_says_how_many_layers_use_an_object` (test na de implementatie geschreven: geen aparte rode fase) |
| Sublagen (C11/LY01): toevoegen (kopie van de laag), bewerken, verwijderen, volgorde; elke wijziging = één `SetAdvanced` | Bewerken ▸ Sublagen… | `presentation g5_cam_sublayers` (5), native `sublayers_dialog_adds_edits_and_orders_sub_layers` |
| Z per doorgang + tabmelding bij het tabveld (C04) | Cut Settings | `presentation g5_cam_cut_sheet` (2) |
| Laagkleur in het projectbestand (C01): `layer-slots.json`, niet in de projecthash | Archief | `io layer_slots_roundtrip` (2), `presentation g5_cam_layer_colours` |
| Kerf-zijde op het canvas (C03): geselecteerde vorm toont het pad van de straal | canvas | `presentation g5_cam_kerf_view` (2) |
| Materiaalpaneel: Bijwerken / Bijwerken en toepassen in het echte paneel (C08) | Venster ▸ Materiaalbibliotheek | native `g5_cam_material`; **vond een echte bug**: een pop-up van het paneel verloor de gekozen laag zodra een titel veranderde (`files_ui::set_popup`), waardoor de knoppen op de verkeerde laag werkten; opgelost |
| Productienesting (C12, domein): spiegelen is een keuze (`MirrorPolicy`, standaard verboden), `stock_use` (rest per plaat), `order_status`, `complete_sets` | `burnburn_cam::nest` | `cam tests/nest_production.rs` (5, incl. proptest `better_score_never_violates_forbidden_mirroring`) |

Windows: het menu volgt het Mac-menu (`menus/{arrange,edit,laser}.rs`), `tests/menus.rs` en `tests/form_cam.rs` zijn groen; de
generieke formulieren tonen de nieuwe dialogen zonder eigen code behalve `choice_items` (labels) en de actieknoppenlijst.

## Open

- Productienesting in de UI (C12): het Schikken-paneel kent nog geen spiegel-optie, restoppervlak per plaat, orderstatus of
  complete sets, en toepassen van meer exemplaren/meer platen bestaat niet; dat raakt W07 van de lane G5-UI-gaten
  (restplaten/openstaande aantallen). Domein-API staat klaar, wachtrij-regel toegevoegd.
- Tab Cut Power (C04) niet gebouwd: vraagt vermogen per beweging in het plan en een RD-vorm met bewijs.
- C05: Adjust Image/crop/mask en golden images per beeldmodus (L5a): niet aangeraakt. C03: de zijde van een gat (naar
  binnen) is wel in de offset maar niet apart getest in de weergave.
- C06 "startpunt/richting vergrendelen" per vorm: alleen de globale schakelaars bestaan (Optimalisatie); niet gebouwd.
- Feeder: alleen planning en preview; RD/GRBL weigeren getypeerd (geen bewezen draadvorm). Geen hardware.
- Handmatig: uiterlijk van alle nieuwe panelen, de ghosts van het array-voorbeeld op een echt scherm, zin van de
  tijdschatting met feeder. Windows-run van de nieuwe formulieren: niet gedraaid (alleen headless menus/formulier-tests).
- Het native paneel is gebouwd uit vaste rijen: de sublagen-dialoog bewerkt daarom één gekozen sublaag tegelijk.

## Buiten de lane geraakt (additief)

`burnburn-render` (`LAYER_COLOURS` 32 + twee kleuren), `burnburn-io` (container: `layer-slots.json`, `Project.layer_slots`,
DTO's `ToolDto`, `ModifierKind::PathArray`), `burnburn-job` (PlanEvent `Feed/Pause`, preview-velden), `burnburn-ruida` en
`burnburn-gcode` (typed weigering feeder), `burnburn-machine` (placement laat Feed/Pause door), `burnburn-macos`
(`files_ui::set_popup`, menu, panelen), `burnburn-windows` (menu's, `forms/cam.rs`), bestaande tests aangepast voor 32 palet-
knoppen en het extra veld `ZStep`.
