# Instellingen en hun semantiek

Bewijslabels: **UI** = hier gelezen; **DOC** = officiële 2.1-reference; **ONTWERP** = aanbevolen eigen contract. Waargenomen waarden hieronder zijn sessiewaarden, niet noodzakelijk fabrieksdefaults. De volledige brongebonden optie-index staat in `reference/feature-index.md`.

## Algemene voorkeuren — alle vijf UI-tabbladen

| Tab / groep | Waargenomen opties | Implementatiebetekenis |
|---|---|---|
| Editor / tekst | default font/hoogte; RTL-enable; SHX-pad; double-click text edit | tekstopmaakdefaults en invoergedrag, los van bestaande objectwaarden |
| Editor / bestanden | auto-update; default layers laden op New/Restart; autosave-interval; interne/bypass system-dialogs; job log | lifecycle en opslagbeleid; atomair schrijven en herstel |
| Editor / selectie/output | Default Cut Selected; Default Use Selection Origin; Save Job Origin to Project; Save Cut Selected/Selection Origin to Project | expliciet bepalen welke jobopties documentgebonden zijn |
| Editor / interactie | Use System Clipboard; invert wheel zoom; Move/Rotate/Scale-switches bewaren; na tekenen terug naar Select; Beginner Mode | presentatie en interactie; geen geometrische of machineparameters |
| Editor / uitvoering | ignore out-of-bounds if possible; ignore Start when monitor sleeps; Start mag geen keyboardfocus houden; foutgeluid bij mislukte DSP-send | jobfilter en bedieningsgedrag; buiten-bedgedrag moet in preview zichtbaar zijn |
| Units and Grids / units | Millimeters / Inches | interne fysische grootheid behouden bij omschakelen; waarden converteren |
| Units and Grids / snap | Snap to Objects; Snap to Grid; Grid Snap Distance; Object Snap Distance px | eenheden en schermpixels niet mengen |
| Units and Grids / grid | contrast; visual spacing; selection tolerance px | gridtekening en hit-test onafhankelijk |
| Units and Grids / keyboard | Ctrl-Arrow; Arrow; Shift-Arrow increments | kleine/normale/grote documentverplaatsing |
| Display | theme; dark background; font/icon size; curve quality; cursor/node size; full-screen crosshair; bed center cross | viewportinstellingen; vereis geen recompilatie van jobs |
| Display / rendering | antialiasing; filled rendering; true high-DPI workspace; Reduce Motion; startup display checks | prestaties/leesbaarheid; outputfoutbudget blijft afzonderlijk |
| Display / toggles | palette labels; rotary enable; cylinder correction enable op hoofdvenster | alleen zichtbaarheid van bediening |
| Import / general | Select Imported; Group Imported; AI hidden layers; Allow Tool Layers; Center Imported | importpolicy; mapping vóór selectie en jobplanning |
| Import / SVG | 96 DPI/Inkscape; 72 DPI/Illustrator; autodetect | px→mm en viewBox/units eenduidig verwerken |
| Import / DXF | auto-units; µm/mm/cm/m/km/in/ft; ignore TEXT/MTEXT; auto-close tolerance | houd insert/blocktransforms en units apart; close alleen binnen tolerantie |
| Export / DXF | arcs behouden of naar lijnen | curvefit/foutmarge en ondersteunde DXF-entiteiten |
| Import / HPGL | 1 inch/1000 of /1016 | correcte plottereennheden |
| Output / print | Curve Tolerance mm; print line width mm | outputflattening en printerrendering zijn andere instellingen |
| Camera / capture | Recommended / Fallback capture; default mode/framerate | bronselectie en drivercompatibiliteit |
| Camera / overlay | Black and White / Full Color; preview quality; preview FPS cap | geheugen/CPU-beheer; calibratie niet aan kleurbeleid koppelen |

Waargenomen getallen: autosave 2 min; grid 10 mm; snapgrid 1 mm; selection 3 px; objectsnap 10 px; keyboard increments 1/5/20 mm; DXF-close 0,01 mm; outputcurvetolerantie 0,05 mm; printlijn 0,2 mm; cameravoorkeur 20 fps; previewlimiet 2,5 fps. Bevestig grenzen en defaults in nieuw gebruikersprofiel voordat je deze als standaard overneemt.

## Ruida Device Settings — alle drie UI-tabbladen

### Dimensions / Units

- Werkgebied Width/Height: hier 1500/900 mm.
- Vier origin-keuzes. Bepaal de fysieke hoek via controllerconfiguratie; indices in accessibility bewijzen de hoek niet. Het screenshot toonde het groene origin-symbool rechtsboven.
- Machine Coordinates: tooltip verwijst naar een CNC met G54-offset. Niet aannemen dat de Ruida-datastroom G54 ondersteunt omdat dit gedeelde UI-element zichtbaar is.
- Control Units (hier mm/sec), Link Control and Design Units.
- Enable Z Axis; Relative Z Moves Only; Optimize Z Moves; OMTech Polar.
- Enable pointer offset met X/Y; Enable U Axis voor per-layer U offset.

