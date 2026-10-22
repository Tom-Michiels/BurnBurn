# G5-windows: Windows-host afgewerkt (opstartargument, toetsenbord, menu's, camera, recente lijsten)

Lane `G5-windows`, basis `be65aea`. Alleen `crates/burnburn-windows` (plus `Cargo.lock` voor `serde_json`
in die crate en deze notitie). Geen domeinlogica: alles loopt via dezelfde intents als de Mac. De Win32-laag
(`src/win32/**`, `src/win32_panels/**`) is alleen type-gecheckt en door clippy gehaald, nooit gelinkt of
gedraaid (zie "Handmatig" onderaan).

## Wat er gebouwd is

| Onderdeel | Headless (getest) | Win32 (alleen type-check + clippy) |
|---|---|---|
| Opstarten met bestand | `startup::startup_intents`: `.burnburn`-argumenten -> `Open`, anders `NewDocument`; aanhalingstekens weg, dubbele paden en vlaggen genegeerd (zoals Mac `lifecycle::open_requests`) | `main.rs` geeft `args_os().skip(1)` aan `run_app`; die verstuurt de intents (de MSIX-manifest-koppeling in `xtask/src/msix.rs` bestond al: geen xtask-wijziging) |
| Recente lijsten op schijf | `persist::{load_intents, persist_effect}` over de Mac-`recent.rs` (via `#[path]`): `recent-projects.json` en `recent-templates.json` in `%LOCALAPPDATA%\BurnBurn` | `run_app` laadt ze vóór het eerste venster; `apply` schrijft bij elk `Recent*Changed`-effect. Op Windows werden ook de recente *projecten* nog nergens bewaard (de L8b-notitie nam aan dat L8a dat deed): nu wel |
| Menu's op de juiste plek | `MenuItem::Host(host_panels::menu::Item)`; popups `file` (volledig als Mac, incl. dynamische submenu's Recente templates/projecten), `image`, `production`, `laser` (venster eerst), `window`, `help` in `menus/*`; `Item::ClearRecentProjects` erbij | `win32/menu.rs` bouwt alles uit de tabellen; `append_menus` van L8b is weg |
| Recent-menu's in alle vensters | – | `manager::register_recent_menus/forget_recent_menus`: per documentvenster de twee popups, `refill_recent_menus` vult ze allemaal; items genummerd `&1 naam` |
| Mnemonics | `menus::mnemonics::{assign, reapply, numbered}` (unieke letter per niveau, woordbeginletters eerst, `&` verdubbeld, maximale toewijzing via verplaatsen) | captions in `win32/menu.rs`; `refresh_popup` behoudt de onderstreepte letter bij wijzigende titel (Ongedaan maken: ...) |
| Sneltoetsen | `Accelerator.alt`; Ctrl+Alt+I (selectie omkeren), Ctrl+B Compile, Ctrl+Shift+E Export RD, Ctrl+Shift+I Importeer, Ctrl+Shift+L Laservenster: alle Mac-toetsen (ook `keyed(...)`, `item(MenuItem::..)`) staan in de test | `FALT` in de ACCEL-tabel |
| Tab-volgorde / toetsen | `ControlKind::is_tab_stop`, `PanelSpec::tab_order`, `keys::{control_wants_key, canvas_wants_key}` | `IsDialogMessageW` in de berichtenlus voor frame, formulier- en paneelvensters; `WM_GETDLGCODE` in controls, canvas en edits (Return/pijlen/opname-toetsen blijven van de control, Tab beweegt altijd); `WS_EX_CONTROLPARENT` voor gedockte panelen; Escape (IDCANCEL) sluit een paneel |
| Tooltips | `disabled_reason_at` (panelen), `forms::layout::disabled_tip_at`, `forms::toolstrip::disabled_tip_at` | `win32/tips.rs`: tool per ingeschakelde control + één gebiedstool met callbacktekst; het ouder-venster vraagt bij `WM_MOUSEMOVE` de reden van de uitgeschakelde control onder de muis. Nieuw: tooltips voor de gereedschapsknoppen (stonden alleen als venstertekst) en voor veldhints in formulieren |
| Onkiesbare keuzelijst-items | `forms::choice::ChoiceList`: geblokkeerde items worden niet getoond (het geselecteerde blijft), posities mappen terug; tooltip noemt de geblokkeerde items met reden | `win32/forms.rs` vult/leest de combobox via `ChoiceList` (geen terugspringen meer: een geblokkeerd item kan niet gekozen worden) |
| Camera | `host_panels/camera_source.rs`: subtype -> formaat, modes (fps in duizendsten, dubbelen weg, grootste eerst), `choose_native`, `camera_error`, `AccessState`, `rgb32_frame` (stride, negatieve stride = bottom-up, alpha 255), `DeviceCache` | `win32/camera.rs`: `WindowsCameraPort` (Media Foundation: `MFEnumDeviceSources`, `IMFSourceReader` met video-processing naar RGB32, capture-thread met MTA, `catch_unwind`, wake <= 10x/s via `WM_CAMERA_FRAME`) |

