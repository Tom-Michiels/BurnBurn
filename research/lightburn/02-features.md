# Functionele analyse

Bewijs: **UI**, **DOC** en waar vermeld **RD**. De technische oplossingen zijn **ONTWERP**, tenzij expliciet als waarneming beschreven. De [bronindex](reference/feature-index.md) koppelt alle geraadpleegde referentiethema’s aan de officiële pagina’s. De huidige analyse richt zich eerst op een XY-laser met Ruida en legt daarnaast de productbrede families vast.

## 1. Werkruimte en documentbewerking

Een werkruimte bevat een fysiek machinebed, ontwerpobjecten, hulpmiddelen en een zichtbare viewport. Zoom, pan, fit-to-page en fit-to-selection veranderen de viewport, nooit de fysieke maten. Wireframe/Filled en Coarse/Smooth zijn weergavekeuzes; uitvoercurvetolerantie is een afzonderlijke parameter.

Selectie ondersteunt klikken, meervoudige selectie, gebiedsselectie, omkeren, alle objecten van een laag, open paden, open paden met Fill, ingesloten objecten, kleinere vormen en cirkels. Een herbouw heeft hit-testing van lijnen én vlakken nodig, inclusief subgroepen, lock-state en selectietolerantie in schermpixels. Een object kan zichtbaar maar niet selecteerbaar zijn; uitvoerbaarheid staat daar weer los van.

Transformaties: slepen, numerieke X/Y, breedte/hoogte, procentuele schaal, aspectlock, rotatie, shear en een 3×3-referentiepunt. Vier hoofdschakelaars beperken Move/Size/Rotate/Shear. Snapping kan objecten of het grid volgen; tijdelijke hulplijnen ondersteunen uitlijning. Houd documenteenheden intern consistent en reken pixel-snaptoleranties om via de viewport.

Groepen bewaren hiërarchie; Ungroup verwijdert een hiërarchieniveau. Auto-Group gebruikt insluiting. Break Apart splitst samengestelde geometrie in afzonderlijke elementen. Render-volgorde, objecthiërarchie, laagvolgorde en uitvoerprioriteit zijn vier verschillende zaken.

Undo/Redo en Undo History horen het volledige bewerkbare document terug te brengen, inclusief geometrierelaties, instellingen en selectie waar toepasselijk. Gebruik samengestelde transacties voor een import, boolean of array. Autosave heeft herstelgedrag nodig en is geen vervanging voor expliciet Save.

## 2. Vectorgeometrie en gereedschappen

| Familie | Functioneel contract | Algoritmische herbouw |
|---|---|---|
| Basisvormen | rechthoek, ellips, driehoek, vijfhoek, regelmatige veelhoek, achthoek, ster, dubbele ster | parametrische objecten met bewerkbare vormparameters; pas op verzoek naar paden omzetten |
| Draw Lines | polyline en gebogen segmenten; afsluiten of open laten | pad met nodes en lijn/Béziersegmenten, duidelijke begin/eindpunten |
| Edit Nodes | toevoegen, verwijderen, verplaatsen, smooth/corner, curve↔line, break, trim, extend, align | topologische bewerking met behoud van subpadidentiteit en sluiting |
| Close/Join | open uiteinden sluiten of nabije uiteinden koppelen | ruimtelijke index, expliciete tolerantie; geen onbedoeld verbinden van nabije losse ontwerpen |
| Optimize Shapes | eenvoudiger lijnen/curven binnen foutmarge | fitting of vereenvoudiging met maximaal geometrisch foutbudget |
| Offset | binnen/buiten/beide, afstand en hoekbehandeling | robuuste polygon-offset; detecteer verdwenen gebieden en zelfsnijdingen |
| Weld/Boolean | union, subtract, intersection, assistentpreview | oppervlakteboolean; subtract is operandvolgorde-afhankelijk |
| Cut Shapes | selectie doorsnijden met een snijvorm | segmentintersecties, splitsing en herclassificatie van resulterende delen |
| Trim/Extend | segment tot volgende kruising verwijderen/verlengen | snijpunten op onderliggende krommen; voorkom onbedoelde auto-join |
| Round Corners | gecontroleerd afronden van hoeken | geometrische fillet met limiet bij korte aangrenzende segmenten |
| Start Point/Direction | snijstart en padoriëntatie aanpassen | apart metadata bewaren; planner respecteert expliciete keuzes |
| Tangent Circle | raakcirkel aan gekozen geometrie | oplosser met meerdere oplossingen, tekenrichting en degeneratieafhandeling |
| Warp 4 points | vierhoekige vervorming | bilineaire/projectieve mapping volgens gekozen eigen contract; LightBurn-exacte mapping OPEN |
| Deform 16 points | vervormbaar 4×4-controleveld | tensorpatch of vergelijkbaar continue vervorming; sampling/fitting met foutmarge |
| Resize Slots | sleuf-/tabmaten aan plaatdikte aanpassen | detecteer sleufgeometrie en wijzig lokale maten, niet de hele vorm schalen |
| Measure | afstanden, bounds en padmetingen | lengte/oppervlak met geometrische tolerantie; klikpunten en hoverdetails |

