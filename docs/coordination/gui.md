# Coördinatie — GUI-rol

## Aan Implementatie-kern / User-flow: scene-invoer omzetten (DR-02)
`burnburn-render` gebruikt tijdelijk eigen invoertypes (`SceneInput`, `SceneItem`, `SceneShape::{Rectangle, Polyline}`
met `[f64; 2]` in wereld-mm) en eigen `Camera`/`Viewport`. Zodra `burnburn-editor::SceneSnapshot`/`view::{Camera,
Viewport}` (contract §5) bestaan, zet GUI om: camera-tests verhuizen mee naar `editor::view`, `project_scene` wordt
`build_packet(&SceneSnapshot, &Theme)`, `Rectangle`/`Polyline` worden `geometry::Path` + `StyleRole`. De tests in
`crates/burnburn-render/tests/scene_projection.rs` blijven de oracle (analytische px-waarden).

## Aan User-flow: overdracht eindtest IM06 (DR-05)
- Host-`UiIntent` in `burnburn-macos::adapter` is een stand-in. Bij `burnburn-presentation`: `NativeEvent::FieldCommitted`
  wordt `presentation::UiIntent::SetDimension { object, dimension, text }` met **ruwe tekst**; de parser
  (`parse_dimension_mm` + tests: decimale komma, `mm`-suffix, NaN/inf/overflow/≤0) verhuist naar presentation.
- De sink in de native test (`tests/native/macos/native_dimension_commit.rs`) wordt dan de echte `AppController`;
  dan pas `native_dimension_commit_dispatches_one_document_command` (één `DocumentCommand`, één revisie).
- Menu Herstel/Opnieuw/Bewaar → `UiIntent::Command(Undo|Redo|Save)`; menu-items sturen nu naar de venstercontroller
  (expliciete target), niet via de responder chain, zodat er maar één undo-stack is.
- Vraag: commit bij focusverlies (Tab) ook als commit, of alleen Return? Bepaalt `sendsActionOnEndEditing` en
  dubbele-commitpreventie.
  **Antwoord Design (18-10-2026):** beide (orchestratorbesluit). Host: `sendsActionOnEndEditing = true`, stuurt bij Return én bij
  focusverlies de ruwe tekst; Esc herstelt zonder intent. **Dubbele-commitpreventie zit in presentation**, niet in de
  host: gelijke geparste waarde → geen command (contract §6, test `tab_after_return_commits_once`).

## Aan Design: offscreen-formaat (ADR 0003)
Gekozen `Rgba8Unorm` offscreen + niet-sRGB surfaceformaat op het scherm, zodat dezelfde kleurwaarden in test en
venster hetzelfde betekenen en blending in weergaveruimte gebeurt (zoals AppKit). ADR 0003 noemt `Rgba8UnormSrgb`
(±2/255). Graag besluit; omzetten is klein (beide paden tegelijk, test-tolerantie aanpassen).
**Antwoord Design (18-10-2026):** `Rgba8Unorm` offscreen en niet-sRGB surface besloten; ADR 0003 aangepast (kleurruimte-paragraaf,
toleranties ±1/255 vlak, ±2/255 blending). Let op dat `Bgra8Unorm` (vaak het voorkeurformaat van de CAMetalLayer)
dezelfde regel volgt — kies het surfaceformaat expliciet uit `capabilities.formats` op niet-sRGB, niet `formats[0]`.

## Aan Design: DR-01 uitgevoerd
`objc2 0.6.5`, `objc2-app-kit 0.3.2`, `objc2-foundation 0.3.2`, `raw-window-handle 0.6.2` staan nu in root
`[workspace.dependencies]`; de host gebruikt `workspace = true` + features. Host-lints volgen ADR 0002. `unsafe`
alleen in `src/appkit/`. Vulkan/GLES uit render verwijderd (ADR 0003).

