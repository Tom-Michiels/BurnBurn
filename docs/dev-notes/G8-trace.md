# G8-trace: tangentiële lens, beschermen door te klikken, batchkeuze, A/B op het canvas, paletkoppeling, tekst en prestatiebewaking

Basis 16f0f7d (main na golf 7). Alle commando's met `CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/g8-trace`,
`CARGO_INCREMENTAL=0`, `--locked`, toolchain 1.99.1, macOS 27 arm64. Alleen werkelijk uitgevoerde runs staan hieronder; waar een
rode run ontbrak of alleen een compileerfout was, staat dat erbij.

## Wat er is (bedienbaar)

| Punt | Wat | Waar |
|---|---|---|
| T03 tangentieel | Brown-Conrady `p1`, `p2` naast `k1`, `k2`: `x_d = x(1+k1 r²+k2 r⁴) + 2 p1 x y + p2 (r²+2x²)`, `y_d = y(…) + p1 (r²+2y²) + 2 p2 x y`; twee velden extra in Lensvervorming; een zuiver radiale lens behoudt sleutel (`lens-v1`) en recept-vorm (2 getallen), met tangentiële termen `lens-v2` en 4 getallen | `burnburn-image/src/lens.rs`, `trace/src/document.rs`, `TracePrepIntent::UseLens{p1,p2}`, `G5Control::{LensP1,LensP2}` |
| T05 | beschermen door op het canvas te klikken: "Contouren aanklikken om te beschermen" legt de foto en alle omtrekken van het resultaat op het blad; een klik op een omtrek (binnen 6 schermpunten, ten hoogste 12 beeldpixels) of binnen een deel/gat beschermt de kleinste omsluitende contour tegen de volgende lokale hertrace, een tweede klik laat los; beschermde omtrekken zijn zwaarder getekend | `TracePick::{BeginProtectContours, ProtectAt}`, `TraceResult::contour_at_px`, `Overlay::ProtectedOutline` |
| T06 | beschermde hoeken: "Hoeken aanklikken om te beschermen"; een klik kiest het scherpste hoekpunt van een omtrek binnen 3 px, de fit houdt daar een hoeknode, ook bij een bocht onder de hoekdrempel of een tolerantie die hem zou afronden; staat in het recept (`contour.corners`), dus in de opgeslagen trace; lijst met "Laat hoek n los" in het paneel; bij een lokale hertrace blijft de node als anker | `ContourSettings.corners`, `fit_closed_with_corners`, `TraceResult::corner_near_px`, `TracePick::BeginProtectCorners`, `TracePrepIntent::RemoveProtectedCorner`, `TracePrepViewModel.corners` |
| T04 | knooppuntmarkeringen op de centerline-streken: alle knopen als gewone markering, de open uiteinden groot en hol | `controller/trace_overlay.rs::stroke_nodes` (`Overlay::Node`) |
| T10 bestandskeuze | "Kies bestanden…" in het batchgedeelte: macOS `NSOpenPanel` met `allowsMultipleSelection`, Windows `IFileOpenDialog` + `FOS_ALLOWMULTISELECT`; de gekozen bestanden zijn één batch (elk bestand één keer, volgorde van het paneel); het tekstvak blijft | `G5Control::BatchChoose`, `panels::batch_files_intent`, `NativeEvent::TraceBatchFilesChosen`, `Reaction::ShowTraceBatchPanel`, `FilePick::TraceBatch`, `forms::g5::batch_files_chosen`, `win32/dialogs.rs::open_many_dialog_of` |
| T10 A/B | de vergelijking past de lokale correcties van de tracer toe op A (zoals de tracer ze toont) en op B waar ze nog binden (een correctie die op B niet meer past wordt daar niet toegepast, niet elders); het paneel meldt "Lokale correcties: n bewaard; A past er x toe, B past er y toe"; beide omtrekken liggen op het blad (A breed en donker, B dun en rood) waar toepassen ze zou zetten | `trace::compare_recipes_corrected`, `CompareCorrections`, `CompareViewModel.corrections`, `Overlay::CompareOutline{side}` |
| P04 | importlagen worden gekoppeld aan de dichtstbijzijnde vrije paletkleur (gekwadrateerde sRGB-afstand, gelijkspel naar het lagere slot, elke laag een eigen slot, slots van bestaande lagen niet opnieuw, pennen op hun nummer); zonder palet (headless host) lage vrije slots zoals voorheen | `controller/import.rs::palette_slots`, `Libraries::layer_palette` |
| Tekst | zie "Tekst" hieronder | `burnburn-text` |
| X04 | deterministische, telling-gebaseerde prestatiebewaking van de 100k-route als eigen stap in `cargo xtask verify` | `scenarios/tests/perf_guard_100k.rs`, `burnburn_editor::{editor_work, reset_editor_work}`, `xtask::verify_steps` |

