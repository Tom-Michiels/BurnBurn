# Voorstel voor de 2D editor

De editor combineert **directe vectorbewerking** met **optionele parametrische schetsen**. Een logo moet snel te corrigeren zijn; een passing moet exact te wijzigen zijn. Het document kan beide soorten geometrie tegelijk bevatten.

## Zinvolle principes uit Onshape en Fusion

Onshape documenteert onder meer geometrische constraints, automatische relaties tijdens tekenen, toestandweergave en een constraintmanager. Bemating kan sturend of refererend zijn. Fusion documenteert parameters met expressies, associatieve projecties en beoordeelbare AutoConstrain-resultaten. Dit zijn de referentieprincipes; de onderstaande toepassing op BurnBurn is een eigen ontwerp. [Onshape constraints](https://cad.onshape.com/help/Content/Sketch/working_with_constraints.htm), [bemating](https://cad.onshape.com/help/Content/Sketch/dimension.htm), [Fusion parameters](https://help.autodesk.com/cloudhelp/ENU/Fusion-Model/files/SLD-MODIFY-CHANGE-PARAMETERS.htm), [projecties](https://help.autodesk.com/cloudhelp/ENU/Fusion-Sketch/files/SKT-SKETCH-CREATE-PROJECT-INCLUDE.htm), [AutoConstrain](https://help.autodesk.com/cloudhelp/ENU/Fusion-Sketch/files/REF-AUTO-CONSTRAIN.htm).

| Idee | Waarde voor laserwerk | Voorstel voor BurnBurn | Besluit |
|---|---|---|---|
| Horizontaal, verticaal, coincident | Rechte panelen en gesloten hoeken | Tijdens tekenen voorstellen, tijdelijk onderdrukbaar | Eerste versie |
| Gelijke lengte/radius | Gaten en vingers uniform houden | Eén maat kan de hele geselecteerde familie sturen | Eerste versie |
| Parallel en loodrecht | Sleuven, randen en gatenpatronen | Beschikbare vrijheid zichtbaar houden bij verslepen | Eerste versie |
| Tangent | Vloeiende aansluitingen | Raakrelatie tussen lijn, boog en ondersteunde curve | Uitbreiden na basis |
| Symmetrie en middenpunt | Gecentreerde uitsparingen | Spiegelas als construction geometry | Eerste versie |
| Driving/driven dimensions | Maat wijzigen versus meten | Sturende maat en meetmaat duidelijk verschillend | Eerste versie |
| Parameters en expressies | Dikte- en maatvarianten | Namen, eenheden, afhankelijkheden en waarden samen tonen | Eerste versie |
| Construction geometry | Hulplijnen zonder productie | Eigen geometrische rol; nooit automatisch output | Eerste versie |
| Project/Use-referenties | Onderdelen op elkaar laten aansluiten | Associatieve 2D-referentie met bron-ID en breukmelding | Na basis |
| Constraintdiagnose | Begrijpen waarom iets vastzit | Betrokken relaties markeren en herstel voorstellen | Eerste versie |
| Trim, extend, offset, fillet | Dagelijkse voorbereiding | Live geometriepreview; bronrelaties behouden waar mogelijk | Eerste versie, live varianten later |
| Associatieve patronen | Herhaalde gaten en producten | Aantal, stap en rotatie als parameters | Na basis |
| AutoConstrain | Import sneller bruikbaar maken | Beoordeelbaar voorstel op een selectie, binnen mm-tolerantie | Geavanceerd |
| 3D-vlakken, assemblies en surfacing | Beperkte eerste waarde | Geen volledige 3D-CAD-interface | Buiten eerste scope |

## Direct tekenen

Lijn, polylijn, rechthoek vanuit hoek of centrum, afgeronde rechthoek, cirkel, driepuntsboog, tangentboog, ellips en Bézierpad vormen de basis. Tijdens tekenen kan de gebruiker lengte en hoek typen, ook als expressie. Een meetwaarde laat eenheid en precisie zien; komma en punt volgen een ondubbelzinnige localeparser.

Snappen werkt op eindpunten, middens, centra, kwadranten, snijpunten en hulplijnen. Bij drukke tekeningen kan de gebruiker doelen filteren en overlappende kandidaten doorlopen. Het canvas benoemt het gekozen doel. Een snap is een tijdelijke plaatsingshulp; een blijvende constraint krijgt een eigen zichtbaar symbool.

Selectie werkt met klik, doorklikken, selectiekader, lasso en filters voor object, pad, knoop, proceslaag en onderdeel. De objectboom en het canvas tonen steeds dezelfde selectie. Vergrendelen, isoleren en tijdelijk vervagen helpen bij complexe bestanden.

## Nodebewerking die dagelijks prettig is

Iedere knoop toont zijn type: hoek, glad of symmetrisch. Handvatten zijn rechtstreeks te bewerken; numerieke invoer blijft beschikbaar. Segmenten kunnen worden gesplitst, verbonden, omgekeerd of vervangen door lijn/boog/Bézier. Eén opdracht vereenvoudigt geselecteerde stukken binnen een opgegeven maximale afstandsfout.

Een lokale wijziging mag de rest van een lange illustratie niet opnieuw fitten. Daarom werken bewerkingen op expliciete segmentbereiken met vaste grensankers. Een gebruiker kan kwetsbare hoeken en kleine details beschermen. Een afwijkingsoverlay laat zien wat vereenvoudiging verandert.

Geslotenheid, knoopaantal en fysieke afmetingen zijn zichtbaar. “Verbind deze twee eindpunten” verschilt van “sluit dit pad met een nieuw segment”. Duplicaten of zelfdoorsnijdingen worden gelokaliseerd voordat een reparatie wordt toegepast. Selectie en zoom blijven bij dezelfde plek na de reparatie.

## Parametrische sketches

Een Sketch bevat entiteiten, constraints en dimensies in een eigen lokaal coördinatenstelsel. De rest van de tekening kan vrij blijven. Verplaatsing van een heel onderdeel wijzigt zijn plaatsing, niet de interne maten.

Minimale eerste set: coincident, horizontaal, verticaal, parallel, loodrecht, gelijke lengte/radius, concentriciteit, symmetrie, middenpunt en fix; daarnaast afstand, lengte, hoek, radius en diameter. Tangentie volgt zodra de lijn-/boogbasis stabiel is; complexe splineconstraints worden later toegevoegd.

De solver rapporteert vrije, volledig bepaalde, redundante, conflicterende en niet-opgeloste toestanden. De laatste geldige geometrie blijft zichtbaar bij een fout. Een niet volledig bepaalde schets mag geproduceerd worden als zijn huidige geometrie geldig en bewust gekozen is. “Volledig bepaald” is een ontwerphulpmiddel, geen algemene productievoorwaarde.

Een overbepaalde schets toont een voldoende verklarende conflictset; de app belooft geen wiskundig minimale set als de gekozen solver die niet kan leveren. Voorstellen laten zien welke relatie kan worden verwijderd en wat dan beweegt. De gebruiker hoeft geen cryptische solvercodes te interpreteren.

## Parameters en pasvorm

Voorbeeld:

```text
breedte = 120 mm
hoogte = 80 mm
materiaaldikte = 3.1 mm
speling = 0.12 mm
sleufbreedte = materiaaldikte + speling
gatdiameter = 4 mm
randafstand = 8 mm
```

`speling` wijzigt de bedoelde nominale passing. Kerfcompensatie corrigeert daarna het productiepad voor de gemeten snijbreedte. Deze twee stappen krijgen verschillende velden, previewlagen en uitleg. Positieve speling maakt in dit voorbeeld de nominale sleuf breder; kerf mag niet nogmaals als ontwerpmaat worden ingevoerd.

Parameters hebben scope: werkplaatsrecept → projectsnapshot → onderdeel → variant. Het project toont de effectieve waarde en herkomst. Bij materiaaldikte uit een recept wordt de gemeten projectwaarde pas vervangen na een zichtbare keuze. Expressies krijgen eenhedencontrole, cyclusdetectie en afhankelijkheidsweergave.

## Relaties en stabiele identiteit

Een gat wordt gekoppeld aan een benoemde rand of geometrische entiteit, niet aan “de zevende lijn in het resultaat”. Wijzigt een boolean of trace de topologie, dan probeert de app de referentie alleen te herstellen met voldoende bewijs. Bij ambiguïteit toont zij de betrokken afhankelijkheid en vraagt ze een nieuwe selectie.

Een vrije geïmporteerde contour kan als vaste referentie in een kleine schets worden gebruikt. De gebruiker hoeft de hele import niet parametrisch te reconstrueren. Omgekeerd kan een schets als afgeleide vrije kopie worden bewerkt; de originele schets blijft behouden en de nieuwe kopie toont dat hij losstaat.

## Niet destructieve bewerkingen

Een modifierketen kan bijvoorbeeld zijn: bronpad → spiegelen → offset → boolean → patroon. Iedere stap heeft parameters, eigen resultaat-ID's en een fouttoestand. De gebruiker kan een stap uitschakelen en naar de bron terugkeren. Undo herstelt één inhoudelijke actie; het slepen van een slider maakt niet honderden permanente stappen.

Er zijn twee soorten offset. Een ontwerpoffset maakt een geometrisch afgeleid object, bijvoorbeeld een rand om een naamplaat. Kerf is een CAM-afleiding van een bewerking. De inspecteur benoemt welk type wordt gewijzigd.

## Tekst en laserconstructies

Tekst blijft tekst zolang dat kan: font, uitlijning, regels, tekenafstand, padtekst en variabele velden. Bij een ontbrekend font blijft een opgeslagen contourweergave bruikbaar, met een melding dat opnieuw zetten kan veranderen. Omzetten naar contouren bewaart desgewenst de bewerkbare tekstbron. Open enkelvoudige graveerletterpaden worden als aparte font-/padcategorie aangeboden.

Laserspecifieke generators maken onderdelen met wijzigbare intentie: doosverbindingen, tab/slot, finger joints, stencilbruggen, gatenpatronen, living hinges en testcoupons. Zij houden rekening met minimale details en geven ongeldige parametercombinaties aan. Een generator levert gewone inspecteerbare geometrie en blijft als bronobject bestaan.

## Wat het eerste editorbewijs moet aantonen

Een montageplaat verandert van breedte terwijl gaten symmetrisch blijven. Een sleuf reageert op materiaaldikte. Een logo kan op knoopniveau worden hersteld zonder constraints. Beide staan in hetzelfde document, worden opgeslagen en opnieuw geopend, behouden proceskoppelingen en leveren dezelfde nominale geometrie op. Een opzettelijk constraintconflict is gericht op te lossen zonder verlies van het laatste geldige ontwerp.