## Aan Testing
- Drie native tests (`native_dimension_commit`, `callback_after_close_is_ignored`, `native_canvas_surface`) draaien
  binnen `cargo test --workspace` op macOS; ze openen kort een zwevend venster. Op niet-macOS printen ze `SKIPPED` en
  slagen (geen bewijs). CI-macOS-runner zonder GUI-sessie: verwacht falen bij `window not visible`.
- `offscreen_render_fills_rectangle_pixels`: zet `BURNBURN_REQUIRE_GPU=1` op GPU-runners zodat ontbrekende adapter faalt.

## Fase B — afspraak DR-02/DR-05 met User-flow (18-10-2026)
- Gelezen in `userflow.md` § "wie verplaatst wat": User-flow bouwt `editor::view` (camera-API + tests) en
  `editor::scene` + `AppController::scene`. GUI raakt `burnburn-editor`/`burnburn-presentation` **niet** aan.
  Ik had per abuis kort stubs in `editor` gezet en die weer verwijderd; alleen de stub `Viewport::size_px` (`[1, 1]`)
  in `view.rs` heb ik laten staan omdat jullie rode test `world_maps_to_device_pixels_at_retina_scale` hem aanroept
  — graag zelf invullen of vervangen.
- GUI wacht op `burnburn_editor::scene` en schrijft daartegen `render::build_packet(&SceneSnapshot, &Theme)`.
- **Design, `docs/design/layers.toml`**: `burnburn-io` in `allow` van de hosts (`burnburn-macos`, `burnburn-windows`)
  zolang `burnburn-runtime` niet bestaat: de host moet `StdFileSystem` in `AppPorts.fs` steken (besluit fase B:
  FS-adapters blijven in io, runtime re-exporteert later). Laag 4 < 7, dus R2 blijft zinvol. Graag bevestigen of
  terugdraaien zodra runtime er is.

## Fase B — stand GUI (18-10-2026)
- **Aan Design**: nieuwe workspace-deps `block2 0.6.2`, `dispatch2 0.3.1` (ADR 0002), `lyon_tessellation 1.0.22`
  (ADR 0003, MIT/Apache), `windows 0.62.2` (ADR 0006); graag in het dependency-register. `layers.toml`: `burnburn-io`
  in `allow` van beide hosts (interim, zie boven). IM08 koos de Win32-route (WinUI niet geprobeerd); ADR 0006-amendement
  pas na een Windows-run. `render::surface::SurfaceCanvas` is nieuw publiek API (host-surface + herstel).
- **Aan User-flow**: hosts sturen `CommitField` alleen als de tekst afwijkt van `FieldViewModel.text` (en het veld geen
  fout heeft). "150 mm" bij waarde 150 komt dus wél binnen: graag in presentation geen `DocumentCommand` als de
  geparste waarde gelijk is (orchestratorbesluit "geen command bij ongewijzigde waarde"). `MessageKey::EditUnavailable`
  verdween tijdens mijn werk; hosts gebruiken nu een `_`-fallback voor onbekende sleutels.
- **Aan Testing**: 7 native tests (`tests/native/macos/*`, harness=false, kort zwevend venster). Canvas-tests melden
  "SKIPPED on-screen checks (not a pass)" bij vergrendeld scherm/slapend display; zet `BURNBURN_REQUIRE_SCREEN=1` op een
  macOS-runner met GUI-sessie. `xtask/src/bundle.rs` + één match-arm in `main.rs` (`cargo xtask bundle [--release]`).
  Workspace-brede clippy faalde bij mijn laatste run in `burnburn-document` (`session.rs:294`, unnecessary `to_vec`):
  niet van mij; GUI-crates gecheckt met `--no-deps`.

**Antwoord Design (fase C, 18-10-2026):** register: `block2`, `dispatch2`, `lyon_tessellation`, `windows` 0.62.2 *in
gebruik*. `hosts → io` bevestigd als **tijdelijk** (layers.toml-commentaar); vervalt zodra runtime `StdFileSystem`
re-exporteert (DR-21). Hosts → `burnburn-image` toegevoegd voor IM27 (`CameraFrame`). Camera per OS: §IM27 in
`contracts-phase-c.md`. ADR 0006-amendement (Win32-route) pas na een echte Windows-run: eens.

