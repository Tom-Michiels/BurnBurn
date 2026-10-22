# LF4 App-identiteit (+ Q5 About-paneel) — 23-10-2026

Commando's met `CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/lf4 CARGO_INCREMENTAL=0`, `--locked`.

## Ontwerp
- Eigen ontwerp, geen merk of bestaand logo: een vlam (de "burn") boven een laser-snijlijn met focusvonk op een donkere
  squircle-plaat (macOS-iconraster: plaat 824 px op een canvas van 1024, zachte schaduw eromheen).
- Bronnen: `assets/app-icon/burnburn.svg` (app) en `assets/app-icon/burnburn-document.svg` (`.burnburn`-document: blad met
  omgevouwen hoek en de vlam). De SVG's zijn met een eenmalig script (squircle als superellips n=5, 160 punten) geschreven
  en daarna met de hand bijgesteld; het script is geen onderdeel van de repo, de SVG is de bron.
- Rasterizer: **geen resvg/usvg** (usvg is in het register "niet gebruikt"; niets nieuws gedownload). `xtask/src/icon.rs`
  bevat een kleine, strikte SVG-subset (paden M L H V C Q Z absoluut, circle, g met translate/scale, lineaire en radiale
  gradients, opacity, round-cap strokes) bovenop `tiny-skia` (al in lock/register) en `image` (PNG-codering, al in lock).
  Alles buiten de subset is een fout, nooit een stil leeg icoon. Alleen `xtask` krijgt deze twee deps (dev-tool);
  registerregels aangepast.
- Windows: dezelfde SVG, uitgesneden op de plaat (`viewBox 100 100 824 824`) zodat het icoon zijn vierkant vult.

## Gerenderde iconen (bekeken met Read)
Map `/private/tmp/claude-501/-Users-tom--claude-spares-spare-7SCiVl/b7379f7e-55e7-4285-a7c2-e3f3d2728135/scratchpad/lf-lf4/icon/`:
`app-1024.png`, `app-128.png`, `app-64.png`, `app-32.png`, `app-16.png`, `windows-256.png`, `windows-32.png`,
`windows-16.png`, `document-512.png`, `document-64.png` (opnieuw te maken met `cargo xtask icon --preview <map>`).
Beoordeling: 1024/128 verzorgd; 16 px is (door de macOS-marge) klein maar herkenbaar als oranje vlam. De eerste versie
had een te dunne snijlijn en een te kleine vlam op het document; beide bijgesteld.

## Artefacten
- `cargo xtask icon` herschrijft `assets/app-icon/burnburn-512.png` (host-embed) en
  `crates/burnburn-windows/res/burnburn.ico` (16/24/32/48/64/128/256, PNG-entries). Een test houdt ze bit-gelijk aan de bron.
- `cargo xtask bundle`: `BurnBurn.icns` (icp4 16, icp5 32, ic11 32@2x, ic12 64, ic07 128, ic13 128@2x, ic08 256, ic14
  256@2x, ic09 512, ic10 512@2x = 1024) en `BurnBurnDocument.icns`; plist: `CFBundleIconFile` en `CFBundleIconName` =
  `BurnBurn`, documenttype `CFBundleTypeIconFile` = `BurnBurnDocument`.
- `xtask msix-layout`: Square44/150, StoreLogo uit de Windows-uitsnede, `FileIcon.png` = documenticoon.

## Tests, rode en groene fase
Rood (stubs `render`/`png`/`icns`/`ico` die leeg teruggeven; bundle/msix nog met het oude placeholder-icoon):
`cargo test -p xtask --locked icon::` → **10 FAILED, 1 passed**; de ene "passed"
(`committed_icon_files_are_current`) was leeg-gelijk-aan-leeg en is daarna verscherpt met `!is_empty()`.
`cargo test -p xtask --locked -- bundle:: msix::` → 3 FAILED: `plist_names_icon_localisations_and_camera_purpose`,
`bundle_contains_icon_and_language_folders`, `the_logos_are_the_plate_edge_to_edge_and_the_file_icon_is_the_document`.
Native: `cargo test -p burnburn-macos --locked --test app_panels` met `about_panel_options` als stub → `menus_follow_the_register…`
FAILED ("ApplicationName"); `dock_icon_is_the_burnburn_flame_when_unbundled` FAILED zonder `install_app_icon`
("square icon of at least 512 px, got 256 x 256": het generieke icoon), definitief opnieuw rood bevestigd met de hook uitgecommentarieerd.
Groen: `cargo test -p xtask --locked` 88 passed; `app_panels` alle scenario's ok.

Tests (xtask `icon.rs`): `the_macos_canvas_keeps_the_squircle_margin_and_the_plate_is_opaque`,
`the_windows_frame_fills_the_square_edge_to_edge`, `the_plate_is_dark_and_the_flame_is_warm_and_bright`,
`gradients_run_top_to_bottom_and_opacity_blends`, `the_document_icon_is_a_light_page_with_a_folded_corner`,
`unsupported_svg_is_an_error_not_a_blank_icon`, `png_output_decodes_to_the_same_pixels`,
`the_icns_holds_every_size_from_16_to_1024_as_png` (parset de container), `iconutil_unpacks_the_icns_into_an_iconset`
(onafhankelijk orakel: macOS `iconutil`, 10 beelden), `the_ico_holds_the_windows_sizes_as_png`, `committed_icon_files_are_current`;
`bundle.rs`: `app_and_document_icons_are_readable_by_sips`, plist/bundle-tests; `msix.rs`: de logo-test.
Native (`tests/native/macos/app_panels.rs`): `menus_follow_the_register_and_the_debug_log_switch` (nu met de About-opties:
naam, versie, credits, icoon), `dock_icon_is_the_burnburn_flame_when_unbundled`.

## Q5 About
`HostApp::about_panel_options()` bouwt de opties-dictionary (ApplicationName, ApplicationVersion, lege Version, Credits =
platform + offline-belofte in de UI-taal, ApplicationIcon); `show_about` roept `orderFrontStandardAboutPanelWithOptions:`.
De NSAlert-variant (`about_alert`) is vervallen; de offscreen-shot `about` in `gui_snapshots.rs` ook (het standaardpaneel
laat zich niet zonder tonen layouten).
Dock-icoon van een niet-gebundelde run: `install_app_icon` (aangeroepen in `HostApp::new`, in `app.rs`) zet
`applicationIconImage` uit het ingebedde 512-px-PNG; binnen een `.app` doet het niets (daar levert de bundle de `.icns`).

## Open / handmatig
- Het getoonde About-paneel en het Dock-icoon van de gebundelde app zijn niet visueel gecontroleerd (tonen zou het scherm
  van de gebruiker overnemen): **handmatige acceptatie**.
- Windows: het `.ico` staat klaar in `crates/burnburn-windows/res/`; inbedden als exe-resource vraagt een build-script
  (niet-triviaal, buiten deze lane). MSIX-logo's zijn wel gekoppeld. Geen Windows-run gedaan.
- Gebruiker/ontwerper kan het icoon later vervangen: alleen de twee SVG's aanpassen en `cargo xtask icon` draaien.
