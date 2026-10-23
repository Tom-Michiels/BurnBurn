# Plan: look and feel van de hele GUI (22-10-2026)

Doel: BurnBurn moet ogen als een volwassen native Mac-app en straks als een even nette Windows-app. De lat is
"LightBurn-klasse, maar met de afwerking van een Apple- of Affinity-app". Dit plan bouwt nog niets aan het product.
Het levert de diagnose, een ontwerpsysteem en werkpakketten met de tests die eerst geschreven moeten worden.

Basis: `integratie/golf-9` op `09f6a9d`. Het eerdere look-plan `docs/plan/solidcraft-look.md` is uitgevoerd en
gemerged (`golf/solidcraft-look`, commit `6c483d9`). Dat leverde de icoonwerkbalk, de toolcatalogus, `burnburn-icons`
(SVG naar tiny-skia, kleurrollen licht/donker) en de sketch-tekenset. Dit plan bouwt daarop voort. Het raakt de
toolcatalogus alleen waar dat nodig is voor stijl.

## Hoe er gekeken is

- **Offscreen-screenshots (macOS).** `tests/native/macos/gui_snapshots.rs` (werkpakket LF0) is een opt-in native test.
  - Hij start de host stil: alpha 0, click-through, nooit actief. Er komt niets op het scherm en de focus wordt niet
    overgenomen.
  - Hij tekent een rechthoek, ellips en lijn, voegt een bewerking toe en compileert met het fixture-dialect. Er wordt
    nooit een laser gecontacteerd.
  - Daarna opent hij ongeveer 35 vensters, panelen en sheets via het menu. Elk venster gaat naar PNG via
    `cacheDisplayInRect:toBitmapImageRep:` op de frameview, dus met titelbalk en toolbar.
  - Het Metal-canvas tekent niet in zo'n cache. Daarom rendert de tool dezelfde scène met
    `burnburn_render::gpu::render_offscreen` en legt die op de plek van de canvasview.
  - Aanroep:

    ```
    BURNBURN_GUI_SNAPSHOT_DIR=<map> BURNBURN_GUI_SNAPSHOT_THEME=light|dark BURNBURN_GUI_SNAPSHOT_LOCALE=nl|en \
      cargo test --locked -p burnburn-macos --test gui_snapshots
    ```

    Zonder `BURNBURN_GUI_SNAPSHOT_DIR` meldt de test "ignored" en maakt hij geen bestanden.
- **Resultaat.** 156 PNG's (39 per variant: licht/donker × NL/EN) in
  `/private/tmp/claude-501/-Users-tom--claude-spares-spare-7SCiVl/b7379f7e-55e7-4285-a7c2-e3f3d2728135/scratchpad/gui-review/`.
  - Bestandsnamen: `<thema>-<taal>-<nn>-<stap>-<titel>.png`, bijvoorbeeld `light-nl-03-selection-naamloos.png`.
  - Hieronder heet een screenshot kortweg `NN-stap`, bijvoorbeeld `03-selection`. Het nummer is gelijk in alle vier
    varianten.
- **Code.** De macOS-host, `burnburn-host-models`, `burnburn-render`, `burnburn-presentation` (teksten en getallen),
  `burnburn-icons`/`assets/icons` en de Windows-host zijn regel voor regel nagelopen.
- **Beperkingen.**
  - Windows is alleen uit de code beoordeeld: er is geen Windows-runner.
  - Op de Mac waren niet offscreen te zien: systeempanelen (NSOpen/NSSavePanel), het menu zelf, tooltips,
    hover/focus-staten, de default-knopkleur (een niet-key-venster tekent geen accentkleur) en live-resize.
  - Het canvas in de composiet is herbemonsterd, dus aliasing is daar iets verzacht. De ruwe pixels zijn beoordeeld via
    de code en `crates/burnburn-render/tests/offscreen.rs`.
  - Niet bereikt in de run:
    - Beeld voorbereiden en Operatorweergave: het menu-item was uitgeschakeld.
    - Import- en tracer-sheets: die vragen eerst een bestand.
    - Het camera- en GRBL-venster.
    - Tekst- en variabele-tekstpaneel: geen tekstobject geselecteerd.

## Quick wins (klein, grote impact; eerst doen)

| # | Wat | Waar | Waarom |
|---|---|---|---|
| Q1 | **Dubbele verschuiving in inspecteur en panelen opheffen.** Eerst frames verschuiven en dán `setFrameSize`/`setContentSize` laat AppKit de `TOP_PINNED`-views (flexibele onderrand) nóg eens verschuiven. Zet `autoresizesSubviews(false)` tijdens het herschikken, of verschuif niet zelf. | `appkit/panels/g5.rs:262-270` (`embed_stack`), `g5.rs:687-689`, `schikken.rs:457`, `widgets.rs:16-25` | De transformvelden X/Y/B/H staan buiten beeld. Het 3×3-referentieraster, de ↔/↕-knoppen en "Machine" overlappen. Zie `01..06` (rechterkolom). Panelen zijn bovenaan afgekapt of overlappen: `13`/`14`/`15`/`18`/`20`/`21`. Grootste esthetische fout. |
| Q2 | Statusregel alleen rood bij een fout; anders `secondaryLabelColor`. | `appkit/window.rs:189-191, 545` | Metingen en gewone meldingen ogen nu als fouten. |
| Q3 | Primaire knop rechts en Esc/Return overal. | Export (`export.rs:128-135`), Bundle (`bundle.rs:89-96`), import (`import.rs:110-125`), Beeld aanpassen (`image.rs:591-596`) | HIG; zie `33-export-svg`, `34-export-bundle`. |
| Q4 | Plan-id niet als titel en niet in de inspecteur; "Plan van 22-10 14:03 · 1,6 s" volstaat. De id gaat naar de tooltip of naar "Kopieer id". | `window.rs:195-198`, `host-models/src/panels/plan.rs:120`, previewvenster `app_panels/windows.rs` | Een contenthash `plan-c3c37…` oogt als debugoutput (`06-compiled`, `36-preview`). |
| Q5 | Standaard About-paneel (`orderFrontStandardAboutPanelWithOptions:`) met app-icoon en credits, plus een `.icns`/Info.plist. | `appkit/app/app_host.rs:427-455` | Nu een NSAlert met een **mapicoon** en "OK" (`04-about`). |
| Q6 | Overloop-meldingen in de CAM-dialogen: de capability-reden één keer boven de velden in plaats van per veld, en voetnoten onder het laatste veld in plaats van eroverheen. | `appkit/panels/cam_dialog.rs` (rij-opbouw), `host-models/src/panels/cam.rs` | `29-cam-focus-test`: 10× dezelfde afgekapte zin, en de notitie overlapt "Labelhoogte". `28-cam-interval-test` en `35-cam-preflight` overlappen ook. |
| Q7 | NL/EN-lekken in modelnamen: "Naamloos", "Rechthoek 1", "Schets 1" verschijnen in de Engelse UI. "text or bytes, up to version 10" verschijnt in de Nederlandse UI. | `presentation/src/controller/sketch.rs:249,271`, `macos/src/adapter.rs:1240`, `generators/src/symbology/engine.rs:151-176` | Zie `light-en-*`/`dark-en-*` en `32-barcode`. |
| Q8 | Meervoud en getallen: "1 objecten", "250.000 × 160.000 mm" (punt en drie decimalen in NL). | `host-models/src/l10n_files.rs:209`, de reekssamenvatting in `panels/cam.rs` | Zie `33-export-svg` en `26-cam-array`. |
| Q9 | Lege, ongelabelde velden: de stapgrootte bij jog en het framevermogen in het laservenster. | `appkit/panels/laser.rs` (stap- en frame-rij) | `11-laser`: rechts staat een leeg vak zonder label of eenheid. |
| Q10 | "Voorkeuren…" wordt "Instellingen…" (macOS 13+), zonder ellips in de venstertitel. | `host-models/src/app_text.rs:34` | `07-preferences`: de titel toont "Voorkeuren"; het menu hoort Instellingen… te zijn. |

