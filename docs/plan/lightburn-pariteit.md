# Plan: BurnBurn naar LightBurn-pariteit (18-10-2026)

Doel: elke functie uit het LightBurn-dossier (`research/lightburn/01-menu-inventaris.md`, `02-features.md`,
`03-instellingen.md`, `06-dialoogcatalogus.md`) is in BurnBurn **bedienbaar in het venster**, getest volgens AGENTS.md,
op macOS en (later) Windows. Bovenop LightBurn blijven de BurnBurn-extra's (sketches/constraints, varianten, historie).

Uitgangssituatie op 18-10-2026 (`docs/user-flows/story-coverage.md`): 74 stories, 0 volledig, 33 deels, 30 alleen domein, 11 gepland. Stand op 22-10-2026 na golf 6: 43 groen (software), 30 deels, 1 domein, 0 gepland (zie de sectie "Golf 6 (uitgevoerd)" en "Wat overblijft").
De kern heeft al veel algoritmen; **de flessenhals is de UI-route** (presentation-viewmodel + intent + native paneel) en
**machinebewijs** (geen geverifieerd Ruida-wire-dialect: upload/start worden geweigerd).

## Werkwijze voor agents

- **Lanes met bestandseigendom.** Elke lane schrijft alleen in haar crates. Een domeinlane die UI nodig heeft, schrijft
  een regel in `docs/plan/integratie-wachtrij.md` (feature, domein-API, gewenste intent/viewmodel, story-test); de
  UI-lane pakt die op. Gedeelde bestanden (`burnburn-macos/src/l10n.rs`, `presentation/src/{controller,intent,view_model}.rs`,
  `macos/src/appkit/{window,app,classes}.rs`) zijn van L1, tot die gemodulariseerd zijn (W1-L1a).
- **TDD per AGENTS.md**: rode fase observeren en loggen in `docs/dev-notes/<pakket>.md`; geen hardware-contact; native
  tests `harness = false`; handmatige checks als handmatig melden.
- **Build-isolatie**: elke sessie zet een eigen `CARGO_TARGET_DIR=target/<lane>` (geen lock-strijd); tijdens het werk
  crate-gerichte tests; `cargo xtask verify` aan het eind. Faalt verify in de crate van een andere lane: melden, niet fixen.
- **Modellen**: Sonnet voor implementatie (domein en UI), Haiku voor mechanisch/leeswerk (matrix, l10n-teksten,
  coverage/doc-updates, fixtures). Opus (deze sessie) plant, reviewt, integreert en verifieert.
- **Klaar per pakket** = feature bedienbaar in `./target/debug/burnburn`, story-test groen, coverage bijgewerkt, verify groen.

## Lanes

| Lane | Eigendom | Model |
|---|---|---|
| L1 UI-integratie | `burnburn-presentation`, `burnburn-macos`, `burnburn-render` | Sonnet (1 tegelijk tot L1a klaar, daarna 2–3 op aparte modules) |
| L2 Geometrie & tekentools | `burnburn-geometry`, `burnburn-editor`, `burnburn-sketch` | Sonnet |
| L3 CAM & optimalisatie | `burnburn-cam`, `burnburn-job`, procesmodel in `burnburn-document/src/process*` | Sonnet |
| L4 Machine & laservenster | `burnburn-ruida`, `burnburn-machine`, `burnburn-runtime`, `burnburn-history` | Sonnet |
| L5 Beeld, trace, tekst, generators | `burnburn-image`, `burnburn-trace`, `burnburn-text`, `burnburn-generators` | Sonnet |
| L6 Bestanden | `burnburn-io` (import/export/projecten/bibliotheken) | Sonnet |
| L7 Inventaris & docs | `docs/plan/*`, `docs/user-flows/story-coverage.md`, l10n-tekstvoorstellen | Haiku |
| L8 Windows-host | `burnburn-windows` | Sonnet (vanaf golf 3) |

## Featurefamilies → pakketten

Stand: **D** = domein bestaat, **UI** = bedienbaar, **–** = ontbreekt. Pakket-ID's zijn `P<golf>-<lane><letter>`.