Een kwalitatieve herbouw valt of staat met degeneraties: samenvallende punten, bijna parallelle lijnen, piepkleine contouren, gaten, tangentiële aanrakingen, reflecties en gemengde open/gesloten paden. Renderen mag een probleem niet verbergen dat later een andere snijtopologie oplevert.

## 3. Ordenen, dupliceren en nesting

Align gebruikt randen of centra. Distribute verdeelt centra of vrije tussenruimte, horizontaal/verticaal. Move Together verwijdert tussenruimte. Dock beweegt tot de relevante geometriegrens. Maak de keuze van referentieobject en de behandeling van groepen expliciet; verifieer die tegen meerdere selecteerordes.

Grid Array gebruikt rijen/kolommen en offsets of spacing. De UI biedt ook virtuele arrays, afwisselende spiegelingen, rij-/kolomshifts, random orientation en automatische variable-text-offsets; zie de dialoogcatalogus. Circular Array roteert kopieën rond een centrum met instelbare verdeling. Copy Along Path verdeelt instanties langs booglengte en kan hun oriëntatie laten meedraaien. Geef vooraf zichtbare previews en behandel een array als één undo-actie.

Nest Selected en Quick Nest zijn afzonderlijke workflows. De Quick Nest-documentatie noemt onder meer Move as Group, Lock Inner Objects, Lock Objects For Rotation, Padding en Edge Buffer en een asuitgelijnde doelrechthoek. Een herbouw moet niet alleen bounding boxes stapelen: gebruik echte contourbotsingen, gaten en materiaalranden, en meld objecten die niet passen. Ontwerp een deterministische seed, annuleerbare optimalisatie en een resultaat dat bruikbaar blijft als niet alles geplaatst is.

De rubber-band outline is een omhullende rond objecten. Gebruik een convex hull als eerste eigen implementatie; exacte LightBurn-uitkomst bij verschillende typen selectie blijft een verificatiepunt.

## 4. Tekst, variabele tekst en barcodes

De teksttoolbar toont font, hoogte, bold, italic, uppercase, horizontale/verticale uitlijning, HSpace/VSpace, Welded, Distort en een type voor variabele tekst. Tekst kan aan een pad worden gekoppeld; kromming mag de lettervormen volgens de Distort-keuze wijzigen. SHX-fontpaden en RTL-ondersteuning zijn expliciete voorkeuren.

De engine moet Unicode-shaping, kerning, fontfallback, baseline en multilinetekst ondersteunen. Bewaar zowel de tekst als de gebruikte fontreferentie. Convert to Path legt de huidige glyphcontouren vast, zodat uitvoer niet afhankelijk blijft van een later veranderd font. Een enkel lettergat mag bij omzetting niet dichtgevuld worden.

Variabele tekst kent serials, datum/tijd, CSV/merge en verwijzingen naar cut settings. Het paneel bevat Current, Start, End, Advance By, Auto-Advance, Previous/Next/Reset, Test en Bake, plus CSV kiezen/wissen. Objecten hebben hun eigen offset; automatische offsets helpen bij arrays.

**ONTWERP-contract:** werk met een immutable `ProductionRecord` per job. Resolve alle tekst vóór de preview/compile en verbind de job aan CSV-rij, tijdzone en serienummer. Advance een teller op een gedefinieerde gebeurtenis, niet na een mislukte upload. De exacte LightBurn-gebeurtenis waarop Auto-Advance verhoogt moet nog differentieel worden getest. Bake vervangt dynamische inhoud door vaste inhoud en hoort undo-baar te zijn. CSV ondersteunt quoted velden, scheidingsteken, UTF-8 en ontbrekende waarden met een foutmelding in plaats van onbedoelde lege labels.

