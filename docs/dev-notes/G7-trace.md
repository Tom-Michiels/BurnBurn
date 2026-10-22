# G7-trace: wijzen, lens, beschermen, recepten, lagen en Arabisch

Basis 40c71b9 (main na golf 6). Alle commando's met `CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/g7-trace`,
`CARGO_INCREMENTAL=0`, `--locked`, toolchain 1.99.1, macOS 27 arm64. Alleen werkelijk uitgevoerde runs staan hieronder; waar een
rode run ontbrak staat dat erbij.

## Wat er is (bedienbaar)

| Punt | Wat | Waar |
|---|---|---|
| T03 lens | radiale vervorming (k1, k2), vóór de vlakcorrectie, in het recept | `burnburn-image/src/lens.rs`, `ImageRecipe.lens`, param `image.lens`; paneel Beeld voorbereiden |
| T03 wijzen | foto komt op het blad (`Overlay::BedImage`), klik = hoek (4 klikken in willekeurige volgorde), slepen met de kwast = één masker (`RegionShape::Stroke`); Escape stopt; begin zoomt de foto in beeld | `controller/trace_g7.rs`, `controller/trace_overlay.rs`, `trace_pick.rs`; hosts hoeven niets eigens |
| T05 | beschermde contouren (vinkje per contour) en knooppuntdoel (max. knopen + grofste tolerantie) voor de volgende lokale hertrace; de correctie meldt "gehaald/NIET gehaald" | `TracePrepIntent::{ProtectContour, SetNodeTarget}` |
| T06 | contouren behouden tegen "negeer kleiner dan" (vinkje) en weggelaten stukken per stuk terugzetten; wordt `contour.keep` in het recept (anker = een hoekpunt van de eigen omtrek) | `burnburn-trace` (`ContourSettings.keep`), `KeepContour`, `RestoreRemoved` |
| T09 | Beeld ▸ Beeld vervangen en passen…: het kader krijgt de beeldverhouding van de nieuwe pixels, zo groot als het oude kader toelaat, gecentreerd; één undo-stap (`CompoundLabel::ReplaceToFit`) | `controller/image.rs`; menu op beide hosts, knop in het Windows-Beeldpaneel |
| T10 | eigen recepten bewaren/toepassen/verwijderen (bestand `trace-recipes.json` in de gebruikersmap), A/B-vergelijking met een opgeslagen of ingebouwd recept (cijfers + één plaatje met beide omtrekken), batch over meerdere bestanden met rapport per bestand en annuleren | `burnburn-io/src/trace_recipe_library.rs`, `trace_study.rs`, `controller/trace_recipes.rs` |
| T04 | centerline als open streken op het canvas, op de plek waar toepassen ze zou zetten | `controller/trace_overlay.rs` |
| P04 | PDF (streek-/vulkleur) en HPGL (pen) stellen lagen voor; vinkje in het importvenster maakt per laag een Line- (streek, pen) of Fill-operatie in dezelfde undo-stap als de objecten | `ImportedPath.layer`, `controller/import.rs::commit_with_layers` |
| Tekst | Arabisch getest (isol/init/medi/fina, lam-alef, marktekens, RTL-volgorde tussen Latijn); geen fout gevonden, dus geen codewijziging | `burnburn-text/tests/arabic.rs`, fonttestbestand `test-data/fonts/DejaVuSans.ttf` |

Beide hosts: de nieuwe secties staan in het bestaande paneel Beeld voorbereiden (`G5Panel::TracePrep`), dus macOS en Windows delen
`panels/g7_trace.rs` en `l10n_g7_trace.rs` (Windows compileert die via `#[path]`). Importvinkje: eigen widget op beide hosts.

## Beslissingen

- **Wijzen op het canvas, niet in een paneelplaatje.** De generieke G5-panelen kennen geen klikbaar beeld; de canvasaanwijzer wel (en
  `HeadlessEditorHarness::click/drag` oefent hem). De foto ligt op het blad met 1 pixel = 1 pixel van de opgegeven schaal; de
  controller zet mm om naar foto-pixels (y omgekeerd). Hoeken zijn pixels van de foto na uitsnede, draaiing en lenscorrectie, vóór de
  vlakcorrectie; kwaststreken zijn pixels van het werkbeeld (na de vlakcorrectie).
