# Plan: BurnBurn naar LightBurn-pariteit (18-10-2026)

Doel: elke functie uit het LightBurn-dossier (`research/lightburn/01-menu-inventaris.md`, `02-features.md`,
`03-instellingen.md`, `06-dialoogcatalogus.md`) is in BurnBurn **bedienbaar in het venster**, getest volgens AGENTS.md,
op macOS en (later) Windows. Bovenop LightBurn blijven de BurnBurn-extra's (sketches/constraints, varianten, historie).

Uitgangssituatie (`docs/user-flows/story-coverage.md`): 74 stories, 0 volledig, 33 deels, 30 alleen domein, 11 gepland.
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

**Golf 3**: resterende panelen (bibliotheken, variabele tekst, nesting, camera-overlay, voorkeuren/hotkeys),
Windows-host op pariteit (L8), acceptatieplan `research/lightburn/05-acceptatieplan.md` doorlopen.

**Golf 4**: G-code- en galvo-backends, prestaties (100k objecten), installer/updates, echte Windows-run.

## Volgen

Na elke golf: `cargo xtask verify`, `story-coverage.md` + matrix bijgewerkt (Haiku), status in `docs/status.md`,
review van de diffs door de plannende sessie vóór de volgende golf.