## Diagnose per gebied

Ernst: **H** = valt direct op of is stuk; **M** = oogt amateuristisch of onaf; **L** = afwerking.

### 1. Documentvenster (`01..03`, `06`, `39`)

| Bevinding | Ernst | Bewijs |
|---|---|---|
| Inspecteur stuk: transformvelden buiten beeld, overlappende knoppen en labels (zie Q1) | H | `03-selection`; `window.rs:184-300`, `g5.rs:250-288` |
| Geen Auto Layout, NSStackView of NSGridView: alles met de hand op `rect()` en `TOP_PINNED`. Elke vergroting verschuift of rekt verkeerd. | H | `widgets.rs:16-25`; ongeveer 30 vaste maatconstanten (`window.rs:47-51`, `toolstrip.rs:35-44`, `production.rs:16-17`) |
| Splitviews zonder delegate, min/max of `holdingPriority`; geen `minSize`; geen frame-autosave; hercentreren bij elke open | H | `window.rs:129-149, 166-169, 425` |
| De productiestrook (palet, bewerkingen, bedcamera) is 250 pt hoog en vast. De kolommen van de bewerkingentabel zijn afgekapt ("Snelheid…", "V"). De invoerrij heeft afgekapte placeholders ("Snelheid (r", "Vermoge"). Twee knoppen heten "Vraag to…" en "Start ca…". | H | `02-drawing`; `operations.rs:91-121`, `panels/camera.rs:39-77` |
| De laagkleur in de tabel is een zwart blok met tekst "C00" in plaats van een kleurstaal plus naam | M | `06-compiled`; `operations.rs:26-38, 61-64` |
| De rij van 32 paletknoppen wordt rechts afgekapt (halve staal) | M | `02-drawing` |
| Er is een NSToolbar met alleen "Uitlijnen" (14 segmenten) en een overflow-chevron. Daaronder staat een eigen lintstrook met 9 pt-titels in hoofdletters ("MAKEN", "WIJZIGEN") en ▾-carets. Twee concurrerende werkbalken: dubbel en druk. | M | `01-empty-document`; `window.rs:418-423`, `toolstrip.rs:35-44, 111-133`, `assets/l10n/tools.toml:6-14` |
| "Pen" is de enige tekstknop tussen de iconen | M | `01`; `toolstrip.rs:228-233` |
| De objectlijst is een kale NSTableView met kop "Objecten": geen source-list-stijl, geen type-iconen, geen laagkleur en geen zichtbaar/slot-iconen | M | `02`; `window.rs:152-162` |
| Geen statusbalk. Coördinaten, zoom, eenheid, snap-stand en machinestatus ontbreken onderaan het venster. | M | `window.rs:189` (één label in de inspecteur) |
| Het standaardvenster van 1100×800 plus toolbar is te hoog voor 13" | L | `window.rs:47` |

### 2. Canvas (`02`, `03`, `zz-canvas-closeup-light.png`)

| Bevinding | Ernst | Bewijs |
|---|---|---|
| Geen anti-aliasing (MSAA 1, geen AA in de shader): schuine lijnen en bogen zijn getrapt | H | `burnburn-render/src/gpu.rs:206,363,501`; closeup |
| Geen donker canvasthema: in Dark Mode staat een wit canvas in donkere chrome | H | `dark-*-0[1-6]`; `render/src/scene.rs:105-131`, `macos/appkit/canvas.rs:23,30` |
| Geen bed/werkgebied, geen linialen en geen oorsprong. Het raster is oneindig en eenkleurig. | H | `editor/src/scene.rs:51-169, 200-221`; `render/src/scene.rs:117` |
| Elke gesloten vorm krijgt een grijze vulling (paper 0,93). Ongebruikelijk voor laser-CAD; laagkleuren worden modderig. | M | `02`; `scene.rs:108,111,157` |
| Selectie, hover, preview en laag C01 gebruiken alle vier bijna hetzelfde blauw | M | `scene.rs:107,113,120,29` |
| Handgrepen van 6 pt; de draaigreep is hetzelfde vierkantje, zonder steel. Geen randgrepen en geen hoverstaat. | M | closeup; `scene.rs:124,238-248,406-408`; `editor/src/tool/select.rs:18-25` |
| De snap-marker is een magenta vierkantje van 3,6 pt zonder label (`MessageKey::Snap` ongebruikt) | M | `scene.rs:435-437`; `presentation/src/view_model.rs:797` |
| Maatlabels gebruiken een eigen zevensegment-streepfont zonder halo; onbekende tekens worden blokjes | M | `render/src/scene/sketch.rs:77-137,119` |
| Het previewvenster rastert met Bresenham op de CPU (getrapt). Rood/groen voor gespeeld/ongespeeld is niet kleurenblind-veilig. De legenda bestaat alleen uit tekst. | M | `36-preview`; `host-models/src/app_view.rs:428-519` |
| De kleuren gaan als sRGB naar een niet-sRGB-doel; het commentaar zegt "linear" | L | `render/src/input.rs:3` |

