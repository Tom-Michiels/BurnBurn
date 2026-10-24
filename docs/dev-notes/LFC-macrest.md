# LFC-macrest: open macOS-punten uit golf LF-B

Lane LFC-macrest. Alle commando's met `CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/lfcmac CARGO_INCREMENTAL=0`,
native tests met `BURNBURN_NATIVE_QUIET=1`. Screenshots: `scratchpad/lf-lfcmac/voor` en `na` (42 PNG's, `gui_snapshots`).

## 1. Basislijn: `BASELINE_PENDING` weg
- Mechanisme verwijderd uit `tests/native/macos/layout_invariants.rs`. Rood: `layout_invariants` zonder lijst (met
  `BURNBURN_BASELINE_ALL=1` op de oude code): 10 overtredingen, 3 bronnen: `Prioriteit` (+2,0 pt, `priority.rs`),
  `Stapgrootte (mm)` en `Framesnelheid (mm/s)` (+2,0 pt, `laser.rs`).
- Fix: caption-frames 2 pt lager (`priority.rs`, `laser.rs`). Groen: `layout_invariants ... ok`.
- Afwijking: het laservenster (1100 regels handwerk) is niet naar de formulierbouwer verhuisd, alleen de basislijn is gelijk.
  Open voor een volgende golf.

## 2. Barcode en Hoeken afronden op de bouwer
- Nieuw `host-models/src/panels/dialog_forms.rs` (`barcode_form`, `geometry_dialog_form`, id-modules), Mac: `barcode.rs` en
  `shapes.rs` bouwen met `FormSurface` (additief `update`/`load` in `form_surface.rs`). De widgetstructs (`BarcodeWidgets`,
  `dialog_widgets`) blijven gelijk, dus de bestaande tests hoefden niet te veranderen; ze vinden de widgets nu via de rij-id.
- Tests `crates/burnburn-host-models/tests/lfc_dialog_forms.rs` (5). Rood (stub met lege `FormSpec`): 4 mislukt, de vijfde
  (vorm blijft gelijk) slaagde triviaal met de stub. Groen: 5/5. Native groen: `text_panels_native`, `shape_tools`, `layout_invariants`.
- Eerlijk: een aparte native test dat de sheets uit de bouwer komen is er niet; `layout_invariants` loopt ze wel af
  (inclusief eenheid-als-suffix voor barcode en round-corners).

## 3. Reden per sectie: `FormSection::unavailable`
- `G5Line::Unavailable` / `FormLine::Unavailable` (nieuw), `lines_form` zet de eerste per sectie als `unavailable`. Omgezet:
  structuur "Referentie maken" (geen enkel object geselecteerd), operator (geen machine), herkenning (geen camera).
- Tests `crates/burnburn-macos/tests/lfc_unavailable.rs` (3, een per paneel). Rood: alle drie (`unavailable` was `None`, reden
  stond als rij). Groen na de wijziging; de `texts`-helpers van tien bestaande paneeltests tellen `Unavailable` mee.
- Windows: een regel in `burnburn-windows/src/forms/g5.rs` (de reden blijft daar een tekstregel tot LF13).

## 4. Laagkleur als rondje
- `panels/layer_cells.rs`: `LayerDot` (getekende cirkel, 12 pt), `LayerTableDelegate` (view-based cellen), de laagtabel is
  view-based; velden en vinkjes sturen via `layerFieldEdited:`/`layerToggled:` dezelfde `NativeEvent`s als de oude celbewerking.
  De data-source-route (`setObjectValue`) blijft voor de tests bestaan.
- Bevinding: `reloadData` wist de selectie in een view-based tabel; `layer_palette` werd rood (Snij-instellingen opende niet).
  Fix in `refresh_operations`: selectie herstellen. Dat is regressiewerk gevonden door de bestaande test.
- Tests `tests/native/macos/layer_cells.rs` (celview, cirkel 12x12, pixel: midden = laagkleur, hoek transparant, naam
  read-only, velden bewerkbaar, vinkje en bewerking bereiken presentation). Rood: "no cell view in column layer". Groen.
  `layout_invariants` kreeg `check_layer_table` (een cirkel per rij, 12 pt, links van en gelijk met de naam); mutatie
  (DOT=14) maakte hem rood, teruggezet groen. `lf7_document_window` meet nu de view-cellen.

## Handmatig
Dubbelklik op een laagnaam bewerken en Return, tabnavigatie in de laagtabel, donker thema, VoiceOver op de cellen.
