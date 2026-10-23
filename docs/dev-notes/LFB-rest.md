# LFB-rest: open LF-A-restpunten

Lane LFB-rest, golf LF-B. Alle commando's met `CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/lfbrest CARGO_INCREMENTAL=0`.

## 1. Zoom naar het bed
- `CommandKey::ZoomBed` ("Zoom naar bed"), Weergave-menu Mac (`app.rs`) en Windows (`menus/view.rs`); zonder machine een no-op.
- Een nieuw of geopend document kadert het bed (`Session::fit_pending`): direct met de standaardviewport en opnieuw bij de eerste
  echte `CameraIntent::Resized`; pan/zoom of een zoomcommando wist de vlag. Zonder machine blijft de standaardcamera (bewust: content
  kaderen brak de hit-tolerantie van de fillet-tests, regressietest `without_a_machine_an_opened_document_keeps_the_default_camera`).
- Tests `crates/burnburn-presentation/tests/zoom_bed.rs` (6). Rood: `cargo test -p burnburn-presentation --test zoom_bed`: 4 mislukt
  ("the 1500 x 900 bed is visible: -200 -150 200 150"), later 1 mislukt (standaardcamera, 27,27 pt/mm) na de eerste implementatie. Groen: 6/6;
  `-p burnburn-presentation` volledig groen.

## 2. Vulling per Fill-bewerking
- `SceneItem::fill_slot: Option<u16>` (editor), gezet in `layers::style_scene` voor de eerste zichtbare Fill/OffsetFill-bewerking;
  `Theme::layer_fill_alpha` (0,22 licht, 0,28 donker, 0 in draadmodel); render tekent de tint onder de omtrek.
- Tests `burnburn-render/tests/lfb_layer_fill.rs` (3, incl. offscreen-pixeltest; slaat over zonder GPU) en
  `burnburn-presentation/tests/layer_fill_scene.rs` (2). Rood: render 3 mislukt (geen Fill-primitive, pixel 0 vs 31); scene 2 mislukt
  (`fill_slot` None). Groen na implementatie; `burnburn-render` en `burnburn-presentation` volledig groen.

## 3. Laservenster op de schermhoogte
- `laser_window_height(scherm, inhoud)` en `laser_window_top_left(...)` in `host-models/src/panels/laser.rs` (puur); macOS gebruikt
  `NSWindow::screen().visibleFrame()` (feature `NSScreen`) in `toggle_laser`.
- Tests `burnburn-host-models/tests/lfb_laser_window.rs` (6): rood (stub 700 / ongewijzigd), dan groen. Native guard in
  `tests/native/macos/laser_window.rs` (paneelframe binnen visibleFrame), groen met `BURNBURN_NATIVE_QUIET=1`.
  Handmatig: het gedrag op een echt klein scherm.

## 4. Windows
- (a) Icoon: geen resource-crate in de lock; `crates/burnburn-windows/build.rs` + `build/ico_res.rs` schrijven zelf een `.res`
  (RT_ICON + RT_GROUP_ICON id 1, geen dependencies) en geven het via `cargo:rustc-link-arg-bins` aan de MSVC-linker; de vensterklasse
  laadt het icoon (`LoadIconW`). Test `tests/exe_icon_resource.rs` (3) leest de `.res` met een onafhankelijke lezer en vergelijkt met de
  `.ico`; rood: groepsrecord ontbrak (stub). **Niet uitgevoerd**: de PE van een gelinkte exe controleren (vraagt een Windows-runner).
- (b) `appearance.rs` (`AppsUseLightTheme` -> `Appearance`, `WM_SETTINGCHANGE` "ImmersiveColorSet"), `win32` leest het register
  (feature `Win32_System_Registry`) en kiest `Theme::dark()`; test `tests/appearance.rs` (4), rood (stub), groen.
- (c) `status_line.rs` (glyph voor waarschuwing/fout + kleur van `StatusTone`, `WM_CTLCOLORSTATIC`); test `tests/status_line.rs` (4),
  rood (stub zonder toon), groen; contrast >= 4,5:1 op `COLOR_BTNFACE`.
- Typecheck + clippy `--target x86_64-pc-windows-msvc --all-targets -D warnings`: schoon. De Win32-laag zelf is niet gedraaid.

Screenshots: `scratchpad/lf-lfbrest` (snapshottool heeft geen machine, dus geen bed/vulling te zien; de laservenster-PNG toont de
volledige inhoud op deze schermhoogte).