**ONTWERP:** werkgebied/origin zijn profielwaarden. Pointeroffset hoort bij pointerpositionering/framing en mag niet stilzwijgend elke snijcoördinaat verplaatsen. Z-enable gate’t materiaalhoogte, Z-offset en Z-per-pass. Controllercapabilities bepalen of een getoonde keuze effectief is.

### Basic Settings

| Groep | Velden / opties |
|---|---|
| General | Frame Continuously; Tab Pulse Width; Overwrite Files on Device by Default; Enable Job Checklist + Edit; Save/Load Rotary Settings with Project Files |
| Scanning Offset Adjust | enable; Laser 1/2; tabel; Add/Delete/Import/Export |
| Delays | Start Delay en End Delay in seconden |
| Start/send | Disable Start Button; Increment filename on Send; Check bounds when framing |
| Laser 1 | naam (hier disabled); RF tube; Fiber |
| Laser 2 | Enable Controls; naam; Only allow one laser at a time; RF tube; Fiber; Enable Laser 2 Offset met X/Y |

Hier: Tab Pulse Width 0,05 mm; start/end delay 0; scanning offset uit; laser 2 controls uit. De RF/Fiber-tooltip noemt het onderdrukken van de min-power-zero-waarschuwing. Dit zijn geen keuzes om een hardwarebron fysiek om te zetten.

Scanning offset is een snelheidsafhankelijke compensatie voor verschillen tussen scanrichtingen. **ONTWERP:** interpolatietabel per laser en snelheid, met aparte behandeling van lijnshift. Leg exact teken, intervallering en extrapolatie vast met een rastertest; die details zijn hier niet experimenteel bewezen.

DOC noemt daarnaast Network Timeout en IP Address/Hostname in de DSP-context. Ze waren op deze seriële/Auto-profielweergave niet zichtbaar. Markeer dergelijke netwerkvelden conditioneel; verzin geen globale constante als UI-default.

### Additional Settings

De app toont expliciet dat dit **simulatie-/previewtiming** betreft en de controller niet verandert. Velden: maximum speed X/Y, rapid speed, cut acceleration X/Y, engrave acceleration X/Y, minimum corner speed, corner accuracy, global speed scale, cut speed scale, Read From Controller. AX-labels groeperen enkele velden; de individuele waarde-toewijzing van alle steppers is daarom niet als bewezen overgenomen.

**ONTWERP:** bewaar `SimulationProfile` apart van `ControllerSettingsSnapshot`. Een Read kan een simulatieprofiel invullen. Een editorwijziging van timing is geen Write naar machineparameters.

## Cut Settings — gedeelde kop

Naam en Output horen bij de layer. Elke sublaag heeft eigen naam/output en mode. Speed is fysiek een snelheid, ongeacht de UI-notatie. Min/max power zijn procentuele opdrachten, geen gemeten optische watts. Ruida Default Speed/Power kiest de controllerpaneelwaarden; niet vervangen door toevallig laatst gelezen getallen. Air Assist vraagt een controlleroutput, maar de feitelijke luchthardware moet aanwezig zijn.

De screenshot en dialogen bevestigden C00, één sublaag, 100 mm/s, min/max 20%, Air Assist aan. De laatste enabled sublaag kan niet worden verwijderd of uitgezet.

## Line — Common en Advanced

| Optie | Gedrag / eigen compilercontract |
|---|---|
| Number of Passes | herhaal de effectieve bewerking; respecteer cut-order en Z-per-passbeleid |
| Z Offset / Z step per pass | alleen met Z-capability; houd materiaalreferentie en relatieve/absolute modus apart |
| Kerf offset | verschuif gesloten snijcontour volgens winding/gatrelatie; verifieer UI-teken met binnen/buitenproef |
| Perforation Cut/Skip | splits booglengte in laser-aan/uit-intervallen; neem fase over segmentgrenzen mee |
| Tabs / Bridges | handmatig of automatisch; tabs per shape of even spacing; min/max tabs; spacing; size; skip inner; clear |
| Tab Cut Power | 0 slaat over; partial-power-tooltip noemt pulsen over de tablengte; Tab Pulse Width staat in Device Settings |
| Overcut | voorbij gesloten eindpunt doorsnijden op laatste pass; voorkom dubbel overcut op iedere pass |
| Start/End Pause | dwell ms; optioneel Cut Through met eigen power; aparte controller-delaysemantiek |
| Lead-In/Out | enable per einde; line/arc; lengte en hoek; relatie tot startpunt expliciet |
| Override PWM Frequency | Ruida-optie, kHz in UI; unitconversie naar backend verifiëren |
| Dot Mode | discrete pulsen op ruimtelijke afstand, met tijd/power; geen reguliere continue snijlijn |
| U Offset | per-sublayer verplaatsing mits ondersteund en geactiveerd |

