# Totaalplan voor BurnBurn

Datum: **17 oktober 2026**. Status: **productvoorstel en uitvoerbare backlog**. Dit plan bouwt voort op de bestaande analyses van [LightBurn](../lightburn/README.md), [Ruida](../rudia/README.md) en [andere lasersoftware](../andere-software/README.md). Nieuwe referenties voor schetsbewerking en tracing staan in het [bronregister](06-bronnen.md). Alle voorgestelde BurnBurn-functies zijn ontwerpkeuzes, geen geïmplementeerde functies.

**Voorstel: bouw een lokale 2D-ontwerp- en laserapplicatie die de praktische laserworkflow van LightBurn combineert met de precisie van een CAD-schetseditor en de vrijheid van een vectorillustratie-editor.** De grootste productinvestering gaat naar goed bewerkbare geometrie, voorspelbare wijzigingen en tracing die bruikbare laserpaden oplevert. Ruida is de eerste productiebackend.

Het onderscheid moet zichtbaar worden in een dagelijkse taak: een klant stuurt een slechte afbeelding; de gebruiker maakt er nette, bemaatbare onderdelen van, wijzigt de materiaaldikte en produceert ze zonder heen en weer te gaan tussen drie programma's.

Het voorstel bevat **74 user stories in acht domeinen**, met prioriteit, release, afhankelijkheden en acceptatiecriteria. De bouwvolgorde bestaat uit technische proeven, een bruikbare editor, de eerste Ruida-productieversie, onderscheidende ontwerpvoorbereiding en verdere werkplaatsfuncties.

## Leesroute

| Document | Inhoud |
|---|---|
| Dit overzicht | Productkeuzes, volledige scope en belangrijkste werkstromen |
| [Editor](01-editor.md) | Zinvolle ideeën uit Onshape en Fusion, interactie en parametrische geometrie |
| [Tracer](02-tracer.md) | Tracewerkstromen, algoritmische keten, herstel en meetbare kwaliteit |
| [User stories](03-user-stories.md) | Geprioriteerde stories met acceptatiecriteria voor de volledige applicatie |
| [Architectuur](04-architectuur.md) | Documentmodel, geometrie, solver, compiler, Ruida en opslag |
| [Bouwplan](05-bouwplan.md) | Releases, afhankelijkheden, onderzoek, validatie en beslismomenten |
| [Bronnen](06-bronnen.md) | Onderbouwing en onderscheid tussen bestaande functies en nieuwe voorstellen |
| [Rust implementatieplan](../implementatieplan/README.md) | Native AppKit voor Mac, gedeelde Rust-kern, Windows-route, TDD en concrete werkpakketten |

## Productkeuzes

1. **Eén ontwerp, meerdere bewerkingen.** Een contour kan worden gemarkeerd én gesneden zonder geometrie te dupliceren. Ontwerplagen organiseren het document; proceslagen bepalen productie. Kleur blijft een handige snelkoppeling.
2. **Vrij tekenen en bematen samenwerken.** Gewone paden blijven direct bewerkbaar. Een gebruiker kan een onderdeel bewust parametrisch maken. Geïmporteerde illustraties krijgen niet automatisch duizenden constraints.
3. **Bronnen behouden.** Afbeeldingen, tekst, traces, offsets en patronen blijven herleidbaar. Destructieve omzetting is een expliciete bewerkingskeuze met undo.
4. **Productie volgt een beoordeelde revisie.** De preview, RD-export en upload komen uit hetzelfde bevroren JobPlan. Een editorwijziging verandert een klaargezette job niet.
5. **Eerst dagelijks beter, daarna breder.** Een uitstekende 2D-editor met betrouwbare Ruida-uitvoer gaat vóór galvo, 3D-CAD, marktplaatsen of verplichte cloudfuncties.

## Gebruikers en succes

| Gebruiker | Belangrijkste taak | Gewenst resultaat |
|---|---|---|
| Maker | Onderdeel tekenen en passend maken | Maten wijzigen zonder opnieuw tekenen |
| Grafisch ontwerper | Logo, tekst en illustratie voorbereiden | Snelle curvebewerking en nette contouren |
| Operator | Een eerder voorbereid werkstuk produceren | Juiste job, materiaal, oorsprong en status zichtbaar |
| Werkplaatsbeheerder | Machines, recepten en herhaalwerk beheren | Reproduceerbare instellingen en duidelijke compatibiliteit |
| Productiebedrijf | Varianten en orders efficiënt maken | Minder voorbereiding, minder uitval, betere plaatindeling |