Barcode/QR-generatie vereist valide payloadencoding, foutcorrectie waar toepasselijk, quiet zones en vectormodules met echte fysieke maten. Neem een bestaande geteste barcodebibliotheek en valideer uiteindelijke leesbaarheid; een visueel gelijkend patroon volstaat niet. De 29 waargenomen symbologieën staan in `06-dialoogcatalogus.md`; typeafhankelijke subformulieren zijn nog niet allemaal doorlopen.

## 5. Beelden en tracing

Importfilters zijn direct waargenomen; inhoudelijke ondersteuning per variant is nog te testen. SVG, DXF, AI/PDF, HPGL/PLT en bitmapformaten vereisen aparte importers. De appfilter vermeldt daarnaast `.sc`, `.scpro2`, `.mage`, project/templates en machinebestanden. Een filtervermelding bewijst geen verliesloze import van ieder subtype.

Adjust Image en Shape Properties leveren beeldbewerking en presets. Implementeer helderheid/contrast/gamma, schaal/resampling, kleur-naar-grijs, scherpte en eventuele aanvullende velden pas volgens de bronindex en tests. Mask verbindt beeld met vectorclip; Crop verandert de beeldgrens. Refresh en Replace bewaren de plaatsingsrelatie; Replace to Fit is een afzonderlijk schaalbeleid. Save Processed Bitmap moet de daadwerkelijk verwerkte pixels exporteren.

Trace Image heeft Cutoff/Threshold, Ignore Less Than, Optimize, Smoothness, transparantie, Sketch Trace, Boundary en previewhulpmiddelen. Bouw tracing als pipeline: luminantie/alpha → drempel- of randmasker → verbonden gebieden → contourhiërarchie → vereenvoudiging/curvefitting. Zorg dat gaten gaten blijven en kleine ruis op expliciete oppervlakte-/maattolerantie wordt gefilterd. Multi-File Trace (Labs) voegt batchinvoer, outputbestanden, fouten per bestand en voortgang toe.

Image engraving modes uit DOC: Threshold, Ordered, Atkinson, Dither, Stucki, Jarvis, Newsprint, Halftone, Sketch en Grayscale. De kernel en exacte afronding van LightBurn zijn niet uit de gesloten implementatie bewezen. Voor een vergelijkbare tool gebruik je expliciet gedocumenteerde varianten en golden images. Dithering moet na de fysieke uitvoerschaal plaatsvinden, anders verandert het puntenpatroon bij export.

Grayscale is vermogensmodulatie tussen min/max, geen binaire dither. Negative Image wisselt de tooninterpretatie. Dot Width Correction verkort gegraveerde runs om beam width te compenseren. Pass-Through heeft een eigen resamplingcontract nodig. Leg alpha-compositie, gamma-/kleurruimte, pixelcentra en volgorde van filters vast; verschillen van één pixel kunnen bij herhaald graveren zichtbaar zijn.

## 6. Lagen en bewerkingsmodi

Er zijn 30 kleurknoppen C00–C29 en twee tool layers T1/T2 waargenomen. Bewaar layer-ID onafhankelijk van RGB. Tool layers zijn tekenhulp en horen niet automatisch in machine-output. Een laag heeft onder meer naam, uitvoer, weergave, air assist, snelheid, min/max power, modus en geordende sublagen.

- **Line:** volgt contouren. Passes, kerf, tabs/bridges, perforation, lead-in/out, overcut, dot/dwell en Z/U bepalen het uiteindelijke pad.
- **Fill:** zet gesloten contouren om naar parallelle scanruns. De officiële interactie binnen dezelfde laag is parity-achtig: ingesloten gebieden en overlappende delen worden uitgespaard. Verschillende lagen worden onafhankelijk uitgevoerd en kunnen dubbel graveren.
- **Offset Fill:** vult door opeenvolgende contour-offsets. Detecteer gesplitste binnengebieden en beëindig op lege offsetresultaten; het is geen rasterhatch.
- **Image:** zet verwerkte pixels om in on/off-runs of powerruns, met scanhoek en interval.
- **Sub-Layers:** meerdere bewerkingen op dezelfde geometrie, met volgorde, naam, dupliceren, verwijderen en enable. De UI liet niet toe de enige ingeschakelde sublaag te verwijderen of uit te schakelen.

Default layer settings zijn per device en kleur; Image en niet-Image hebben gescheiden defaults volgens de UI-tooltip. Reset Default, Make Default en Make Default for All zijn drie verschillende wijzigingen. Shift-Reset verwijst naar ingebouwde defaults. Defaults mogen later opgeslagen projectinstellingen niet stilzwijgend overschrijven.

## 7. CAM en uitvoeroptimalisatie