### 3. Panelen, sheets en dialogen (`05`, `07..38`)

| Bevinding | Ernst | Bewijs |
|---|---|---|
| G5/G6/G7-panelen (Notities, Restplaten, Projecten zoeken, Doos of scharnier, Schikken) zijn bovenaan afgekapt of overlappen (Q1) | H | `14`, `15`, `18`, `20`, `21`; `g5.rs:676-697`, `schikken.rs:452-460` |
| Snij-instellingen heeft twee veldkolommen (x≈210 en x≈265 pt), met verschillende breedtes. De scrollinhoud is afgekapt zonder zichtbare scroller ("Uitloop, lengte" half). De capability-noot staat over het veld. | H | `05-cut-settings`; `cut_settings.rs:160-206` |
| Het laservenster is hoger dan het scherm en afgekapt (boven "Positie uitlezen" half, onder "Bestanden op de controller"). Er zijn velden zonder label. De pijlglyphs ◀▶▲▼ en ●○ staan op knoppen. | H | `11-laser`; `laser.rs:39-40, 568-571, 1001` |
| Labels staan niet op de basislijn van hun veld: het label staat 3–6 pt hoger, met per paneel een andere offset | M | `07-preferences`, `33-export-svg`, `12-material-library`; `windows.rs:149`, `priority.rs:27`, `cut_settings.rs:160-186` |
| Dubbelepunten zijn inconsistent: "Materiaal:", "Actief apparaat:" naast "Eenheden", "Modus" | M | `12`, `10`, `07`, `05` |
| Eenheden staan in het label ("Snelheid (mm/s)"), behalve in Voorkeuren waar ze achter het veld staan | M | `07` vs `05`/`22..30`; `host-models/src/l10n.rs:546,580,608` |
| De machinehub is een groot leeg venster met een pop-up als tabkiezer. Er staat een technische zin ("dezelfde matrix als de compiler"). | M | `10-machine-hub`; `hub.rs:99,165-176` |
| Brede knopstapels over de volle breedte ("Voorbeeld", "Pas offset toe", "Sluit") in plaats van rechts uitgelijnde knoppen met een vaste breedte | M | `14`, `16`, `20`, `21`; `g5.rs:394-470` |
| Knoppen worden ongelijk afgekapt: "Standaardwaarden he…", "Bijwerken en toepas…", "Vervang item door l…", "Naar machine-oorspro…" | M | `07`, `12`, `11` |
| Werkwoorden en registers zijn gemengd: Sluit/Sluiten, Annuleer/Annuleren, Pas toe/Toepassen, "je" naast "u" | M | `22` vs `33`; `l10n_files.rs:105`, `l10n.rs:736,833`, `app_text.rs:52` |
| Vervreemdend jargon in de UI: "Ontbrekende capability", "Preflight", "Feeder", "Debuglog", "Docken", "Kunstbibliotheek" | M | `29`, `30`, `35`; `host-models/src/panels/cam.rs:33-34`, `app_text.rs:46` |
| Grote lege vlakken: Exporteer als SVG (640 pt hoog, inhoud ~250 pt), Producten herkennen, Reeks | L | `33`, `19`, `26` |
| De RD-inspecteur toont Rust-`Debug` ("Bounds { kind: ProcessTopLeft, x: AbsCoord(2000…") | L | `37-inspect-rd`; host-models RD-model |
| Sneltoetsen: bijna alles "geen". De opnameknop is een onzichtbare view zonder focusring. | L | `08-shortcuts`; `windows.rs:256-262`, `target.rs:286-300` |
| Er zijn geen voortgangsindicatoren; rekenen en machinevoortgang zijn kale tekst | L | `window.rs:265`, `tracer.rs:133` |

### 4. Menu's

| Bevinding | Ernst | Bewijs |
|---|---|---|
| 14 hoofdmenu's. "Productie" heeft er 2. CAM-instellingen zitten onder Wijzig. "Omzetten naar pad" staat er dubbel in. | M | `appkit/app.rs:1618-1948` (1732-1735, 1850, 1868, 1913) |
| `setHelpMenu` ontbreekt (geen zoekveld in Help). Er is geen Toon/Pas werkbalk aan. | M | `app_host.rs:146-166` |
| Ellipsen inconsistent bij paneel-toggles ("Structuur…" vs "Materiaalbibliotheek") | L | `l10n_g5.rs:48,101`, `l10n.rs:969` |
| Select All heeft geen menutag (de snapshot-tool kon hem niet vinden) | L | `common::find_item` faalde op `CommandKey::SelectAll` |

### 5. Iconen

| Bevinding | Ernst | Bewijs |
|---|---|---|
| Er is geen app-icoon. Dock, About en alerts tonen een map of het generieke executable-icoon. | H | `04-about`; geen `.icns`/Info.plist in de repo |
| SF Symbols worden alleen in Uitlijnen gebruikt. Elders staan Unicode-glyphs (▾ ■ ↔ ↕ ◀ ▶ ●○ ‹ › ↑ ↓). | M | `arrange.rs:16-40`; `toolstrip.rs:111`, `inspector.rs:29`, `laser.rs:568`, `windows.rs:431`, `cam_dialog.rs:282` |
| Toolstrip-iconen zijn geen template images. Ze volgen de accentkleur en de geselecteerde staat niet. | M | `toolstrip.rs:350-405` |
| De schaal wordt afgerond naar 1/2/3, dus fractionele schalen worden fout gerasterd. Thema-detectie gebeurt via de string "Dark". | L | `toolstrip.rs:323-330` |
| Streekbreedtes in de SVG's buiten de gedeclareerde set (3, 2, 1,5, 3,4…) | L | `assets/icons/ui/svg/*`, `palette.toml [strokes]` |

### 6. Kleur, typografie en getallen

