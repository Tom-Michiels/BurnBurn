# G9-trace-infra: trace-restpunten, uitvullen, .ttc, gedeelde testfixtures, fuzzrun

Basis d751627 (main na golf 8). Alle commando's met `CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/g9-trace`
(fuzz: `.../target/g9-fuzz`), `CARGO_INCREMENTAL=0`, `--locked`, toolchain 1.99.1 (fuzz: nightly), macOS 27 arm64.
Alleen werkelijk uitgevoerde runs staan hieronder. Waar de rode fase een stub was of een eerdere vorm van de test
bevatte, staat dat erbij.

## Wat er is

| Punt | Wat | Waar |
|---|---|---|
| Fixtures | de testbinaries delen fixtures niet meer via `#[path]` tussen crates: `g5_fixtures`, `g6_fixtures`, `presentation/tests/common/{mod,laser,camera}.rs` en `grbl_fake.rs` staan in `burnburn-test-support`, module `host`, achter de feature `host` (laag 6, hangt van presentation en lager af). presentation, macos en windows vragen de feature als dev-dependency; de andere gebruikers van de testkit bouwen presentation niet mee | `crates/burnburn-test-support/src/host/{g5,g6,rig,camera,grbl_fake}.rs`, ADR 0009 |
| `path-attrs` | controleert nu ook `tests/` van elke crate en de native tests onder `tests/native/` (daar telt `tests/` als eigen gebied) | `xtask/src/path_attrs.rs::check_tree`, `xtask/src/main.rs` |
| T10 relink | een correctie die op B niet bindt kan vanuit de A/B-vergelijking opnieuw aan B gekoppeld worden ("Koppel correctie n toch aan B") en weer losgehaald; de beschermde contouren van de correctie worden op B opnieuw gezocht op soort, kader (overlap ≥ 0,5) en oppervlak (± 25 %), niet op id; wat geen tegenhanger heeft valt weg en wordt geteld | `trace::{relink_correction, replay_corrections_relinking, compare_recipes_corrected_relinking}`, `RecipeComparison::{b_unbound, b_relinked}`, `TraceRecipeIntent::RelinkCorrection`, `CompareViewModel::b_corrections`, `G5Control::CompareRelink` |
| T06 verschoven hoek | een beschermde hoek in een lokale hertrace met een andere drempel wordt ook gevonden als het hoekpunt tot 8 px (`SHIFTED_CORNER_REACH_PX`) van de bewaarde plek ligt: binnen 3 px het dichtstbijzijnde hoekpunt (zoals voorheen), anders het scherpste hoekpunt (draaiing ≥ 5°) binnen 8 px; daarbuiten telt de hoek als "niet gevonden" (`stats.corners_unmatched`, voor de lokale hertrace nu ook echt geteld) | `trace/src/correct.rs::resolve_corners` |
| T09 gedraaid kader | Replace to Fit werkt ook voor een kader dat een rechthoek op een hoek is: de zijde die het dichtst bij de x-as van het blad ligt volgt de breedte van het plaatje, het kader behoudt zijn hoek en middelpunt; een kader dat geen rechthoek is blijft geweigerd | `presentation/src/controller/image.rs::{rectangular_frame, fit_transform}` |
| Tekst | uitvullen (`Align::Justify`, keuze "Uitvullen" in het tekstpaneel van beide hosts): elke regel behalve de laatste wordt zo breed als de breedste regel, over de woordgaten; Arabische regels krijgen eerst kashida (tatweel) op de verbindingen waar dat mag, de rest gaat naar de gaten; een regel zonder gat gaat over de letters, behalve in een cursief schrift (daar blijft hij zoals hij is) | `text/src/shape.rs`, `Align::Justify` in document/io (`text_dto`, canonical) en `panels/text.rs` |
| Tekst | de systeemfontlijst neemt `.ttc`/`.otc` mee, elke face; een face heeft een eigen asset-hash (`FontData::from_bytes_face`) | `text/src/font.rs::{face_names, from_bytes_face}`, `runtime/src/fonts.rs` |

## Rood/groen (echte runs)

**`path-attrs` (`cargo test -p xtask --locked path_attrs`)**: eerst de twee nieuwe tests tegen een `check_tree` die alleen de
eerste map las: `2 failed; 5 passed` (`a_path_in_the_tests_directory_that_leaves_the_crate_is_reported`,
`..._own_crate_is_fine_and_missing_directories_are_skipped`). Daarna `7 passed`. Rood op de echte werkspace
(`cargo run -p xtask --locked -- path-attrs`): `xtask: 21 #[path] attribute(s) leave their own crate` (8 in macos/tests, 12 in windows/tests,
`tests/native/macos/g5_panels.rs`, ...). Na de verhuizing: `path-attrs: 27 crates, 1341 source files, no #[path] leaves its own crate (src, tests)`.

