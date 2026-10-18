# Softwareprofielen

Geraadpleegd op **17 oktober 2026**. Bewijslabels en onderzoeksgrenzen staan in [README](README.md). Broncodes R01–X02 verwijzen naar het [bronregister](05-bronnen-en-methode.md). De paragrafen “Betekenis voor BurnBurn” zijn eigen ontwerpinterpretaties.

## 1. RDWorks V8 / RDCAM / LaserWorks

**Rol:** fabrikantsoftware voor ontwerpvoorbereiding en laseruitvoer. De officiële catalogus vermeldt RDWorksV8 **8.01.72.001**. Voor de inhoudelijke analyse is een oudere, 64 pagina's tellende Thunder Laser-handleiding gebruikt; het actuele pakket is niet uitgevoerd. Namen als RDCAM en LaserWorks komen in dit ecosysteem voor, maar bewijzen niet dat iedere OEM-editie identiek is. [R01: Ruida-catalogus](https://www.rdacs.com/en/search?keyword=rdworksv8), [R02: OEM-handleiding](https://www.thunderlaserusa.com/knowledge/rdworksv8-0-software-manual).

**DOC — functies uit de handleiding:** vector- en bitmapimport; tekenen en uitlijnen; kleurgebonden bewerkingen met snelheid en min/maxvermogen; handmatige volgorde, startpunt en snijrichting; rechte/gebogen lead-in/out; curves sluiten, verbinden en ontdubbelen met toleranties; bitmapbewerking; padpreview met tijd en afstand, ook voor RD-bestanden. Download bewaart een job in controllergeheugen. Arrays bieden aantallen, tussenruimte, verschuiving en bedvulling. Verder worden Y-vervangende rotary, feeder en twee laserkanalen beschreven. Vindplaatsen: §§1.2, 2.13 en 4.5–4.10. [R02: directe PDF](https://maiyawuwbsbvimbjxytt.supabase.co/storage/v1/object/public/kb-assets/z/att-37d737c7917032fe87f2.pdf).

**ONTWERP — betekenis voor BurnBurn:** de waarde zit vooral in procescontrole. Een gebruiker die een buitencontour bewust als laatste zet, verwacht dat automatische optimalisatie die keuze respecteert. Maak handmatige beperkingen zichtbaar in de preview. Gebruik geometrische reparaties als voorstel met tolerantie en undo. Een rij/kolomarray vult een bed met herhalingen; voor willekeurige verschillende onderdelen is echte nesting een aparte taak.

Een geschikte werkstroom is: import → geometrie controleren → bewerkingen toewijzen → volgorde beoordelen → plaatsing/framing → opslaan/versturen. Bedienings- en fabrikantparameters horen verschillende wijzigingsroutes te krijgen. RDWorks is daarnaast nuttig als vergelijkingsreferentie voor hetzelfde proefontwerp, zonder byte-identieke exports te eisen.

## 2. MeerK40t

**Rol:** open uitbreidbare ontwerp- en besturingsapp. De README noemt directe Ruida-apparaten én een afzonderlijke Ruida-emulator. Het zijn verschillende gebruikswijzen: een emulator presenteert zich richting andere software als Ruida, en bewijst op zichzelf geen directe bediening van een echte Ruida-controller. Het project beschrijft GUI-aanpassing en een uitgebreide CLI. [M01: project-README](https://github.com/meerk40t/meerk40t).

**DOC — bewerkingsmodel:** geometrie wordt via “classification” aan operations gekoppeld. Cut en Engrave volgen vectoren; Raster maakt een gecombineerd raster; Image bewaart de eigen verwerking en resolutie per afbeelding; Dots beschrijft puntbelichting. Naast brandbewerkingen bestaan speciale procesacties. [M02: Operations](https://github.com/meerk40t/meerk40t/wiki/Online-Help:-OPERATIONS).

**DOC — productie:** de Spooler scheidt de actieve wachtrij van jobhistorie, met apparaatfilter, prioriteit, stappen, herhalingen, tijdinformatie en CSV-export. Deze appfuncties bewijzen niet dat de voortgang van elke Ruida-firmware even volledig wordt teruggemeld. [M03: Spooler](https://github.com/meerk40t/meerk40t/wiki/Online-Help-spooler).

**CODE — Ruida-aansluiting:** `RuidaDevice` verbindt een eigen driver aan een spooler en sessie. De gelezen code bevat pause/resume, abort en een interfacewissel die de oude sessie afsluit. Transporttimeouts hebben een expliciete UDP-beperking. Dit is codebewijs, geen hardwarevalidatie. [M04: device.py](https://github.com/meerk40t/meerk40t/blob/main/meerk40t/ruida/device.py).

**ONTWERP — betekenis voor BurnBurn:** maak procesintentie eerste klas. Bewaar één geometrieobject en meerdere verwijzingen vanuit bewerkingen, bijvoorbeeld “lichte markering” gevolgd door “uitsnijden”. Een historie met schatting én gemeten duur helpt tijdmodellen verbeteren. Een CLI hoort dezelfde compiler aan te roepen als de GUI, zodat automatisering geen tweede, afwijkende uitvoerroute krijgt.

Het interessantste onderscheid bij afbeeldingen is de plaats van resampling. Een vooraf geditherde afbeelding opnieuw schalen kan de puntverdeling veranderen. Bewaar bronafbeelding, fysieke maat, resolutie en filterketen apart en laat de gebruiker kiezen wanneer opnieuw rekenen nodig is.

## 3. VisiCut + LibLaserCut + VisiCam

**Rol:** voorbereiding en verzending naast een externe tekenapp. De projectsite noemt Linux, macOS en Windows, werkplaatsinstellingen, materiaalpreview, camera-positionering en het Portable Laser Format. [V01: projectsite](https://visicut.org/).

**DOC — werkstroom:** VisiCut onderscheidt materiaal/dikte, mapping van objecten naar bewerkingen en aanpasbare laserprofielen. Eén profiel kan meerdere behandelingen bevatten. De Inkscape-extensie ruimt de invoer op en zet tekst om naar paden. De handleiding maakt onderscheid tussen het referentiepunt van de tekening en het startpunt/origin. [V02: handleiding](https://github.com/t-oster/VisiCut/wiki/VisiCut-manual).

**DOC — Ruida-bewijs:** de hardwarewiki plaatst Ruida-machines, waaronder Bodor BCL 1309-XM en Thunderlaser Nova35, onder “Well-Tested”. Dit is een concrete projectclaim voor voorbeelden, geen certificering van alle Ruida-modellen. [V03: hardware](https://github.com/t-oster/VisiCut/wiki/Supported-Hardware).

**CODE — uitvoer:** `Ruida.java` heeft IP/UDP-, serial- en `.rd`-bestandsroutes. Rasterdelen worden naar vectorrepresentatie omgezet. De code noemt resoluties 100/200/500/1000 DPI. Dat toont een uitvoerstrategie, niet dat rasterkwaliteit of snelheid gelijk is aan native DSP-scanuitvoer. [V04: Ruida-driver](https://github.com/t-oster/LibLaserCut/blob/master/src/main/java/de/thomas_oster/liblasercut/drivers/Ruida.java).

**DOC — delen:** werkplaatsinstellingen kunnen als `.vcsettings` of ZIP op een vaste URL worden aangeboden. [V05: labinstellingen](https://github.com/t-oster/VisiCut/wiki/How-to-add-default-settings-for-your-lab).

**ONTWERP — betekenis voor BurnBurn:** maak een “voorbereiden”-modus voor gebruikers die elders tekenen. Laat regels bijvoorbeeld rood aan uitsnijden koppelen, maar geef de bewerking een eigen identiteit. Toon welke objecten ongekoppeld of dubbel gekoppeld zijn. Een werkplaatspakket moet naast instellingen ook herkomst, revisie en passend machineprofiel bevatten.

VisiCam is vooral relevant als afzonderlijke cameraservice. Voor BurnBurn is het architectuuridee dat beeldinvoer en controllerverbinding onafhankelijk mogen werken. Camera-uitval hoeft documentbewerking niet te blokkeren; de app moet wel weten dat de laatst berekende plaatsing mogelijk verouderd is.

## 4. Rayforge

**Rol:** moderne CAD/CAM-app met een werkstroom van opeenvolgende bewerkingen. De README beschrijft parametrische schetsen, lagen, voorraadmateriaal met geometrie/dikte, tabs, kerf, simulatie en materiaalbeheer. Het zijn algemene productfuncties, geen Ruida-dekkingslijst. [Y01: project-README](https://github.com/barebaric/rayforge).

**DOC — echte Ruida-grens:** de specifieke firmwarepagina beschrijft de Ruida RPA-driver via USB, UDP of RPA-RPC, plus jobverzending, positie, joggen, homing, framing zonder laser en vermogensinstellingen. Dezelfde pagina noemt de status experimenteel, meldt testen op RDC6442S en sluit rotary nog uit. [Y02: firmwaredocumentatie](https://rayforge.org/docs/reference/firmware/).

**DOC — ontwikkeling:** release notes van 1.12 beschrijven de nieuwe driver, een UDP-correctie voor Windows en correcties voor buiten bereik gemelde posities. De Command-stap waarschuwt voor drivers zoals Ruida die geen machinecodetekst consumeren. Een aanwezige G-codefunctie geldt dus niet vanzelf voor Ruida. [Y03: changelog](https://github.com/barebaric/rayforge/blob/main/CHANGELOG.md).

**DOC — configuratie:** de wizard past stappen aan de gekozen profielbron aan en kan een LightBurn `.lbdev`-profiel importeren. Dat is configuratie-import, geen bewijs van volledige project- of machinecompatibiliteit. [Y04: setup](https://rayforge.org/docs/getting-started/first-time-setup/).

**ONTWERP — betekenis voor BurnBurn:** neem expliciet voorraadmateriaal en bewerkingsstappen op. De brongeometrie blijft bewerkbaar terwijl kerf, tabs en rasterinstellingen er als afgeleide processtappen op worden toegepast. Toon bij apparaatkeuze een dekkingskaart en verberg of blokkeer niet-uitvoerbare stappen vóór compilatie.

De positiecorrecties zijn een nuttige ontwerpwaarschuwing: een foutieve of ongeldige Z-melding mag geen onverwachte XY-verplaatsing veroorzaken. Valideer assen afzonderlijk en toon onbekende posities als onbekend. Automatisch wisselen van transport vraagt bovendien dezelfde machine-identiteit en een ondubbelzinnige jobtoestand.

## 5. MantiSoft / MantiSolo / MantiSurf

**Rol:** nieuwe officiële Ruida-softwarefamilie. De productsite noemt MantiSoft een product van Ruida's Team X met Windows-, Linux- en macOS-ondersteuning. [T01: productsite](https://www.rdmantisoft.com/en).

**DOC/CAT — aangetroffen aanbod:** de downloadpagina noemt desktop MantiSoft, mobiel MantiSolo en webgebaseerd MantiSurf. Bij de handleidingvermeldingen staan respectievelijk V1.1.8, V1.00.04 en V1.2 met datum 21 september 2026. De Ruida-catalogus bevestigt een MantiSoft 1.1.8-pakket. Exacte prijzen, licentievoorwaarden, controllerlijst en functies per client zijn niet vastgesteld. [T02: downloads](https://www.rdmantisoft.com/en/download), [R03: Ruida-downloads](https://www.rdacs.com/en/download).

**OPEN:** de supportlinks “Device supported” en “Functions” leverden in de tekstweergave opnieuw de algemene supportpagina op. Daarom is hier geen claim dat MantiSoft een RDC6442/6445 zonder aanvullende hardware bedient. Ook “webgebaseerd” bewijst geen universeel browser-to-UDP-pad. [T03: support](https://www.rdmantisoft.com/en/support).

**ONTWERP — betekenis voor BurnBurn:** scheid volledige voorbereiding op desktop van een compacte operatorweergave voor status, framing en jobselectie. Beide moeten dezelfde job-ID en revisie tonen. Eerst is echter een concrete compatibiliteitstest nodig; de familie is een relevante onderzoekskandidaat, nog geen bewezen vervanger voor deze machine.

## 6. RDVision, RDVisionWorks en MarkVision-varianten

**Rol:** gespecialiseerde visuele registratie, contourherkenning en productie. De RDV6445G-productpagina noemt RDVision, RDCMarkVision, RDWorksV8 en RDImage bij een systeem met feature- en markpuntpositionering. Het is een visionvariant op basis van RDC6445G; de families zijn niet zonder meer uitwisselbaar met een standaardcontroller. [C01: RDV6445G](https://www.rdacs.com/en/product-detail/rdv6445g.html).

**DOC — RDVision:** de door Ruida geschreven handleiding uit 2016 scheidt featurecontouren van snijcontouren. Een model kan beeldkenmerken herkennen en een apart geïmporteerd of bewerkt snijpad bevatten. Er zijn een matchtest, zoekhoek/similarity-instellingen, meerdere modellen, calibratiebestanden en beeldstitching met meerdere features. Vindplaatsen: hoofdstukken 4–5; PDF-bladen 44–68, waarbij de afgedrukte nummering later defect is. [C02: Ruida-handleiding via mirror](https://www.ruidacontroller.com/wp-content/uploads/2021/10/Rdvision-Software-User-Manual-v1.0.pdf).

**DOC — RDVisionWorks:** Ruida's eigen tutorials beschrijven template matching op grijswaarde, vorm of contour en een contour-extractiewerkstroom met drempel-, rand- en kleurkeuzes. Het betreft deels grootformaat- en messnijvoorbeelden; die zijn geen bewijs dat alle functies op een standaard CO₂-machine beschikbaar zijn. [C03: fabrikanttutorials](https://www.rdacs.com/support/faq).

**ONTWERP — betekenis voor BurnBurn:** laat een gedrukt logo de plaatsing bepalen, terwijl het snijpad een vaste offset rondom het product blijft. Eén knop “trace” kan die twee doelen niet goed vertegenwoordigen. Bewaar een herkenningsmodel, een gewenste snijcontour en de gemeten transform apart. Toon matches ter beoordeling en rapporteer ontbrekende of dubbelzinnige detecties. Camera-overlay, markregistratie en automatische templateherkenning verdienen elk een eigen werkstroom.

## 7. RDNest

**Rol:** plaatindeling vóór de machine-uitvoer. Ruida beschrijft Windows 10+, beheer van onderdelen en platen, batchinvoer, automatische nesting, spiegel-/rotatiekeuzes, inner nesting en export. Er worden meerdere autorisatieroutes genoemd. Dit bewijst geen directe controllerbediening en geen gemeten opbrengstverbetering. [N01: officiële productintroductie](https://www.rdacs.com/en/news-detail/162.html).

**ONTWERP — betekenis voor BurnBurn:** optimaliseer op echt productiemateriaal: plaatcontour, gaten, aantallen en toegestane rotaties. Materiaalrichting kan rotatie of spiegeling verbieden. Geef een geneste layout een eigen resultaatobject, zodat wijzigen van aantallen opnieuw rekenen mogelijk maakt. Meet zowel materiaalbenutting als aantallen correct geplaatste onderdelen; een visueel volle plaat kan nog een onvolledige order zijn. Common-line cutting is een extra procesfunctie en wordt hier niet aan RDNest toegeschreven.

## 8. Ruida Protocol Analyzer (RPA)

**Rol:** onderzoeksgereedschap én driverbasis. Het project beschrijft capture-analyse, getypeerde decodevelden, resynchronisatie, move/cut-plots, RD-generatie en een `RdDriver` met listeners. Een applicatieadapter vertaalt eigen jobs naar de driver; integratie kan lokaal of via RPC. De README waarschuwt dat het project snel verandert. [P01: projectdocumentatie](https://github.com/StevenIsaacs/ruida-pa).

**ONTWERP — betekenis voor BurnBurn:** gebruik analyse en transport achter een begrensde adapter. Bewaar onbekende commands bij import, koppel logregels aan job-ID's en bouw een offline replaymodus. Zo kan een fout worden teruggebracht tot planning, encoding of transport. RPA is geen complete tekenapp en de beschreven transportroutes betekenen geen garantie dat elk controllerboard native alle routes ondersteunt.

## 9. Aanpalende software en afbakening

**LaserGRBL** zegt zelf uitsluitend met GRBL-boards te werken. Het is daarom geen directe vervanger voor een Ruida-RD-backend. De firmwareafhankelijke zichtbaarheid van knoppen is wel een bruikbaar UX-principe. [X01: officiële FAQ](https://lasergrbl.com/faq/).

**Inkscape/Illustrator/CorelDRAW** zijn in deze studie ontwerppartners via import of extensies, geen zelfstandig bewezen Ruida-besturingsapps. Het [ruida-laser-project](https://github.com/jnweiger/ruida-laser) blijft een aanvullende protocolreferentie; zie het bestaande [protocolonderzoek](../rudia/README.md).

**LaserCAD, LaserCut, RDMarkVision, RDCutist en andere gelijkende namen** mogen niet op naam alleen als uitwisselbare RDC6442/6445-software worden behandeld. De officiële catalogus groepeert meerdere machinesoorten. Een afzonderlijke featurevergelijking van die specialistische families valt buiten de bewezen standaard-Ruida-selectie van dit dossier.
