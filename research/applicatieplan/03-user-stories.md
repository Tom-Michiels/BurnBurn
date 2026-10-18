# User stories voor de volledige applicatie

Iedere story beschrijft een waarneembaar gebruikersresultaat. Acceptatiecriteria zijn minimaal; de gezamenlijke gereedcriteria uit het [bouwplan](05-bouwplan.md) gelden aanvullend.

**Prioriteit:** P0 = noodzakelijk voor de eerste dagelijkse productieroute; P1 = belangrijke productverbetering; P2 = verdere verbreding. **Releases:** R1 = bewerkbaar ontwerp en offline bewijs, R2 = dagelijkse productie op één gevalideerde Ruida-configuratie, R3 = onderscheidende ontwerp- en tracewerkstromen, R4 = uitgebreid werkplaatsproduct. R0 bevat technische proeven vóór deze releases. Prioriteit is geen tijdschatting.

## Projecten en overdracht

### P01 — Een project betrouwbaar bewaren

**P0 · R1 · Afhankelijkheden: geen.** Als maker wil ik een volledig project lokaal opslaan, zodat ik het zonder internet en zonder verloren bronbestanden kan voortzetten.

**Acceptatie:** opslaan/openen behoudt stabiele object-ID's, eenheden, geometrie, proceskoppelingen en assets; atomair opslaan laat na een onderbreking de laatste geldige versie bestaan; een niet-ondersteunde nieuwere schemafunctie wordt expliciet gemeld en niet weggegooid.

### P02 — Handelingen terugdraaien en herstellen

**P0 · R1 · Afhankelijkheden: P01.** Als ontwerper wil ik fouten en een appcrash kunnen herstellen, zodat ik durf te experimenteren.

**Acceptatie:** één drag, boolean of trace-apply is één undoactie; redo en herstel herstellen ook relaties en selectie; opnieuw openen of undo verstuurt nooit machinecommando's.

### P03 — Bestanden met juiste afmetingen importeren

**P0 · R1 · Afhankelijkheden: P01.** Als maker wil ik SVG, DXF, PNG en JPEG importeren, zodat externe ontwerpen op de bedoelde schaal bruikbaar worden.

**Acceptatie:** bekende fixtures bewaren fysieke afmetingen binnen de gespecificeerde importtolerantie; eenheidloze DXF vraagt een eenheidskeuze met maatpreview; niet-ondersteunde entiteiten en effecten verschijnen in een importverslag met verwijzing naar de bron.

### P04 — PDF en bestaand LightBurn-werk migreren

**P1 · R3 · Afhankelijkheden: P03, C01, C02.** Als overstapper wil ik relevante bestaande ontwerpen en instellingen meenemen, zodat ik niet vanaf nul begin.

**Acceptatie:** PDF-import laat pagina, schaal en vector/rasterkeuze zien; de ondersteunde LBRN2-subset wordt expliciet opgesomd en onbekende instellingen blijven in een rapport bewaard; projectimport en machineprofielimport hebben afzonderlijke dekking en beloven geen universele compatibiliteit.

### P05 — Bruikbare geometrie exporteren

**P0 · R1 · Afhankelijkheden: P03, E02.** Als ontwerper wil ik SVG en DXF exporteren, zodat mijn werk ook elders bruikbaar blijft.

**Acceptatie:** fysieke schaal en geslotenheid overleven de afgesproken roundtripfixtures; tekst kan als tekst of expliciete contouren worden geëxporteerd; verlies van constraints, modifiers of niet-ondersteunde DXF-curven wordt vóór export benoemd.

### P06 — Meerdere platen en onderdelen beheren

**P1 · R3 · Afhankelijkheden: P01, S07.** Als maker wil ik een samenhangend ontwerp over meerdere platen verdelen, zodat onderdeelrelaties en materiaalkeuze behouden blijven.

**Acceptatie:** platen hebben eigen afmetingen, materiaal en plaatsingen; hetzelfde onderdeel kan op meerdere platen voorkomen zonder bronkopie; compilatie gebruikt uitsluitend de gekozen plaat/revisie en meldt verwijzingen naar ontbrekende bronnen.

### P07 — Een overdraagbaar projectpakket maken

**P1 · R3 · Afhankelijkheden: P01, C08, M01.** Als werkplaatsmedewerker wil ik een project met fontsituatie, assets en profielsnapshots delen, zodat een collega dezelfde voorbereiding ziet.

**Acceptatie:** het pakket meldt ontbrekende en extern gelicentieerde assets; een andere computer kan zonder oorspronkelijke mappen dezelfde canonieke geometrie en JobPlan reproduceren met de vastgelegde toolversies; ingebedde instellingen worden geïnspecteerd voordat zij een lokaal machineprofiel vervangen.

### P08 — Projecten en revisies terugvinden

**P1 · R3 · Afhankelijkheden: P01, W03.** Als terugkerende gebruiker wil ik zoeken op naam, materiaal, onderdeel en job, zodat ik betrouwbaar herhaalwerk vind.