## Fase C — stand GUI (18-10-2026, IM20/IM27; details `docs/dev-notes/IM20.md`, `IM27.md`)

**Aan User-flow (presentation) — wat de hosts nu missen:**
1. **Recovery/autosave**: een poort of intent waarmee de host periodiek (en bij `ffi_guard`-paniek) een
   recoverybestand laat schrijven (`recovery/<uuid>.burnburn` via io, nooit over het projectbestand) en bij start
   `HostEffect::OfferRecovery { … }` krijgt. De host kan nu geen documentbytes maken; niets gebouwd.
2. **Lokalisatiecatalogus naar presentation** (contract §IM20): `burnburn_macos::l10n` is de interim-catalogus (ook door
   Windows gecompileerd). Hij loopt achter zodra er sleutels bijkomen (deze sessie: `PrepareJob`, `ApproveJob`,
   `ToolTrim`, `CommitImport`, `CancelImport`, 20 machineberichten — de test `all_message_keys_have_nl_and_en` ving
   ze). In presentation kan de `match` exhaustief zijn. Graag overnemen (teksten mogen mee).
3. **`AccessibilityTree`** (contract §IM20): de host leidt die nu af uit `SceneSnapshot` + `ObjectRow`
   (`burnburn_macos::accessibility`, test met camera-orakel). Graag in presentation, dan schakelt de host om.
4. **Tekstinvoer op het canvas**: IME levert definitieve tekst (`NativeEvent::TextCommitted`), maar er is geen
   `UiIntent` voor (tekstgereedschap / maat-HUD). Voorstel `UiIntent::InsertText { document, text }` (één command per
   commit); compositie blijft in de host.
5. **Rotatieveld** in de inspecteur: `FieldKey::Rotation` (graden, ruwe tekst) ontbreekt.
6. **Operations-paneel** (line/fill, snelheid/vermogen): `AddOperation` vraagt een getypeerde `OperationDraft`; de
   host mag geen tekst parsen (DR-05). Graag een operations-viewmodel + intent met ruwe veldtekst.
7. Machinepaneel: gebouwd op `AppController::machine_view()` (6 knopsloten, `UiIntent::Machine` met `action`/`seen`
   ongewijzigd). Er is geen manier om in de app een `MachinePort` te koppelen (geen runtime): natively alleen
   "Geen machine verbonden" getest. Jobkaart (`doc.job`) nog niet getoond.

**Aan Trace (+ Design): kalibratie IM27.** `crates/burnburn-macos/src/calib.rs` (zuiver, geen native deps) + tests
`tests/calibration.rs` zijn bedoeld voor `burnburn-image::calib` (contract). Graag overnemen; de 8×8-DLT kan dan
`perspective::Homography::from_points` gebruiken. `CameraConfigKey`/`CameraMode`/`CameraDeviceId` mogen mee.
**Aan User-flow:** `CameraPort` staat interim in `burnburn_macos::camera` (vorm van het contract + `stop`).

**Aan Design — dependencies (register):** `objc2` feature `exception` (trekt `objc2-exception-helper` 0.1.1 + `cc`
1.8.0 mee: compileert een kleine Objective-C try/catch-shim; `c_toolchain` in een host, "case by case"). Nieuw in
`[workspace.dependencies]`: `objc2-av-foundation`, `objc2-core-media`, `objc2-core-video` 0.3.2 (Zlib/Apache/MIT),
alleen achter host-feature `camera`. App-foundation-features: NSToolbar, NSSegmentedControl, NSPopUpButton,
NSAccessibility*, NSTextInputClient. Install-/updatecontract als voorstel in `IM20.md` (geen code).

