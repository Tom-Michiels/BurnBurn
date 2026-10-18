# Bouwplan en vrijgavecriteria

**Bouw de betere editor vanaf het begin en bewijs tegelijk een smalle laserroute.** Eerst een bijna volledige LightBurn-kopie maken zou de belangrijkste productverbetering uitstellen. Alleen een mooie editor bouwen zou de verbinding met echte productie missen.

Er zijn nog geen implementatiecode, vastgesteld team of bekende doelcontroller waaruit een betrouwbare kalenderplanning volgt. Onderstaande fasen zijn daarom resultaat- en afhankelijkheidsgericht. Een realistische tijd-/kostenraming volgt na R0 met gemeten prototypesnelheid en benoemde bezetting.

## R0 Technische en productmatige keuzes

| Proef | Concreet resultaat | Beslissing |
|---|---|---|
| Canvas en geometrie | Selecteren, nodes slepen, booleans en offsets op 10k/100k segmenten | Canvasstack en geometriekern kiezen |
| Sketchsolver | Montageplaat, parameterupdate, conflict en heropenen | Solver kiezen op kwaliteit, diagnose, licentie en responstijd |
| Tracing | Eerste 20 representatieve beelden met contour/centerline/kleur | Engine per doel en eigen toegevoegde waarde bepalen |
| Ruida | Offline compiler voor de bekende Line/Fill-fixtures met onafhankelijke inspectie | Minimale encodeerdekking en foutbudget vastleggen |
| Doelmachine | Model, firmware, bed, origin, interface en instellingsbackup vastleggen | Eerste supportconfiguratie en hardwarematrix kiezen |
| Dagelijkse taken | Gebruiker voert vijf referentietaken in huidige software uit | Baseline voor snelheid, kwaliteit en herstelwerk vastleggen |

R0 levert beslisnotities en kleine technische bewijzen op, geen weggegooide complete app. Rust, native AppKit op Mac, een latere native Windows-host en test-driven design staan vast. R0 kiest concrete Rust-libraries en bewijst de native canvas-/hostgrenzen volgens het [implementatieplan](../implementatieplan/README.md). Onderzoek van source/licenties hoort vóór distributiekeuzes. Er is geen verplichting om alle genoemde referentie-engines in het product op te nemen.

## R1 Bruikbare editor met offline laserbewijs

**Gebruikersresultaat:** een montageplaat maken, een logo corrigeren, een contour traceren en een project correct bewaren. Een beperkte offline preview/RD-proef bewaakt de productiearchitectuur, maar deze release belooft nog geen live machineondersteuning.

Lever P01–P03/P05, E01–E05, S01–S04, T01/T02, C01, M01 en X02. Onderliggende editorbasis omvat transacties, objectidentiteit, spatial index en een duidelijke scheiding tussen ontwerp- en proceslagen.

**Poort:** symmetrische gaten reageren op breedtewijziging; materiaaldikte stuurt een sleuf; vrij logo-artwork blijft zonder solver bruikbaar; opslaan/openen en undo bewaren relaties; protected-hole-trace blijft correct; de offline Line/Fill-proef past in dezelfde JobPlan-keten.

## R2 Dagelijks produceren op de eerste Ruida-configuratie

**Gebruikersresultaat:** ontwerp/import → herstel → bewerkingen → preview → RD → upload → framing → expliciete start → betrouwbare historie. Lokale tracecorrectie en maatcontrole maken al deel uit van de productwaarde.

Lever de resterende P0-stories: E07/E09, T05/T06/T09, C02/C03/C05–C09, M02–M07, W01–W03, X01/X03/X07. De verplichte beeldroute begint met gevalideerde binaire/dither-scanuitvoer. **Grayscale-vermogensmodulatie blijft geblokkeerd tot de basispower-/modulatiesemantiek op de doelmachine is bevestigd.** Volledige grayscale, Offset Fill en aanvullende scanvarianten kunnen als afgebakende uitbreiding naar R3 gaan; die keuze staat in de capabilitymatrix.

**Poort:** één benoemde model/firmware/interfaceconfiguratie doorstaat de vector-, raster-, oorsprong-, status- en foutinjectiematrix. De operator kan gewone dagelijkse jobs uitvoeren, bekende grenzen begrijpen en een onzekere start herstellen zonder een ongeplande tweede uitvoering.

R2 is de eerste productieversie, geen claim van volledige featurepariteit met LightBurn. Line/Fill/Image hebben een gespecificeerde en geteste parameterdekking; niet-geteste hoeken, kanalen of modes worden niet stil ondersteund verklaard.