**Acceptatie:** zoekresultaten tonen preview en revisiedatum; een oude productierevisie blijft naast het huidige ontwerp bereikbaar; heropenen maakt geen nieuwe productie-intentie.

## Vrije vectorbewerking

### E01 — Snel en precies tekenen

**P0 · R1 · Afhankelijkheden: P01.** Als maker wil ik basisvormen met getypte maten tekenen, zodat ik minder correcties achteraf nodig heb.

**Acceptatie:** lijn, rechthoek, cirkel, ellips en boog ondersteunen exacte invoer; mm/inch en geldige eenvoudige expressies leveren dezelfde canonieke geometrie; annuleren laat geen half object achter.

### E02 — Curven lokaal corrigeren

**P0 · R1 · Afhankelijkheden: E01.** Als grafisch ontwerper wil ik knopen, handvatten en segmenten direct bewerken, zodat ik een geïmporteerd logo netjes kan herstellen.

**Acceptatie:** hoek/glad/symmetrisch zijn expliciet instelbaar; split, join en reverse behouden de bedoelde segmenten; een lokale edit verandert geen niet-geselecteerde curvebereiken.

### E03 — Het juiste object selecteren

**P0 · R1 · Afhankelijkheden: E01.** Als ontwerper wil ik overlappende objecten kunnen onderscheiden, zodat ik geen verkeerde lijnen wijzig.

**Acceptatie:** selectiecyclen en typefilters geven toegang tot verborgen kandidaten; objectboom en canvas blijven gesynchroniseerd; isoleren en vergrendelen werken ook bij geneste groepen.

### E04 — Voorspelbaar snappen en uitlijnen

**P0 · R1 · Afhankelijkheden: E03.** Als maker wil ik zien waaraan ik snap, zodat ik exacte relaties kan plaatsen.

**Acceptatie:** eindpunt, midden, centrum en snijpunt zijn benoemde doelen; een tijdelijke modifier onderdrukt snapping; align/distribute/mirror hebben een zichtbaar referentieobject en behouden numerieke afmetingen.

### E05 — Vormen samenstellen en bijwerken

**P0 · R1 · Afhankelijkheden: E02.** Als maker wil ik boolean, trim, extend, fillet en offset gebruiken, zodat ik productiegeschikte contouren kan opbouwen.

**Acceptatie:** live preview toont resultaat en fouten vóór toepassen; gaten en winding blijven in de regressievoorbeelden correct; niet-oplosbare offsets leveren een gerichte melding en geen gedeeltelijk beschadigd document.

### E06 — Niet destructief vormvarianten maken

**P1 · R3 · Afhankelijkheden: E05, S02.** Als ontwerper wil ik modifierstappen kunnen wijzigen, zodat ik niet steeds naar een oude bronkopie hoef terug te gaan.

**Acceptatie:** bron → offset → boolean → patroon blijft inspecteerbaar; een stap kan worden uitgeschakeld zonder bronverlies; gebroken resultaatreferenties worden gemeld en niet op willekeurige segmenten overgezet.

### E07 — Tekst correct voorbereiden

**P0 · R2 · Afhankelijkheden: E02, P01.** Als ontwerper wil ik tekst, regelafstand en letterafstand aanpassen, zodat labels en namen mooi geproduceerd worden.

**Acceptatie:** tekst blijft herbewerkbaar na opslaan; ontbrekende fonts tonen een opgeslagen contourweergave en een duidelijke vervangingskeuze; contourconversie en stencilbruggen kunnen de oorspronkelijke tekst als bron bewaren.

### E08 — Padtekst en enkelvoudige graveerpaden gebruiken

**P1 · R3 · Afhankelijkheden: E07.** Als maker wil ik tekst langs een curve en geschikte single-line letters gebruiken, zodat ik ringen en dunne markeringen kan maken.

**Acceptatie:** tekst op een pad reageert voorspelbaar op richting en offset; single-line paden worden niet automatisch gevuld of gesloten; de preview toont dezelfde padgeometrie als de productieplanner.

### E09 — Geometriefouten gericht repareren

**P0 · R2 · Afhankelijkheden: E02, E05.** Als operator wil ik open eindpunten, duplicaten en zelfdoorsnijdingen lokaliseren, zodat ik verkeerde of dubbele sneden voorkom.

**Acceptatie:** ieder probleem heeft locatie, betrokken objecten en een fysieke maat; reparatie heeft een voor/na-overlay en eigen tolerantie; een bedoelde sleuf van 0,15 mm blijft bestaan wanneer elders een expliciet gekozen naad van 0,05 mm wordt gesloten.

### E10 — Patronen en constructies genereren

**P1 · R3 · Afhankelijkheden: S02, S07, C03.** Als maker wil ik tab/slot, finger joints, doosonderdelen en scharnierpatronen parametrisch maken, zodat maatwijzigingen geen hertekenwerk vragen.

