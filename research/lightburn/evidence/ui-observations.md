# Rechtstreekse appwaarnemingen

## Onderzoeksopstelling

UI-inspectie via native accessibility en screenshot: LightBurn Core 2.1.04, macOS, profielnaam Ruida, werkgebied 1500 × 900 mm. Aanvankelijk leeg, ongetiteld document. Status `No device found` / `Disconnected`. Er was geen bestaand ontwerp om te overschrijven.

De menu’s zijn daadwerkelijk geopend en hun accessibility-bomen gelezen. Dit omvatte LightBurn, File, Edit, Tools, Arrange, Laser Tools, Web Tools, Window, Language en Help, inclusief de door accessibility blootgelegde submenu’s. De geordende transcriptie staat in `01-menu-inventaris.md`; disabled is selectie- en contextafhankelijk en is geen bewijs van afwezigheid.

## Bekeken dialogen

| Oppervlak | Direct gezien | Uitkomst / grens |
|---|---|---|
| Settings | Editor Settings, Units and Grids, Display, Import / Export, Camera | Alle vijf tabbladen gelezen; afgesloten met Cancel |
| Device Settings | Dimensions / Units, Basic Settings, Additional Settings | Alle drie Ruida-tabbladen gelezen; afgesloten met Cancel |
| Cut Settings | Line, Fill en Offset Fill Common/Advanced; Image/Grayscale | Werkelijke waarden en afhankelijkheden gelezen |
| Rotary | Chuck/Roller, Enable, Mirror, Y, steps/rotation, diameter, circumference, Test, Read Settings | Instellingen konden niet uit controller gelezen worden; geen Test gestart |
| Auto-feed | Enable, Repeat Count, afstandsmodus, Distance Adjust, Pause After Feed, Feed Before Cutting | Uitgeschakeld; geen feed uitgevoerd |
| Focus Test | Voorwaardecontrole | Dialoog meldt dat Z-as ingeschakeld moet zijn; inhoud van testgenerator hier niet zichtbaar |
| Interval Test | stappen, min/max interval, maat, speed, power, Simple Fill/Dithered Image, Preview/Frame/Start/Send/Save | Alleen bekeken |
| Material Test Generator | presets import/export, titel, rij/kolomparameter en bereik, aantallen, dimensies, centrum, tekst/border/material settings | Alleen bekeken; geen Start, Send of Frame |
| Center Finder | eerste cirkelpunt, Reset, OK/Cancel | Geen posities gemeten |
| Machine Settings | Read, Write, Calibrate Axis, Save/Load/Backup | `Communication with controller failed`; parameterboom leeg; geen Write |
| Get Controller Info | statusquery | Fout bij lezen controllerstatus |
| Device Cameras | Add New, Edit, Remove | Lijst leeg; geen camerakalibratie uitgevoerd |
| Preview | traversal, power shading, invert, legend, playback speed, timeline, Save Image, Start here | Fill: 4500 mm cut, circa 1442 mm rapid, schatting 1:04 |
| Optimization | Layer/Group/Priority, herordening en alle aanvullende toggles | Gelezen; opties opgenomen in dossier |
| Adjust Image | Brightness, Contrast, Gamma, Enhance Amount/Radius, mode/interval/DPI, presets, Invert Display | Met eigen toontrap bekeken; Cancel |
| Trace Image | Cutoff/Threshold, Ignore Less Than, Smoothness, Optimize, transparency/sketch, Fade/Points/Boundary, Delete Image | Alleen bekeken; Cancel |
| Offset / Grid / Circular | alle dialoogvelden gelezen | Cancel; virtual arrays expliciet aanwezig |
| Bar Code Properties | hoofddialoog en 29 typekeuzes | tijdelijke barcodeproef verworpen |
| Import | bestandsfilter | `.ai .pdf .sc .dxf .gc .gcode .nc .hpgl .plt .rd .scpro2 .svg .lbrn .lbrn2 .mage .lbt .bmp .jpg .jpeg .png .gif .tif .tiff .tga .webp` |

## Testexport: handelingen

1. Eigen SVG gemaakt: rechthoek 30 × 15 mm, hoekpunten (10,20), (40,20), (40,35), (10,35).
2. Geïmporteerd in het aanvankelijk lege document. Een object geselecteerd; weergegeven maat 30 × 15 mm.
3. De app plaatste het object elders in het werkgebied. Numerieke positie en opgeslagen XForm zijn daarom leidend voor de RD-coördinaten.
4. Laag C00: Line; speed 100 mm/s; min/max power 20%; air assist aan; één pass. Save RD file leverde `rectangle-default.rd` op.
5. Cut Settings bekeken; modus naar Fill gezet. Interval 0,1 mm (254 LPI); hoek 0°; bidirectioneel aan; cross-hatch uit; één pass; flood fill uit; ramp 0. Overige speed/power ongewijzigd.
6. Save RD file leverde `rectangle-fill.rd` op. Save Project leverde `rectangle-fill.lbrn2` op.
7. Preview bevestigde 4500 mm cut distance. Dat komt overeen met 150 lijnen × 30 mm in de offline decode.

Er is geen hardwarejob gestart. De opgeslagen bestanden zijn de primaire meetdata; menu- en dialoogbeschrijvingen zijn een handmatig gestructureerde transcriptie, geen opgeslagen ruwe accessibility-log en geen screenshotarchief.

## Interpretatie die niet mag worden gemaakt

- De titel Core bewijst niet welke licentierechten op andere computers gelden. In deze sessie werkte offline Ruida-export daadwerkelijk.
- Console en Macros staan in Window, maar dat bewijst geen tekstconsole voor Ruida. De officiële documentatie beperkt die functies tot G-code.
- Een zichtbare Write-, Home-, Start- of Test-knop is geen bewijs dat de actie op hardware is uitgevoerd.
- Een mislukte automatische read is geen bevestiging van registeradressen, machinetype of firmwareversie.

## Aanvullend beeldexperiment

Een nieuw eigen document kreeg de 16×4 grayscale-toontrap. De Image-modekeuzelijst is volledig gelezen en alle tien modi kwamen overeen met de bronindex. Grayscale is geselecteerd; min 1% en max 60% zijn via de UI bevestigd. Save RD en Save Project leverden de gelijknamige `gray-ramp-grayscale`-fixtures. Adjust Image en Trace Image zijn daarna geopend en met Cancel gesloten. Er is geen hardwareactie uitgevoerd.