Beide hosts: de nieuwe bedieningselementen zijn `G5Control`s van het bestaande paneel Beeld voorbereiden (`panels/g7_trace.rs` is
gedeeld via `#[path]`), dus Windows krijgt ze zonder eigen paneelcode; Windows-eigen is `FilePick::TraceBatch` met de dialoog.
P04 gebruikt het palet dat beide hosts al aan de controller geven (`set_layer_palette`).

## Beslissingen

- **Tangentieel zonder schemawijziging.** `image.lens` is een lijst van 2 (radiaal) of 4 getallen; `lens-v1` blijft voor radiale
  lenzen, zodat bestaande recepten en sleutels gelijk blijven. `validate` weigert niet-eindige waarden, |p| > 1 en een afbeelding die
  binnen de eenheidsschijf terugvouwt (determinant van de Jacobiaan ≤ 1e-3, op een rooster van 101 × 101).
- **Beschermen als aanwijsstand, niet als twee soorten klik.** Contouren en hoeken zijn twee stands (`BeginProtectContours`,
  `BeginProtectCorners`): een hoek is een hoekpunt van een omtrek, een contour is de hele omtrek, en een klik op een hoek is ook een
  klik op een omtrek. Twee stands geven geen dubbelzinnigheid en geen heuristiek op de scherpte van de bocht.
- **Hoek = scherpste hoekpunt bij de klik**, niet het dichtstbijzijnde. De eerste versie nam het dichtstbijzijnde bronpunt en zat
  0,15 mm naast de top (rode run, tabel); `corner_near_px` kiest binnen het bereik het hoekpunt met de scherpste draaiing en slaat
  zijn positie op, dus het recept bewaart het hoekpunt zelf, niet de klik.
- **Beschermen van contouren is toestand van de tracer, niet van het recept** (zoals in golf 7): `t.protect` geldt voor de volgende
  lokale hertrace en wordt daarna gewist. Beschermde hoeken zijn wel recept.
- **A/B past correcties toe op beide varianten** met dezelfde `replay_corrections` als de tracer; een correctie die op B niet bindt,
  wordt geteld en niet toegepast. Het canvas toont alleen contourtraces met een klaar resultaat.
- **Palet: dichtstbijzijnde vrije slot**, niet "dichtstbijzijnde, ook als bezet": twee bijna gelijke roden blijven twee lagen
  (twee behandelingen); LightBurn voegt ze samen, BurnBurn bewaart wat het bestand onderscheidt. Pennen hebben geen kleur: pen n
  neemt slot n−1 of het eerstvolgende vrije.
- **Prestatiebewaking met tellers, zonder `unsafe`.** Een tellende allocator kan niet (`unsafe_code = forbid`). In de editor staan
  drie thread-lokale tellers (`pick_candidates`, `selection_steps`, `scene_items`); de klik gebruikt daarnaast `pick_index_builds()` en
  de eigen knooptelling van `SpatialIndex::query_point_cost`; de compile-schaal is de lineariteit van het plan (events van 120 → 240
  contouren) plus de tellertests van het planningsalgoritme in `burnburn-cam` (`line::tests::choosing_the_next_contour_…`). Een teller
  beschermt alleen wat erachter staat: wie `OrderedSet::contains` weer lineair maakt, houdt de teller op 1 per aanroep; daarvoor blijft
  de telsleutel-test in `editor/selection.rs`. Wandklokbenchmarks (`benchmarks_100k.rs`) blijven `#[ignore]`.