| Bevinding | Ernst | Bewijs |
|---|---|---|
| De chrome gebruikt correct semantische systeemkleuren (Dark Mode werkt daar: `dark-*`). Canvas, laagstalen en preview zijn hard gecodeerd. | M | `operations.rs:27,34`, `app_view.rs:428-435` |
| Lettergroottes ad hoc: 9, 10, 11, 12, 13 en 20 pt, plus bold 12/13 als kop. Geen `monospacedDigitSystemFont` voor coördinaten. | M | `toolstrip.rs:112,133`, `window.rs:193-205`, `laser.rs:505` |
| De NL-decimaal is inconsistent: invoervelden tonen "0.1" en labels "1,6 s". Er zijn 12 eigen komma-vervangers. | M | `07`, `22`, `06`; `presentation/src/field.rs:135-139`; `host-models/src/panels/{laser.rs:288,plan.rs:140,…}` |
| "mm" is hard gecodeerd, ook bij de voorkeur "inch" | M | `l10n.rs:612,679-683`, `toolbar.rs:124-129` |
| Debug-fallbacks die in de UI kunnen komen: `[{key:?}]`, `{feature:?}`, `UndoLabel {:?}` | M | `l10n.rs:2206,2224,1747`, `app_view.rs:245` |

### 7. Toegankelijkheid

Er zijn 74 `setAccessibilityLabel`-aanroepen en het canvas heeft een rol. Problemen:

- Er is geen `initialFirstResponder` of `nextKeyView`, dus de tabvolgorde volgt de subview-volgorde.
- De previewknop heeft als label "‹".
- De sneltoets-recorder heeft geen focusring.
- Kleur is de enige drager bij preview, status en laagkleur.

Contrast is niet gemeten: dat is werk voor LF1.

### 8. Windows-host (alleen code)

| Bevinding | Ernst | Bewijs |
|---|---|---|
| Geen manifest of resources: geen Common Controls v6 (Win95-look), geen app-icoon en geen DPI-manifest | H | geen `.manifest`/`.rc`; `win32_panels/mod.rs:161-164` |
| `DEFAULT_GUI_FONT` (MS Sans Serif 8 pt, niet geschaald) in plaats van Segoe UI 9 pt | H | `win32/forms.rs:140`, `win32_panels/mod.rs:238,301` |
| De toolstrip bestaat uit tekstknoppen zonder iconen (`burnburn-icons` is geen dependency) | H | `forms/toolstrip.rs:111-160` |
| Twee layoutsystemen met andere marges (8/4/24 vs 10/6/24) en andere achtergronden (BTNFACE vs WINDOW). De tekstbreedte wordt geschat op 7 px per teken. | M | `forms/layout.rs:103-124`, `host_panels/spec.rs:231`, `win32/mod.rs:1040`, `win32_panels/mod.rs:181` |
| Panelen gebruiken de systeem-DPI. Geen `WM_DPICHANGED`. De preview is vast 520×340 px. | M | `win32_panels/mod.rs:140`, `host_panels/app.rs:125` |
| Annuleren staat links van OK. Knoppen zijn uitgerekt en er is geen `BS_DEFPUSHBUTTON`. De bewaarvraag is een MessageBox Ja/Nee/Annuleren met NL-tekst in code. | M | `forms/cut_settings.rs:132-134`, `layout.rs:190-200`, `win32/mod.rs:296-299` |
| Geen statusbalk (een STATIC rechts), geen dark mode en geen weergave van ongeldige velden | L | `win32/mod.rs:527,587`; `win32/forms.rs` |

## Ontwerpsysteem

Eén bron in `burnburn-host-models::style`, een nieuw, puur Rust-module zonder platformtypes. De hosts vertalen het naar
NSColor/NSFont/metrics resp. Win32/DWM. Het canvasthema staat in `burnburn-render::Theme` en leest dezelfde tokens.

### Kleurtokens

**Chrome.** Gebruik op de Mac altijd semantische systeemkleuren:

- tekst: `labelColor`, `secondaryLabelColor`, `tertiaryLabelColor`;
- achtergronden: `windowBackgroundColor`, `controlBackgroundColor`, `underPageBackgroundColor`;
- scheidingen: `separatorColor`;
- accent: `controlAccentColor`.

Op Windows: `GetSysColor` plus de DWM immersive dark mode. Gedeeld zijn alleen de **rollen**, niet de RGB-waarden.

**Canvas** (render-tokens, licht / donker, sRGB):

| Token | Licht | Donker | Gebruik |
|---|---|---|---|
| `canvas.bg` | #FFFFFF | #1E1F22 | buiten het bed |
| `canvas.bed` | #FAFAFB | #26282C | werkgebied van het machineprofiel |
| `canvas.bed.edge` | #C9CCD1 | #4A4E55 | rand van het bed, 1 device-px |
| `grid.minor` | #ECEEF1 | #2C2F34 | kleine rasterlijn |
| `grid.major` | #D9DCE1 | #383C42 | elke 5e/10e lijn |
| `axis.origin` | #E05A47 (x) / #3FA35B (y), α0,6 | idem | oorsprong |
| `ink.design` | #2A2D33 | #D7DAE0 | ontwerp zonder laag |
| `fill.closed` | geen (standaard) | geen | vulling alleen in "Gevuld"-weergave, met laagkleur α0,12 |
| `select.stroke` | `controlAccentColor` | idem | selectiekader, 1 pt, **gestreept** 4-3 |
| `select.handle.fill` | #FFFFFF | #2A2D33 | handgreep |
| `select.handle.stroke` | accent | accent | 1 pt |
| `hover` | accent, α0,45, 2 pt halo | idem | hover-contour (geen eigen kleur) |
| `preview` | #8E44AD, gestreept | #C08BE0 | tekenvoorbeeld (onderscheidt zich van laag C01) |
| `snap` | #E6007E | #FF4FA8 | snapmarker plus label |
| `issue` | `systemRed` | idem | problemen, altijd met symbool |
| `sketch.constraint` | #0F7B83 | #4FC3CC | constraint-glyphs |
| `dimension` | #1E7A36 | #6BCB82 | maatlijnen en -tekst |
| `label.halo` | bg, α0,85 | idem | achter elke canvastekst |