DOC beschrijft daarnaast het instellen/activeren van lead start points en Cut Order Priority. Die kunnen buiten het geopende dialoogtabblad liggen. PPI is in DOC Trocen-specifiek en was niet aanwezig in het onderzochte Ruida-tabblad.

## Fill — Common en Advanced

UI: Bi-directional Fill, Cross-Hatch, Line Interval/LPI, Scan Angle, Number of Passes, Z Offset/Z step, Fill All Shapes at Once / Fill Groups Together / Fill Shapes Individually. Advanced: Ramp Length, Ramp Outer Edge, Flood Fill, PWM override, U Offset.

Hier 0,1 mm ↔ 254 LPI, hoek 0°, één pass. Ramp Outer Edge was disabled bij ramp 0. Voor Ruida ontbreekt de handmatige overscanswitch: de DSP behandelt die beweging. Bij scanhoeken buiten veelvouden van 90° beschrijft DOC emulatie via gewone cutting moves; verifieer dit met eigen export voordat je native scanning claimt.

Flood Fill is een andere padplanning. Cross-Hatch voegt 90° toe. Een LPI-wijziging moet dezelfde ene intervalwaarde wijzigen, niet een tweede onafhankelijk veld. Rounding op µm mag geen scanrijen dubbel maken.

## Offset Fill — UI + DOC

Opties: interval/LPI, passes, Z-offset/-step, grouping, PWM override, controllergebonden PPI en U. De contourgebaseerde fill kan geometrisch in meerdere eilanden uiteenvallen. Vereis begrensde iteratie en afbreken bij lege of niet-verkleinende offsets. Common/Advanced zijn aanvullend geopend. In dit Ruida-profiel waren grouping, scanhoek, cross-hatch, bidirectional, flood en ramp zichtbaar maar disabled; zie `06-dialoogcatalogus.md`.

## Image — UI + DOC

Image Settings en alle tien image modes zijn ook rechtstreeks geopend met de eigen toontrap. Opties: bidirectioneel, Negative Image, DSP-/controllergebonden overscan, interval/DPI, Dot Width Correction, Scan Angle, Angle Increment waar ondersteund, Z-offset, Cells per Inch, Halftone Angle, passes, Ramp Length, Pass-Through, beeldmodus, grouping, PWM override en U.

DPI = 25,4 / interval in mm. Dot Width Correction moet binnen de toegestane intervalgrens blijven. Grayscale vereist powerruns; dither gebruikt binaire runs. Beeldmodi en filterinstellingen kunnen verschillen per beeldobject terwijl de laag zijn eigen uitvoerinstellingen heeft. De eigen engine moet daarom objectpixelverwerking en laagparameters afzonderlijk opslaan.

## Machine Settings versus host settings

Machine Settings was leeg met communicatiefout. Read/Write, axis calibration en save/load/backups zijn gezien; de volledige registerboom is **niet** uit deze app verkregen. `Read` moet een schema met model/firmware en ruwe waarden opleveren. `Write` moet een expliciete diff van een gelezen snapshot toepassen en onbekende velden bewaren. Een geïmporteerd configuratiebestand mag niet als universeel profiel voor ieder Ruida-model worden toegepast.

De genoemde limieten en afhankelijkheden horen in dezelfde centrale settings-schema’s als UI, import/export en compiler. Iedere instelling krijgt: stabiele ID, type, eenheid, bereik, defaultbron, opslagscope, capabilityvoorwaarde en compilatie-effect. Een veld mag niet zichtbaar uitvoer beloven als de backend het niet ondersteunt.

Primaire bronnen: [Settings](https://docs.lightburnsoftware.com/2.1/Reference/SettingsPreferences/), [Device Settings](https://docs.lightburnsoftware.com/2.1/Reference/DeviceSettings/), [Shared Cut Settings](https://docs.lightburnsoftware.com/2.1/Reference/CutSettingsEditor/SharedSettings/), [Line](https://docs.lightburnsoftware.com/2.1/Reference/CutSettingsEditor/LineMode/), [Machine Settings](https://docs.lightburnsoftware.com/2.1/Reference/MachineSettings/). De lokale bronindex bevat alle subpagina’s.

## Adjust Image en Trace — aanvullend UI-bewijs

Adjust Image toonde Contrast, Brightness, Gamma, Enhance Amount en Enhance Radius, plus Image Mode, interval/DPI, Negative, halftonevelden en presets. In het opgeslagen Bitmap-object staat daarnaast `EnhanceDenoise`. De AX-veldlabels zijn deels gegroepeerd; de exacte koppeling van iedere onbenoemde slider is daarom niet als bewezen overgenomen. `Invert Display` wijzigt het previewbeeld en moet niet met de uitvoeroptie Negative Image worden verward. Trace Image toonde Cutoff 0, Threshold 128, Ignore Less Than 2, Smoothness 1, Optimize 0,2 en de genoemde transparency/sketch/previewopties. Dit zijn geobserveerde sessiewaarden, geen universele defaults.