Gekozen afwijkingen/keuzes:
- Windows-only menu-items (de Mac heeft ze elders): Bestandenvenster (laatste in Archief), Beeldvenster (laatste in
  Beeld), Camera (laatste in Laser); Voorkeuren, Sneltoetsen en Over staan onderaan Help (de Mac heeft een
  programmamenu). De vergelijkingstest kent deze lijst (`windows_only`).
- Ctrl+Alt is AltGr op veel toetsenborden; Ctrl+Alt+I is de gevraagde Mac-pariteit (Cmd+Option+I). AltGr+I levert
  op gangbare indelingen geen teken op; op een indeling die dat wel doet wint de sneltoets. Handmatig te beoordelen.
- Geblokkeerde keuzelijst-items worden weggelaten in plaats van grijs getekend (een systeem-keuzelijst kan geen
  enkel item uitgrijzen en owner-draw zou de muis/toetsen-blokkade zelf moeten nabouwen). De reden blijft zichtbaar
  in de tooltip van de keuzelijst en (voor de CAM-keuzes) in de titel van het geselecteerde item.
- De gereedschapsstrook heeft geen tab-stops (ruim 60 knoppen); toetsenbordgebruikers hebben menu en sneltoetsen.
- Windows toont de camerapermissie niet als prompt: `AccessState` is `Authorized` tot een start met toegang
  geweigerd faalt, dan `Denied`; "toegang vragen" wist de weigering en laat de volgende start het opnieuw proberen.

## TDD-log (echt uitgevoerd; `CARGO_TARGET_DIR=target/g5-windows`, toolchain 1.99.1, `--locked`)

Rode fase = gedragsfout na een minimale stub (compilerfouten tellen niet). Commando per rij:
`cargo test -p burnburn-windows --locked --test <bestand>`.

| Test (bestand) | Rood | Groen |
|---|---|---|
| `g5_startup` (6): geen argument, pad, meerdere + dubbel, aanhalingstekens, vlaggen/andere bestanden, geldig tussen genegeerde | 6 van 6 faalden | 6 groen |
| `g5_persist` (6): template-lijst schrijven + terug, project-lijst, wissen, andere effecten, ontbrekend/beschadigd, herstart met echte controller | 5 van 6 faalden (`other_effects_are_not_persisted` slaagde triviaal tegen de stub) | 6 groen |
| `g5_menus` (14): mnemonics (unieke letter, woordbegin, `&&`, geen vrije letter, `reapply`), echte menubalk NL/EN, Productie-popup, Ctrl+Alt+I-label, geen dubbele akkoorden, Host-items per popup, dynamische recent-submenu's, id-roundtrip, Wis recente projecten | 10 van 13 faalden (3 slaagden triviaal: akkoorden-uniek, id-roundtrip, Wis-recent die al in de plumbing zat); `recent_entries_are_numbered...` apart rood (1 van 1) | eerst 12 van 13: dekking van mnemonics in de Schikken-popup was 76% (< 80%): algoritme uitgebreid met verplaatsen van eerdere titels (maximale toewijzing), daarna 14 groen |
| `menus` (8, uitgebreid): alle popups (nu ook image, production, window, help, Windows-only uitgezonderd), elke Mac-toets incl. Option, dynamische recent-submenu's op de Mac-plek | 2 van 8 faalden (popup-vergelijking, sneltoetsen) | 2 aanpassingen aan de test zelf (Mac-bron: `separatorItem`/`App` zijn geen items; de diagnose-ingang staat in de Mac-bron vóór de lijst) -> 8 groen |
| `g5_keyboard` (15): tab-volgorde, tab-stops, edit houdt Return, laser houdt pijlen, opname houdt alles, canvas houdt alles behalve Tab, reden bij uitgeschakeld (paneel/formulier/strook, scroll), `ChoiceList` (mapping, geselecteerd geblokkeerd, tooltip, buiten bereik) | 11 van 13 faalden (2 slaagden triviaal tegen de stub); daarna formulier- en strook-tips: 2 van 2 rood | 15 groen |
| `g5_camera` (15): subtypes, formaten, pack/unpack, modes, geen geldige fps/grootte, `choose_native`, onbekende mode, fouten -> typed, `AccessState`, `rgb32_frame` (padding, bottom-up, ongeldige layout), `DeviceCache` (3) | 11 van 12 faalden (`a_frame_that_does_not_fit...` slaagde triviaal); van de 3 cache-tests is alleen de eerste rood waargenomen (filter), de andere twee zijn samen met de implementatie groen gedraaid | 15 groen |