Een gebruiker kan meerdere rollen hebben; voor één zelfstandige mogen daar geen administratieve stappen uit volgen. Rollen en rechten zijn pas nodig bij gedeelde bediening.

De eerste vergelijkingsmeting gebruikt vijf taken: een montageplaat bematen, een doos aan materiaaldikte aanpassen, een logo corrigeren, handschrift als enkel pad tracen en een job na een verbindingsfout hervinden. Meet actieve werktijd, herstelacties, foutieve geometrie en fysieke uitkomst. **Doelhypothese:** minstens 30% minder actieve voorbereidingstijd dan de eigen LightBurn-werkwijze op de eerste vier taken, zonder verlies van uitvoerkwaliteit. Dit is een te toetsen productdoel, geen bewezen voordeel.

## Wat overnemen en wat verbeteren

| Referentie | Waarde behouden | Eigen verbetering |
|---|---|---|
| LightBurn | Proceslagen, Line/Fill/Offset Fill/Image, instellingen, preview, materiaaltests, Ruida-route | Betere geometriebewerking, relaties tussen onderdelen, diagnose en niet-destructieve voorbereiding |
| RDWorks | Handmatige padvolgorde, startpunt en richting, reparatie en herhaling | Beperkingen zichtbaar maken; repareren met fysieke tolerantie en voor/na-vergelijking |
| MeerK40t | Operations, wachtrij en historie | Zelfde geometrie via meerdere behandelingen; status gekoppeld aan bewijs |
| VisiCut | Materiaal, mapping en machineprofiel scheiden | Werkplaatsrecepten met revisies en reproduceerbare jobs |
| Rayforge | Processtappen en parametrische schetsen | Hybride vector/CAD-editor en expliciete capabilities per Ruida-route |
| Onshape | Constraints, bemating, construction geometry en gerichte conflictdiagnose | De relevante 2D-principes toepassen op laseronderdelen en vrij artwork |
| Fusion | Parameters, gekoppelde referenties en beoordeelbare AutoConstrain-voorstellen | Materiaaldikte, pasvorm en varianten rechtstreeks laten doorwerken |
| Potrace, AutoTrace, VTracer | Verschillende tracebenaderingen | Een gezamenlijke, lokaal herstelbare productiewerkstroom met topologie- en maatcontrole |

Voor laserproducten is dit gebaseerd op de bestaande [featurevergelijking](../andere-software/02-featurevergelijking.md). Voor CAD en tracing: [primaire bronnen](06-bronnen.md). De verbeterkolom is een eigen voorstel. Er wordt niet beweerd dat al deze verbeteringen in andere producten ontbreken.

## Werkruimte

Vier taakgerichte werkruimtes delen hetzelfde document:

| Werkruimte | Centrale vraag | Zichtbare hulpmiddelen |
|---|---|---|
| Ontwerpen | Welke vorm wil ik maken? | Objectboom, teken-/nodegereedschap, maten, constraints en parameters |
| Traceren | Welke lijnen bedoel ik met dit beeld? | Bron, masker, trace, lokale correcties en kwaliteitskaart |
| Voorbereiden | Hoe wordt dit onderdeel gemaakt? | Materiaal, proceslagen, kerf, tabs, volgorde, plaatsing en preflight |
| Produceren | Welke revisie gaat naar welke machine? | Jobpreview, framing, upload, bediening, wachtrij en historie |

Een snel traject laat ervaren gebruikers vanuit Ontwerpen direct een bekende proceslaag toewijzen en preview openen. Werkruimtes zijn geen verplicht stappenformulier. Een onbekende importeenheid of onuitvoerbare bewerking vraagt wel een concrete keuze.

Het canvas blijft centraal. Links staat een doorzoekbare object-/onderdelenboom, rechts een contextinspecteur. Proceslagen hebben een eigen paneel met naam, kleur, modus en uitvoerstatus. Een commandopalet en consistente sneltoetsen ondersteunen dagelijks werk. In schetsmodus gebruiken constraints symbolen en statuslabels; ze concurreren niet met de kleurcodering van proceslagen.

## Volledige functionele scope