**Acceptatie:** generatorparameters en gewone resultaatgeometrie zijn beide inspecteerbaar; materiaaldikte en pasvorm hebben afzonderlijke betekenis van kerf; ongeldige aantallen, negatieve segmenten of te kleine details geven een concrete fout.

### E11 — Personalisatie en variabele tekst maken

**P1 · R3 · Afhankelijkheden: E07, S07, W01.** Als producent wil ik namen, serienummers en barcodes uit tabelgegevens gebruiken, zodat ik gepersonaliseerde reeksen reproduceerbaar kan maken.

**Acceptatie:** een rijpreview toont opgeloste tekst, volgnummer en eventuele ontbrekende velden; barcode-/QR-geometrie krijgt controle op fysieke modulegrootte en een benoemde leesproef; de bevroren job bewaart de gekozen datarijen en tijdwaarden, zodat heropenen geen andere personalisatie oplevert.

## Parametrische sketches

### S01 — Relaties tijdens tekenen vastleggen

**P0 · R1 · Afhankelijkheden: E01, E04.** Als maker wil ik optioneel blijvende geometrische relaties vastleggen, zodat mijn ontwerp bij wijzigingen zijn opbouw behoudt.

**Acceptatie:** de basisconstraintset uit het editorplan wordt opgeslagen; voorgestelde relaties zijn herkenbaar en tijdelijk onderdrukbaar; een vrije contour krijgt zonder keuze geen blijvende constraints.

### S02 — Maten met parameters aansturen

**P0 · R1 · Afhankelijkheden: S01.** Als maker wil ik benoemde maten en expressies gebruiken, zodat één wijziging alle bedoelde onderdelen bijwerkt.

**Acceptatie:** `sleufbreedte = materiaaldikte + speling` rekent met eenheden; cycli en ongeldige dimensies worden gericht gemeld; een parameterwijziging met alle afgeleiden is één undoactie.

### S03 — Maatvoering en metingen onderscheiden

**P0 · R1 · Afhankelijkheden: S02.** Als ontwerper wil ik zien welke maten sturen en welke alleen meten, zodat ik de werking van een schets begrijp.

**Acceptatie:** driving en driven hebben verschillende labels; een ongeldige omzetting wordt als constraintconflict getoond; hulplijnen en maatannotaties blijven standaard buiten de laseroutput.

### S04 — Een vastgelopen schets begrijpen

**P0 · R1 · Afhankelijkheden: S01, S03.** Als maker wil ik zien waarom geometrie niet beweegt, zodat ik een conflict kan herstellen zonder alles te verwijderen.

**Acceptatie:** vrije, volledig bepaalde en conflicterende toestand zijn met symbool en tekst zichtbaar; betrokken relaties worden geselecteerd vanuit de foutmelding; de laatste geldige geometrie blijft behouden en kan worden hersteld.

### S05 — Vloeiende en symmetrische details maken

**P1 · R3 · Afhankelijkheden: S04, E05.** Als maker wil ik tangentie, symmetrie en gekoppelde afrondingen gebruiken, zodat technische contouren netjes blijven bij maatwijzigingen.

**Acceptatie:** ondersteunde lijn-/boogrelaties blijven geldig bij de parameterfixtures; een negatieve of niet-passende radius geeft een fout; de app springt niet zonder melding naar een andere geometrische oplossing.

### S06 — Geometrie als gekoppelde referentie gebruiken

**P1 · R3 · Afhankelijkheden: S02, E06.** Als maker wil ik een rand uit een ander onderdeel gebruiken, zodat passende delen samen veranderen.

**Acceptatie:** referenties tonen brononderdeel en entiteit; bronwijzigingen werken door als de identiteit behouden blijft; verwijderen of splitsen van een referentierand levert een zichtbare gebroken koppeling op.

### S07 — Onderdelen en varianten maken

**P1 · R3 · Afhankelijkheden: S02.** Als productontwerper wil ik componenten en maatvarianten gebruiken, zodat ik een familie producten uit één ontwerp kan maken.

**Acceptatie:** plaatsingen delen één definitie maar hebben eigen transformatie; varianten hebben benoemde parameterwaarden en falen afzonderlijk; een lokale override is zichtbaar en overschrijft de bron niet stilzwijgend.

### S08 — Importgeometrie selectief parametriseren

**P2 · R4 · Afhankelijkheden: S04, S06, T08.** Als maker wil ik op een gekozen importdeel constraints laten voorstellen, zodat ik bruikbare CAD-geometrie krijg zonder de hele illustratie te verbouwen.

**Acceptatie:** voorstel toont relaties en maximale verplaatsing in mm; toepassen vereist een gekozen voorstel en behoudt de vrije bron; onoplosbare of ambigue voorstellen worden niet automatisch vastgezet.

## Tracing en beeldvoorbereiding

### T01 — Een beeld op fysieke schaal voorbereiden