Gebruik een planner die geometrie vertaalt naar concrete bewerkingen. Order by Layer, Group en Priority is een **geordende hiërarchie**: Layer→Priority geeft een andere planning dan Priority→Layer. Binnen deze buckets volgen inner-first, richting, travelreductie, startpuntkeuze, hoekenvoorkeur, omkeren, minder richtingswissels en backlashheuristiek.

De aanbevolen eigen aanpak is een constraint-DAG voor must-before, met deterministische heuristieken voor reistijd binnen toegestane groepen. Winst mag een gat niet ná de buitencontour plaatsen als inner-first actief is. Handmatige startpunten en verboden richtingen begrenzen de optimalisatie. Remove Overlapping Lines werkt op de geplande uitvoer en heeft afstandstolerantie; verwijder niet de brongeometrie uit het project.

Fill Grouping bepaalt of alle vormen, rootgroepen of afzonderlijke vormen samen worden gescand. Flood Fill deelt gebieden op om grote lege reizen te vermijden; dit verandert het lokale scanpad en kan naden zichtbaar maken. Cross-Hatch voegt een tweede scanrichting toe. Ramp Length moduleert het vermogen aan de randen voor bijvoorbeeld stempels.

Preview moet dezelfde operationele data gebruiken als export. Anders kun je tabs, maskers, selection-origin of omitted layers correct tekenen maar verkeerd uitsturen. Timing is een schatting met speed, acceleration, corner speed, delays en controller-overhead. De preview-instellingen in Device Settings schrijven volgens de UI geen machineacceleraties.

## 8. Positionering en machinebediening

Laser Window bevat Start, Stop, Pause, Send, rechthoekframe/rubber-band-frame, Save/Run machine file, Home, Go to Origin, Start From, 3×3 Job Origin, Cut Selected Graphics, Use Selection Origin, Show Last Position en Optimization Settings. Ruida toont Save/Run RD. Start en Send zijn andere gebruikersintenties: een benoemd bestand bewaren is niet hetzelfde als uitvoering starten.

Absolute Coordinates, Current Position en User Origin moeten eigen referentiestelsels blijven. Een 3×3-jobanchor is een punt op de jobbounds, niet de oorsprong van het machinebed. Selection Origin kiest welke bounds de plaatsing bepalen; Cut Selected kiest welke objecten uitgevoerd worden. Stel filters en origin samen voordat bounds worden gecompileerd.

Move levert jog en numerieke verplaatsing; Position Laser koppelt een canvasklik aan een controllerbeweging. Deze acties vereisen verbinding, coherente coordinaten en bevoegdheden binnen de backend. Een offline editor moet beschikbaar blijven zonder verbinding.

File List beheert bestanden in DSP-opslag en is verschillend van lokale projecten. Support voor listing, metadata, send, run en delete moet per firmware bewezen worden. Een herstelprocedure voor een onderbroken upload is essentieel; netwerksucces betekent niet automatisch dat een machinejob valide of uitgevoerd is.

## 9. Camera en registratie

LightBurn 2.1 documenteert camera’s met aparte bron, lenscalibratie en workspace-alignments. Het menu toont Manage Cameras; de onderzochte lijst was leeg. Herbouw het niet uitsluitend als één webcam-overlay.

Lenscalibratie schat lensvervorming uit patroonbeelden; workspace alignment koppelt gecorrigeerde beeldpunten aan fysieke bedposities. Bewaar meerdere calibraties/alignments, camera-ID, resolutie, framerate en materiaalhoogte. De documenten noemen onder andere hoogte-interpolatie, warm-up frames en reframing van het calibratieveld. Een veranderde camera of resolutie moet een bestaande calibratie ongeldig kunnen maken.

Cameras Window heeft update/trace/adjust overlay, DPM, opacity, afmetingen, shifts, preview en capture. Meerdere camera’s moeten naar één bedcoördinatenstelsel worden getransformeerd, met duidelijk gedrag in overlappende gebieden. Een head-mounted camera heeft een aparte workflow omdat de camera met de kop meebeweegt.

Print and Cut registreert twee fysieke targets op ontwerptargets. Een eigen similarity transform gebruikt translatie, rotatie en optioneel uniforme schaal; de no-scaling-keuze fixeert de schaal. De exacte LightBurn-matrix en foutafhandeling blijven een verificatiepunt. Twee samenvallende of zeer nabije targets moeten worden afgewezen; na reset mag geen registratietransform actief blijven.