**Laagkleuren.** De LightBurn-palet blijft identiek, voor uitwisselbaarheid. In donker krijgen donkere slots
(C00 zwart, C09 marine) een lichte contourhalo, zodat ze leesbaar blijven. Ze worden niet van kleur veranderd.

**Statuskleuren.** Altijd **kleur + symbool + woord**:

| Status | Kleur | Mac-symbool | Woord (NL / EN) |
|---|---|---|---|
| ok | `systemGreen` | `checkmark.circle.fill` | Klaar / Ready |
| info | `secondaryLabel` | `info.circle` | — |
| waarschuwing | `systemOrange` | `exclamationmark.triangle.fill` | Let op / Warning |
| fout | `systemRed` | `xmark.octagon.fill` | Fout / Error |
| bezig | `controlAccent` | `NSProgressIndicator` | Bezig… / Working… |
| niet beschikbaar | `tertiaryLabel` | `nosign` | Niet beschikbaar / Unavailable, met reden in de tooltip |

**Preview van het toolpad:**

| Rol | Kleur |
|---|---|
| snijden | laagkleur |
| verplaatsing | gestreept grijs |
| gespeeld | 35 % gedimd |
| gekozen | accent + 2 pt |
| afspeelkop | kruis met cirkel |

De rood/groen-tegenstelling vervalt.

### Typografie

Mac: alleen de systeemfont, met tekststijlen:

| Rol | Stijl / maat |
|---|---|
| venstertitel | systeem |
| sectiekop | `.headline` 13 semibold |
| label | `.body` 13 |
| hulptekst | `.callout`/`.footnote` 11, `secondaryLabel` |
| tabel | 12 |
| lintgroepnaam | 10, gewone hoofdletters, `secondaryLabel` |
| getallen in velden, coördinaten en tijden | `monospacedDigitSystemFont` |

Geen 9 pt. Geen bold als kop zonder semibold.

Windows: Segoe UI 9 pt (Segoe UI Variable op Windows 11) via `SystemParametersInfo(SPI_GETNONCLIENTMETRICS)`, geschaald
per DPI. Koppen in Segoe UI Semibold 10,5.

Canvastekst: één echte font (de system UI font via `burnburn-text`-glyph-outlines, getesselleerd), 11 pt, met halo. Het
zevensegment-font verdwijnt.

### Spatiëringsraster

Basis 4 pt. Tokens: `xs 4`, `s 8`, `m 12`, `l 16`, `xl 20`, `xxl 24`.

| Element | Maat |
|---|---|
| venstermarge | 20 (Mac), 11 DLU ≈ 16 px (Windows) |
| tussen rijen | 8 |
| tussen secties | 20 |
| label–veld | 8 |
| knoppenrij onder | 20 marge, 12 tussen knoppen |
| veldhoogte | 22 (regular) / 19 (small, inspecteur) |
| knop | ≥ 80 breed (Mac), 75×23 DLU-basis (Windows) |

### Formulierregels (componenten)

- **Labels.** Rechts uitgelijnd tegen de veldkolom (Mac-formulier) of links met een vaste kolombreedte per venster.
  Altijd baseline-uitgelijnd via NSGridView-rijen.
  - Mac: zonder dubbelepunt in inspecteur en Instellingen, mét dubbelepunt in dialogen, één regel per venster.
  - Windows: altijd met dubbelepunt.
- **Eenheden.** Altijd als suffix achter het veld, in `secondaryLabel`, uit de eenheidsvoorkeur (mm/inch, mm/s of
  in/min, %, °, s). Nooit in het label.
- **Getallen.**
  - Eén `format_quantity(value, unit, decimals, locale)` met decimaalteken per taal, zonder overbodige nullen, en
    invoer met zowel komma als punt.
  - `NSNumberFormatter` alleen voor weergave in tabellen.
  - Velden met een bereik krijgen een NSStepper (doorgangen, aantallen).
- **Knoppen.**
  - Mac: primair rechts (Return), annuleren links ervan (Esc), destructief apart links.
  - Windows: OK, Annuleren, Toepassen van links naar rechts, rechts uitgelijnd, OK = `BS_DEFPUSHBUTTON`.
  - Werkwoorden: één register. NL: imperatief kort ("Bewaar", "Annuleer", "Pas toe", "Sluit") of infinitief
    ("Bewaren"…). Voorstel: **infinitief** zoals macOS-NL ("Annuleren", "Bewaren", "Toepassen", "Sluiten").
  - Gebruik "u" niet; gebruik overal "je" (macOS-NL-toon).
- **Segmenten.** Voor 2–5 exclusieve opties (startmodus, eenheden). Een pop-up bij meer opties. Tabs (NSTabView of de
  toolbar-stijl Instellingen) in plaats van een pop-up als sectiekiezer.
- **Lijsten.** Laagtabel met kleurstaal (cirkel 12 pt) + naam + modus-icoon + snelheid/vermogen in mono-digits. De
  objectlijst als source list met type-icoon, oog en slot.
- **Lege toestanden.** Gecentreerd SF Symbol (32 pt, `tertiaryLabel`) + één zin + optioneel één actieknop. Dus niet
  "Geen machine verbonden." in rood.
- **Foutweergave.**
  - Veldfout: rode rand of `NSTextField`-validatie met een uitroepicoon in het veld, en de tekst onder het veld (11 pt,
    `systemRed`).
  - Formulierfout: een banner bovenaan het sheet.
  - Toepassingsfout: een NSAlert met de tekst uit de l10n-catalogus. Nooit `Debug`.
- **Niet beschikbaar.** Het besturingselement uitgegrijsd. De reden **één keer** als infobanner per sectie, met een
  "Meer…"-link; de reden per veld alleen in de tooltip.
- **Voortgang.** `NSProgressIndicator` (determinate bij bekend percentage, anders spinner) in de statusbalk en in
  sheets. Annuleerknop bij lange taken.
- **Tooltips.** Naam + sneltoets ("Rechthoek (R)") + één zin, geen ontwikkelaarstaal.

### Iconografie

- **Toolstrip en panelen** blijven de SolidCraft-masters (64-viewBox), gerasterd voor de pixelmaat:
  - maten: 16 pt (menu, tabel), 20 pt (lint), 24 pt (toolbar);
  - op @1x/@2x/@3x en met **fractionele** schaal correct;
  - streekset beperkt tot 2,55/4,25 (masters normaliseren);
  - mono-varianten als **template image**, zodat accent, geselecteerd en uitgeschakeld de systeemstijl volgen.