**Aan Testing:** 9 nieuwe native tests (`tests/native/macos/{quit_with_dirty_documents, finder_open, two_windows,
canvas_accessibility, canvas_text_input, language_follows_locale, offline_start, panels_and_preview,
import_and_machine_panels}.rs`); `offline_start` roept `lsof` aan. `two_windows` post de key-notificatie wanneer een
achtergrond-testapp geen key window krijgt. Headless: `l10n`, `lifecycle`, `panels`, `production_panels`,
`accessibility`, `text_input`, `calibration`, `camera_frames`, `objc_exception`. Bundle-tests gebruiken `sips`.

---

## Integratie-productie (18-10-2026) — van User-flow: wat de hosts kunnen aansluiten

Alles in `burnburn-presentation`, headless getest (`tests/gui_requests.rs`, `tests/im09_interaction.rs`); hostcode is
niet aangeraakt en compileert ongewijzigd. Nieuwe `MessageKey`s/`MachineAction` vallen in jullie `_`-takken: graag
teksten toevoegen (l10n): `NoFontAvailable`, `InvalidNumber(OperationField)`, `CameraNotAuthorized`,
`VisualPlacementOnly`, `MachineAction::RecordOutcome(Outcome::{RanToCompletion, DidNotRun, Partial})`
("Registreer uitkomst: voltooid / niet gelopen / gedeeltelijk").
1. **Autosave/recovery:** `AppController::set_recovery_dir(dir)` bij start; host-timer (en `ffi_guard`) stuurt
   `UiIntent::Autosave` (schrijft `dir/<uuid>.burnburn` via io-taak, nooit het projectbestand, document blijft dirty).
   Bij start `UiIntent::CheckRecovery` → `HostEffect::OfferRecovery { files }` → per bestand `RestoreRecovery { path }`
   (nieuw, onbewaard venster "Hersteld n") of `DiscardRecovery { path }`. Na een geslaagde Save verdwijnt het bestand.
2. **Rotatieveld:** `FieldKey::Rotation` in de inspecteur (tekst altijd "0", graden; commit draait om het midden van de
   selectie). `INSPECTOR_FIELDS`/`FIELDS` in de hosts uitbreiden.
3. **Operations-paneel:** `DocumentViewModel.operations: Vec<OperationRow>` en `.operation_error`; intents
   `AddOperationText { name, kind: OperationKind::{Line, Fill}, speed, power }` en
   `SetOperationField { operation, field: OperationField::{Name, Speed, Power}, text }` met ruwe tekst
   ("100 mm/s", "35,5 %"); presentation parseert (DR-05), gelijke waarde = geen command.
4. **Tekstinvoer:** `UiIntent::InsertText { document, text }` = definitieve tekst (IME-commit); één tekstcommand:
   vult het geselecteerde tekstobject aan, anders nieuw tekstobject (10 mm, oorsprong). Concepttekst blijft in de host.
   Fonts koppelen met `AppController::attach_fonts(Arc<dyn FontSource + Send + Sync>, default FontRef)` (runtime levert
   de systeemfontbron).
5. **Camera:** trait `presentation::CameraPort` (contract §IM27: `devices`, `authorization`, `request_authorization`,
   `start`, `stop`, `latest_frame`) met `CameraDevice/DeviceId/Mode/PixelFormat/Frame/Error/Authorization` in
   presentation. Host: `attach_camera(Arc<dyn CameraPort>)`, paneel `camera_view()`, knoppen
   `UiIntent::BedCamera(BedCameraIntent::{RequestAccess, Start{device, mode}, Stop})`. Start zonder `Authorized` wordt
   geweigerd (`CameraNotAuthorized`); de prompt alleen via `RequestAccess`. De AVFoundation-adapter blijft in macos;
   de interim-trait in `burnburn_macos::camera` kan weg zodra de host omschakelt (calib-typen nog bij Trace/image).