| LightBurn-familie | Stand | Wat nog moet | Pakket |
|---|---|---|---|
| Werkruimte: zoom/pan/fit, wireframe/filled, grid, rulers, guides | UI deels | fit-to-page/selection, weergavemodus, linialen, hulplijnen | W2-L1b |
| Selectie: gebied, omkeren, per laag, open paden, ingesloten, lock/hide | UI deels | selectiefilters en -commando's, lock/hide, selecteer-per-laag | W2-L2a + L1 |
| Transform: X/Y/B/H/rotatie, %-schaal, aspectlock, shear, 3×3-referentie, Move/Size/Rotate-locks | UI deels | %-schaal, aspectlock, shear, referentiepunt, spiegelen | W2-L1c |
| Groepen, break apart, auto-group, undo-historie | D | UI-commando's, historiepaneel | W2-L1c |
| Basisvormen (driehoek, veelhoek, ster, …) parametrisch | – | vormen + parameterpaneel | W1-L2b |
| Lijnen/pen/nodes: break, join, close, smooth/corner, curve↔line, trim/extend | D/UI deels | knoopbewerkingen in werkbalk + node-contextmenu | W1-L2a → L1 |
| Offset, weld/booleans, cut shapes, round corners, optimize shapes, start point/direction, tangent circle, warp/deform, resize slots, measure | D deels | ontbrekende algoritmen + werkbalk/dialogen | W1-L2a, W2-L2c |
| Align/distribute/dock/move together | D/UI deels | dock, move together, referentiekeuze | W2-L2a |
| Grid/circular array, copy along path, virtuele arrays | D (cam/array) | dialogen met preview | W2-L1d |
| Nest selected / quick nest | D | nestingpaneel als taak | W3-L1 |
| Tekst: font, bold/italic, uitlijning, spatiëring, welded, op pad (distort), convert to path, SHX/RTL | D deels | teksttoolbar, fontkeuze, tekst-op-pad UI, convert to path | W1-L5b, W2-L1e |
| Variabele tekst: serial, datum, CSV, advance, bake | – / D deels | engine + paneel; ProductionRecord-binding | W1-L5c, W3-L1 |
| Barcodes/QR (29 symbologieën) | D deels (qr, barcode) | ontbrekende symbologieën + dialoog | W2-L5d, W3-L1 |
| Beeld: adjust (helderheid/contrast/gamma/scherpte), crop, mask, replace, save processed | D deels | adjust-filters + paneel met live preview | W1-L5a, W2-L1f |
| Image modes: threshold, ordered, Atkinson, dither(FS), Stucki, Jarvis, newsprint, halftone, sketch, grayscale; negative, dot width, pass-through | D deels | ontbrekende kernels, golden images | W1-L5a |
| Trace image (+ multi-file) | D | tracer-paneel, recepten, batch | W2-L1g |
| Lagen C00–C29, T1/T2, sublagen, defaults per device | UI (lagensessie) | tool layers, sublagen-UI, Make Default (for All) | W2-L3a + L1 |
| Line-mode: passes, kerf, tabs/bridges, perforation, lead-in/out, overcut, dot/dwell | D deels | tabs, perforatie, lead-in/out, overcut + Cut Settings-velden | W1-L3a → L1 |
| Fill/offset fill: grouping, flood fill, cross-hatch, ramp, scan angle, bidirectional, overscan | D deels | ontbrekende opties + velden | W1-L3b → L1 |
| Optimalisatie: order by layer/group/priority, inner-first, startpunt, richting, remove overlapping, backlash | D deels | volledige optimalisatie-instellingen + dialoog | W2-L3c |
| Preview: simulatie, tijdschatting, travel tonen, scrubben | D (plan) / UI minimaal | previewvenster met afspelen en klik-naar-bron | W2-L1h |
| Laservenster: start/pause/stop, frame (rechthoek/rubber band), home, go to origin, start from (abs/current/user), 3×3 job origin, cut selected, selection origin | D deels | ontbrekende machineacties + laservenster | W1-L4a → W2-L1i |
| Move/jog, position laser, save/run RD, file list (DSP) | D deels | jog, file-list, pauze/hervat | W1-L4a |
| **Machinebewijs**: verified wire-dialect op echte Ruida (model/firmware/interface) | – | **handmatig met hardware**; vastleggen in `docs/hardware/` | W1-HW (gebruiker) |
| Device settings, rotary (chuck/roller), auto-feed, Z/focus | D deels | profielvenster, rotary-transform, feeder | W2-L4b, W2-L3d |
| Material test, interval test, focus test | – | generators via dezelfde compiler + dialoog | W2-L3e |
| Material library, art library, bundles, presets | – / D deels | bibliotheken met schema-migratie + paneel | W2-L6b, W3-L1 |
| Camera: lens-calibratie, workspace alignment, overlay op canvas, opacity, capture, meerdere camera's | D deels | beeld tekenen op canvas, kalibratiewizards | W2-L5e, W3-L1 |
| Print and cut, center finder | – | registratie-algoritmen + flow | W2-L2d, W3-L1 |
| Import: SVG, DXF, AI/PDF, HPGL/PLT, bitmaps, LBRN2, templates | D deels | PDF/AI, HPGL, meer LBRN2, bitmap-DPI-flow | W1-L6a |
| Export: SVG, DXF, RD, processed bitmap | D deels | exportdialogen | W2-L1 |
| Voorkeuren, sneltoetsen-editor, taal, help, supportbundel, debuglog | – | voorkeurenvenster, hotkeys | W3-L1 |
| G-code-backend, galvo | – | aparte backends (na Ruida-pariteit) | W4 |
| Windows-host op pariteit | typecheck | alle panelen, echte run | W3–W4-L8 |
| BurnBurn-extra: sketches/constraints | D | schetsmodus, constraint-glyphs, paneel | W1-L1b |

