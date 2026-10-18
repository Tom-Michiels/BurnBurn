# IM13 Lagen: kleurenpalet, Cut Settings en laagkleur op het canvas (18-10-2026, GUI)

Status: groen op macOS arm64 (headless + native, zie Verificatie). Windows: niet aangesloten (alleen de gedeelde
`l10n.rs` compileert mee). Rol: Implementatie (presentation + Mac-host).

Opdracht: de LightBurn-achtige kern van de productie-UI: operations als gekleurde lagen, objecten toewijzen door een
kleur te klikken met een selectie, een Cut Settings-sheet per bewerking, objecten in hun laagkleur tekenen. Stories
C01, C02, C03, C05 (B10/B12, C8).

## Ontwerp
- **Laagkleur en zichtbaarheid zijn vensterstaat van de sessie** (`controller/layers.rs::LayerState`), geen
  documentdata en geen undo: een `OperationDraft` heeft geen kleur- of zichtbaarheidsveld, en dat toevoegen raakt
  >20 bestanden in document/io/cam/ruida/scenarios (canonieke hash, projectformaat). Gevolg: kleuren worden niet in
  het project bewaard. Een bewerking zonder gekozen kleur (operations-paneel, heropend bestand) krijgt de laagste
  vrije kleur in lijstvolgorde; `layers::sync` legt die na elke wijziging vast zodat latere bewerkingen ze niet
  verschuiven. Coördinatievraag aan Document/IO: zie `docs/coordination/gui.md` § "Lagen (IM13-lagen)".
- **Palet** (`UiIntent::AssignLayer { slot, add }`, 30 kleuren `LAYER_SLOTS`): selectie leeg → `SelectObjectsFirst`,
  geen commando. Bestaat de laag van die kleur → `AssignGeometry` (alleen objecten die er nog niet in zitten);
  anders `AddOperation` "C0n" (Line, 100 mm/s, 20 %, 1 doorgang). Zonder Shift verlaten de objecten eerst hun andere
  bewerkingen (`UnassignGeometry` per bewerking; LightBurn-gedrag "één laag per object"); met Shift blijven ze erin
  (tweede behandeling, C02). De selectie blijft de geklikte objecten.
- **Zichtbaar ≠ uitvoer** (C01): `SetOperationVisible` is alleen vensterstaat (geen revisie, geen undo, alleen
  hertekenen); `SetOperationOutput` is één `UpdateOperation`.
- **Cut Settings** (`OpenCutSettings` / `ApplyCutSettings { CutSettingsInput }` / `CancelCutSettings`): ruwe tekst
  (DR-05) voor snelheid, min/max vermogen, doorgangen, kerf; modus Line/Fill/Offset fill; twee toggles. Alles geldig
  → één `UpdateOperation` (niets bij ongewijzigd) en de sheet sluit; eerste fout → sheet blijft open met
  `CutSettingsViewModel::error`. Volgorde van controle: snelheid, min, max, min ≤ max (`PowerMin`), doorgangen
  (geheel 1–1000), kerf (≥ 0, alleen Line), modus-capability, kerf-capability.
- **Capabilities** (C05, M02): `ModeChoice::blocked` en de OK-controle lezen dezelfde `MachineCapabilities` als de
  compiler (actief profiel, anders `conservative()`): Offset fill vraagt `Feature::OffsetFill`, kerf > 0 vraagt
  `Feature::KerfCompensation`; anders `MessageKey::MissingCapability(feature)` en geen commando.
- **Tabelroute**: `SetOperationField` kent nu ook `PowerMin`, `PowerMax`, `Passes`, `Kerf`; de vermogenkolom van de
  Mac-tabel zet `PowerMax` (zodat een minimum uit Cut Settings blijft).