**P0 · R1 · Afhankelijkheden: P03.** Als maker wil ik bronbeeld, uitsnede en schaal instellen, zodat de trace op de bedoelde maat wordt beoordeeld.

**Acceptatie:** originele pixels blijven behouden; schaal kan uit afmetingen of een bekende afstand volgen; de UI onderscheidt bronresolutie van ingestelde fittingtolerantie.

### T02 — Een silhouet met correcte gaten traceren

**P0 · R1 · Afhankelijkheden: T01, E02.** Als ontwerper wil ik een zwart-wit- of alphasilhouet omzetten naar gesloten contouren, zodat ik het kan uitsnijden of vullen.

**Acceptatie:** voorgrond, gaten en eilanden volgen de corpusreferentie; bron, masker en vector zijn op dezelfde schaal vergelijkbaar; toepassen bewaart recept en engineversie als herbewerkbare bron.

### T03 — Een foto of ongelijke scan opschonen

**P1 · R3 · Afhankelijkheden: T02.** Als maker wil ik lokale belichting en perspectief corrigeren, zodat schaduwen geen onbedoelde onderdelen worden.

**Acceptatie:** globale en lokale threshold zijn afzonderlijk kiesbaar; vierpuntscorrectie toont de resulterende vlaktransformatie; een maskerkwast kan voorgrond behouden of verwijderen zonder het bronbeeld te overschrijven.

### T04 — Enkele lijnen uit lijnkunst halen

**P1 · R3 · Afhankelijkheden: T02, E09.** Als maker wil ik centerline-tracing gebruiken, zodat een getekende streek één laserpad wordt.

**Acceptatie:** T/X/Y-fixtures tonen verwachte eindpunten en vertakkingen; open paden blijven open en krijgen geen automatische fill; dubbelzinnige kruisingen zijn zichtbaar en bewerkbaar.

### T05 — Eén fout gebied herstellen

**P0 · R2 · Afhankelijkheden: T02, E02.** Als ontwerper wil ik lokaal opnieuw traceren met vaste grensankers, zodat goede delen ongemoeid blijven.

**Acceptatie:** drempel en masker kunnen per regio verschillen; niet-betrokken geometrie blijft identiek; opnieuw segmenteren meldt correcties waarvan de bronkoppeling verloren is.

### T06 — Detail en vereenvoudiging controleren

**P0 · R2 · Afhankelijkheden: T02, E09.** Als maker wil ik afwijking en topologiewijzigingen zien, zodat minder knopen niet tot een verkeerd onderdeel leiden.

**Acceptatie:** maximale fittingtolerantie wordt in mm opgegeven; beschermde gaten en hoeken blijven behouden; verwijderde componenten en afwijkingen worden gelokaliseerd, ook bij hoge pixeloverlap.

### T07 — Meerkleurige beelden in regio's omzetten

**P2 · R4 · Afhankelijkheden: T05, C02.** Als ontwerper wil ik kleuren naar afzonderlijke regio's en bewerkingen mappen, zodat ik een logo in verschillende behandelingen kan uitvoeren.

**Acceptatie:** palet en mappings zijn expliciet; gedeelde grenzen worden consistent gefit; CAM rapporteert dubbel uitgevoerde grenzen en overlappende graveergebieden.

### T08 — Technische vormen uit een scan herkennen

**P1 · R3 · Afhankelijkheden: T06, S04.** Als maker wil ik lijn-, boog- en cirkelkandidaten beoordelen, zodat ik een scan verder als technische geometrie kan bewerken.

**Acceptatie:** kandidaten tonen bronoverlay en fitresidu; geaccepteerde vervanging maakt echte primitives; maattekst of veronderstelde symmetrie wordt uitsluitend na beoordeling sturend.

### T09 — Gravurebeelden reproduceerbaar verwerken

**P0 · R2 · Afhankelijkheden: T01, C05.** Als operator wil ik tooncorrectie, resampling, dithering en grayscale expliciet instellen, zodat het gegraveerde resultaat voorspelbaar blijft.

**Acceptatie:** dithering gebeurt na keuze van fysieke uitvoerschaal en rasterinterval; een vooraf geditherd beeld heeft een expliciete pixelschaalmodus; bron, verwerkt beeld en geplande scanruns zijn afzonderlijk te bekijken.

### T10 — Tracerecepten vergelijken en herhalen

**P1 · R3 · Afhankelijkheden: T05, T06.** Als terugkerende gebruiker wil ik varianten en recepten bewaren, zodat ik niet bij iedere afbeelding opnieuw begin.

**Acceptatie:** A/B-varianten delen zoom en tonen pad-/knooptelling; batchverwerking rapporteert probleemgevallen per beeld; dezelfde bron/recept/engineversie levert dezelfde genormaliseerde geometriehash.

## Laserplanning en materialen

### C01 — Herkenbare proceslagen gebruiken

**P0 · R1 · Afhankelijkheden: P01, E01.** Als LightBurn-gebruiker wil ik kleur, naam en procesinstellingen per laag beheren, zodat de bekende laserworkflow snel werkt.