## Golven

**Golf 1 (nu, 7 sessies parallel)**
- W1-L1a (Sonnet): presentation `controller.rs` opsplitsen in featuremodules (`controller/{process,sketch,machine,import,camera,…}.rs`)
  en macOS-panelen in losse bestanden (`appkit/panels/*.rs`) — gedrag ongewijzigd, verify groen. **Daarna** W1-L1b:
  schetsmodus met constraints (S01–S04).
- W1-L2a (Sonnet): knoopbewerkingen en geometrie-ops in geometry/editor (break, join, close, smooth/corner, curve↔line,
  round corners, cut shapes, start point/direction, optimize shapes) met editor-tools + wachtrij-items voor L1.
- W1-L2b (Sonnet): parametrische basisvormen (driehoek, regelmatige veelhoek, ster, dubbele ster) in document/editor.
  *Let op*: document-wijzigingen alleen in nieuwe modules; meld enum-uitbreidingen in de wachtrij.
- W1-L3a+b (Sonnet): Line-mode-extra's (tabs/bridges, perforatie, lead-in/out, overcut, dot) en Fill-extra's
  (cross-hatch, flood fill, ramp, scan angle, fill grouping) in cam/job/process.
- W1-L4a (Sonnet): laservenster-domein tegen de fake controller: frame (rect/rubber band), jog/move, home, go to origin,
  start-from-modi, 3×3 job origin, pause/resume, save/run RD, file list. Nooit een echte laser.
- W1-L5a (Sonnet): alle image modes + adjust-filters met golden images; W1-L5b/c daarna: tekstopties, variabele tekst.
- W1-L7 (Haiku): traceerbaarheidsmatrix `docs/plan/lightburn-matrix.md`: elk menu-item/dialoogveld uit het dossier →
  BurnBurn-crate/test/UI-stand (✔/D/–), en de wachtrij `docs/plan/integratie-wachtrij.md` aanmaken.
- W1-HW (gebruiker): echte Ruida aansluiten en het wire-dialect bewijzen (`docs/hardware/`); zonder dit blijft
  uitvoeren geweigerd.

**Golf 2 (na merge golf 1; UI-lane gesplitst nu `controller/` en `appkit/panels/` modulair zijn)**
Elke L1-sessie werkt in eigen modules; gedeelde bestanden (`intent.rs`, `view_model.rs`, `l10n.rs`, menu in
`appkit/app.rs`) alleen additief — de plannende sessie lost append-conflicten op bij de merge.
- W2-L1-tekenen (Sonnet): wachtrij W1-L2a + W1-L2b: payload-mapping `EditCommit::Ops`, knoopwerkbalk (smooth/corner/
  symmetric, lijn/kromme, sluiten/verbinden, break, delete), Round Corners, Cut Shapes, Start Point, richting,
  Optimize Shapes, basisvormtools + parameterpaneel + Convert to Path, undo-labels.