- **Tekst: drie echte fouten gevonden en gefixt** (zie "Tekst").

## Tekst

Fonts (herkomst in `test-data/fonts/README.md` en `manifest.json`): DejaVu Sans (Arabisch, Latijnse accenten; bestond) en
Mukta Mahee (Ek Type, OFL 1.1), ongewijzigd uit `/System/Library/Fonts/` op deze machine; Mukta Mahee dekt Gurmukhi en Latijn, **niet**
Devanagari. Er is op deze machine geen permissief gelicenseerd Devanagari-font (Apples Kohinoor/Devanagari MT zijn niet
herdistribueerbaar, Noto Sans Devanagari staat er niet), dus Devanagari is niet getest; Gurmukhi gebruikt dezelfde Indic-shaper
(matra vóór de medeklinker, onderstaande en samengestelde letters, tekens boven en onder de letter).

- **GPOS-marktekens** (geen fout): fatha en shadda staan boven, kasra onder hun letter en binnen het lichaam ervan, in alle
  verbindingsvormen; marks verschuiven de letters niet; Latijns x + accent (geen voorgecomponeerd teken) staat boven de x; Gurmukhi
  u-teken onder, ai-teken en bindi boven. Shadda + kasra is in DejaVu één glyph (kasra onder de shadda, beide boven de letter): keuze
  van het font, geen fout.
- **Kashida** (geen fout): een tatweel loopt in beide naburige letters, de woorden worden precies zijn advance breder en de letters
  houden hun verbindingsvormen.
- **Fout 1, gefixt: lettertypecollecties.** `FontData::from_bytes` beloofde "eerste gezicht van een collectie" maar weigerde elke `.ttc`
  (`Unreadable`), want skrifa's `FontRef::new` leest geen collecties. Nu `FontRef::from_index(bytes, 0)` overal (laden, `has_glyph`,
  metrics, outlines, `font_names`). De lijst van systeemfonts in `burnburn-runtime` kijkt nog alleen naar `.ttf`/`.otf` (open).