**Acceptatie:** zichtbaarheid en output-enabled zijn afzonderlijk; snelheid heeft een expliciete fysieke eenheid; modus en instellingsbeschikbaarheid volgen het gekozen machineprofiel.

### C02 — Eén object meerdere behandelingen geven

**P0 · R2 · Afhankelijkheden: C01.** Als maker wil ik dezelfde vorm eerst markeren en later snijden, zodat ik geen dubbele geometrie hoef te onderhouden.

**Acceptatie:** meerdere operations verwijzen naar dezelfde GeometryId; wijzigen van de bron werkt door in alle verwijzingen; ongekoppelde en meervoudig gekoppelde geometrie is inspecteerbaar.

### C03 — Kerf op de bewerking toepassen

**P0 · R2 · Afhankelijkheden: C02, E05.** Als maker wil ik kerfcompensatie op het afgeleide snijpad toepassen, zodat nominale afmetingen en passing begrijpelijk blijven.

**Acceptatie:** binnen/buiten-zijde wordt visueel aangegeven; nominale maat blijft behouden en offset is bij een totale kerfbreedte k normaal k/2 per rand; speling en kerf worden niet automatisch bij elkaar opgeteld.

### C04 — Tabs, leads en herhaalde passes sturen

**P1 · R3 · Afhankelijkheden: C03, M02.** Als operator wil ik het loskomen en de insteek van onderdelen beheersen, zodat delicate vormen bruikbaar blijven.

**Acceptatie:** tabs en leads zijn in de geplande paden zichtbaar; multipass- en Z-beleid worden alleen aangeboden bij bewezen backendmogelijkheden; ongeldige tabplaatsing na bronwijziging geeft een melding.

### C05 — Line, Fill, Offset Fill en Image plannen

**P0 · R2 voor Line/Fill/Image; P1 · R3 voor Offset Fill · Afhankelijkheden: C01, E09.** Als operator wil ik de juiste bewerkingsmodus kiezen, zodat ik contouren, vlakken en beelden gericht kan produceren.

**Acceptatie:** mode heeft een expliciet effect op het JobPlan; rasterinterval, hoek, richting en powerruns zijn inspecteerbaar; een nog niet gevalideerde modus wordt geblokkeerd met de ontbrekende capability.

### C06 — Een verstandige snijvolgorde kiezen

**P0 · R2 · Afhankelijkheden: C02, C05.** Als operator wil ik binnenvormen vóór buitenranden uitvoeren en eigen keuzes vastzetten, zodat onderdelen niet te vroeg loskomen.

**Acceptatie:** harde afhankelijkheden gaan vóór traveloptimalisatie; startpunt, richting en groepsvolgorde kunnen worden vergrendeld; een cyclus geeft een gerichte fout met betrokken operations.

### C07 — Het werkelijke plan beoordelen

**P0 · R2 · Afhankelijkheden: C05, C06.** Als operator wil ik travels, sneden, scans, volgorde en tijd zien, zodat ik de uitvoering vooraf begrijp.

**Acceptatie:** preview en encoder consumeren dezelfde bevroren planrevisie; aanklikken van een beweging selecteert bronobject en operation; schatting, firmware-afhankelijke beweging en gemeten duur worden onderscheiden.

### C08 — Materiaalrecepten en testkaarten gebruiken

**P0 · R2 · Afhankelijkheden: C01, M01.** Als werkplaatsgebruiker wil ik machinegebonden materiaalrecepten meten en bewaren, zodat ik reproduceerbaar kan starten.

**Acceptatie:** recept bevat materiaal, dikte, machinebinding, revisie en testnotities; testkaart produceert benoemde varianten zonder bronproject te vervuilen; een bibliotheekupdate verandert bestaande jobinstellingen niet.

### C09 — Problemen vóór productie zien

**P0 · R2 · Afhankelijkheden: E09, C03, C07, M02.** Als operator wil ik een concrete preflight, zodat fouten op de juiste plek kunnen worden hersteld.

**Acceptatie:** controle omvat output buiten bed, ongeldige operations, onbekende eenheden, dubbele sneden en open fillcontouren; fouten en beoordeelbare waarschuwingen zijn onderscheiden; iedere geometrische melding zoomt naar de oorzaak.

### C10 — Een nieuwe plaatindeling maken

**P1 · R3 · Afhankelijkheden: P06, C09.** Als producent wil ik arrays en mixed-part nesting met aantallen gebruiken, zodat ik materiaal efficiënt benut.

**Acceptatie:** arrays en nesting hebben verschillende modellen; plaatgaten, marges, nerfrichting en toegestane rotaties worden gerespecteerd; niet-geplaatste aantallen en benutting met expliciete noemer blijven zichtbaar.

### C11 — Geavanceerde procesopties precies instellen

**P1 · R3 · Afhankelijkheden: C04–C07, M02, M03.** Als ervaren operator wil ik perforatie, overcut, puntmarkering, subbewerkingen en ondersteunde scanopties instellen, zodat ik mijn bestaande werkwijzen kan voortzetten.

