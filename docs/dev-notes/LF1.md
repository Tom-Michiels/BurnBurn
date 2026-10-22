# LF1: layout-invarianten en quick wins Q1, Q3, Q6, Q9 (look-and-feel)

Basis `84bf89a`. Alle commando's met `CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/lf1 CARGO_INCREMENTAL=0
BURNBURN_NATIVE_QUIET=1` (stille vensters, nooit `--native`). Screenshots alleen offscreen via `gui_snapshots`.

## Test

`tests/native/macos/layout_invariants.rs` (harness = false, stil, `[[test]] layout_invariants` in `crates/burnburn-macos/Cargo.toml`).
Hij loopt door het documentvenster (leeg, met selectie, gecompileerd), elk venster/paneel uit de gedeelde lijst
`tests/native/macos/common/windows.rs` (`steps()`, ook gebruikt door `gui_snapshots`) en alle gebouwde dialoogsheets
(`WindowParts::built_sheets`, ook als ze niet open zijn). Elk venster wordt gecontroleerd zoals geopend én na vergroten
(+120 x +80 pt, alleen als het venster `Resizable` is):

- (a) elk zichtbaar control ligt binnen de bounds van zijn container;
- (b) geen twee zichtbare controls van één container overlappen (tolerantie 1 pt);
- (c) geen label of knop is afgekapt (wrap: hoogte; anders breedte; bewust afkappen vraagt een tooltip);
- (d) een scrollview met controls begint bovenaan zijn inhoud;
- (e) de standaardknop (Return, via `defaultButtonCell`) staat rechts in zijn rij; een sheet antwoordt op Esc en, met een knoppenrij, op Return;
- (f) een lange tekst staat niet twee keer in één venster (een reden wordt één keer gezegd);
- (g) een bewerkbaar veld heeft een caption ernaast of erboven, of een placeholder;
- plus de regressietest `check_transform_fields_in_view`: X, Y, Breedte en Hoogte van de inspecteur zijn in beeld bij het openen.

Niet gedaan: de basislijncontrole uit het plan (label en veld ±1 pt). Zie "Open".

## Red/green

| Test | Verwachte fout (red) | Red-commando en waargenomen resultaat | Green-commando en resultaat | Refactor |
|---|---|---|---|---|
| `layout_invariants` (a)-(d) | inspecteur en panelen verschoven, knoppen afgekapt (Q1, Q9) | `cargo test -p burnburn-macos --test layout_invariants --locked` op `84bf89a` + alleen de test: `FAILED, 474 layout violations` (X/Y/B/H op y 1368..1544 in een kolom van 1183, 3x3-raster over "Machine", panelen 13..21 verschoven, ~30 afgekapte knoppen) | zelfde commando na de fixes: `ok. 1 passed` | `cargo fmt`, clippy -D warnings schoon |
| Q1 `setAutoresizesSubviews(false)` op de met de hand gelegde views (inspecteurkolom, G5-inhoud, Schikken, vormenpaneel, tekstformulieren) | dubbele verschuiving | na alleen Q1: 146 violations (van 474), alle overlappingen in de inspecteur en panelen weg | zie boven | – |
| (e)(f)(g) + sheets, tegen de oude paneelcode | Export/Bundle primair links, import/tracer/Beeld aanpassen zonder Return/Esc, 10x dezelfde reden in de focustest, laserstap- en framesnelheidveld zonder label | `layout_invariants` met de oude `cam_dialog.rs`, `export.rs`, `bundle.rs`, `import.rs`, `tracer.rs`, `laser.rs`, `cut_settings.rs`, `image.rs` van `84bf89a` (tijdelijk teruggezet, log `scratchpad/lf-lf1/red-oldpanels.log`): `FAILED, 92 layout violations`, onder andere `cam-focus-test: the same text 10 times`, `laser: editable field without caption or placeholder` (2x), `export-svg: the default button is not rightmost`, `sheet import: 0 buttons answer Escape`, `sheet tracer: 0 buttons answer Return` | zelfde commando met de nieuwe code: `ok. 1 passed` | – |
| `cam_dialogs::test_dialogs_make_a_private_job_and_focus_test_is_greyed_with_its_reason` (aangepast, Q6) | de reden stond naast elk veld | de oude test eiste de reden per veld; na de Q6-wijziging `FAILED: the field is greyed with its reason: ""` (volledige `cargo test -p burnburn-macos`) | test aangepast: reden één keer in `reasons` (nieuw in `CamPanelParts`), rij zonder eigen notitie; `cargo test -p burnburn-macos --locked --no-fail-fast`: alle testbinaries ok (de paniek in `recovery_offer` is de bedoelde gesimuleerde callback-paniek) | – |

## Wat er veranderd is

- **Q1**: `setAutoresizesSubviews(false)` op de inspecteurkolom (`g5.rs embed_stack`), de G5-panelen, Schikken, het vormenpaneel en `textform`. De inspecteur scrolt na het tonen naar boven (`window.rs`).
- **Production-strip**: 32 pt hoger (`LIFT`), swatches 19 pt pitch (alle 32 passen), "Voeg toe voor selectie" op een eigen rij, cameraknoppen op twee regels, camerakolom rechts vastgepind, laagtabel boven vastgepind. Lock-checkbox onder het 3x3-raster over de volle breedte.
- **Q3**: Export en Bundel: Sluit links, primair rechts. Import (vectorbestanden), Beeld aanpassen en tracer: Esc en Return toegevoegd.
- **Q6**: `cam_dialog.rs`: unieke redenen één keer boven de velden (`reasons`), de voetnoten onder het laatste veld (de blokken onderaan hadden 3 in plaats van 4 marges, waardoor ze over de laatste rij lagen). Snij-instellingen: opeenvolgende rijen met dezelfde notitie tonen hem één keer.
- **Q9**: laservenster: stapgrootte en framesnelheid hebben een caption met eenheid (de bestaande teksten zeggen "(mm)" en "(mm/s)"); het venster is 590 pt breed, scrolt (inhoud 928 pt, venster hoogstens 700 pt) en is vergrootbaar.
- Knoppen en teksten die te smal waren: voorkeuren, materiaal- en kunstbibliotheek, cam-dialogen (knoppen 146 pt, pijlen 34 pt, caption 272 pt), barcode, schets, afstel-sheet, Snij-instellingen.
- Hergrootbare vensters: `button()` rekt niet meer mee in breedte; sneltoetsen en RD-inspectie houden onder- en rechterrij op hun plaats.

## Screenshots (offscreen, `gui_snapshots`, licht, NL)

Voor: `/private/tmp/claude-501/-Users-tom--claude-spares-spare-7SCiVl/b7379f7e-55e7-4285-a7c2-e3f3d2728135/scratchpad/lf-lf1/before/`
Na: `.../scratchpad/lf-lf1/after/` (zelfde bestandsnamen). Bekeken: `light-nl-03-selection-naamloos.png` (inspecteur), `light-nl-29-cam-focus-test-focustest.png` (reden één keer boven de velden).

## Open / afwijkingen

- Basislijncontrole (d uit het plan) is niet gebouwd; labels en velden delen de basislijn niet gemeten.
- Afwijking: de lijst "voor elk venster" bevat de 33 menu-stappen plus de gebouwde sheets; sheets die een bestand nodig hebben worden ongeopend gecontroleerd (tekst nog leeg, dus geen (c) op gewrapte tekst daarin).
- Een niet-geflipte kolom behoudt na vergroten en weer verkleinen zijn ankerpunt linksonder; de test scrolt dan terug. Echte oplossing: LF7 (NSStackView of geflipte documentview).
- Het laservenster is 700 pt hoog begrensd, niet op de werkelijke schermhoogte (NSScreen niet beschikbaar in de features). Handmatig te beoordelen op een klein scherm.
- Windows: niet geraakt; de Windows-hosts kennen deze vaste maten niet.