**Fixtures**: pure verhuizing (bestanden `mv`, `use burnburn_test_support::host::… as common;` in plaats van `mod common;`/`#[path]`);
`cargo test -p burnburn-presentation -p burnburn-macos -p burnburn-windows -p burnburn-test-support --locked --no-fail-fast`
(`BURNBURN_NATIVE_QUIET=1`): exit 0, 2053 tests geslaagd, 0 gefaald (som van alle `test result`-regels). Clippy vond in de lib-vorm een
ontbrekende `Default` voor `FakeGrbl` (nu toegevoegd).

**Relink (trace)** `trace/tests/relink_correction.rs` (6): `relink_correction` als stub die de basis teruggaf: `1 passed; 5 failed`
(`relinking_applies_the_region_and_recipe_to_the_contours_now_there`, `a_protected_contour_is_found_again_by_kind_and_bounds_not_by_id`,
`replaying_with_the_index_relinks_and_without_it_reports_unbound`, `the_comparison_lists_what_did_not_bind_on_b_and_relinks_on_request`,
`relinking_refuses_another_source_and_honours_cancellation`). Daarna `6 passed`. Oracle: de rand van de halve bult (π·0,8²/2 = 1,005 mm²,
toegelaten 0,7–1,3) komt erbij; de beschermde tegenhanger op drempel 130 is gevonden (1/0), op 175 niet (0/1).
`presentation/tests/g9_trace_relink.rs` (3): intent/viewmodel als stub (`AppUpdate::default()`): `1 failed` (de koppel/ontkoppel-test), 2 geslaagd
(trivieel, de rest is de "doet niets"-rand); daarna `3 passed`. Paneel: `macos/tests/g9_trace_relink_panel.rs` (2) stub-intent: `1 failed; 1 passed`;
daarna `2 passed`; `windows/tests/g9_trace_relink_forms.rs` (1) `passed` (zelfde gedeelde paneelcode; geen aparte rode run).

**Verschoven hoek** `trace/tests/shifted_corners.rs` (4), foto met zachte randen (1,5 mm helling): oracle is de analytische top van het
gekrompen huis (daken 0,6 mm op 6 mm; top zakt d/sin(helft van de binnenhoek)). Vóór `resolve_corners`: `3 failed; 1 passed` (de
controle zonder beschermde hoek ronde de top af, zoals het hoort); daarna `4 passed`. Het bestaande `protected_corners.rs` en de hele
`burnburn-trace`-suite bleven groen.

**Uitvullen** `text/tests/justify.rs` (7): `Align::Justify` als alias van links: `5 failed; 2 passed` (de twee geslaagde zijn de
"niets te doen"-gevallen). Eerste groene poging `6 passed; 1 failed`: mijn fixture `"fff"` is een ligatuur (2 glyphs) en mijn controle nam
gelijke advances aan; fixture/controle aangepast (`"xyz"`, per glyph het eigen advance), toen `7 passed` en de hele `burnburn-text`-suite groen.
Oracle: rechterrand van elke regel uit de glyphs (`origin.x + advance`) tegen de breedste natuurlijke regel; letters binnen een woord houden hun
onderlinge afstand; Arabisch: meer glyphs (tatweels), de woordgaten groeien minder dan de hele rek, `lam`+`alef` blijft één ligatuur.
`io/tests/text_roundtrip.rs` en `macos/tests/text_panels.rs` (keuze 3 = Justify) zijn **na** de implementatie geschreven (het modelstuk is mee
gemaakt met de uitputtende `match`-reparaties): geen rode fase.

**.ttc** `runtime/tests/system_fonts.rs` (2 nieuw, `MuktaMahee.ttc` met 7 faces): vóór: `2 failed; 5 passed` (lijst leeg: `.ttc` werd overgeslagen);
daarna `7 passed`. Oracle voor de face-namen: de name table van elke face van de collectie, uitgelezen met `struct` (familienaam ID 1,
vet = `head.macStyle`); dat het juiste face geladen wordt volgt uit de breedte van drie Gurmukhi-letters: ExtraLight < Regular < Bold.

**Replace to Fit gedraaid** `presentation/tests/g9_replace_fit_rotated.rs` (3): `1 failed; 2 passed` (30°: geweigerd; de kwartslag en de
scheve rand waren al goed: een kwartslag is nog een as-parallelle rechthoek). Daarna `3 passed`; de oude test
`a_frame_that_was_turned_at_an_angle_is_refused_and_nothing_changes` in `g7_replace_fit.rs` is vervallen (contract gewijzigd, zie de
opmerking daar), de scheve rand is in de nieuwe test gedekt.

## Beslissingen

- **Relink beslist de operator**, niet het systeem: de herkoppeling gebeurt per knop, de vergelijking blijft eerlijk ("past niet op B") tot hij
  klikt, en een tweede klik haalt de koppeling weer weg. Beschermde contouren worden op soort/kader/oppervlak opnieuw gezocht omdat id's bij één
  trace horen; wat niet terugkomt vervalt en wordt geteld (`Relinked::protected_lost`), niet geraden.