- **Fout 2, gefixt: letterafstand scheurde verbonden Arabische woorden.** Tussen twee letters van een cursief schrift (Arabisch,
  Syrisch, N'Ko, Mandaeïsch, Mongools) komt geen letterafstand meer; spaties, cijfers en leestekens krijgen hem wel, Latijn ongewijzigd.
- **Fout 3, gefixt: onzichtbare opmaaktekens als "ontbrekend".** ZWSP, LRM/RLM, woordverbinder, BOM, variatieselectors enz. werden
  gerapporteerd als ontbrekende glyph (en weggelaten) zodra het font ze niet mapt (Mukta Mahee kent U+200E niet). Ze blijven nu in de run
  van de tekens ervoor en gaan naar de shaper, nooit in `missing`.

## TDD-log

| Test | Verwachte fout (red) | Red-commando en resultaat | Green-resultaat |
|---|---|---|---|
| `image/tests/lens_tangential.rs` (5) | stub-`distort` negeert p1/p2 | `cargo test -p burnburn-image --test lens_tangential`: 4 FAILED van 5 (handwaarde: x = 155,90 i.p.v. 100 + 0,5075·√12500 = 156,74; sleutel gelijk; weigertest; rechtzetten); de "fixture is echt krom"-test was al groen (karakterisering) | 5 ok; `lens_distortion` (6) ok |
| `trace/tests/document_recipe.rs::tangential_lens_terms_survive_…` | recept schreef 2 getallen | `--test document_recipe`: 1 FAILED | 5 ok |
| `presentation/tests/g8_trace_lens.rs` (3) | stub `UseLens` negeerde p1/p2 | 3 FAILED (`("0","0")` i.p.v. `("0.03","-0.06")`, geen weigering van `abc`) | 3 ok; `g7_trace_pick` (11) ok |
| `macos/tests/g7_trace_panels.rs::the_lens_panel_has_tangential_fields_…` | intent met vaste `"0"` | 1 FAILED van 11 | 11 ok; `windows/tests/g7_trace_forms.rs` (4) ok (gedeeld model, geen aparte rode run) |
| `scenarios/tests/perf_guard_100k.rs` (4) | stubtellers geven 0 | `cargo test -p burnburn-scenarios --test perf_guard_100k`: 3 FAILED ("0 selection steps for 100000 objects", scene_items 0, 0 outlines per click); de planlineariteit was direct groen (karakterisering) | 4 ok in 1,2 s (debug). Mutatie: pick-cache uitgezet → klikbewaking FAILED ("200 clicks … share one index: left 200"; de run duurde 121 s omdat de index 200 keer opnieuw gebouwd werd), daarna teruggezet. Waarden: ergste indexvraag 104 knopen+boxen, 7 omtrekken per klik; select all 100 000 stappen voor 100 000 objecten; scene 100 000 items; plan 12 121 → 24 241 events (×2,000) |
| `xtask tests::verify_runs_the_100k_performance_guard_once_as_its_own_step` | stap ontbrak | `cargo test -p xtask verify_`: 1 FAILED, 1 ok (`verify_never_runs_the_wall_clock_benchmarks`, karakterisering) | 2 ok |
| `presentation/tests/g8_import_palette.rs` (8) | koppeling bestond niet | 4 FAILED van 8 (`[0,1,2]` i.p.v. `[2,1,0]`; twee roden; tweede import; "tussenkleuren"); tie, pennen, zonder palet en undo/redo waren groen (karakterisering) | 8 ok; `g7_import_layers` (5) ok. Eén testfout van mezelf gecorrigeerd vóór groen (`g` i.p.v. `rg` in de PDF-inhoud) |
| `macos/tests/g8_trace_batch_files.rs` (6) | stubs | 3 FAILED van 6 | 6 ok |
| `windows/tests/g8_trace_batch_files.rs` (6) | stubs | 3 FAILED van 6 (de derde was een fout in mijn test: tweede waarde voor dezelfde sleutel; gecorrigeerd) | 6 ok |
| native `trace_batch_panel_native` | – | eerste run FAILED: het sheet is een proxy-`NSWindow`, geen `NSOpenPanel` (de paneeleigenschappen zijn niet via `attachedSheet` te lezen); daarom bewaart `HostApp` het paneel zolang het open is (`trace_batch_panel()`) | 1 ok: paneel met `allowsMultipleSelection`, de knop opent het sheet, Annuleer start niets, de gekozen bestanden starten één batch in paneelvolgorde. De OK-knop van het paneel is een ander proces: handmatig |
| `trace/tests/compare_corrected.rs` (4) | stub negeerde de correcties | 2 FAILED van 4 | 4 ok |
| `presentation/tests/g8_trace_compare.rs` (5) | stubs | 4 FAILED van 5 | 5 ok |
| `macos/tests/g8_trace_compare_panel.rs` (2) | tekst ontbrak | 2 FAILED (de tweede viel eerst op een regio buiten het 64 × 64 px-plaatje: testfout, gecorrigeerd) | 2 ok |
| `render/tests/trace_overlays.rs` (4) | **`ResultPreview` werd door de renderer nooit getekend** (vangnet `_ => {}` in `build_packet`): de centerline-strepen van G7 en de boolean-preview waren op het canvas onzichtbaar; `CompareOutline` en `ProtectedOutline` bestonden niet | eerst 3 FAILED van 3 ("one outline: []"), later 1 FAILED voor `ProtectedOutline` | 4 ok; `scene_projection` (10), `sketch_overlays`, `reshape_overlays` ok |
| `trace/tests/protected_corners.rs` (7) | stub-veld zonder gedrag; daarna fixture-fout | 1e run: 3 FAILED van 5 (hoeken niet in sleutel/recept/telling) — maar de fixture (apex 43,6°) verloor de top niet, dus de test bewees niets: aangepast naar een knik van 11,4° (afstand tot de top zonder bescherming 0,24 mm). 2e run: knoop 0,15 mm naast de top (dichtstbijzijnde bronpunt bij een klik 1,8 px ernaast) → `corner_near_px` (scherpste hoekpunt). `contour_at_px`: 1 FAILED (stub `None`) | 7 ok; hele `burnburn-trace`-suite ok |
| `presentation/tests/g8_trace_protect_click.rs` (10) | stubs (`pointed` deed niets) | 9 FAILED van 10 (`protected_corners_are_stored_…` was al groen vóór de implementatie: zonder bescherming geen afwijking; zie de dekking van het andere deel in de test). Fixture: eerst een harde drempel (apex nog 0,095 mm), daarna anti-aliased (8 × 8 samples) tot hij de top verloor | 10 ok |
| `render/tests/trace_overlays.rs::a_protected_outline_…` | geen arm voor `ProtectedOutline` | 1 FAILED | ok |
| `presentation/tests/g8_trace_nodes.rs` (3) | geen markeringen | 3 FAILED | 3 ok; `g7_trace_overlay` (3) ok |
| `macos/tests/g8_trace_protect_panel.rs` (4), `windows/tests/g8_trace_protect_forms.rs` (2) | – | **geen rode run**: knoppen, teksten en de lijst zijn samen met de test geschreven (alleen compileerfout vooraf) | 4 + 2 ok; `every_text_has_both_languages_and_the_same_placeholders` ok |
| `text/tests/marks_and_scripts.rs` (13) | `.ttc` onleesbaar; letterafstand; ZWSP | 1e run: 9 FAILED van 12; daarvan waren 3 te strenge eigen oracles (marktekencentrum binnen 20 % van een smalle beginvorm; shadda + kasra als twee marks; matra links van de medeklinker terwijl hij ermee overlapt) en 1 een fout in mijn aanname (Mukta Mahee heeft geen Devanagari). Na TTC-fix en herschreven oracles: rood op `letter_spacing_does_not_tear_the_joins…` (31,99 i.p.v. 35,99 mm) en `invisible_format_characters_…` (`MissingGlyph U+200B`) | 13 ok; hele `burnburn-text`-suite ok |

## Verificatie

Zie het eindrapport van de lane voor de uitkomst van `cargo fmt --all`, `cargo clippy --workspace --all-targets --locked -- -D warnings`,
de Windows-clippy en `cargo xtask verify`; de uitkomst staat hieronder ingevuld na de laatste run.

## Handmatig / onbevestigd

- **Handmatig**: uiterlijk en gevoel van de beschermstand (omtrekken en knoopmarkeringen op het blad, kleur en dikte van A/B), de OK-knop
  van het meervoudige `NSOpenPanel` en de Windows-dialoog `IFileOpenDialog` met `FOS_ALLOWMULTISELECT` (alleen type-check en clippy voor
  `x86_64-pc-windows-msvc`; nooit gedraaid), de nieuwe lensvelden in beide hosts.
- **Open**: Devanagari zelf (geen permissief font), kashida-*uitvulling* (er is geen uitvulling; alleen getypte tatweel is getest), de
  systeemfontlijst in `burnburn-runtime` kijkt niet naar `.ttc`-bestanden, een correctie die op B niet bindt kan niet opnieuw worden
  gelinkt vanuit de vergelijking, beschermde hoeken staan niet in een lokale correctie die op een andere drempel loopt dan de basis
  tenzij ze binnen 3 px van een hoekpunt van die drempel liggen, de compile-schaalbewaking meet de lineariteit van het plan en vertrouwt
  voor de planning op de tellertests in `burnburn-cam`.
- **Buiten mijn lane geraakt (additief)**: `burnburn-editor` (tellers in `selection.rs`, `scene.rs`, nieuw `work.rs`, `Overlay::{CompareOutline,
  ProtectedOutline}`), `burnburn-render` (tekenen van `ResultPreview`, `CompareOutline`, `ProtectedOutline`; `Theme.compare_a/b`),
  `xtask` (verify-stappen), `test-data/fonts`.