| Domein | Te leveren |
|---|---|
| Projecten | Lokale bestanden, herstel, undo, revisies, bronassets, meerdere platen en overdraagbare pakketten |
| Import/export | SVG/DXF/PNG/JPEG als eerste route, daarna PDF en beperkte LBRN2-migratie; semantische importmelding |
| Vectoren | Paden, bogen, Béziers, knopen, samengestelde vormen, booleans, offsets, trim/extend, tekst en meetgereedschap |
| Parametrisch ontwerp | Sketches, maten, constraints, parameters, gekoppelde referenties, componenten en varianten |
| Laserspecifiek ontwerp | Sleuven, vingers, pasvormproeven, scharnierpatronen, stencilbruggen en registratiemarkeringen |
| Beelden | Niet-destructieve schaal, uitsnede, tooncorrecties, dithering, grayscale en tracebron |
| Tracing | Silhouet, centerline, technische reconstructie, kleurregio's en lokale herstelgereedschappen |
| CAM | Proceslagen, meerdere behandelingen, kerf, tabs, leads, volgorde, rasterplanning en inspecteerbare preview |
| Materialen | Machinegebonden recepten, testkaarten, revisies, herkomst en meetresultaten |
| Plaatindeling | Arrays, mixed-part nesting, nerfrichting, restplaten en orderaantallen |
| Positionering | Origins, framing, camera-overlay, markregistratie en later herkenning |
| Machines | Profielen, capabilitymatrix, Ruida RD-encoder/decoder, transport, diagnose en gecontroleerde bediening |
| Productie | Bevroren jobs, wachtrij, aantallen, herhaalwerk, historie, tijdschatting en afwijkingen |
| Platform | Offline desktop, toegankelijkheid, NL/EN, high-DPI, uitbreidingen en dezelfde compiler via CLI |

## Drie voorbeeldwerkstromen

### Montageplaat met wijzigbare maten

De gebruiker tekent een plaat van 120 × 80 mm met vier gaten. Gaten krijgen gelijke diameter, spiegelrelaties en een randafstandparameter. Een sleuf verwijst naar `materiaaldikte + speling`. Aanpassing naar 150 mm breed verschuift het gatenpatroon mee. Markeer- en snijbewerkingen verwijzen naar hetzelfde onderdeel. CAM past kerf toe op afgeleide paden; de nominale plaatmaat blijft intact. De preview legt uit waarom gaten vóór de buitenrand worden gesneden.

### Slecht klantlogo naar bruikbare productie

De gebruiker importeert een JPEG, kiest de fysieke breedte en traceert de buitenvorm. Kleine binnenletters krijgen een eigen lokale drempel; twee bedoelde gaten worden behouden. De trace toont waar vereenvoudiging te veel afwijkt. Een ronde uitsparing wordt na beoordeling een echte cirkel. Een graveerbewerking gebruikt de binnenvormen en een snijbewerking de buitencontour. Het bronbeeld blijft in het project om een latere correctie opnieuw te kunnen beoordelen.

### Handschrift als enkel laserpad

De gebruiker fotografeert een tekst, corrigeert perspectief en kiest centerline. De preview toont eindpunten en kruisingen. Een ontbrekend streepje wordt lokaal aangevuld; een dubbelzinnige kruising blijft een beslispunt. De uitvoer bestaat uit open lijnpaden met een lijnbewerking. Er wordt geen gesloten snijcontour van gemaakt. Voor een uitgeknipte silhouetversie kiest de gebruiker een afzonderlijke contourtrace.

## Grenzen en keuzes

Eerste product: lokale desktopapp, 2D en 2,5D laserwerk, één benoemde Ruida-configuratie. Windows en macOS zijn productdoelen; Linux volgt zodra verpakking en drivers op een concrete configuratie zijn beproefd. Internet is niet nodig voor openen, bewerken, traceren, compileren of de lokale machineverbinding.

Een volledig 3D-parametrisch CAD-systeem, gelijktijdig cloudtekenen, een universele controllerdriver en generatief verzonnen snijgeometrie vallen buiten de eerste releases. G-code en galvo krijgen later eigen backends op dezelfde JobPlan-grens. Rotary, feeder en meerdere laserkanalen worden uitsluitend als ondersteund aangeboden na tests van de betreffende combinatie.

De [user stories](03-user-stories.md) specificeren de onderdelen. Het [bouwplan](05-bouwplan.md) legt vast wanneer het geheel overtuigend beter en daadwerkelijk produceerbaar is.