**Acceptatie:** iedere optie uit de gekozen LightBurn-featurematrix heeft een benoemde status, uitvoerbetekenis en fixture; alleen bewezen kanaal-, air-, delay-, scanhoek- en poweropties zijn activeerbaar; unsupported opties bij import geven een gerichte melding en worden niet genegeerd.

### C12 — Nesting voor productie verder optimaliseren

**P2 · R4 · Afhankelijkheden: C10, W07, C11.** Als producent wil ik complete sets, materiaalrichting en optioneel gedeelde snijlijnen optimaliseren, zodat een compacte indeling ook werkelijk maakbaar is.

**Acceptatie:** resultaat toont complete orders en niet-geplaatste aantallen naast benutting; common-line cutting is expliciet en corrigeert kerf/volgorde volgens een afzonderlijk gevalideerd proces; een betere benuttingsscore mag verboden spiegelingen, markeringen of minimale afstanden niet schenden.

## Machines en Ruida

### M01 — De juiste machine configureren

**P0 · R1 · Afhankelijkheden: P01.** Als machine-eigenaar wil ik bedmaat, oorsprong en verbinding afzonderlijk vastleggen, zodat ontwerpen correct worden voorbereid.

**Acceptatie:** profiel toont model, firmware indien bekend en revisie; assen en alle ondersteunde origins hebben benoemde transformfixtures; profielwissel invalideert een bijbehorend oud compile-resultaat.

### M02 — Echte ondersteuning herkennen

**P0 · R2 · Afhankelijkheden: M01.** Als gebruiker wil ik per functie weten of mijn configuratie die ondersteunt, zodat ik geen onuitvoerbare job voorbereid.

**Acceptatie:** iedere relevante capability is supported/unsupported/unknown met voorwaarden; UI en compiler controleren dezelfde matrix; onbekende dual-laser, rotary, feeder of Z-functionaliteit wordt niet stilzwijgend vlak uitgevoerd.

### M03 — Een inspecteerbaar RD-bestand genereren

**P0 · R2 · Afhankelijkheden: C07, M02.** Als operator wil ik een correct Ruida-bestand opslaan, zodat ik ook via een offline overdrachtsroute kan produceren.

**Acceptatie:** RD wordt als binaire controllercode behandeld en niet als G-code; onafhankelijke decodevergelijking controleert geometrie, modes, powers, units en bounds binnen vastgelegde toleranties; onbekende records bij inspectie blijven zichtbaar.

### M04 — Betrouwbaar uploaden

**P0 · R2 · Afhankelijkheden: M03.** Als operator wil ik een job via de gevalideerde verbinding versturen, zodat ik weet of hij echt op de controller staat.

**Acceptatie:** uploadprogress, protocol-ACK en bevestigde opslag zijn afzonderlijk; timeout, volle opslag en naamconflict hebben expliciet gedrag; reconnect veroorzaakt geen ongeplande herstart of dubbele productie-intentie.

### M05 — Framen en bewegen met duidelijke coördinaten

**P0 · R2 · Afhankelijkheden: M01, M02, C09.** Als operator wil ik positioneren en framen, zodat ik de geplande plaatsing op het bed controleer.

**Acceptatie:** document-, bed- en jobcoördinaten zijn herkenbaar; laser-uit-framing op de gevalideerde configuratie volgt beoordeelde bounds; ongeldige aspositie wordt niet als nul geïnterpreteerd en veroorzaakt geen afgeleide beweging op andere assen.

### M06 — Start, pauze, hervatten en stop bedienen

**P0 · R2 · Afhankelijkheden: M04, M05.** Als operator wil ik ondubbelzinnige machineacties, zodat ik de jobstatus kan volgen.

**Acceptatie:** start toont machine, jobrevisie en plaatsing; een verloren antwoord leidt niet automatisch tot een tweede start; softwarestop en onbekende terugmelding worden eerlijk weergegeven, zonder hardware-noodstopgarantie te suggereren.

### M07 — Verbindingsproblemen onderzoeken

**P0 · R2 · Afhankelijkheden: M04, M06.** Als beheerder wil ik diagnose kunnen exporteren en offline herhalen, zodat een fout oplosbaar wordt.

**Acceptatie:** rapport koppelt profielrevisie, jobhash en protocolgebeurtenissen; replay reproduceert de toestand zonder machinecontact; delen van diagnostiek laat privé-assets en gevoelige verbindingsdata beoordelen.

### M08 — Speciale machines en extra backends toevoegen

**P2 · R4 · Afhankelijkheden: M02, M07, W01.** Als eigenaar van extra hardware wil ik rotary, feeder, dual-head of een andere backend gebruiken, zodat dezelfde ontwerpworkflow kan meegroeien.