- **Lens vóór vlak.** `prepare` doet crop/draai → lens → vlakcorrectie. De hoeken van de vlakcorrectie staan dus in gecorrigeerde
  fotopixels. Het lensmodel is `r_d = r_u (1 + k1 r_u² + k2 r_u⁴)` met `r` genormaliseerd op de halve diagonaal; `validate` weigert
  niet-eindige waarden, |k| > 1 en een model dat binnen het beeld terugvouwt.
- **Recept alleen met wat er is.** `image.lens`, `contour.keep` en `RegionShape::Stroke` worden alleen geschreven als ze er zijn:
  bestaande recept-sleutels en projecten blijven gelijk (geen schema-migratie nodig).
- **Eigen recept = instellingen van een soort beeld.** Opslaan haalt resolutie, uitsnede, draaiing, vlakcorrectie, maskers en
  behouden stukken eruit (die horen bij één foto); toepassen legt het recept op de foto in de tracer. De lens hoort bij het recept
  als het er een heeft (het beschrijft een camera).
- **Batch per bestand getraced**, niet via één `trace_batch`-aanroep, zodat elk bestand zijn eigen resolutie kan meebrengen als het
  recept er geen heeft en annuleren per bestand werkt. Annuleren geeft géén deelrapport (zoals `trace_batch`).
- **Behouden stuk herkent zich aan een hoekpunt** (tolerantie 0,25 px). Een ander drempelniveau verplaatst de hoekpunten: het anker
  past dan nergens meer, wordt geteld (`TraceStats.keep_unmatched`) en doet niets. Een behouden gat/onderdeel houdt de omtrekken
  eromheen vast.
- **Wijziging van het basisrecept en lokale correcties.** Een stuk terugzetten of behouden verandert de recept-sleutel van de basis; een
  correctie waarvan het gebied daardoor een nieuwe contour krijgt, wordt (zoals altijd) als "niet toegepast: nieuwe contour in het
  gebied" gemeld en niet elders toegepast. Verwijder en maak haar opnieuw.
- **Arabisch**: harfrust (rustybuzz-opvolger) + unicode-bidi deden het goed. De isolated vorm van DejaVu Sans is het basisglyph
  (gid 1365), niet U+FE8F (gid 5254); de eerste oracle was daar fout (zie tabel), de shaper niet.

## TDD-log