- **Mac.** SF Symbols voor alle generieke acties: uitlijnen, verdelen, spiegelen, draaien, pijlen (jog), afspelen,
  vorige/volgende, slot, oog, plus, min, prullenbak, info, waarschuwing, map, camera, laser (`rays`?), statussymbolen.
  Geen Unicode-glyphs meer op knoppen.
- **Windows.** Segoe Fluent Icons (Windows 11) met terugval op Segoe MDL2 Assets (Windows 10) via font-glyphs. Voor de
  CAD-tools dezelfde `burnburn-icons`-masters als 32-bit DIB (premultiplied) per DPI.
- **App-icoon.** Eén master-SVG of -PNG in `assets/app-icon/`, gegenereerd naar `.icns` (Mac) en `.ico` (Windows,
  16–256). Het documenticoon voor `.burnburn`/`.lbrn2`.

### Canvasstijl

- **AA.** MSAA 4× (`sample_count: 4` + resolve). Bij geen MSAA-ondersteuning valt het terug naar 1× met een melding in
  de log. Strepen met ronde caps en joins voor handgrepen en previews.
- **Bed.** Het machineprofiel tekent het bed als een licht vlak met rand, met de oorsprong-hoek gemarkeerd. Buiten het
  bed staat `canvas.bg`.
- **Raster.** Minor/major (1-2-5 met majors elke 5 of 10), alleen binnen het bed. Origin-assen.
- **Linialen.** Boven en links, op 20 pt, met mono-digits en de eenheid uit de voorkeur. Cursorpositie als marker.
- **Selectie.**
  - Gestreept 1 pt accentkader.
  - 8 handgrepen van 7 pt (wit vlak, accentrand), plus een ronde draaigreep op een steel van 16 pt.
  - Hover vergroot de greep naar 9 pt. Ook de cursor verandert, als native cursor.
- **Snapping.** Marker per soort (vierkant = eindpunt, driehoek = midden, cirkel = centrum, X = snijpunt) plus een
  label met halo ("Middelpunt").
- **Maatlabels.** Echte font, 11 pt, halo, mono-digits, eenheid volgens de voorkeur.
- **Previews.** Tekenvoorbeeld gestreept in `preview`. Resultaatvoorbeeld 1,5 pt. Geen vulling.
- **Constraints.** De bestaande glyphs op 12 pt, met een halo-achtergrondpil. Conflict in `issue` met een "!"-badge.

## Native per host, gedeeld via `burnburn-host-models`

| Gedeeld (pure Rust, headless getest) | Mac native (HIG) | Windows native (Fluent/Win32) |
|---|---|---|
| `style`: kleur-, spatiërings- en typografierollen, status-enum (Ok/Info/Warning/Error/Busy/Unavailable) met woord en symboolnaam | NSColor-semantiek, SF Symbol-namen, NSFont-tekststijlen | GetSysColor/DWM, Segoe Fluent-codepoints, NONCLIENTMETRICS |
| `form`: één `FormSpec` (secties, rijen: label, veldsoort, eenheid, bereik, hulptekst, reden-uitgeschakeld), voor alle panelen | NSGridView/NSStackView-bouwer uit `FormSpec` | de dialog-unit-layout uit `FormSpec` (vervangt `forms::Form` en `host_panels::spec`) |
| `DialogButtons { primary, cancel, destructive, extra }` met rollen | volgorde Mac, keyEquivalent ⏎/⎋ | volgorde Windows, BS_DEFPUSHBUTTON, IDOK/IDCANCEL |
| `format_quantity`/`parse_quantity` per locale en eenheid; pluralisatie (`{n} object`/`{n} objecten`) | — | — |
| l10n-catalogus, één register, glossarium | Menu-indeling (App-menu, Instellingen…, Venster, Help met zoekveld) | Menu-indeling (Bestand…Help, F10/Alt-mnemonics) |
| canvasthema-tokens (licht/donker) in `burnburn-render::Theme` | appearance-observer → `Theme::dark()` | `WM_SETTINGCHANGE` ImmersiveColorSet → `Theme::dark()` |
| statusbalkmodel (coördinaat, zoom, eenheid, snap, machine, voortgang) | NSView onderaan, met NSProgressIndicator | `msctls_statusbar32` met parts |
| app-icoon-master | `.icns` + Info.plist (bundle) | `.ico` + manifest (Common Controls v6, PerMonitorV2, `longPathAware`) via `embed-resource` of een build-script; dependency eerst in `docs/design/dependency-register.md` |

## Uitvoerplan in golven

Eigendom per lane zoals in `lightburn-pariteit.md`. Gedeelde bestanden (`intent.rs`, `view_model.rs`, `l10n*.rs`,
`appkit/app.rs` menu) worden alleen additief gewijzigd; de plannende sessie lost conflicten op. TDD volgens AGENTS.md:
eerst de rode test, en red/green-commando's in `docs/dev-notes/LF-<pakket>.md`. Elke lane draait aan het eind
`cargo xtask verify`, nooit `--native` zonder overleg, en `gui_snapshots` voor en na. De PNG's gaan naar de dev-note
als vergelijking. Ze worden **niet** als golden geaccepteerd zonder review.

### Golf LF-A: fundament en de grove fouten (parallel, 4 lanes)