**Acceptatie:** iedere variant heeft eigen capability- en hardwaretestmatrix; terugkeer naar vlakbed herstelt het bedoelde profiel; G-code en galvo gebruiken eigen encoders en worden niet als Ruida-tekstcommando behandeld.

### M09 — Controllerbestanden en instellingen beheren

**P1 · R3 · Afhankelijkheden: M02, M04, M07.** Als machine-eigenaar wil ik opgeslagen bestanden en ondersteunde instellingen kunnen inspecteren, zodat ik de machine overzichtelijk beheer.

**Acceptatie:** bestandlijst, selectie, verwijdering en offline bestandsroute hebben eigen bewezen capabilities; instellingswijzigingen tonen huidige waarde, voorstel en backup/herstelmogelijkheid waar ondersteund; normale jobinstellingen wijzigen niet ongemerkt vendor-/mechanische controllerparameters.

## Werkplaats en productie

### W01 — Een jobrevisie klaarzetten

**P0 · R2 · Afhankelijkheden: C09, M03.** Als voorbereider wil ik een bevroren job opslaan, zodat productie exact de beoordeelde instellingen gebruikt.

**Acceptatie:** job bevat planhash, bronrevisie, machine-/materiaalrevisie en toolversies; editorwijzigingen maken een nieuwe conceptversie; opnieuw produceren maakt expliciet een nieuwe uitvoeringsinstantie van de gekozen job.

### W02 — Wachtrij en aantallen volgen

**P0 · R2 · Afhankelijkheden: W01, M06.** Als operator wil ik uploads, gestarte en afgeronde uitvoeringen onderscheiden, zodat ik orders correct tel.

**Acceptatie:** wachtrij gebruikt de expliciete toestandsmachine uit de architectuur; succesvolle upload telt geen voltooid werkstuk; onzekere uitvoering vraagt reconciliatie of een benoemde operatorregistratie.

### W03 — Historie voor herhaalwerk gebruiken

**P0 · R2 · Afhankelijkheden: W02, C08.** Als producent wil ik materiaal, aantallen, duur en revisie terugzien, zodat ik dezelfde order opnieuw kan maken.

**Acceptatie:** historie bewaart schatting en gemeten/bevestigde duur apart; mislukte en afgebroken uitvoeringen blijven vindbaar; herhaalwerk gebruikt de gekozen snapshot en toont verschillen met actuele recepten.

### W04 — Camera als plaatsingshulp gebruiken

**P1 · R3 · Afhankelijkheden: M01, M05.** Als operator wil ik mijn ontwerp op een bedfoto plaatsen, zodat ik bestaand materiaal beter benut.

**Acceptatie:** foto toont tijdstip en calibratiestatus; gewijzigde resolutie of lensopstelling invalideert de betreffende calibratie; handmatig plaatsen wordt niet als bewezen automatische snijnauwkeurigheid gepresenteerd.

### W05 — Drukwerk op markpunten registreren

**P1 · R3 · Afhankelijkheden: W04, C09.** Als maker wil ik gedrukte markeringen gebruiken, zodat het snijpad aansluit op het drukwerk.

**Acceptatie:** gekozen transformtype en benodigde punten zijn expliciet; onafhankelijke controlepunten rapporteren fout in mm; te weinig, collineaire of conflicterende metingen geven geen stilzwijgend geldig resultaat.

### W06 — Meerdere producten herkennen

**P2 · R4 · Afhankelijkheden: W05, T08.** Als operator wil ik templates op materiaal lokaliseren, zodat herhaalwerk minder handmatige plaatsing vraagt.

**Acceptatie:** herkenningsmodel en snijgeometrie zijn afzonderlijk; meerdere plausibele matches vragen beoordeling; similarityscore wordt niet omgezet in een fictieve millimeternauwkeurigheid.

### W07 — Restmateriaal en orders beheren

**P2 · R4 · Afhankelijkheden: C10, W03.** Als werkplaatsbeheerder wil ik restplaten en openstaande aantallen kennen, zodat nesting bruikbare voorraad inzet.

**Acceptatie:** restplaat heeft materiaal, dikte, contour en uitsluitingsgebieden; orderaantallen veranderen alleen door geregistreerde uitvoeringsresultaten; een herberekende nest behoudt brononderdelen en toont ontbrekende aantallen.

### W08 — Een compacte operatorweergave gebruiken

**P2 · R4 · Afhankelijkheden: W02, W03, X05.** Als operator wil ik alleen benodigde productie-informatie zien, zodat ik zonder ontwerpcomplexiteit de juiste job kan bedienen.

**Acceptatie:** machine, revisie, materiaal, bounds en aantallen staan samen; tweede scherm en desktop delen dezelfde toestand via één lokale machineservice; leesrechten en bedieningsrechten zijn afzonderlijk en reconnect herhaalt geen opdracht.

### W09 — Machine en proces kalibreren

**P1 · R3 · Afhankelijkheden: C08, M05, W03.** Als beheerder wil ik scanoffset, pointeroffset, kerf en plaatsing met benoemde proeven vastleggen, zodat recepten en productie op meetresultaten steunen.