Typecheck en clippy voor Windows: `cargo clippy -p burnburn-windows --target x86_64-pc-windows-msvc --all-targets
--locked -- -D warnings`: schoon (inclusief de `undocumented_unsafe_blocks`/`multiple_unsafe_ops_per_block`-lints voor
de nieuwe `unsafe` in `win32/camera.rs`, `win32/tips.rs` en de dialoogbeheer-code).

## Handmatig / onbevestigd (alles op echte Windows)

- **Geen enkele Windows-run.** `win32/*`, `win32_panels/*`, `win32/camera.rs`, `win32/tips.rs` zijn niet gelinkt of uitgevoerd.
- Opstarten met pad via Verkenner/MSIX-koppeling; tweede start met ander bestand (nieuw proces, geen single-instance).
- Tab/Shift+Tab door panelen, gedockte panelen en het frame (lijst -> canvas -> gedockte controls); Return in een
  veld (commit), Return op een knop, Escape in een paneel; pijltoetsen op het canvas (nudge) en in het laservenster
  (jog) mogen niet door het dialoogbeheer verdwijnen; opname van een sneltoets (Tab/Esc als antwoord).
- Menu-mnemonics (Alt+letter) en de nummers in de recente lijsten; Ctrl+Alt+I tegenover AltGr op een Belgisch/AZERTY-toetsenbord.
- Tooltips: verschijnen ze op de gereedschapsknoppen en velden, en krijgt het ouder-venster `WM_MOUSEMOVE`/`TTN_GETDISPINFOW`
  boven een **uitgeschakeld** kind (de aanname waarop de redenen bij uitgeschakelde controls rusten)?
- Keuzelijst zonder geblokkeerde items (CAM-dialogen, Cut Settings-modus) en de tooltip daarvan.
- Recente lijsten: bestand verschijnt in `%LOCALAPPDATA%\BurnBurn`, menu's in alle open vensters volgen, Wis-items.
- **Camera (Media Foundation), hardware nodig**: apparaten + modes tonen, privacyschakelaar uit (verwacht `Denied`),
  start/stop, frames in het venster, camera in gebruik door een andere app (`Busy`), camera uitgetrokken tijdens
  streamen (verwacht `DeviceGone`, de capture-thread stopt), stride/oriëntatie van RGB32 (we lezen
  `MF_MT_DEFAULT_STRIDE`; zonder attribuut wordt top-down aangenomen), MJPG/NV12/YUY2-camera's via de video-processor,
  `ActivateObject` op een camera die al streamt (de lijst wordt dan bewaard, niet opnieuw opgevraagd),
  UI-haperingen bij `devices()` (opent elke camera kort; lijst 5 s bewaard).
- De 5-seconden-timeout van `start` en het wekken van de UI (max 10x/s) zijn nooit onder last gemeten.
