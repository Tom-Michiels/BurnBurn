# IM08 W4-L8b: Windows-host, deel B (laservenster, beeld, bestanden, camera, app)

Lane `W4-L8b`. Geen domeinlogica: de Windows-host krijgt dezelfde `burnburn-presentation`-intents en
-viewmodels als de Mac, via de AppKit-vrije paneelmodellen van `burnburn-macos` (`#[path]` in `lib.rs`).

## Opzet

- `src/host_panels/` (headless, op elk OS getest): `spec` (paneel als data: rijen met controls,
  layout, in-place diff, `PanelEvent` met de waarden van alle controls), `ids` (command-ids
  **6000..=7999**: menu 6000..6499 per 100 per familie, controls 7000..7999 per 200 per familie),
  per familie `laser`, `image`, `files` (zes vensters: Start, Import, Export, Material, Art, Bundle),
  `camera`, `camera_port`, `app` (zes vensters: voorkeuren, sneltoetsen, geschiedenis, preview, RD,
  support), `menu` (ingangen van de lane) en centrale `on_file_chosen`/`on_confirmed`.
- `src/app_menu.rs`: Windows-versie van de Mac-`app_menu` (Ctrl i.p.v. Cmd: Ctrl wordt de
  `Cmd`-vlag van het register, weergave `Ctrl+Shift+S`; pijltoets-nudge: Alt = klein, Shift = groot,
  Ctrl+pijl blijft tekstnavigatie). `app_view` (gedeeld Mac-bestand) tekent bindings hiermee.
