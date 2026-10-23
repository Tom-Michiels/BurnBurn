# LF7: documentvenster (look-and-feel, golf LF-B) en drie LF-A-restpunten

Basis `cb2166b`, branch `worktree-agent-ade842ab0ff6dd060`. Commando's met `CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/lf7
CARGO_INCREMENTAL=0 BURNBURN_NATIVE_QUIET=1` (stil, nooit `--native`). Screenshots offscreen via `gui_snapshots`:
voor `scratchpad/lf-lf7/before/`, na `scratchpad/lf-lf7/after/` (zie `light-nl-05-compiled-naamloos.png`).

## Wat er is
- **Panes**: `NSSplitView` (zijbalk | canvaskolom | inspecteur) met `NSSplitViewDelegate` in `classes.rs`: bij vergroten
  groeit alleen het canvas (`splitView:resizeSubviewsWithOldSize:`), min/max per pane (zijbalk 150-360, inspecteur
  240-420, canvas >= 420 pt). `setHoldingPriority` bleek niet te werken op deze niet-autolayout-split (eerste rode run).
- **minSize** 960 x 620 (content). **Frame en panebreedtes** worden bewaard (`appkit/window_state.rs`, tekstformaat en
  validatie headless in `host-models/src/window_state.rs`) en hersteld bij het openen; alleen als het frame nog op een
  scherm ligt. `HostApp` bewaart standaard niets (`WindowState::Off`): de app zet `use_standard_window_state()`, tests
  `use_window_state_suite(<tijdelijk domein>)`; de echte gebruikersdefaults worden nooit aangeraakt (de test vergelijkt de
  sleutel voor en na).
- **Laagtabel**: kolombreedte = max(vloer, koptekstbreedte + 6) in de taal van de UI, `minWidth` gelijk, geen
  kolom-autoresizing (de tabel kneep de eerste kolom tot 20 pt), tussenruimte 4 pt. Staal (gekleurde laagcel) + naam
  blijven de eerste twee kolommen.
- **Statusbalk** (`appkit/status_bar.rs`, model `host-models/src/panels/status_bar.rs`): coordinaat (na muisbeweging),
  zoom (100 % = ware grootte, 72 pt/inch) en eenheid (voorkeur mm/inch).
- **Een `NSToolbar`** (aanpasbaar, `autosavesConfiguration` alleen voor de echte app): elke lintgroep is een toolbaritem
  (view = de knoppen, label = paneltitel, overflow-menu met de tools van het paneel via `menuFormRepresentation`),
  gevolgd door Uitlijnen en Schets. De eigen lintstrook (`ToolStrip.view()`) en haar hoofdletter-titels zijn weg.
- **LF-A-rest**: `PlanViewModel.compiled_at` (geinjecteerde `attach_clock`), plantitel "Plan van <datum tijd> · <duur>",
  knop "Kopieer id" (tekst op de pasteboard van de host; privé in tests); statusregel met `StatusTone::glyph()` voor de tekst.
- **SF Symbols** (rechtstreeks `NSImage::imageWithSystemSymbolName_accessibilityDescription`, `toolstrip::symbol_image`):
  `chevron.down` (variantpijlen), `arrow.left.and.right` / `arrow.up.and.down` (spiegelen), `square.fill` (laagstaal, in de
  laagkleur getint). Beschrijving = commando- of laagnaam.

## Afwijkingen van het plan
- Standaardvenster **1400 x 820** (plan: 1100 -> 1600): bij 1100 pt is het canvas 640 pt en past de laagtabel (630 pt) niet
  naast de bedcamera (250 pt). De vergroottest groeit 500 pt vanaf de standaardmaat.
- Geen presentation-intent voor "Kopieer id": het id staat al in het viewmodel en kopieren is host-I/O.
- Het laagstaal is nog de gekleurde cel (geen cirkel van 12 pt): een cel-gebaseerde tabel tekent geen cirkel; vraagt een
  view-based tabel (apart pakket).
- `setFrame:display:` dwingt `contentMinSize` niet af (alleen gebruikersacties doen dat); de test controleert de waarde en
  dat de panes op de minimummaat binnen hun bereik blijven.
- Wijzigingen buiten de lane (additief): `appkit/app.rs` (accessors, pointer, state), `appkit/mod.rs` (2 regels),
  `appkit/clipboard.rs` (`write_text`), `Cargo.toml` macos (features `NSUserDefaults`, `NSFontDescriptor`,
  `NSLayoutConstraint`, `NSTableHeaderCell`, `NSScreen`), `host-models` (`panels/plan.rs`, `panels/status_bar.rs`,
  `window_state.rs`, `lib.rs`, `panels/mod.rs`), `presentation` (`view_model.rs`, `controller*.rs`),
  `tests/native/macos/panels_and_preview.rs` (titelverwachting).

## Tests, rood en groen
| Test | Rood (echt gedraaid) | Groen |
|---|---|---|
| `presentation/tests/lf7_plan_time.rs::a_compiled_plan_carries_the_moment_of_its_compile_from_the_injected_clock`, `::without_a_usable_clock_…` | eerste faalde: `left: None right: Some(JobClock{..})` (stub `compiled_at: None`); de tweede slaagde al tegen de stub (correct gedrag) | `cargo test -p burnburn-presentation --test lf7_plan_time`: 2 geslaagd |
| `macos/tests/lf7_plan_title.rs::the_plan_title_names_the_compile_moment_…` | `Voorbeeld van het plan` (assert op "Plan van 18-06-2026 05:04 · ") | 2 geslaagd |
| `macos/tests/lf7_status_bar.rs` (4) | 4 gefaald tegen een stub met lege teksten | 4 geslaagd (onafhankelijk bepaalde waarden: 72/25,4 voor 100 %, vaste camerapunten) |
| `host-models/tests/lf7_window_state.rs` (5) | 3 gefaald tegen een stub (`to_text` leeg, `parse` None, zichtbaar altijd waar); de twee "wordt verworpen"-tests slaagden vacuüm tegen de stub | 5 geslaagd |
| native `tests/native/macos/lf7_document_window.rs` (`[[test]] lf7_document_window`, stil) | opeenvolgend: (a) `object list changed width: [199.5, 639.0, 259.5] -> [290.5, 930.0, 377.5]`; (c) `header "Snelheid (mm/s)" needs 89 pt in a 64 pt column; ... the columns need 663 pt, the table shows 508 pt`, daarna kolom 0 gekneld tot 20 pt; (d) `coordinate after the move: ""`; (f) `status line "" lacks the error symbol`; `symbols: flip 0: no symbol image; ...` | `test lf7_document_window ... ok` |

Opmerking: de native test stopt bij de eerste fout. Alleen de rode run van (a) is gezien tegen de stubs
(`use_window_state_suite` no-op, `panel_view` None); de scenario's (b) frame/minSize en (e) toolbar zijn nooit apart rood
gedraaid, omdat de implementatie van (a) en de toolbar/staat in één stap werd geschreven. Dat is een zwakke plek in het
bewijs: (b) en (e) hebben alleen een groene run.

## Verificatie
`cargo fmt --all`, `cargo clippy --workspace --all-targets --locked -- -D warnings` schoon; native `layout_invariants`,
`tool_strip`, `panels_and_preview`, `shape_tools`, `inspector_transform`, `lf7_document_window` groen (stil);
Windows `clippy --target x86_64-pc-windows-msvc` schoon en `cargo test -p burnburn-windows` groen.

## Open / handmatig
- Gevoel van toolbar en lint met de gebruiker (plan: handmatig); het overflow-menu van niet-passende groepen.
- Toolbar-aanpassing (palet) en autosave van de configuratie alleen met de echte app te zien.
- Windows: statusbalk met coordinaat/zoom/eenheid en opgeslagen venster (`host-models::window_state` is gedeeld) zijn niet
  gebouwd (LF14); "Kopieer id" en de plantitel gebruiken `plan_panel`, dus de titel komt vanzelf op Windows.
- Laagstaal als cirkel (view-based tabel); standaard zoom naar het bed (LFB-rest).