## R3 Aantoonbaar betere ontwerpvoorbereiding

**Gebruikersresultaat:** maatvarianten, passende onderdelen, centerline en technische reconstructie werken samen met dezelfde betrouwbare laseruitvoer.

Lever P04/P06–P08, E06/E08/E10, S05–S07, T03/T04/T08/T10, C04/Offset Fill/C10, W04/W05 en X04/X05. Vul gevalideerde laseropties aan zoals beschreven in C11. Voeg variabele tekst toe via E11. Extra interface- en profielbeheer volgt M09/W09. Eenvoudige arrays kunnen eerder beschikbaar komen, maar mogen de mixed-part-nestingpoort niet vervangen.

**Poort:** montageplaat, doosvariant, klantlogo en handschrift zijn op de afgesproken benchmark sneller voorbereid met gelijkwaardige of betere fysieke uitkomst. De 30%-productiviteitsdoelen zijn hier besliscriteria, geen marketingclaim vóór meting. Bij tegenvallende resultaten gaan editorherstel en tracekwaliteit vóór extra features.

## R4 Volledig werkplaatsproduct

**Gebruikersresultaat:** meerkleuren, orders, restmateriaal, herkenning, speciale machines en uitbreidingen vormen één beheersbare productieketen.

Lever S08, T07, M08, W06–W08, X06 en geavanceerde nesting volgens C12. Platformverpakking en releasebeheer volgens X07 zijn al vanaf de eerste publieke release nodig en worden hier alleen naar extra platformen uitgebreid. Hardwareafhankelijke routes mogen afzonderlijk verschijnen zodra hun eigen poort slaagt.

**Poort:** gedeelde bediening heeft één verbindingseigenaar; rollen en intenties zijn duidelijk; herkenning rapporteert ambiguïteit; extra backends en assen hebben eigen fysieke validatie; volledige orders en voorraadregistratie blijven consistent.

## Kritieke afhankelijkheden

```text
Documentidentiteit en transacties
  ├─ vrije editor → importherstel → lokale tracecorrecties
  ├─ schetsen en parameters → componenten → generators → varianten
  └─ operations → JobPlan → preview → RD → transport → executionhistorie

Plaat-/onderdeelmodel → nesting → orders en restmateriaal
Machineframe → calibratie → markregistratie → automatische herkenning
Stabiele CLI/compilergrens → uitbreidingen en operatorclient
```

R0 kan gespecialiseerde werkstromen naast elkaar onderzoeken; integratie en release gebeuren op gedeelde contracten. Capaciteit voor editorinteractie, numerieke geometrie/solver, beeldverwerking, CAM/protocol en productvalidatie moet expliciet worden toegewezen. Dit zijn expertisegebieden, geen voorgeschreven aantal voltijdse medewerkers.

## Testplan

| Niveau | Belangrijkste gevallen | Bewijs |
|---|---|---|
| Document | Assets, oude schema's, crash tijdens save, undo van trace en parameterupdate | Roundtrip met identiteiten en inhoudshashes |
| Geometrie | Kleine gaten, raakpunten, bijna parallel, zelfdoorsnijding, extreme schaal | Afstandsfout, topologie, eindpunten en regressiegeometrie |
| Solver | Vrijheid, redundant/conflict, wijziging, branches en ongeldige maten | Geldige residuen, gerichte diagnose en last-valid-result |
| Tracer | Corpus met aparte holdout, lage resolutie, alpha en kruisingen | Afwijking, topologie, herstelminuten, tijd en geheugen |
| CAM | Binnen/buiten, meerdere bewerkingen, kerf, rasterhoek, intervals en tabs | Semantische JobPlan-goldens en bronverwijzingen |
| Ruida | Kwantisatie, relatieve grenzen, bounds, min/max en mode | Golden files plus onafhankelijke decode/differentiële referentie |
| Transport | ACK kwijt, dubbele antwoorden, timeout, opslag vol en naamconflict | Replay en toetsing van toestandsovergangen |
| Hardware | Origin, afmetingen, raster, frame, pauze/stop en reconnect | Gelabelde model/firmware/interfaceverslagen en meetresultaten |
| UX | Montageplaat, doos, logo, handschrift en fout na start | Taaktijd, fouten, herstel en fysieke geschiktheid |
| Platform | Installeer/update/herstel, DPI, NL/EN en offline | Herhaalbare releasechecks per ondersteund OS |