| Test | Verwachte fout (red) | Red-commando en resultaat | Green-resultaat |
|---|---|---|---|
| `text/tests/arabic.rs` (8) | eerst: fixture ontbreekt in manifest (`NotInManifest`, geen gedragsrood); daarna oracle-fout | `cargo test -p burnburn-text --test arabic`: na het manifest 3 FAILED (isolated beh/meem = base-cmap-glyph 1365/1389, niet FE8F/FEE1; `reference_metrics.py` geeft dezelfde advance en bbox). Oracle aangepast, geen codewijziging | 8 ok; hele `burnburn-text`-suite ok |
| `image/tests/lens_distortion.rs` (6) | stub `undistort` klonen | `--test lens_distortion`: 2 FAILED van 6 (rechte lijn nog gebogen); sleutel-/weigertests waren al groen door de stub | 6 ok |
| `trace/tests/document_recipe.rs` (lens, stroke, keep in `every_field` + 1 nieuwe) | param ontbreekt | `--test document_recipe`: 2 FAILED (rondreis, weigering van `image.lens`); de stroke-/keep-uitbreiding van `every_field` kreeg geen aparte rode run (zonder de `match`-armen compileert het niet) | 4 ok |
| `image/tests/brush_mask.rs` (5) | stub `Stroke` dekt niets | `--test brush_mask`: 5 FAILED | 5 ok |
| `trace/tests/keep_features.rs` (6) | stubs: anker `[0,0]`, filter negeert `keep` | `--test keep_features`: 6 FAILED | 6 ok; hele `burnburn-trace`- en `burnburn-image`-suite ok |
| `presentation/tests/g7_trace_pick.rs` (11) | intents deden niets | 10 FAILED (lens, hoeken, kwast, plaatje); typed radius: 1 FAILED | 11 ok |
| `presentation/tests/g7_trace_protect.rs` (10) | – | **geen rode run**: implementatie en test in één stap. Vervangend bewijs: met `protect_contour`/`restore_removed` uitgeschakeld (mutatie, daarna teruggezet) 4 FAILED van 10 | 10 ok. Eén oracle was fout en is gecorrigeerd vóór groen: het bounding-box-midden van de ring valt samen met dat van zijn binnenrand (filter op breedte < 4 mm) en `ApplyTraceLinked` geeft 3 objecten (ring+gat is één onderdeel) |
| `presentation/tests/g7_replace_fit.rs` (6) | stub = gewone Replace | 2 FAILED van 6 (kader niet aangepast) | 6 ok; `image_flow` (26) ok |
| `io/tests/g7_trace_recipe_library.rs` (5) | stub-bibliotheek | 5 FAILED | 5 ok |
| `presentation/tests/g7_trace_recipes.rs` (12) | stub-intent/viewmodel leeg | 12 FAILED | 12 ok |
| `presentation/tests/g7_trace_overlay.rs` (3) | overlay niet in de scène | 3 FAILED | 3 ok |
| `io/tests/{pdf,hpgl}.rs` laag-sleutel | `layer: None` | `--test hpgl`: 1 FAILED (pdf-test draaide in dezelfde aanroep niet mee; na de implementatie ok) | pdf 19 ok, hpgl 14 ok |
| `presentation/tests/g7_import_layers.rs` (5) | stub-intent deed niets | 3 FAILED van 5 | 5 ok |
| `presentation/tests/g7_trace_canvas.rs` (5) | geen pointer-hook | 4 FAILED; later de zoom-test: 1 FAILED (foto 25 px breed in beeld) | 5 ok |
| `macos/tests/g7_trace_panels.rs` (10), `windows/tests/g7_trace_forms.rs` (4) | – | **geen rode run**: model en test samen geschreven | 10 + 4 ok |
| `macos/tests/g7_replace_fit_menu.rs` (2), `windows/tests/g7_replace_fit.rs` (2) | – | alleen compileerfout vooraf (geen gedragsrood) | 2 + 2 ok; `menus` (8), `w4b_image`, `w4b_menu` ok |
| `macos/tests/g7_import_layers_panel.rs` (3) | stubs `None` | 2 FAILED van 3 | 3 ok |
| `windows/tests/g7_import_layers_form.rs` (2) | – | **geen rode run** (besturingselement en test samen) | 2 ok; `w4b_files` (11) ok |
| native `import_layers_native` | – | geen rode run | 1 ok (echte AppKit-sheet: vinkje verborgen bij DXF, zichtbaar bij PLT, klik maakt 2 operaties) |

Native macOS-runs na de wijzigingen: `g5_panels_native` (6), `file_panels_native`, `native_file_panels`, `import_layers_native`: ok.

## Handmatig / onbevestigd

- **Handmatig**: hoe de foto op het blad eruitziet en voelt (kwast slepen, hoeken klikken, zoom), uiterlijk (licht/donker) van de nieuwe
  secties, NSOpenPanel van Replace to Fit, de Windows-realisatie (`FormLine::Check`/`Memo`/`Image`, het importvinkje, de knop
  Replace to Fit) — alleen headless en type-check/clippy voor `x86_64-pc-windows-msvc`.
- **Niet gebouwd**: meervoudskeuzedialoog voor de batch (paden komen uit een tekstvak, één per regel); contouren beschermen door erop
  te klikken (het is een lijst met vinkjes); beschermde *hoeken*; knooppuntmarkeringen bij de centerline-overlay; A/B op het canvas
  (de twee omtrekken staan in een plaatje in het paneel); vergelijking met lokale correcties erin; Replace to Fit voor een gedraaid
  kader (wordt geweigerd met `CommandRejected`); een lensmodel met tangentiële termen; kleur → palet-slot bij P04.
- **Tekst**: Arabisch is met één lettertype getest; marktekenpositie (GPOS), kashida/uitvullen en Indische scripts niet. Geen SHX-parser.