| Pakket | Lane / eigendom | Eerst te schrijven tests | Handmatig |
|---|---|---|---|
| **LF0 Snapshot-tool** (dit plan; gecommit) | `tests/native/macos/gui_snapshots.rs`, 1 `[[test]]` in `crates/burnburn-macos/Cargo.toml` | opt-in; geen assertions | screenshots bekijken |
| **LF1 Layout-invarianten + Q1** | `macos/src/appkit/{window.rs,panels/g5.rs,panels/schikken.rs,panels/widgets.rs,panels/cut_settings.rs,panels/laser.rs}` | Native test `layout_invariants` (harness=false, stil): voor elk venster/paneel uit de snapshotlijst (gedeelde lijst in `tests/native/macos/common/windows.rs`) geldt (a) elk zichtbaar control ligt binnen de bounds van zijn (scroll)documentview, (b) geen twee zichtbare controls overlappen, behalve label-in-box, (c) geen `NSTextField`-label is afgekapt (`intrinsicContentSize.width ≤ frame.width`, of truncation expliciet gezet en tooltip aanwezig), (d) labels en hun velden delen de basislijn ±1 pt. Rood op `03`, `13..21`, `29`. Regressietest: inspecteur X/Y/B/H zichtbaar na `embed_stack`. | visueel op echt scherm, resize |
| **LF2 Canvas-AA + donker thema + bed** | `burnburn-render/src/{gpu.rs,scene.rs,tess.rs}`, `burnburn-editor/src/scene.rs` (overlay `Bed`, grid major/minor), `macos/appkit/canvas.rs` (themakeuze) | Offscreen-pixeltests in `render/tests/` met tolerantie: (a) een schuine lijn heeft ≥ 3 tussentinten op een rand (AA aan); (b) `Theme::dark()` achtergrond-luminantie < 0,1 en inkt-contrast ≥ 7:1 tegen de achtergrond; (c) gridpixel binnen het bed, geen gridpixel buiten; (d) selectie, hover en preview zijn onderscheidbaar (ΔE ≥ 20). Bestaande exact-pixeltests herzien op tolerantie, met uitleg per test in de dev-note. | AA-kwaliteit op 1×/2×-schermen |
| **LF3 Tokens en tekst** | `burnburn-host-models/src/{style.rs (nieuw),format.rs (nieuw),l10n*.rs,app_text.rs}`, `presentation/src/field.rs` (alleen `format_*`) | Headless: (a) `format_quantity(1.5, Mm, nl)=="1,5 mm"`, inch-voorkeur geeft "in", round-trip parse/format; (b) catalogus-lint: geen sleutel met NL==EN behalve een allowlist, geen `{…:?}`, geen "u/uw" in NL, één werkwoordsvorm voor Annuleer/Sluit/Pas toe, ellips-regel (opent dialoog ⇔ "…"); (c) pluralisatie `{0} objecten`; (d) WCAG-contrast van `style`-tokenparen ≥ 4,5:1 (tekst) en 3:1 (UI) in licht en donker. Q2, Q4, Q7, Q8, Q10. | NL-toon nalezen (Haiku-voorstel, gebruiker beslist) |
| **LF4 App-identiteit** | `assets/app-icon/` (nieuw), `xtask` (bundle/Info.plist), `macos/src/appkit/app/app_host.rs` (About) | Test: de bundle bevat `CFBundleIconFile`, `.icns` heeft 16–1024 @1x/@2x; de About-opties bevatten naam, versie en credits (headless, via de options-dictionary). Q5. | het icoon zelf ontwerpen (gebruiker of ontwerper) |

### Golf LF-A: uitgevoerd (23-10-2026)

- **Quick wins**: Q1 (dubbele verschuiving, inspecteur en panelen), Q3 (primaire knop rechts, Esc/Return), Q6 (CAM-reden één
  keer), Q9 (laservenster gelabeld en scrollend, Snij-instellingen niet afgekapt) in LF1; Q2 (statuskleur), Q4 (geen plan-id
  als titel), Q7 (NL/EN-namen), Q8 (meervoud/getallen), Q10 (Instellingen…/Opties…) in LF3; Q5 (standaard About-paneel) in LF4.
- **LF1**: native `layout_invariants` (geen overlap, binnen de container, niet afgekapt, vóór en na vergroten; rood: 474
  overtredingen). **LF2**: MSAA 4×, licht en donker canvasthema met contrastchecks, bed met oorsprong en hoofd-/hulpraster,
  geen grijze vulling. **LF3**: `style.rs`, `format.rs`, lint op de vertalingen. **LF4**: eigen app- en documenticoon
  (`assets/app-icon/`, `cargo xtask icon`), `.icns`/`.ico`/MSIX-logo's, plist-sleutels.
- Verify (stil) op de integratie: 4928 geslaagd, 0 gefaald, 15 genegeerd; Windows-clippy schoon. Dev-notes `LF1.md`–`LF4.md`.
- **Nog open uit LF-A**: zoom naar het bed bij openen (de standaardcamera kadert het bed niet); "Plan van <tijd> · <duur>"
  (geen compile-tijdstip in `PlanViewModel`) en een knop "Kopieer id"; statussymbool naast de kleur in de statusregel (Mac)
  en statuskleur op Windows; basislijncontrole label/veld in `layout_invariants`; laservenster begrensd op 700 pt i.p.v. de
  echte schermhoogte; vulling per Fill-bewerking op het canvas (vraagt een scene-veld); Windows: icoon inbedden in de exe,
  donker canvasthema volgen. Handmatig: het getoonde About-paneel, Dock-icoon van de gebundelde app, NL-toon, het echte
  menu-item Instellingen…, licht/donker op een echt scherm.

### Golf LF-B: formulieren en componenten (na LF-A)

| Pakket | Lane / eigendom | Eerst te schrijven tests | Handmatig |
|---|---|---|---|
| **LF5 FormSpec + Mac-formulierbouwer** | `host-models/src/form.rs` (nieuw); `macos/src/appkit/panels/form.rs` (nieuw, NSGridView) | Headless: elk paneelmodel levert een `FormSpec` met eenheid als suffix, geen eenheid in het label, en een `DialogButtons`-rol per knop. Native: een gebouwd formulier haalt de LF1-invarianten en de knopvolgorde [annuleer, primair] met ⏎/⎋. | — |
| **LF6 Panelen migreren (Mac)**, 3 sublanes | a) CAM-dialogen + Snij-instellingen (`cam_dialog.rs`, `cut_settings.rs`); b) bestanden/bibliotheken/export (`export.rs`, `bundle.rs`, `material.rs`, `art.rs`, `import.rs`, `image.rs`); c) G5/G6/G7, Schikken, Voorkeuren (tabs), Machinehub (tabs), laser | Per paneel: de LF1-invarianten, de knoprollen en de "niet beschikbaar"-reden één keer per sectie (telt banners, rood op `29`). Laser: stapgrootte en framevermogen hebben een label en eenheid (Q9). | elk paneel naast een HIG-referentie |
| **LF7 Documentvenster** | `macos/src/appkit/{window.rs,toolstrip.rs}`, `panels/{operations.rs,production.rs,inspector.rs,priority.rs}`, `classes.rs` (splitdelegate) | Native: (a) bij vergroten van 1100→1600 pt groeit alleen het canvas, zijbalk en inspecteur blijven gelijk; (b) minSize, en het frame wordt bewaard en hersteld (via een temp-defaults-domein); (c) de laagtabel toont staal + naam en de kolommen zijn niet afgekapt bij de standaardbreedte; (d) de statusbalk toont coördinaat, zoom en eenheid na een muisbeweging. Eén werkbalk: NSToolbar (aanpasbaar) met de lintgroepen als toolbar-items of een segment-per-groep. Q3. | het gevoel van de werkbalk en het lint, met de gebruiker |
| **LF8 Iconen** | `burnburn-icons`, `assets/icons/ui/*`, `macos/src/appkit/symbols.rs` (nieuw) | Headless: elke SVG gebruikt alleen de streekset; elk icoon rendert bij schaal 1,0/1,5/2,0/3,0 met de juiste pixelmaat. Native: geen knoptitel bevat een glyph uit {▾■↔↕◀▶▲▼●○‹›↑↓}; elke SF Symbol-naam bestaat (`imageWithSystemSymbolName` ≠ nil). | iconenset als geheel beoordelen |