- W2-L1-cut (Sonnet): wachtrij W1-L3a/b + W1-L5a: Cut Settings Line (tabs, leads, overcut, perforatie, dot), Fill
  (scan angle, cross-hatch, flood, grouping, ramp), Image-modus + opties + scherm, capability-uitgrijzen met reden.
- W2-L1-beeld (Sonnet): Adjust Image-paneel met live preview, crop, mask, Save Processed Bitmap, tracer-paneel (F16).
- W2-L1-laser (Sonnet): wachtrij W1-L4a: laservenster (frame rechthoek/rubber band, start-from + 3×3, cut selected,
  jog/move/position laser, set origin, home, pause/resume, stop, positie, save/run RD, file list, diagnose-export),
  inclusief de `MachineService`/`HistoryStore`-aansluiting van live-acties.
- W2-L1-bewerken (Sonnet): Edit-menu en klembord (cut/copy/paste/paste in place/duplicate/delete duplicates),
  tekenvolgorde (push forward/backward/front/back), transform (%-schaal, aspectlock, spiegelen, shear, 3×3-referentie),
  groeperen/break apart, align/distribute-knoppen voor de bestaande `CommandKey`s, recente projecten.
- W2-L2 (Sonnet): align dock/move together, print-and-cut + center finder (registratie), tangent circle, warp/deform.
- W2-L3 (Sonnet): optimalisatie-instellingen (order layer/group/priority, remove overlapping, backlash), material test,
  interval test, rotary-transform; RD-lowering voor cross-hatch, dot mode en ramp (nu getypeerd geweigerd).
- W2-L5 (Sonnet): tekstopties (W1-L5b) en variabele tekst (W1-L5c), ontbrekende barcodes, camera-lenskalibratie.
- W2-L6 (Sonnet): import PDF/AI, HPGL/PLT, bitmap-DPI-flow, meer LBRN2 (W1-L6a, nog niet gestart); bibliotheken.
- W2-L7 (Haiku): matrix + story-coverage bijwerken na de merge van golf 2; l10n-tekstvoorstellen voor nieuwe sleutels.
Beslissingen nodig (gebruiker): scope van tweede laser (S92–S96), OMTech Polar, Trocen PPI, Print (Black Only/Keep
Colors), Cuttle, OS-diensten (M002–M013) — voorstel: buiten golf 2, in golf 4 heroverwegen.

**Golf 3 (na golf 2, main a4bdc4f): de domeinwachtrij van golf 2 bedienbaar + robuustheid**
- W3-UI-schikken (Sonnet): dock, move together, warp 4/deform 16, resize slots, tangent circle, print-and-cut en center
  finder (W2-L2-regels), plus Quick Nest / Nest Selected (paneel als taak, C10).
- W3-UI-cam (Sonnet): Optimization Settings-dialoog, cut-priority per vorm, Material/Interval/Focus Test-dialogen,
  Z-offset in Cut Settings, Device Settings-venster (machineprofiel, rotary, M01/M02/M08) (W2-L3-regels).
- W3-UI-tekst (Sonnet): teksttoolbar (face, uppercase, uitlijning, spatiëring, welded), Convert to Path, tekst op pad
  met Distort, variabele-tekstpaneel met ProductionRecord-binding, barcode-dialoog (W2-L5b/c/d).
- W3-UI-bestanden (Sonnet): HPGL/PDF/AI-import (paginakeuze), LBRN2-import-UI, templates, Material Library-, Art
  Library- en Bundle-panelen, exportopties met verliesrapport (W2-L6).
- W3-UI-camera (Sonnet): camerabeeld als overlay op het canvas (opacity, capture), lenskalibratie- en
  alignmentwizards, kalibratiebeheer, meerdere camera's (W2-L5e, W04).
- W3-UI-app (Sonnet): voorkeurenvenster, sneltoetseneditor, ongedaanmaakgeschiedenis, previewvenster met afspelen en
  klik-naar-bron (C07), "Inspecteer RD", help/over/supportbundel/debuglog.
- W3-robuust (Sonnet): fuzzdoelen bouwen en kort draaien voor PDF/HPGL/LBRN2/klembord/bundles, de 4 resterende
  barcodetypes, golden-images controleren.
- W3-L7 (Haiku): wachtrij ontdubbelen (union-merges gaven oude "open" naast nieuwe "klaar"), matrix en coverage bijwerken.