- **Canvas**: `layers::style_scene` na `build_scene`: object in de kleur van zijn eerste zichtbare bewerking
  (lijstvolgorde) → `StyleRole::ProcessLayer(slot)`; selectie gaat voor; een object waarvan álle bewerkingen verborgen
  zijn wordt niet getekend. `burnburn-render::Theme::process_layers` heeft nu 30 kleuren (`LAYER_COLOURS`); de
  Mac-paletknoppen en de laagkolom gebruiken exact die kleuren (`appkit::window::layer_colour`).
- **Mac-host**: palet (30 randloze knoppen "■" met `contentTintColor`, tooltip/AX-help "C03 — naam (n objecten)" of
  "nog leeg") boven de laagtabel in de productiestrook (strook 250 pt). Laagtabel-kolommen Laag (cel in laagkleur),
  Naam, Soort, Snelheid, Vermogen, Doorgangen, Objecten, Uitvoer ☑, Tonen ☑ (`panels::LAYER_COLUMNS`,
  `NSButtonCell`-checkboxen → `NativeEvent::LayerToggled`). Dubbelklik op een rij of "Snij-instellingen…" →
  `LayerOpened` → sheet met modus-pop-up (`setAutoenablesItems(false)`, geblokkeerde modus uitgeschakeld met
  "— Ontbrekende capability: …"), vijf velden, twee checkboxen, foutregel, Annuleer (Esc) / OK (Return). Velden worden
  één keer per opening gevuld, dus een geweigerde OK houdt de getypte tekst. Shift komt uit de klik-`NSEvent`.

## Cycli
Commando's met `export PATH="$HOME/.cargo/bin:$PATH"`, `--offline`.

| Test | Verwachte fout (red) | Red-commando en waargenomen resultaat | Green | Refactor |
|---|---|---|---|---|
| `colour_click_with_selection_creates_a_layer_and_moves_objects_between_colours` | stubs: handlers no-op | `cargo test -p burnburn-presentation --test layer_palette` → FAILED `left: None right: Some(SelectObjectsFirst)` | passed | — |
| `shift_colour_click_adds_a_second_treatment_to_the_same_geometry` (C02) | idem | FAILED `one command for the second treatment left: 2 right: 3` | passed | — |
| `visibility_and_output_toggle_independently` (C01) | idem | FAILED :145 `Option::unwrap()` on `None` (geen laag) | passed | — |
| `cut_settings_sheet_applies_raw_text_in_one_command_and_keeps_errors_open` | idem, `cut_settings` altijd `None` | FAILED :182 unwrap on `None` | passed | — |
| `kerf_field_sets_line_kerf_in_one_command` (C03) | idem | FAILED :258 unwrap on `None` | passed | — |
| `unvalidated_mode_is_blocked_with_missing_capability` (C05) | idem | FAILED :293 unwrap on `None` | passed | — |
| `objects_are_drawn_in_their_layer_colour_and_hidden_layers_are_not` | scène ongewijzigd | FAILED `left: [(1, Design), (2, Design)] right: [(1, ProcessLayer(4)), (2, Design)]` | passed (alle 7 in één run) | — |
| `layer_table_edits_power_min_passes_and_kerf_as_raw_text` | tabeltak eerst gebouwd: voor de red tijdelijk teruggezet naar `_ => Ok(false)` | FAILED `min > max left: None right: Some(InvalidNumber(PowerMin))` | tak hersteld → 8 passed | — |
| `[render] layer_palette_has_thirty_distinct_opaque_colours` | `process_layers: [Rgba; 4]` | `cargo test -p burnburn-render --test scene_projection` → FAILED `left: 4 right: 30` | passed (8/8); `roles_map_to_theme_styles` wikkelt nu op 30 i.p.v. 4 (bewust aangepast) | — |
| `[macos] all_message_keys_have_nl_and_en` | nieuwe sleutels in `known_message_keys`, nog geen tekst | `cargo test -p burnburn-macos --test l10n` → FAILED `missing translations: [(InvalidNumber(PowerMin), Nl), … (SelectObjectsFirst, Nl), … (MissingCapability(OffsetFill), Nl), …]` | passed (5/5) | — |
| `[macos] layer_panels.rs` (6 tests: palette click/Shift, paletknoppen, laagrijen + toggles, dubbelklik + OK met ruwe tekst, geblokkeerde modus, l10n-sleutels) | stubs: `translate` → `Nothing`, panelen leeg/`None` | `cargo test -p burnburn-macos --test layer_panels` → 6 FAILED, o.a. `left: None right: Some(AssignLayer { slot: 3, add: false })`, `SelectObjectsFirst Nl` | 6 passed | vermogenkolom → `PowerMax`: test eerst aangepast, FAILED `… Some(Power) … right: … Some(PowerMax)`, daarna groen |
| idem `kerf_applies` (assertie in `double_click_opens_cut_settings_and_ok_sends_the_typed_text`) | stub `false` | FAILED `left: [false, false, false, false] right: [true, false, false, false]` | passed | — |
| `[native] layer_palette_and_cut_settings_drive_presentation` (`tests/native/macos/layer_palette.rs`) | widgets gebouwd, palet-action tijdelijk `None` | `cargo test -p burnburn-macos --test layer_palette` → FAILED `plain click assigns, got []` | eerste echte run FAILED `timed out waiting for a redraw` (venster niet op scherm: Metal presenteert niet) → test wacht nu via `wait_on_screen` en slaat de framecheck over als het venster niet zichtbaar is; daarna passed | — |