### Golf LF-C: canvasafwerking en preview (na LF2)

| Pakket | Eigendom | Eerst te schrijven tests | Handmatig |
|---|---|---|---|
| **LF9 Linialen, handgrepen, snaplabels** | `editor/src/scene.rs`, `editor/src/tool/select.rs` (Handle-soort doorgeven), `render/src/scene.rs`, `presentation` (`MessageKey::Snap` gebruiken) | Editor: de scène heeft 8 `Handle::Scale` plus `Handle::Rotate` met steel. Render-pixeltest: de draaigreep is rond (pixelpatroon), de hovergreep is groter. Presentation: een snap levert een label in de view model. | cursorgedrag |
| **LF10 Canvastekst** | `render/src/scene/sketch.rs` → glyph-outlines via `burnburn-text` | Pixeltest: label "R12,5 mm" rendert zonder vervangblokjes; de halo-pixels hebben de achtergrondkleur. | — |
| **LF11 Toolpad-preview** | `host-models/src/app_view.rs` (preview naar de GPU of AA-raster), `macos/appkit/app_panels/windows.rs` | Pixeltest met tolerantie: AA op een schuine beweging; "gespeeld" is gedimd, niet groen; de legenda is een getekende staal + woord. | afspelen en scrubben |

### Golf LF-D: Windows op hetzelfde niveau (L8; eerst een Windows-runner)

| Pakket | Eigendom | Eerst te schrijven tests | Handmatig |
|---|---|---|---|
| **LF12 Manifest, icoon, DPI, font** | `crates/burnburn-windows/{build.rs,res/}`, `win32/mod.rs` | Headless: het build-script embedt een manifest met CCv6 en PerMonitorV2 (resource-lint op de PE). Op een Windows-runner: `GetDpiForWindow` gebruikt per venster; het font is Segoe UI en schaalt met DPI; `WM_DPICHANGED` herschikt. | echte run op 100/150/200 % |
| **LF13 FormSpec-bouwer Windows** | `burnburn-windows/src/forms/**`, `win32_panels/**` (de twee systemen samenvoegen) | Headless layout-tests op `FormSpec` (DLU naar px per DPI, label/veld-kolommen, knopvolgorde OK/Annuleren/Toepassen, de default-knop); tekstbreedte via `GetTextExtentPoint32` achter een injecteerbare meter. | — |
| **LF14 Toolstrip, iconen, statusbalk, dark mode** | `forms/toolstrip.rs`, `win32/mod.rs` | Headless: de toolstrip-items krijgen icoon-DIB's uit `burnburn-icons`. Runner: de statusbalk heeft ≥ 4 parts; `DwmSetWindowAttribute(DWMWA_USE_IMMERSIVE_DARK_MODE)` volgt het systeem; het canvas krijgt `Theme::dark()`. | Fluent-uitstraling beoordelen |

### Golf LF-E: afronding

- **LF15 Toegankelijkheid.** Tabvolgorde (`nextKeyView`-keten per paneel; native test: Tab doorloopt alle velden in
  leesvolgorde). Labels zonder glyphs. VoiceOver-rondgang (handmatig). Kleur nooit enige drager (lint op het
  statusmodel).
- **LF16 Menu's.**
  - Structuur terug naar ±10 menu's: Productie samenvoegen met Laser, CAM-instellingen naar Laser/Bewerkingen.
  - `setHelpMenu`, Toon/Pas werkbalk aan, en de dubbele "Omzetten naar pad" weg.
  - Test: de bestaande `menus.rs`-pariteit uitbreiden. Er is geen item dubbel, en elk item dat een dialoog opent
    eindigt op "…".
- **LF17 Lege toestanden en voortgang.** De lege-toestandcomponent in objectlijst, bewerkingen, bibliotheken en
  hub. `NSProgressIndicator` bij compile, trace en nest. Test: het paneelmodel levert `EmptyState { symbol, text,
  action }` en de host toont het.

### Volgorde en parallelisme

- **LF-A:** LF1, LF2, LF3 en LF4 tegelijk. Hun bestanden zijn disjunct, behalve `window.rs` (alleen LF1) en
  `l10n*.rs` (alleen LF3).
- **LF-B:** start na de merge van LF-A. LF5 eerst, daarna LF6a/b/c en LF7 parallel; LF8 kan direct na LF-A.
- **LF-C:** kan parallel met LF-B (andere crates).
- **LF-D:** wacht op een Windows-runner (zie AGENTS.md: cross-compilatie bewijst geen GUI).
- Na elke golf: `gui_snapshots` in vier varianten. De mensen die de screenshots beoordelen zijn de gebruiker plus
  een reviewsessie.

## Wat handmatig blijft

- Het app-icoon en de definitieve iconenset (ontwerp).
- Het register en de toon van de NL-teksten: de gebruiker kiest "je" en de infinitief.
- De werkbalkkeuze (NSToolbar-only versus lint).
- AA en dark mode beoordelen op echte schermen.
- Windows op 100/150/200 % DPI.
- VoiceOver en Verteller.
- Systeempanelen.
- Tooltips, hover en focus: niet offscreen te vangen.