**Golf 4**: Windows-host op pariteit (L8, in delen: catalogus-toolstrip, lagen/Cut Settings, laservenster, beeld,
bewerken — typecheck + headless adaptertests; echte Windows-run handmatig), G-code-backend (GRBL) met eigen
capability-matrix, acceptatieplan `research/lightburn/05-acceptatieplan.md` als scenario-tests, prestaties (100k
objecten, X04), installer/bundle en updatecontract (X07). Galvo pas na beslissing van de gebruiker.

## Golf 4 (uitgevoerd)

Per lane één regel resultaat (dev-notes in `docs/dev-notes/`). Windows is in deze golf alleen type-gecheckt en door clippy gehaald.

- **W4-L8a** (Windows-host deel A, `IM08-W4`): menubalk als data, formulieren voor inspecteur, lagen, Cut Settings, CAM, Schikken, tekst,
  vormen, schets en toolstrip; paneelmodellen gedeeld met macOS via `#[path]`; `menus.rs` bewijst pariteit met het Mac-menu. Geen Windows-run.
- **W4-L8b** (Windows-host deel B, `IM08-W4b`): laservenster, beeld, bestanden, camera en app-panelen op Windows met dezelfde intents
  (command-ids 6000–7999); camera-poort `UnavailableCameraPort`. Geen Windows-run.
- **W4-L4** (G-code/GRBL, `W4-L4-gcode`): crate `burnburn-gcode` met lowering, capabilitymatrix, statusparser, sessie met
  character-counting, driver, console, macro's en CLI-uitbreiding, bewezen tegen een onafhankelijke interpreter en een fake
  controller. Geen GRBL-controller gecontacteerd, geen seriële adapter, upload/start geweigerd.
- **W4-acceptatie** (`W4-acceptatie`): het acceptatieplan als scenario-tests (`docs/user-flows/acceptatie.md`); één echte bug
  gevonden en opgelost (IM03: Vernieuw laadde na Vervang+Undo het vervangende bestand).
- **W4-prestaties** (`W4-prestaties`): 100k-fixtures F100K-A/B/C; selectie, verplaatsen en scène sterk versneld; geschiedenisbudget
  (de melding in de UI kwam in golf 5); annulering midden in compile en nest getest; de kwadratische compile werd gemeten en doorgegeven.
- **W4-UI-machine** (`W4-UI-machine`): samengestelde commando's (beeld importeren/maskeren = één undo-stap), beeldvoorbeeld op het
  canvas (`Overlay::ImagePreview`), de verouderde laser-wachtrijregels opgeruimd en bevestigd.
- **W4-UI-gaten** (`W4-UI-gaten`): S08 dubbelzinnige auto-constrain, S04 "Herstel laatste geldige", S02 decimale komma en parameter
  verwijderen, S01 bogen, P03 BMP/TIFF, T01, T02 "Pas toe als trace", P01 melding nieuwer schema, E07/E08/E03/E04/E01, P07 profielbibliotheek.
- **W4-release** (`W4-release`, X07): reproduceerbare zip/dmg (ad-hoc ondertekend), MSIX-layout, CI-workflows, updatepoort
  (`decide_update`) en vorige versie bewaren; notarisatie en alles op Windows handmatig, workflows nooit gedraaid.

## Golf 5 (uitgevoerd)

- **G5-cam** (`G5-cam`): compile-schaal (genest index; F100K-C ~92 s → 1,45 s), pad-arrays en Reeks-dialoog (AR02), toollagen T1/T2 (LY01),
  sublagen, preflight-paneel, snijvolgorde, Z per doorgang, laagkleur in het projectbestand, kerf op het canvas, feeder-planning
  (RD/GRBL geweigerd), productienesting (domein), en een echte annuleer-fout in de nest opgelost.
- **G5-UI-gaten** (`G5-UI-gaten`): P02 selectie in undo-records, P04 notities en LBRN2 als één stap, T04 centerline als `TraceCommand`,
  E11 tellers in het projectbestand, E05 offsetvoorbeeld, E09 voor/na repareren, S06/S07/P06 structuurpaneel en plaatkeuze, X03.
- **G5-tekst-api** (`G5-tekst-api`): bidi/RTL (Hebreeuws, TX01 groen), desktop-automatisering over UDP op loopback (API01, 16 tests),
  CLI-machineacties alleen met naam en intent (X05), plugincontract (X06).