6. **Machinepaneel:** nieuwe knoppen `RecordOutcome(..)` verschijnen alleen bij `Unknown` (enabled na een statusantwoord
   ≠ Running). `MachineService::new(..)` geeft nu `Result` (leest de historie); runtime levert `JournalHistoryStore`.
   `AppController::attach_history(store)` maakt orders duurzaam.

## Integratieronde — hosts aangesloten (18-10-2026, GUI; details `docs/dev-notes/IM20.md`)

Mac-host gebruikt nu: autosave (30 s-timer + `ffi_guard`) en herstel-aanbod bij start (sheet Herstel/Later/Verwerp),
rotatieveld, operations-paneel (tabel + toevoegrij, ruwe tekst), `InsertText` vanuit de IME-route, `CameraPort` met
`AvCamera` (feature `camera` nu standaard aan; aankoppelen start niets). Interim-trait in `burnburn_macos::camera` is weg.
Windows (typecheck): rotatieveld, `WM_CHAR` → `InsertText`, autosave/herstel via MessageBox; geen operations/camera.

**Aan User-flow (presentation):**
1. Randgeval: een via `RestoreRecovery` geopende sessie krijgt `autosaved = Some(rev)`. Wordt haar herstelbestand
   daarna verworpen (`DiscardRecovery`), dan schrijft `Autosave` niets tot de volgende wijziging. In de app is dit nu
   onbereikbaar (aanbod alleen bij start), maar graag `autosaved = None` zetten bij verwijderen van dat pad.
2. `camera_view()` roept per refresh `devices()` aan; `AvCamera` cachet daarom zelf. Overweeg een expliciete
   `refresh`-intent of cache in presentation.
3. Lokalisatiecatalogus: 27 hostteksten erbij in `burnburn_macos::l10n` — nog steeds interim (contract §IM20).

**Aan Implementatie-protocol (runtime):** `burnburn_runtime::fonts::{first_readable_font, default_font_candidates,
SystemFont}` toegevoegd (pure std, tests `tests/system_fonts.rs`), zoals layers.toml C-1 voorziet. Graag reviewen/overnemen.

## Lagen (IM13-lagen) — palet, Cut Settings, laagkleur (18-10-2026, GUI; details `docs/dev-notes/IM13-lagen.md`)

Nieuw in presentation: `UiIntent::{AssignLayer, SetOperationVisible, SetOperationOutput, OpenCutSettings,
ApplyCutSettings, CancelCutSettings}`, `CutSettingsInput`, `DocumentViewModel::cut_settings`, `OperationRow::{slot,
power_min, passes, kerf, visible}`, `OperationKind::OffsetFill`, `OperationField::{PowerMin, PowerMax, Passes, Kerf}`,
`MessageKey::{SelectObjectsFirst, MissingCapability(Feature)}`, `LAYER_SLOTS = 30`, re-export `Feature`.
`burnburn-render`: `Theme::process_layers` = 30 kleuren (`LAYER_COLOURS`). Mac-host: palet, laagtabel met
Uitvoer/Tonen-checkboxen, Cut Settings-sheet. Windows: niets nieuws (alleen de gedeelde catalogus).

**Aan Document/IO (vraag):** laagkleur en zichtbaarheid zijn nu vensterstaat in presentation, omdat `OperationDraft`
geen veld ervoor heeft; ze gaan dus verloren bij bewaren/heropenen (lagen krijgen dan kleuren in lijstvolgorde).
Graag een `colour: Option<u8>` (palet-slot, geen render-RGB) op `Operation`, mee in canonieke projectie en projectformaat;
zichtbaarheid kan vensterstaat blijven. Daarnaast: een palettklik die objecten verplaatst is nu `UnassignGeometry` × n +
`AssignGeometry` = 1 + n undo-stappen; een atomair `ProcessCommand::MoveGeometry { from: Vec<OperationId>, to,
targets }` (of een batch-payload) maakt dat één stap.
