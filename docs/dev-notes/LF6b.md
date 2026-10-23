# LF6b: bestanden, bibliotheken, export, import en beeld naar de formulierbouwer (look-and-feel, golf LF-B)

Basis `0b517ed`. Alle commando's met `CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/lf6b CARGO_INCREMENTAL=0`, native
tests met `BURNBURN_NATIVE_QUIET=1` (stille vensters, nooit `--native`). Screenshots alleen offscreen via `gui_snapshots` naar
`/private/tmp/claude-501/-Users-tom--claude-spares-spare-7SCiVl/b7379f7e-55e7-4285-a7c2-e3f3d2728135/scratchpad/lf-lf6b/{before,after,after-en}`.

## Wat er is

- `host-models/src/panels/files_form.rs`: de skeletten van acht panelen als `FormSpec` (`export_form`, `bundle_form`,
  `material_form`, `art_form`, `import_form`, `image_import_form`, `image_adjust_forms`, `tracer_forms`) plus de rij-id's per paneel
  (`export::STROKE`, enz.). Eenheid als suffix (`Lijndikte:` + `mm`), bereiken als hulp (`Contrast` + `0…10`), secties met kop, knoprollen
  [annuleer, primair]; de bibliotheekpanelen hebben alleen annuleer (Sluit, ⎋). Het label-zonder-eenheid komt uit de bestaande
  Windows-teksten (`split_caption`), zodat de Windows-teksten ongewijzigd blijven.
- `host-models/src/form.rs`: vier nieuwe controlsoorten: `Note {value, lines, tone, strong}` (wrappende alleen-lezen tekst die `lines`
  regels vrijhoudt), `Report {text, lines}` (alleen-lezen scrolltekst), `Preview {width, height}` (beeld), `Actions {titles}` (knoppen in
  één rij; ids `id`, `id + 1`, …); `FormSpec::row(id)`. `format::Quantity` kreeg `DotsPerInch` ("dpi") en `SquareMillimeters` ("mm²").
- `macos/src/appkit/panels/form.rs`: de bouwer kent de vier soorten, `FormView::{control, caption, set_hidden, set_section_hidden,
  relayout(min_height)}` en zet de toegankelijkheidslabel van een veld uit zijn caption. Lege captions tellen niet meer mee voor de
  captionkolom; zonder captions begint een samengevoegde rij in de controlkolom (de verborgen eerste kolom verborg anders de inhoud).
- `macos/src/appkit/panels/form_surface.rs` (nieuw): `FormSurface` (één of twee formulieren naast elkaar, widgets per rij-id, verbergen,
  venster op maat zetten met behoud van de bovenhoek), `form_sheet`, `form_panel`, `point_at`.
- Gemigreerd: `export.rs`, `bundle.rs`, `material.rs`, `art.rs`, `import.rs`, `image.rs` (bitmap-import en Beeld aanpassen), `tracer.rs`. De
  panelen houden hun widgetvelden en hun `refresh_*`-code (de view-modellen worden zoals voorheen in de widgets gekopieerd, intents
  en `FilesParts`/`AppParts` zijn ongewijzigd); wat veranderde is wie de widgets maakte en legde: `build_form` uit het skelet. Rijen die
  een bestandstype niet heeft zijn verborgen rijen (`set_hidden`) en het venster past zich aan (`fit_window`). Beeld aanpassen en de tracer
  zijn twee formulieren naast elkaar (waarden links, voorbeeld + knoppen rechts). Ongebruikte hulpfuncties (`heading`, `input`, `popup`,
  `checkbox`, `tri_state_checkbox`, `report_view`) zijn weg.
- `FilesText` kreeg 17 sectiekoppen (NL/EN). `BASELINE_PENDING` in `layout_invariants`: `"sheet"` (matcht elk sheet) is vervangen door
  `"sheet cut-settings"` en `"sheet geometry"` (LF6a en LF6c); `material-library`, `art-library` en `export-svg` zijn weg.
- `gui_snapshots` toont nu ook de sheets Import (SVG), Beeld importeren, Beeld aanpassen en Beeld tracen (een gekozen bestand opent ze
  zoals de gebruiker).

## Red/green