Center Finder gebruikt gemeten randpunten om een cirkelcentrum te vinden. Gebruik minstens drie niet-collineaire punten en toon residuals bij overbepaalde meting. Het eerste puntenscherm is UI-waargenomen; de verdere meetflow is nog niet uitgevoerd.

## 10. Rotary, feeder, testen en productiebibliotheken

Ruida Rotary toont Chuck/Roller, Y-as, steps per rotation, diameter/circumference, mirror en Test. Het wijzigen kan controllerparameters raken; machineprofiel en tijdelijke jobinstellingen moeten gescheiden worden. Voor een chuck is omtrek πD; voor rollers bepaalt de rolomtrek de overbrenging. Steps per revolution zijn motormechanica, niet beeld-DPI. Bouw terugzetten naar vlakbedconfiguratie in als expliciete lifecycle.

Auto-feed heeft Enable, Repeat Count, automatische Job Height of handmatige afstand, Distance Adjust, Pause After Feed en Feed Before Cutting. Vereis capability-detectie; een zichtbare U-as betekent niet dat iedere machine een conveyor heeft.

Material Test genereert een matrix over twee parameters met min/max/count, vakmaten, labels en borders. Interval Test varieert scaninterval voor eenvoudige Fill of geditherd beeld. Focus Test heeft Z nodig. Gebruik dezelfde operationele compiler, maar houd tijdelijke testinstellingen gescheiden van het lopende document. Presets import/export horen bij de workflow.

Material Library bewaart herbruikbare instellingen per materiaal en dikte; onderscheid een gekopieerde instelling van een gelinkte bibliotheekwaarde. Art Library bewaart herbruikbare ontwerpassets met groepen, thumbnails en projectplaatsing. Geef bibliotheken versiebeheer/schema-migratie en een padbeleid dat werkt als bestanden verplaatst worden. Bundles verpakken geselecteerde resources voor overdracht tussen computers.

Variabele tekst + arrays + material presets + machinefile send vormen samen batchproductie. Leg jobidentiteit, herhalingen, voortgang en tellercommit vast om een onderbroken productie niet dubbel of met verkeerde serienummers te hervatten.

## 11. Andere controllerfamilies en productfuncties

De documentatie-index omvat G-code-instellingen, custom scripts/macros, Console, galvo-port/timinginstellingen, COR/lenscalibratie, frequentie/Q-pulse, wobble, 3D sliced engraving, repeat/split marking, cylinder correction en taper warp. Deze worden niet door de Ruida-backend gerealiseerd. Een tool met de volledige productbreedte heeft minstens een G-code-backend en een afzonderlijke galvo motion/timing-backend nodig.

Niet-geobserveerde profielen blijven **DOC/OPEN**. Het model moet capabilities gebruiken: `supportsPPI` hoort bij de gedocumenteerde Trocen-optie, `supportsPWMOverride` bij Ruida; een galvo-optie zoals Q-pulse kan niet zomaar naar een CO₂-Ruida worden vertaald. De native Split Marking-toolbarbutton is waargenomen, maar dat bewijst niet dat Ruida de volledige gedocumenteerde galvoworkflow ondersteunt.

Taalkeuze, toegankelijke labels, hotkey-editor, help, EULA, licenties, updates, supportbundels, debuglog en realtime statistics horen bij het product. Voor functionele herbouw kunnen licentie- en updatesystemen een eigen ontwerp krijgen. Het kopiëren van proprietary assets, branding of licentieservers is niet nodig voor gelijkwaardige ontwerp- en laserfunctionaliteit.

## Aanvullende primaire bronnen

- [Editor en werkruimte](https://docs.lightburnsoftware.com/2.1/Reference/UI/Workspace/)
- [Quick Nest](https://docs.lightburnsoftware.com/2.1/Reference/QuickNest/)
- [Variable Text](https://docs.lightburnsoftware.com/2.1/Reference/VariableText/)
- [Image Mode](https://docs.lightburnsoftware.com/2.1/Reference/CutSettingsEditor/ImageMode/)
- [Fill Mode](https://docs.lightburnsoftware.com/2.1/Reference/CutSettingsEditor/FillMode/)
- [Optimization Settings](https://docs.lightburnsoftware.com/2.1/Reference/OptimizationSettings/)
- [Camera’s](https://docs.lightburnsoftware.com/2.1/Reference/Cameras/)
- [Laser Window](https://docs.lightburnsoftware.com/2.1/Reference/LaserWindow/)

Alle overige gebruikte onderwerpen en exacte URLs staan in de lokale bronindex. Een link in dit dossier is geen claim dat ieder daarin genoemd gedrag hier experimenteel geverifieerd is.
