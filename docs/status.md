# Uitvoeringsstatus BurnBurn (22-10-2026)

Verificatie op main 9335726 (na golf 6): `cargo xtask verify` (fmt, layers, workspace-deps, clippy -D warnings, tests, doctests)
**slaagt** met exit 0 op macOS 27 arm64 met toolchain 1.99.1: **5200 tests geslaagd, 0 gefaald, 15 genegeerd** (na look-and-feel LF-E; native tests stil); 26 crates in
`crates/*`, 318 707 regels Rust (`wc -l` over `crates/**/*.rs`, inclusief de tests in de crates). Daarnaast is
`cargo clippy -p burnburn-windows --target x86_64-pc-windows-msvc --all-targets -D warnings` **schoon**.
Dat is een typecheck, geen Windows-run: **de Win32-laag is nooit gelinkt of gedraaid**. In de documentatieronde van 22-10-2026 zijn
geen tests gedraaid; deze cijfers komen uit de laatste verificatie van de integratiebranch.

Per werkpakket staat de TDD-log in `docs/dev-notes/` (IM01–IM32, W4-*, G5-*, G6-*). "Gebouwd" = de geplande eerste test en de
kernlevering zijn groen; het betekent niet dat alle acceptatiecriteria van de stories gehaald zijn en niet dat iets op hardware of
Windows is gezien. De werkelijke storystand staat in `docs/user-flows/story-coverage.md`: van de 74 stories zijn **43 groen
(software)** (alle softwarecriteria getest; wat openstaat is handmatig, hardware of een eigenaarsbeslissing), **30 deels**,
**1 alleen in het domein** (T07) en **0 gepland**. Het acceptatieplan als scenario-tests staat in `docs/user-flows/acceptatie.md`;
de LightBurn-traceerbaarheid in `docs/plan/lightburn-matrix.md` en `docs/plan/lightburn-pariteit.md`; de open software-punten
in `docs/plan/integratie-wachtrij.md` (sectie "Nog open na golf 6").

## Pakketten

| Pakket | Stand | Belangrijkste open punt |
|---|---|---|
| IM01 workspace/harness | gebouwd | CI nooit gedraaid (`ci.yml`, `release.yml` staan er, nooit door GitHub uitgevoerd) |
| IM02 eenheden/geometrie | gebouwd | Windows-determinisme (libm-golden) onbevestigd |
| IM03 commands/undo | gebouwd; selectie in undo-records (G5-UI-gaten) | — |
| IM04 opslag/recovery | gebouwd; bewaart de vorige versie bij migratie (G5-UI-machine) | echte stroomuitval, Windows-replace |
| IM05 presentatie/input | gebouwd | — |
| IM06 AppKit-host | gebouwd | handmatig: Cmd-Q-alert, Finder-dubbelklik, uiterlijk van de panelen uit golf 4–6 |
| IM07 GPU-canvas | gebouwd; beeldvoorbeeld als bitmap (W4-UI-machine) | echt surface-/deviceverlies |
| IM08 Windows-proef → Windows-host | headless gebouwd op pariteit (W4-L8a/b, G5-windows); typecheck + clippy voor `x86_64-pc-windows-msvc` | **nooit op Windows gedraaid**; machinehub (`host_panels/hub.rs`, `tests/g5_hub.rs`) en updatepoort (`tests/quit_guard.rs`) headless gebouwd |
| IM09 tekenen/repareren | gebouwd | extend/fillet-UI, curve-trim |
| IM10 import/export | gebouwd; BMP/TIFF (W4-UI-gaten) | SVG `<use>`/tekst, DXF-blocks |
| IM11 sketches | gebouwd; bogen (W4) en ellips (G6) | externe entiteiten; kleine as van een ellips als sturende maat |
| IM12 contourtracing | gebouwd | EXIF-oriëntatie |
| IM13 operations/compiler | gebouwd; toollagen, sublagen, Z per doorgang (G5-cam) | — |
| IM14 Ruida-codec | gebouwd | grayscale-semantiek op een controller (golden uit een Python-referentie, G6) |
| IM15 preflight/preview | gebouwd; preflight-paneel (G5-cam) | acceptatie door een echte controller |
| IM16 transport/actor | gebouwd (fake) | geen geverifieerd wire-dialect; upload/start geweigerd |
| IM17 jobs/historie | gebouwd; goedkeuring duurzaam (G6-machine) | orders kiezen geen vrijgegeven job zonder venster |
| IM18 tekst/raster | gebouwd; bidi/RTL voor Hebreeuws (G5-tekst-api) | Arabisch ongetest, geen SHX, grayscale op hardware |
| IM19 lokale tracecorrectie | gebouwd; correcties in het recept (G6-werk) | beschermde contouren en knooppuntdoel niet in de UI |
| IM20 native afwerking | gebouwd, hosts aangesloten | macOS-uiterlijk handmatig; Windows-run |
| IM21 modifiers/componenten | gebouwd; pad-array (G5-cam), onderdeel uit selectie (G6-werk) | modifierstapel in de inspecteur |
| IM22 generators | gebouwd; doos/scharnier en barcode als object (G6-editor) | barcode-leesproef handmatig; ArUco MIP 36h11 en Grid Matrix niet |
| IM23 centerline | gebouwd; beslissingen in het recept (G6-werk) | stroke-overlay op het canvas |
| IM24 traceerrecepten | gebouwd | eigen recepten bewaren, A/B in de UI |
| IM25 laserprocessen | gebouwd | `Unverified`-features op hardware; Tab Cut Power |
| IM26 nesting | gebouwd; productienesting met spiegelen, platen, rest, orderstatus (G5-cam/G6-werk) | onderdelen in gaten, korrelrichting |
| IM27 camera | kalibratie, camerasectie en cameravenster gebouwd | AVFoundation-adapter nooit automatisch gestart (handmatig); kalibraties niet persistent (W04) |
| IM28 migratie | LBRN2, PDF/AI (paginakeuze), HPGL/PLT, zoeken in projecten (G6-editor) | legacy Illustrator geweigerd; lagen per kleur bij PDF/HPGL alleen als voorstel |
| IM29 calibratie/beheer | software gebouwd; kerf en scanoffset in de compiler (G6-machine) | pointeroffset niet toegepast; USB; hardware |
| IM30 CLI | gebouwd; machine-acties alleen met naam en intent (X05), pluginschema (X06) | geen plugin-loader (B-5) |
| IM31 orders/operator | gebouwd; operatorweergave (G6-werk) | tweede proces/IPC (B-8) |
| IM32 kleurtracing | gebouwd | common-line snijden zelf niet gebouwd |
| W4-L4 G-code/GRBL (`burnburn-gcode`) | gebouwd: lowering, capabilitymatrix, parser, sessie met character-counting, driver, console, macro's, CLI; GRBL bedienbaar in het Mac-venster (G5-grbl-ui) | **geen enkele GRBL-controller gecontacteerd**, geen seriële adapter; start/upload geweigerd; fake uit de publieke beschrijving |
| W4-L8a/b, G5-windows Windows-host | menubalk, formulieren, laser/beeld/bestanden/camera/app-panelen, toetsenbord, recente lijsten, opstartargument, Media Foundation-adapter (alleen type-check) | alles op Windows handmatig |
| W4-release (X07) release/installeren | dist-zip/dmg reproduceerbaar (ad-hoc ondertekend), MSIX-layout, workflows, updatepoort bij afsluiten (macOS en Windows), vorige versie bewaren | **notarisatie en MSIX (`makeappx`, ondertekenen, installeren) handmatig**; geen updater (B-9) |
| W4-prestaties (X04) | 100k-fixtures, selectie/verplaatsen/scene; compile-schaal opgelost in G5-cam (F100K-C ~92 s → 1,45 s) | benchmarks `#[ignore]`, één machine onder belasting |
| W4-acceptatie | `docs/user-flows/acceptatie.md` als scenario-tests; één bug (IM03) gevonden en opgelost | RD-corpus en transportcases zonder hardwarebank |
| G5-cam | reeksen, toollagen, sublagen, preflight, feeder-planning, productienesting (domein) | Tab Cut Power; feeder naar RD/GRBL |
| G5-native / G5-UI-gaten | offset/reparatie, centerline-beslissingen, structuurpaneel, notities, API01 aangesloten | API01 niet met echte LightBurn-clients getest |
| G6-editor / G6-werk / G6-machine | generatorpaneel, zoeken, ellips; beeld voorbereiden, restplaten, operatorweergave; profielbibliotheek + GRBL, M01, W01, W09, C05/C06 | zie de lijst hieronder |