| Test | Red (echt gedraaid) | Green |
|---|---|---|
| `host-models panels::files_form::tests::*` (9: `every_file_form_follows_the_form_rules`, export, bundel, materiaal, kunst, import, beeld-import, aanpassen, tracer) | `cargo test -p burnburn-host-models --locked --lib files_form` met `*_form` als stub die `FormSpec::default()` geeft: eerst alleen export `0 passed; 2 failed`, daarna alle acht `FAILED. 0 passed; 9 failed` (o.a. `left: None right: Some("mm")`) | eerste volledige versie `8 passed; 1 failed` (`tracer left: 'Curves (uit = rechte stukken)' carries a unit or a range`: een uitleg in een checkboxtitel); die gaat nu als hulp onder de checkbox; daarna `ok. 9 passed` (hele lib: `37 passed`). De test voor de uitsnede-uitleg (`CROP_NOTE`) is samen met zijn code geschreven, niet apart rood |
| native `layout_invariants` `form_builder_extension_test` (voorbeeld 128×96, rapport ≥ 6 regels, knoppenrij naast elkaar en binnen het formulier, rij en sectie verbergen geeft ruimte terug, `relayout(min)` houdt de knoppen onderaan, een lange notitie laat het formulier groeien) | stubs voor `control`/`set_hidden`/`relayout` en de nieuwe soorten als lege labels: `FAILED, 7 layout violations: form builder: no preview control / no report control inside a scroll view / the button row answers 0 buttons, expected 3 / hiding the report left 274 of 274 pt / hiding a section left 274 of 274 pt / a minimum height of 334 gives 274 / no note control` | tussenstappen (samengevoegde cel rekt zijn inhoud niet: `outside its container (122x24)`; bezel van het voorbeeld +6 pt: `the preview is 134x102, expected 128x96`) opgelost, daarna `ok. 1 passed` |
| native `layout_invariants` met mijn vensters uit `BASELINE_PENDING` | de oude handgelegde panelen, gemeten met `BURNBURN_BASELINE_ALL=1`: 28 basislijnmeldingen (material-library 4, art-library 1, export-svg 2, sheet export 2, import 1, image-import 1, image-adjust 10, tracer 7); met export al gemigreerd: `FAILED, 24 layout violations` (4 + 1 + 1 + 1 + 10 + 7) | na de migratie van alle acht: `layout_invariants ok. 1 passed`, met alleen `sheet cut-settings` en `sheet geometry` nog in de lijst. Tussenstappen: materiaalknoppen afgekapt (`title needs 167 pt of width, has 123`) -> panelbreedte 540 -> 580; voorbeeld buiten de kolom (`outside its container (328x365)`) -> rechterformulier 352 -> 360; alle rijen van het rechterformulier verborgen door de verborgen eerste kolom -> samengevoegde rijen beginnen in kolom 1 |
| native `file_panels_native` (DXF-venster: breedte-rij verborgen, venster op maat) | karakterisering van gedrag dat de oude code ook had (velden verbergen): geen rode fase, eerlijk gelogd | `file_panels_on_a_real_window ok` |

Bestaande tests ongewijzigd groen: `cargo test -p burnburn-macos --locked --no-fail-fast` (EXIT 0; o.a. `file_panels_native`, `image_sheets`,
`import_layers`, `import_and_machine_panels`, `image_panels`, `g5_*`). Geen bestaande testverwachting is aangepast; de panelen kregen wel
andere teksten voor de captions (zonder `(mm)`/`(dpi)`/bereik: die staan als suffix of hulp), maar geen test controleerde die.

## Voor/na (offscreen)

| Venster | Voor | Na |
|---|---|---|
| Materiaalbibliotheek | `before/light-nl-11-material-library-materiaalbibliotheek.png`: labels 2 pt naast de velden, drie knoppen volbreed, geen secties | `after/…-11-…`: vier secties met kop, `Dikte:` + `mm`, knoppen per sectie in één rij, Sluit rechtsonder; EN: `after-en/light-en-11-…` |
| Kunstbibliotheek | `before/…-12-…` | `after/…-12-…`: secties Zoeken / Gekozen item / Toevoegen / Beheren, miniatuur 128 pt, hulp onder de naam |
| Exporteer als SVG | `before/…-32-…`: titel bold, Lijndikte (mm): en Decimalen: op één regel, rapport onzichtbaar (geen rand) | `after/…-32-…`: Opties en Wat verloren gaat, `0.1` + `mm`, rapport met rand, knoppen [Sluit] [Exporteer…] |
| Bundel exporteren | `before/…-33-…` | `after/…-33-…`: sectie "Kies wat in de bundel komt", lege status en rapport verborgen |
| Import (SVG), Beeld importeren, Beeld aanpassen, Beeld tracen | `before/…-38…41-…` (zelfde scènes op de oude code) | `after/…-38…41-…`: twee formulieren naast elkaar voor aanpassen en tracen, `dpi`, `mm`, `mm²` en `px` als suffix, bereiken als hulp |

Een schermafbeelding is geen test; de tests staan hierboven.

## Bewust anders dan het plan

- De panelen gebruiken het `FormSpec`-skelet voor de bouw, niet `FormView::update` voor de waarden: hun `refresh_*`-code (modelwaarden,
  "een veld in fout houdt wat de gebruiker typte", `last_*`-vergelijkingen, tri-state checks) blijft de bron, zodat alle intents en de
  native tests onaangeroerd bleven. Een volledige update-via-spec is een vervolg als het skelet in Windows (LF13) hetzelfde moet tonen.
- "Pas toe als trace" is een knop-in-het-formulier (`Actions`), geen vijfde dialogknop: `DialogButtons` heeft één primaire knop.
- De caption-teksten worden uit de bestaande teksten gesneden (`split_caption`) in plaats van nieuwe sleutels: Windows gebruikt dezelfde
  sleutels nog met eenheid in het label tot LF13.
- Samengevoegde cellen: een notitie, voorbeeld, rapport of knoppenrij zonder caption loopt onder de captions door (samengevoegde rij met
  eigen breedte); een samengevoegde cel rekt zijn inhoud niet over de span.

## Windows

`cargo clippy -p burnburn-windows --all-targets --target x86_64-pc-windows-msvc --locked -- -D warnings` schoon; de specs en de nieuwe
`Control`-soorten staan platformvrij in host-models. De Windows-layout is LF13.

## Open

- Handmatig: de acht vensters naast de HIG-referentie, donker thema, Beeld importeren en Bundel importeren (importmodus) met echte
  bestanden (de importmodus van het bundelvenster heeft nog geen screenshot of native test), Tab-volgorde (LF15), zin- en woordkeuze van de
  nieuwe sectiekoppen.
- `BASELINE_PENDING` bevat nog `sheet cut-settings` (LF6a) en `sheet geometry` (LF6c) en de andere regels van LF6a/c/LF7.
- De Windows-teksten voor de bibliotheek-, export- en beeldpanelen hebben de eenheid nog in het label (LF13).
