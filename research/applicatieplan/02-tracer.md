# Plan voor een hoogwaardige tracer

**Het doel is bewerkbare, fysiek passende lasergeometrie uit beelden halen.** Een mooi SVG-plaatje alleen is onvoldoende: dubbele paden, verdwenen gaten en duizenden onnodige knopen zijn directe productieproblemen.

LightBurn documenteert al threshold/cutoff, transparantie, centerline, Sketch Trace, ruisfiltering, gladheid en optimalisatie. Het voorgestelde verschil zit daarom in blijvende bronkoppeling, herstel per gebied, controle van topologie en fysieke afwijking, en technische reconstructie. [LightBurn Trace Image](https://docs.lightburnsoftware.com/2.2/Reference/TraceImage/).

## Vier werkstromen

| Werkstroom | Invoer | Gewenste uitvoer | Gevaar dat zichtbaar moet zijn |
|---|---|---|---|
| Silhouet en contour | Logo, sticker, zwarte vorm of alpha | Gesloten contouren met correcte gaten | Kleine gaten verliezen of ruis als onderdeel behouden |
| Centerline | Handschrift, lijntekening, gravure | Open en gesloten paden op het midden van streken | Dubbele lijnen, sporen en verkeerd gekoppelde kruisingen |
| Technische reconstructie | Scan van een vlakke technische tekening | Lijnen, cirkels, bogen en optionele CAD-relaties | Onbewezen maten of symmetrie aannemen |
| Kleurregio's | Meerkeurig logo of illustratie | Afzonderlijke regio's en gedeelde grenzen | Dezelfde grens twee keer laseren |

Een foto krijgt eerst de vraag of rastergravure, uitgesneden silhouet of gestileerde vectorgebieden het doel is. Een volledige foto in vectorvorm is niet automatisch de beste laserbewerking. De gebruiker kan verschillende werkstromen op geselecteerde gebieden combineren.

## Bedieningsconcept

De tracer opent naast het canvas met vier vergelijkbare beelden: bron, bewerkt masker, vectorresultaat en afwijkingskaart. Split view en een verschuifbare voor/na-grens houden dezelfde zoom en positie. Punten, gaten, eindpunten en kruisingen zijn afzonderlijk zichtbaar te maken.

De gebruiker kiest eerst fysieke maat of kalibratieafstand, daarna doel en kwaliteitsprofiel. De basisweergave toont achtergrond, drempel, minimaal detail en nauwkeurigheid. Geavanceerde instellingen zijn gegroepeerd per processtap. Elk resultaat meldt aantal paden, gesloten vormen, gaten, knopen, niet-beoordeelde probleemlocaties en kleinste gevonden detail in millimeters.

De laatste geldige preview blijft zichtbaar tijdens een berekening, met de aanduiding dat nieuwe instellingen worden berekend. Oudere resultaten mogen geen nieuwere instelling overschrijven. Annuleren houdt het document intact. Toepassen maakt één undoactie en bewaart bron, recept, engineversie en correcties.

## Verwerkingsketen

```text
Bronbeeld en fysieke schaal
  → oriëntatie, alpha, kleurbeleid en optionele perspectiefcorrectie
  → achtergrondschatting, ruisonderdrukking en contrastnormalisatie
  → globale/lokale drempel of kleursegmentatie
  → contourboom / centerlinegraaf / technische kandidaten
  → lijn-, boog- en curvefitting
  → vereenvoudiging binnen afstandstolerantie
  → topologiecontrole en lokale correcties
  → bewerkbare geometrie met bronrelatie
  → procesmapping en onafhankelijke productiepreflight
```

Ruisonderdrukking en curvevereenvoudiging krijgen verschillende toleranties.

### Bron en schaal

Bewaar originele pixels en metadata. DPI uit een bestand is een voorstel voor schaal; ontbrekende of onwaarschijnlijke DPI vraagt invoer. Een bekende afstand tussen twee beeldpunten kan pixels naar millimeters kalibreren. Een perspectiefcorrectie met vier punten geldt voor een vlak object; lensvervorming en een gebogen oppervlak vragen andere correcties.

Geef de effectieve resolutie weer. Een beeld van 1000 pixels over 100 mm heeft 0,1 mm per pixel. Een fittingtolerantie van 0,02 mm maakt dat bronbeeld niet fysiek zo nauwkeurig. Toon afzonderlijk bronresolutie, ingestelde fittingtolerantie en gemeten referentiefout. Opschalen voegt geen gemeten details toe.

### Voorbewerking en segmentatie

Ondersteun globaal threshold, lokale threshold bij ongelijke belichting, alphasilhouet en expliciete kleurselectie. Bied achtergrond verwijderen met een gekozen achtergrondkleur plus tolerantie, en een maskerkwast voor lokaal behoud of verwijdering. Een transparante rand mag niet ongemerkt als zwarte achtergrond worden geïnterpreteerd.

Ruisspikkels filteren gebeurt in fysieke oppervlakte, bijvoorbeeld mm², met een pixelwaarde als aanvullende technische weergave. Behoud en verwijder kunnen lokaal worden overschreven. Openingen sluiten is een aparte, te beoordelen correctie. Een filter mag niet ongemerkt een betekenisvol punt, gaatje of letteraccent verwijderen.

### Contourtrace

Segmenteer voorgrondcomponenten en bouw een boom van buitencontouren, gaten en eilanden. Fit rechte stukken en curven binnen een opgegeven tolerantie. Bescherm scherpe hoeken en door de gebruiker gemarkeerde details. Leg het gekozen ambiguitybeleid vast voor diagonaal rakende pixels.

Na fitting controleert de app componentaantallen, gatrelaties, zelfdoorsnijdingen en minimale afstanden. Een topologiewijziging wordt getoond, ook als de pixeloverlap hoog blijft. Contouren worden niet automatisch geschikt voor snijden verklaard: een bedoelde spleet of los eiland kan nog een productiebeslissing vragen.

### Centerline

Gebruik een topologiebewuste skeleton/medial-axis-benadering en maak daar een graaf van met eindpunten, kruispunten en ketens. Schat lokale streekbreedte uit het bronmasker; die breedte is metadata en geen opdracht om het pad twee keer uit te voeren.

Verwijder kleine zijtakken alleen met regels voor lengte, breedte en lokale bewijskracht. Bescherm handmatig gemarkeerde takken. Fit ketens met vaste eind-/kruispuntankers. Een X-kruising zegt niet welke penstreek eerst liep; het systeem toont de mogelijke verbindingen en bewaart de keuze. Routeoptimalisatie mag de topologie niet veranderen om één continu pad te suggereren.

Een T-tekening hoort drie takken te geven; twee bijna rakende lijnen moeten gescheiden kunnen blijven. Een onderbroken letter kan met één expliciet goedgekeurd verbindingsvoorstel worden gerepareerd. De gebruiker ziet verschil tussen een waargenomen lijn en een toegevoegd herstelstuk.

### Technische reconstructie

Zoek lijn-, cirkel- en boogkandidaten met residu en dekking. Toon bronpixels, kandidaat en fout samen. Accepteer een cirkel pas wanneer de gebruiker de vervanging goedkeurt of een vooraf gekozen regel aan duidelijke grenzen voldoet. Een bijna ronde handgetekende vorm hoeft geen cirkel te worden.

Stel horizontaal, parallel, tangent, gelijk en symmetrisch voor op een selectie. Laat zien hoeveel geometrie de relatie zou verschuiven. Een geaccepteerde reconstructie maakt een echte Sketch met dimensies en constraints; de vrije trace blijft beschikbaar als referentie. Gelezen maattekst of OCR is een suggestie en wordt nooit zonder beoordeling een sturende maat.

### Kleurregio's

Segmenteer naar een gekozen palet en bewaar aangrenzende grenzen als gedeelde topologische kanten. Een gemeenschappelijke grens wordt eenmaal gefit. De gebruiker koppelt regio's aan gravure of snijbewerking; kleur is geen automatische vermogenswaarde.

Voor gevulde vectorregio's zijn gedeelde wiskundige grenzen zinvol, maar verschillende laserbewerkingen kunnen nog fysieke overlap veroorzaken. CAM toont daarom apart gedeelde contouren, dubbel toegewezen paden en overlappende graveergebieden. Common-line cutting blijft een expliciete latere productiemodus.

## Lokale correctie als hoofdfeature

Een gebruiker kan inzoomen en vervolgens:

- één gebied opnieuw thresholden zonder de rest opnieuw te berekenen;
- een hoek, gat, eindpunt of tak vastzetten;
- een verkeerd verbonden kruising splitsen of expliciet verbinden;
- een klein segment vervangen door een lijn, boog of cirkel;
- een fout deel opnieuw traceren met vaste grensankers;
- ongewenste eilandjes selecteren via grootte, locatie of gelijkenis;
- een correctie terugdraaien en vergelijken met de vorige variant.

Correcties hebben stabiele regio-/segmentverwijzingen waar mogelijk. Als opnieuw segmenteren die identiteit verbreekt, toont de app dat de correctie opnieuw moet worden gekoppeld. Zij past een oude correctie niet stilzwijgend op een naburig detail toe.

## Enginekeuze

| Kandidaat | Rol in de proef | Wat niet uit de naam volgt |
|---|---|---|
| Potrace | Referentie voor zwart-witcontouren en curvefitting | Geen volledige centerline- of CAD-reconstructiewerkstroom |
| AutoTrace | Referentie voor contour én centerline | Geen bewijs van kwaliteit op onze kruisingen of scanruis |
| VTracer | Referentie voor kleurvectorisatie en modulaire tracing | Geen garantie op maatvaste productie of geschiktheid van iedere release |
| Eigen orkestratie | Schaal, lokale maskers, topologie, correcties, maatcontrole en editorintegratie | Niet direct een reden om alle numerieke algoritmen zelf te schrijven |

Deze rollen zijn gebaseerd op [Potrace](https://potrace.sourceforge.net/), [AutoTrace](https://github.com/autotrace/autotrace) en [VTracer](https://github.com/visioncortex/vtracer). Met de vastgelegde Rust-keuze blijven Potrace en AutoTrace externe benchmarkreferenties; productiealgoritmen volgen een Rust-native route, met VTracer als kandidaat. Begin met een verwisselbare engine-interface en dezelfde benchmark. Pin versie/commit, configuratie en licentiebestand vóór integratie. Bepaal per dependency welke distributievoorwaarden gelden; een procesgrens is niet vanzelf een licentieoplossing. AI-segmentatie kan later een optionele, beoordeelbare maskersuggestie leveren. Zij vormt geen onderdeel van de eerste gegarandeerde offline traceketen.

## Benchmark die kwaliteit werkelijk meet

Maak een versiegebonden corpus van minimaal 60 eigen of herbruikbare beelden: twintig contour-/logo-opgaven, vijftien centerline-opgaven, vijftien technische scans en tien kleurregio-opgaven. Ieder geval heeft fysieke schaal, doel, verwachte component-/gat-/graafstructuur en referentiegeometrie of beoordeelde ankers. Voeg ruis, JPEG-randen, alpha, lage resolutie, scheve foto's, kleine letters en T/X/Y-kruisingen toe.

Splits afstelling en holdout; stem defaults niet op alle voorbeelden tegelijk af. Leg LightBurn-versie en instellingen vast. Vergelijk ook een passende enginebaseline per werkstroom. Beoordeel resultaten blind waar menselijk oordeel nodig is.

| Aspect | Meting | Voorgesteld vrijgavecriterium |
|---|---|---|
| Contournauwkeurigheid | Bidirectionele afstand tot geometrische referentie | Op synthetische geschikte invoer p95 ≤ 0,05 mm en maximum ≤ 0,10 mm bij 100 mm uitvoerbreedte |
| Topologie | Componenten, gaten en inbedding | Alle verplichte topologiegevallen correct; elke automatisch gewijzigde structuur gemeld |
| Centerline | Eindpunten, graden, verbindingen en spur-lengtes | T/X/Y-gevallen volgen hun expliciete referentiegraaf; ambiguïteit blijft zichtbaar |
| Complexiteit | Knopen en paden bij gelijke foutgrens | Geen onverklaarde regressie > 10% tegenover gekozen referentie-engine |
| Herstelwerk | Tijd tot geaccepteerde lasergeometrie | Mediaan minstens 30% minder dan LightBurn op het taakcorpus; geen kritieke holdout-regressie |
| Respons | Previewlatentie en annulering | p95 eerste preview ≤ 1 s voor benoemde 2 MP-proeven; finale trace ≤ 5 s als voorlopig doel |
| Robuustheid | Grote invoer en geheugendruk | 20 MP verwerkt of begrijpelijk begrensd, geen verlies van project/correcties |
| Reproduceerbaarheid | Geometriehash met engineversie | Dezelfde bron/recept/versie geeft dezelfde genormaliseerde geometrie |

Getallen zijn ontwerpdoelen. De eerste benchmark legt CPU, RAM, OS, invoerklasse en meetmethode vast en kan prestatiebudgetten herijken. p95 over contourpunten wordt niet verwisseld met p95 over rekentijden. Maximalefoutcontrole gebruikt een conservatieve curveafstandstoets of begrensde subdivisie, niet alleen een paar visueel gekozen samples.

Pixeloverlap/IoU wordt aanvullend gerapporteerd, maar kan een verdwenen klein gat verbergen. Bij echte foto's zonder exacte grondwaarheid rapporteren we referentieankers en menselijk herstelwerk; we verzinnen geen absolute geometrische nauwkeurigheid.

## Bouwvolgorde

Eerst schaal, bronbehoud, contour, voor/na-preview en lokale maskers. Daarna centerline met zichtbare graaf en reparaties. Vervolgens technische primitiefherkenning met bevestigde vervanging. Ten slotte kleurregio's, lokale gemengde modi en beoordeelbare constraintvoorstellen. Iedere stap levert zelfstandig bruikbare uitvoer aan dezelfde editor en CAM-keten.