- `src/win32_panels/` (alleen `cfg(windows)`): `mod.rs` generiek paneelvenster (STATIC/BUTTON/EDIT/
  COMBOBOX/LISTBOX/trackbar/eigen bitmapklasse, in-place patchen, scrollen, subclass voor Return,
  jog-hold, pijltoetsen, toetsopname), `dialogs.rs` (IFileOpen/SaveDialog, bevestiging, bericht),
  `manager.rs` (snapshot van de controller, events -> reacties, menu's, effecten). De host-brug staat
  in `src/win32/panels_bridge.rs`; in `win32/{mod,menu}.rs` zijn alleen enkele aanroepen toegevoegd.
- Camera: `UnavailableCameraPort` (geen apparaten, `Restricted`, start geeft een fout). Er is **geen**
  Media Foundation-bron; dat is niet nagebootst.

## TDD-log (echt uitgevoerd; `CARGO_TARGET_DIR=target/w4-l8b`, toolchain 1.99.1, `--locked`/`--offline`)

Rode fase = gedragsfout na een minimale stub (compilerfouten tellen niet). Commando per rij:
`cargo test -p burnburn-windows --locked --test <bestand>`.

| Test (bestand) | Rood | Groen |
|---|---|---|
| `w4b_spec`: id-blokken, id->familie, layout (gewicht/rijen/rapporthoogte), lege spec, diff Same/Patch/Rebuild, lijstitems, waarden | 5 van 7 faalden (layout leeg, diff altijd Rebuild, id 7999 -> `App`) | 7 groen. Let op: mijn eerste verwachting `control_owner(7999) == None` was fout (7999 ligt in het App-blok); testverwachting gecorrigeerd naar 8000 |
| `w4b_spec::a_control_that_appears_or_disappears_is_a_patch_not_a_rebuild` | `Same` i.p.v. patch | groen na `visible` in diff |
| `w4b_laser` (14): geen machine, ids uniek, positie "onbekend" nooit 0, uit-control met reden, jog press/release/deactivate/close, pijltoetsen, ongeldig getal, Home/frame, setup-intents, ongewijzigde edit, SaveRd, verwijder-bevestiging, diagnose-dialoog, Compile/Export/softstop, verdwenen venster | 11 van 14 faalden (3 slaagden triviaal tegen de stub) | 14 groen |
| `w4b_laser::the_controller_file_list_…` (bestandslijst, "onbekend" grootte, Run uit met reden, Delete vraagt eerst) | **niet rood waargenomen**: na de implementatie geschreven (karakteriseert gedrag dat al bestond) | groen |
| `w4b_image` (11): commando's, bestanddialogen met typen, import-DPI-voorstel, DPI/adjust/trace-events, unchanged = niets | 9 van 11 faalden | 11 groen |
| `w4b_files` (11): ids, `sheet_open`, PDF-pagina's, plotter/SVG-eenheid, export + bevestiging + bewaardialoog, Material, Art, bundel-export, recent/start, template-/bundel-effecten | 10 van 11 faalden | 11 groen |
| `w4b_camera` (7): fake poort, ids, bed-sectie, overlay, kalibratiebadge + weigering, intents uit-knoppen, foto-klik (Y gespiegeld) | 7 van 7 faalden | 7 groen |
| `w4b_camera::the_mark_list_shows_the_mark_the_host_chose…` | `Some(0)` i.p.v. `Some(3)` | groen |
| `w4b_app` (14): menu-ids, enablement, `Ctrl+Shift+S`, toets -> binding, nudge, voorkeuren, sneltoetsen (opname/vervang), geschiedenis, preview (afspelen/seek/klik-naar-bron), RD, support/debuglog, About/handleiding | 14 van 14 faalden | 13 groen, 1 rood: mijn tolerantiegrens (`< 10 mm`) was fout (4 px ≈ 12 mm); testgrens gecorrigeerd -> 14 groen |
| `w4b_menu` (4): ids, titels, enablement, reacties | 4 van 4 faalden | 4 groen |
| `w4b_menu::every_file_purpose_is_answered_by_exactly_one_family` | `OpenProject sends an intent: Nothing` | groen na dispatcher |

Typecheck en clippy voor Windows: `cargo clippy -p burnburn-windows --target x86_64-pc-windows-msvc
--offline --all-targets -- -D warnings`: schoon (na de merge met `integratie/golf-4` opnieuw gedraaid).

## Handmatig / onbevestigd

- **Geen enkele Windows-run**: `win32_panels/*` en `win32/panels_bridge.rs` zijn alleen
  type-gecheckt en door clippy gehaald, nooit gelinkt of uitgevoerd. Te controleren: layout/scroll,
  tab-volgorde, DPI, bitmapweergave, jog-hold (muis en pijltoets), toetsopname in de sneltoetseneditor,
  bestandsdialogen, bevestigingsvenster (de knoppen zijn OK/Annuleer van het systeem; de tekst zegt wat OK doet).
- Tooltips met de reden bij uit-controls: gebouwd met `TTF_IDISHWND`; uitgeschakelde vensters
  ontvangen geen muis, dus de tooltip kan op Windows uitblijven. De reden staat daarnaast in `ControlSpec`.
- Camera: geen echte bron (Media Foundation). Het venster toont de eerlijke toestand "niet beschikbaar".
- Lasercontroller: alles loopt via `MachineService`; hier nooit contact met een machine of netwerk.
- Machineprofiel-/apparaatkeuze: ook de Mac-host heeft geen keuzelijst in het laservenster (de keuze loopt
  via Device Settings, W4-L8a/CAM-dialogen); daarom is er hier geen.
- Recente templates worden niet naar schijf bewaard (de Mac schrijft `recent-templates.json`); de lijst
  leeft per sessie. Recente projecten: persistentie is aan de Bestand-basis (L8a).
- Menu: mijn popups (Laser, Beeld, Bestanden, Camera, Recent, Recente templates, Help) staan als aparte
  popups achter die van L8a (`win32_panels::manager::append_menus`). Verplaatsing naar `menus/file.rs`/
  `menus/laser.rs` vraagt een nieuwe `MenuItem`-variant voor lane-ids; niet gedaan om L8a's menutests
  (vergelijking met de Mac) niet te raken. Bij elk nieuw documentvenster worden de recente-menu's opnieuw
  aangemaakt; alleen het laatste venster werkt ze bij.
- `cargo xtask verify` (vóór de merge met golf 4) faalde alleen op de bekende verouderde barcodetest
  (`macos/tests/text_panels.rs::the_barcode_sheet_lists_29_types…`, 2 i.p.v. 4); gemeld, niet gefixt.