Integratiepoorten A–D draaien headless groen (`burnburn-presentation/tests/gate_*`, `burnburn-scenarios`).
Open eigenaarsbeslissingen: `docs/design/open-beslissingen.md` (B-1..B-12).

## Open punten (eerlijk)

- **Geen verified Ruida-wire en geen verified GRBL-wire op hardware.** Beide zijn alleen getest tegen nagebootste controllers
  (`ScriptedPeer`, een uit de publieke beschrijving nagebouwde GRBL-fake). Upload en start blijven getypeerd geweigerd; een ACK
  geldt nergens als bewijs. `Unverified`: `ScanAngle`, `GrayscaleModulation`, `CommonLine`, `ZStep`, Y-as-scan en scanhoek in RD,
  Tab Cut Power, feeder naar RD/GRBL, controllerinstellingen lezen/schrijven. Geen scanoffset- of kerfmeting op een machine.
- **Nooit op Windows gedraaid.** De Win32-laag (`src/win32/**`, `src/win32_panels/**`) is alleen type-gecheckt en door clippy
  gehaald. Machinehub en updatepoort bestaan voor Windows, maar alleen headless getest. De Media Foundation-camera is nooit gedraaid.
- **CI nooit gedraaid.** `.github/workflows/ci.yml` en `release.yml` zijn lokaal structureel gecontroleerd, maar nooit door GitHub
  uitgevoerd; acties staan op major-tags. Fuzz-doelen compileren maar zijn niet gedraaid (nightly + `cargo-fuzz`: B-10).
- **Notarisatie en MSIX handmatig.** Installatie van dmg/zip op een schone Mac, Gatekeeper, notarisatie, `makeappx`, ondertekenen,
  installeren en SmartScreen zijn niet gedaan.
- **Handmatig niet bekeken (macOS):** het uiterlijk van de panelen uit golf 4–6, OS-dialogen (open/bewaar, het afsluit-alert),
  RTL-weergave, barcode-leesproef, DXF in een CAD-programma, echte AVFoundation-sessie.
- **Software nog open:** zie de sectie "Nog open na golf 6" in `docs/plan/integratie-wachtrij.md` (o.a. Tab Cut
  Power, kalibraties bewaren, herkenning, restenbibliotheek, plug-inloader, extend/fillet-UI, benchmarks als regressiedrempel).