**Acceptatie:** een proef vermeldt machine, materiaal, datum en meetmethode; scanoffset/pointeroffset/kerf hebben aparte waarden en scopes; een gewijzigde calibratie maakt afhankelijke voorbereide plannen ongeldig of houdt hun expliciete oude snapshot vast.

## Platform en dagelijks gebruik

### X01 — Volledig lokaal kunnen werken

**P0 · R2 · Afhankelijkheden: P01, T02, M04.** Als machine-eigenaar wil ik de kernfuncties offline gebruiken, zodat productie niet van een cloudverbinding afhangt.

**Acceptatie:** openen, tekenen, traceren, compileren en lokale machineverbinding werken zonder internet; ontbrekende optionele diensten blokkeren geen lokaal project; updates worden buiten een actieve transfer of machineopdracht toegepast.

### X02 — De interface efficiënt bedienen

**P0 · R1 · Afhankelijkheden: E03, C01.** Als dagelijkse gebruiker wil ik consistente sneltoetsen en panelen, zodat ik opdrachten snel vind.

**Acceptatie:** menu, toolbar en commandopalet gebruiken dezelfde command-ID's; werkruimtes behouden canvaspositie en selectie; veelgebruikte acties zijn met toetsenbord bereikbaar.

### X03 — Zonder kleurafhankelijkheid en op hoge DPI werken

**P0 · R2 · Afhankelijkheden: X02, S04.** Als gebruiker wil ik leesbare status en bediening, zodat ik ook met kleurwaarnemingsverschillen of andere schermschaling goed kan werken.

**Acceptatie:** proceslaag- en solverstatus hebben naam/symbool naast kleur; 100–200% schaal en toetsenbordfocus worden op doelplatformen beoordeeld; NL/EN en komma/punt veranderen geen fysieke uitvoerwaarden.

### X04 — Grote ontwerpen vloeiend bewerken

**P1 · R3 · Afhankelijkheden: E02, T06, C07.** Als ontwerper wil ik bij grote bestanden kunnen blijven selecteren en corrigeren, zodat complex werk praktisch blijft.

**Acceptatie:** p95 selectiereactie ≤ 100 ms op de benoemde 100k-segmentfixture en referentiemachine als voorlopig doel; tracing, nesting en compilatie zijn annuleerbaar; geheugendruk leidt tot begrensd gedrag zonder documentverlies.

### X05 — Dezelfde compiler automatiseren

**P1 · R3 · Afhankelijkheden: W01, M07.** Als gevorderde gebruiker wil ik projecten en varianten via CLI voorbereiden, zodat herhaalwerk reproduceerbaar wordt.

**Acceptatie:** GUI en CLI leveren bij dezelfde invoer dezelfde genormaliseerde planhash; compile/export is gescheiden van upload/start; machineacties vereisen een expliciete doelmachine en opdrachtintentie.

### X06 — Uitbreidingen gecontroleerd toevoegen

**P2 · R4 · Afhankelijkheden: X05, M02.** Als ontwikkelaar wil ik generators, importers en backends via versiegebonden interfaces toevoegen, zodat de kern beheersbaar blijft.

**Acceptatie:** plugins verklaren capabilities en API-versie; toegang tot bestanden, netwerk of machineacties is zichtbaar en begrensd; een pluginfout kan geen stille vervanging van een bevroren job of herhaling van start veroorzaken.

### X07 — De desktopapp betrouwbaar installeren en bijwerken

**P0 · R2 voor eerste doelplatform; P1 · R3/R4 voor verbreding · Afhankelijkheden: P02, X01, X03.** Als gebruiker wil ik een normale desktopinstallatie en herstelbare updates, zodat ik de app zelfstandig kan gebruiken.

**Acceptatie:** iedere ondersteunde OS-release heeft een geteste installatie-, update- en herstelroute; project-/profielmigratie behoudt een herstelbare vorige versie; een update tijdens actieve transfer of machinebediening wordt uitgesteld tot een geschikte toestand.

## Vier integratiescenario's

**Montageplaat:** P03 → E01/E04 → S01–S04 → C01–C03 → C06–C09 → M03–M06 → W01–W03. Breedtewijziging behoudt symmetrische gaten; de uiteindelijke maat en volgorde worden gecontroleerd.

**Klantlogo:** P03 → T01/T02 → T05/T06 → E02/E09 → C02/C05 → C07/C09 → W01. Lokaal traceherstel mag een beschermde letteropening niet sluiten.

**Doosvariant:** S02/S06/S07 → E10 → P06 → C03/C10 → C09 → W01. Een diktewijziging werkt door in verbindingen, maar niet dubbel in kerf.

**Verbroken verbinding:** W01 → M04 → foutinjectie → M07 → W02. Onzekere opslag of start blijft onzeker totdat bewijs of operatorreconciliatie volgt; de app produceert geen tweede exemplaar door alleen opnieuw te verbinden.