- **Verschoven hoek: 8 px en minstens 5° draaiing.** Het scherpste hoekpunt binnen bereik, niet het dichtstbijzijnde (zelfde keuze als bij het
  klikken in G8); verder weg of een vlakke rand → "niet gevonden", zichtbaar in `stats.corners_unmatched`. Binnen 3 px verandert er niets.
- **Uitvullen zonder tekstkader**: er is geen kaderbreedte in het model, dus "de breedte van de breedste regel"; de laatste regel blijft los.
  Een echt kader (breedte instellen, regels afbreken) is een ander werkpakket. Kashida alleen op verbindingen van het basis-Arabisch en de
  Perzische/Urdu-aanvullingen (hardgecodeerde Joining_Type D/R-tabel, geen Unicode-crate), nooit in `lam`+`alef`, hoogstens 6 per woord, één plek per woord
  (het dichtst bij het midden); het lettertype moet een tatweel hebben.
- **Gedraaid kader**: de burn dekt de *bounds* van het kader, dus de zijde langs de x-as is de breedte; bij een hoek buiten 0°/90° blijft de
  bounding box dus niet exact de verhouding van het plaatje (het kader zelf wel).
- **Testfixtures in de testkit met een feature**, geen nieuwe crate: R3/R4 van `layers.toml` laten test-support als dev-dependency van elke laag toe en
  de lijst `allow` bevatte presentation al. Een aparte crate op laag 8 kon geen dev-dependency van presentation zijn (laag 5).

## Fuzz (nightly + `cargo-fuzz` stonden al op deze machine, niets geïnstalleerd)

Commando's (vanuit `fuzz/`, korpus per README uit de ingecheckte fixtures plus `make_seeds`; `corpus/` en `artifacts/` zijn genegeerd):

```
export CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/g9-fuzz CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true
cargo +nightly fuzz build                       # bouwt alle 16 doelen; fuzz/Cargo.lock bijgewerkt (stond achter op de workspace)
cargo +nightly run --bin make_seeds -- corpus   # seeds voor io_clipboard, io_material_library, io_art_library, io_bundle
cargo +nightly fuzz run <doel> -- -max_total_time=60 -max_len=1048576     # elk van de 16 doelen, na elkaar
```

Uitkomst (elk doel exit 0, 61 s, **geen crash, geen timeout, geen OOM**; `fuzz/artifacts/` bleef leeg), runs / dekking (`cov`) / korpus aan het eind:

| Doel | runs | cov | korpus |
|---|---|---|---|
| `ruida_decode` | 756.925 | 349 | 590 |
| `ruida_rd_file` | 345.342 | 454 | 360 |
| `ruida_udp_frame` | 11.202.844 | 57 | 35 |
| `grbl_status` | 1.202.993 | 561 | 752 |
| `io_container` | 348.140 | 2931 | 601 |
| `io_dto_json` | 322.422 | 1304 | 890 |
| `io_clipboard` | 310.988 | 1953 | 1019 |
| `io_parse_svg` | 306.023 | 1544 | 846 |
| `io_parse_dxf` | 345.804 | 1235 | 739 |
| `io_parse_hpgl` | 321.103 | 603 | 901 |
| `io_parse_lbrn2` | 263.545 | 2770 | 777 |
| `io_parse_pdf` | 46.057 | 1922 | 1246 |
| `io_material_library` | 354.840 | 2032 | 910 |
| `io_art_library` | 291.465 | 2453 | 383 |
| `io_bundle` | 347.114 | 2705 | 440 |
| `image_decode` | 206.232 | 1986 | 555 |

Een minuut per doel is een rooktest van de gebouwde doelen (wat B-10 "gebouwd maar niet gedraaid" noemde is nu gedraaid), geen bewijs van
afwezigheid van fouten; `parse_pdf` haalde 755 exec/s en is het traagste doel. Geen regressietest nodig (geen vondst). De uitgebreide
uitvoer per doel staat niet in de repo (scratchpad van de run).

## Verificatie

VERIFY_RESULTS

## Open / handmatig

- **Handmatig**: uiterlijk van de nieuwe knoppen/regels in de vergelijking, de keuze "Uitvullen" en de weergave van uitgevulde (Arabische) tekst in canvas en
  tekstpaneel; native AppKit-run van het tekstpaneel met vier segmenten (de headless paneeltest dekt de mapping, de native test niet de vierde knop apart).
- **Open (software)**: Devanagari zelf: op deze machine staan alleen Apple-fonts (Devanagari Sangam MN, Devanagari MT, ITF Devanagari) en Noto-gezichten voor andere
  schriften; geen permissief Devanagari-font, dus niets getest of toegevoegd. Uitvullen met een kaderbreedte en regelafbreking; kashida buiten de basistabel.
  Het scherpste-hoekpunt-zoeken kent geen beperking tot "dezelfde" hoek bij twee hoeken binnen 8 px van elkaar.
- **Open (hardware/Windows/CI)**: Windows-run van alles (alleen typecheck en clippy `x86_64-pc-windows-msvc`), GitHub-CI (vraagt pushen), hardware.