De bestaande drie RD-fixtures en eerdere offline tests zijn startmateriaal; een encoder/decoder-roundtrip kan dezelfde fout bevatten. Combineer daarom een onafhankelijk geïmplementeerde inspectie, vergelijking met referentie-export en fysieke proeven. DSP-overscan/acceleratie kan beweging toevoegen die niet uit een simpele polylinepreview volgt; tijd en werkbereik krijgen daarop gerichte proeven.

Begin hardwarevalidatie met de echte profielwaarden en laser-uit-beweging; daarna gecontroleerde materiaalproeven van de machine-eigenaar. Gebruik de bestaande fixtures met hun 1500 × 900 mm-profiel niet blind als machinejob.

## Voorgestelde kwaliteitsbudgetten

| Onderdeel | Voorlopig doel |
|---|---|
| Selectie en directe interactie | p95 ≤ 100 ms op vastgelegde 100k-segmentfixture |
| Kleine parametrische schets | p95 parameterupdate ≤ 100 ms voor afgesproken representatieve schetsen |
| Middelgrote schets | Een benoemde 500-entiteitfixture blijft bruikbaar; kostbare oplossingen zijn annuleerbaar |
| Tracing | Preview/finale doelen en foutgrenzen uit het [traceplan](02-tracer.md) |
| Tijdschatting | Na calibratie streven naar mediane absolute procentuele fout ≤ 15% op het benoemde productiecorpus; percentielen ook rapporteren |
| Kritieke uitvoerfouten | Geen bekende onbedoelde start, stille originwissel of stille capabilityfallback in de release |
| Herstelbaarheid | Onderbroken save/trace/upload behoudt bron en eerlijke uitvoeringsstatus |

R0 legt de referentiehardware en testgeometrie vast. Een compiler kan een geometrische tolerantie toetsen; een tolerantie voor afgewerkte onderdelen vereist materiaalproeven. Tijdsdoelen worden niet als universele garantie voor ieder project gepresenteerd.

## Risico's met concrete aanpak

| Risico | Aanpak | Beslismoment |
|---|---|---|
| Solver maakt de editor traag of onbegrijpelijk | Kleine sketches, expliciete relaties, goede diagnose en vrij tekenen behouden | R0/R1 |
| Gebroken entiteitreferenties na boolean/trace | Stabiele IDs, generaties, tests en zichtbare herkoppeling | R1/R3 |
| Trace ziet er goed uit maar snijdt verkeerd | Topologie, mm-schaal, graafcontrole en fysieke voorbeelden | R1–R3 |
| Ruida-subset wordt te breed geclaimd | Modelgebonden capabilities en zelfstandige hardwarepoorten | Iedere productierelease |
| Preview en export lopen uiteen | Eén JobPlan plus inspectie van daadwerkelijke RD | R2 |
| Verloren antwoord veroorzaakt dubbele start | Intentielog, begrensde retries en Unknown-reconciliatie | R2 |
| Featureomvang vertraagt de betere editor | R1/R3-productiviteitspoorten en uitstel van brede hardware | Iedere release |
| Engine is niet geschikt voor distributie of kwaliteit | Versies pinnen, licentie-inventaris, benchmark en verwisselbare adapter | R0 |

## Eerste werkpakket

1. Maak de montageplaat-, logo- en centerlinefixtures met meetbare verwachtingen.
2. Specificeer Project, GeometryObject, Sketch, Operation en JobPlan met één complete voorbeeldjob.
3. Bouw save/undo, canvasselectie en directe nodebewerking.
4. Integreer een solverproef en parameterpaneel op de montageplaat.
5. Bouw contourtrace met schaal, bronbehoud en een lokaal masker.
6. Compileer dezelfde nominale geometrie naar Line/Fill-plan en offline RD.
7. Meet interactie, tracekwaliteit en bron-/uitvoerconsistentie; kies daarna definitieve libraryversies binnen de vastgelegde Rust/native-architectuur.

Daarmee bewijst het eerste werkpakket zowel de beoogde editorverbetering als de koppeling met de laserarchitectuur.

## Gezamenlijke gereedcriteria

Een story is gereed wanneer haar gebruikerspad, foutgedrag, undo waar van toepassing, opslagroundtrip en relevante regressies werken. Een machinefeature vereist bovendien bewijs op de benoemde configuratie. De UI biedt geen werkende knop die downstream zonder effect blijft. Documentatie en capabilitymatrix volgen de werkelijk vrijgegeven dekking.

De productbeslissing na R3 is concreet: doorgaan naar meer werkplaatsfuncties wanneer de kerntaken aantoonbaar beter zijn; anders eerst herstelwerk, interactie of tracekwaliteit verbeteren.