De native test dekt: 30 paletknoppen met tooltip, klik → `AssignLayer { add: false }` en een laagrij "C01/Lijn/1",
echte Shift-klik (mouse-down/up-`NSEvent`) → `AssignLayer { add: true }` met C01 = 2 en C03 = 1 object bij 2
objecten (C02), Tonen/Uitvoer-checkboxen via de data source los van elkaar (Tonen: geen revisie; Uitvoer: wel)
(C01), Snij-instellingen… → sheet aan het venster met velden 100/20/20/1/0, kerf `0,15 mm` + OK → `ApplyCutSettings`
met ruwe tekst, sheet dicht, opgeslagen kerf 0.15 (C03), profiel zonder Offset fill → derde pop-up-item
uitgeschakeld met "ontbrekende capability", Vulling kiezen schakelt het kerfveld uit, doorgangen "0" + OK → sheet
blijft open met foutregel en getypte tekst, Annuleer → alleen `CancelCutSettings` (C05), en de swatchkleur = de
canvasthemakleur.

## Verificatie
Zie onderaan (ingevuld na de eindrun).

## Handmatig (niet als geautomatiseerd geslaagd melden)
- Uiterlijk van palet, laagkolom en sheet in `./target/debug/burnburn`; de laagkleur op het Metal-canvas met eigen
  ogen. In de native run was het venster niet op scherm ("SKIPPED on-screen checks"), dus geen screenshot/framecheck.
- Echte dubbelklik op een tabelrij (de test gebruikt de knop Snij-instellingen…; de dubbelklik deelt dezelfde action).
- Toetsenbord: Return/Esc in de sheet, VoiceOver op de paletknoppen.

## Onbevestigd / open
- Windows-host: palet, laagtabel en sheet ontbreken daar.
- Kleuren en zichtbaarheid worden niet in het project bewaard (zie Ontwerp); na heropenen krijgen lagen kleuren in
  lijstvolgorde.
- Een palettklik die objecten verplaatst is 1 + n undo-stappen (n = bewerkingen die ze verlaten): er is geen
  samengesteld procescommando. Een nieuwe laag zonder verplaatsing of een Shift-klik is één stap.
- Image-modus en vulinstellingen (interval, hoek) staan nog niet in de sheet; een Image-bewerking houdt haar modus.
- Verborgen objecten zijn nog wel aanklikbaar op het canvas (hit-test kent geen zichtbaarheid).