- **G5-windows** (`G5-windows`): opstartargument, recente lijsten op schijf, menu's op de juiste plek, toetsenbord/mnemonics/tooltips,
  Media Foundation-camera (alleen type-check). Geen Windows-run.
- **G5-native** (`G5-native`): offset/reparatie, centerline-beslissingen, structuurpaneel, notities en API01 bedienbaar in beide hosts
  via één headless paneelmodel; macOS native getest, Windows alleen formulier/headless.
- **G5-grbl-ui** (`G5-grbl-ui`): Device Settings met Ruida/GRBL, Exporteer G-code, GRBL-preflight en een GRBL-venster met console en
  macro's — alles tegen een nagebootste controller.
- **G5-UI-machine / machine-rest** (`G5-UI-machine`): opslaan met vorige versie, updatepoort bij afsluiten (macOS), `HistoryTrimmed`,
  actief profiel kiezen en bewaren, machinehub (M01/M02/M04/W02/W03/W09) op macOS; Windows-venster ontbreekt.

## Golf 6 (uitgevoerd)

- **G6-editor** (`G6-editor`): ellips in een schets (S01), vertakkingswissel zichtbaar (S05, met een echte bug opgelost), doos/scharnier en
  barcode als generatorobject (E10), zoeken in projecten (P08), X01 offline-bewakers, LOADFILE vervangt het schone venster.
- **G6-werk** (`G6-werk`): beeld voorbereiden (T03), correcties en centerline-beslissingen in het recept (T04/T05), afwijkingsrapport (T06),
  onderdelen en referenties (S06/S07), productienesting in het Schikken-paneel (C12), nesten op een restplaat (W07), operatorweergave (W08).
- **G6-machine** (`G6-machine`): profielbibliotheek bewaart de GRBL-dialect en een host opent een profielbestand, model/firmware als
  gebruikersopgave (M01), goedkeuring duurzaam (W01), kalibratiebewijzen in de compiler voor kerf en scanoffset (W09), startpunt/richting per vorm
  vergrendelen (C06), grayscale-golden en pass-through-waarschuwing (C05).

Stand na golf 6 (main 9335726): `cargo xtask verify` exit 0, 3946 tests geslaagd, 0 gefaald, 11 genegeerd; stories: 43 groen (software),
30 deels, 1 domein, 0 gepland (`docs/user-flows/story-coverage.md`). Niets is op echte hardware of op Windows bewezen.

## Wat overblijft

- **Hardware (W1-HW, eigenaar B-2/B-3):** een geverifieerd Ruida-wire-dialect en het vrijgeven van `ScanAngle`, `GrayscaleModulation`,
  `CommonLine`, `ZStep`; Tab Cut Power, feeder, Y-scan en Z-offset in RD; GRBL op een echte controller met een seriële adapter; echte
  camerabron, lens- en uitlijnwizard op een machine; rotary Y-schaal; scanoffset- en kerfmetingen.
- **Windows:** de hele Win32-laag draaien (nooit gedraaid), het machinehub-venster en de updatepoort, Media Foundation-camera, MSIX,
  de Windows-job in CI; B-1 (Win32 of WinUI 3) pas na een echte run.
- **Release:** notarisatie, installatie op een schone Mac, Gatekeeper; CI en `release.yml` daadwerkelijk draaien; fuzz-doelen draaien (B-10).
- **Software:** zie "Nog open na golf 6" in `integratie-wachtrij.md`: o.a. kalibraties bewaren (W04), herkenning (W06), restenbibliotheek (W07),
  extend/fillet-UI, modifierstapel, platen aanmaken (S07/P06), wijzen in het beeld (T03), plug-inloader (X06, B-5), benchmarks als regressiedrempel.
- **Handmatig:** het uiterlijk van alle panelen uit golf 4–6, OS-dialogen, RTL-weergave, barcode-leesproef, pasteboard tussen apps.
- **Eigenaarsbeslissingen:** `docs/design/open-beslissingen.md` (B-1..B-12), gedrag van Dock/Move Together en Auto-Advance in LightBurn,
  tweede laser, OMTech Polar, Trocen PPI, Print-opties, Cuttle, galvo.

## Volgen

Na elke golf: `cargo xtask verify`, `story-coverage.md` + matrix bijgewerkt (Haiku), status in `docs/status.md`,
review van de diffs door de plannende sessie vóór de volgende golf.
