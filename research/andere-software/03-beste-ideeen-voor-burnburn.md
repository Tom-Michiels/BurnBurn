# Beste ideeën voor BurnBurn

Alle onderstaande uitwerkingen zijn **ONTWERP**: eigen voorstellen, geïnspireerd door het bronnenonderzoek. Ze beschrijven geen verborgen implementatie van de onderzochte producten. Voor feitelijke productfuncties en bronlinks: [softwareprofielen](01-softwareprofielen.md) en [bronregister](05-bronnen-en-methode.md).

## I01 — Geometrie, bewerking en materiaal afzonderlijk modelleren

**Aanleiding:** VisiCut maakt materiaal, mapping en laserprofiel afzonderlijk begrijpelijk; MeerK40t gebruikt operations. Het algemene inzicht is dat een kleur een handige selectieregel is, maar geen volledige productie-intentie.

**Voorstel:** een object heeft een stabiele `GeometryId`. Een `Operation` verwijst naar één of meer objecten en bevat bewerkingssoort, instellingen en volgorde. Een `MaterialPreset` levert een benoemde, gereviseerde startwaarde. Dezelfde vorm mag naar meerdere operations verwijzen, bijvoorbeeld markeren en daarna uitsnijden. Een importregel kan rode lijnen aan “snijden” koppelen, maar blijft een hulpmiddel: nadien kan de gebruiker die toewijzing onafhankelijk wijzigen.

De bewerkingenlijst toont aantallen objecten en een fysieke eenheid naast snelheid. Selectie van een operation licht de betrokken geometrie op. Een analysepaneel toont ongekoppelde objecten en meervoudige toewijzingen; die laatste kunnen bewust zijn. Verwijderen van een operation verwijdert de geometrie niet.

**Acceptatie:** één cirkel krijgt twee bewerkingen. Wijzigen van zijn diameter werkt door in beide uitvoerpaden. De bron bevat nog precies één cirkel en undo herstelt de vorige diameter.

## I02 — Eén bevroren plan voor preview, bestand en upload

**Aanleiding:** meerdere apps scheiden ontwerpen en uitvoer; RDWorks documenteert ook preview van opgeslagen RD. Voor BurnBurn moet die scheiding controleerbaar blijven.

**Voorstel:** de compiler maakt een immutable `JobPlan`, met opgeloste geometrie, concrete waarden, oorsprongtransformatie, scanruns, travels en bewerkingsvolgorde. Preview tekent dat plan. De RD-encoder ontvangt datzelfde plan. Maak ook een gedecodeerde RD-weergave mogelijk, zodat het werkelijke bestand met de planning kan worden vergeleken.

Bewaar bij de job een planhash, compiler- en encoder-versie, machineprofielrevisie en bronnenrevisie. Een profielwijziging maakt de oude job niet stilzwijgend anders. Bij verschillen meldt de app dat opnieuw compileren nodig is. Een previewcache is alleen geldig voor de volledige invoersleutel, inclusief materiaalhoogte en registratie indien gebruikt.

**Acceptatie:** twee compilaties van dezelfde invoer leveren hetzelfde plan. RD terugdecoderen bewaart lagen, bounds, eindposities en vermogen binnen vastgelegde numerieke toleranties. Een wijziging van origin maakt de cache ongeldig.

## I03 — Productiewachtrij met eerlijke voortgang

**Aanleiding:** MeerK40t maakt wachtrij en historie zichtbaar. Voor een DSP met eigen opslag zijn hostvoortgang en machinevoortgang verschillende metingen.

**Voorstel:** gebruik de toestanden `Draft → Compiled → Uploading → Stored → StartRequested → Running → Completed`, plus `Failed`, `Cancelled` en `Unknown`. Sla bewijs per overgang op. Een transport-ACK bewijst geen voltooid werkstuk. Als de controller geen betrouwbare uitvoerterugmelding levert, blijft de toestand onbekend of vraagt de operator de fysieke voltooiing te registreren.

Een wachtrij-item bevat job-ID, naam, planhash, apparaat, materiaal, aantal exemplaren, verwachte duur en gemeten duur. Een kopieeractie maakt een nieuwe productie-intentie; opnieuw verbinden maakt geen nieuwe intentie. Ontwerpen kunnen verder worden aangepast terwijl de gecompileerde job onveranderd klaarstaat.

**Acceptatie:** verbreek de verbinding tijdens upload en na start. Geen van beide situaties resulteert automatisch in “klaar”. Opnieuw verbinden veroorzaakt geen tweede start; de historie toont welke stap onzeker bleef.

## I04 — Optimalisatie met vastgelegde beperkingen

**Aanleiding:** RDWorks geeft controle over start en richting. Een optimalisator moet dat kunnen combineren met efficiënte travel.

**Voorstel:** leg eerst harde afhankelijkheden vast, zoals binnencontour vóór buitencontour en graveren vóór los snijden. Voeg daarna vergrendelde startpunten, richting en groepsvolgorde toe. Optimaliseer reistijd uitsluitend binnen de resterende vrijheid. Een cyclus in afhankelijkheden levert een gerichte foutmelding met betrokken bewerkingen op.

Toon in de preview volgordenummers, bewegingsrichting en reden van een beperking. Bied een vergelijking “huidig” versus “voorstel” met travelafstand en geschatte tijd. Een gebruiker kan één onderdeel vastzetten zonder alle optimalisatie uit te schakelen. Reparaties aan losse segmenten moeten vooraf expliciet bepalen of een gesloten contour bestaat.

**Acceptatie:** bij een plaat met twee gaten komen beide gaten vóór de buitenrand. Een vast startpunt verschuift niet. Een snellere route die deze voorwaarden schendt wordt afgewezen.

## I05 — Geometriecontrole met meetbare toleranties

**Aanleiding:** RDWorks combineert meerdere controles en reparaties. De cruciale ontwerpvraag is wat een reparatie fysiek verandert.

**Voorstel:** een controle levert problemen met locatie, ernst, betrokken objecten en meetwaarde: open eindpunten op 0,08 mm, overlappende segmenten over 12 mm, nulsegmenten of zelfdoorsnijding. Repareren is een aparte opdracht met voor/na-overlay. Bewaar toleranties in millimeters en rapporteer hoeveel geometrie verandert.

Gebruik verschillende criteria voor endpoint-verbinden, duplicaten en curvevereenvoudiging. Een globale “precision”-schuif kan anders een klein detail onbedoeld wissen. Pas vereenvoudiging toe met een maximale fysieke afwijking; dicht geen willekeurige opening omdat twee objecten toevallig dichtbij staan.

**Acceptatie:** een DXF met een bedoelde sleuf van 0,15 mm blijft intact terwijl een expliciet geselecteerde naad van 0,05 mm kan worden gesloten. Undo herstelt alle oorspronkelijke segmenten.

## I06 — Niet-destructieve beeldketen op fysieke uitvoerschaal

**Aanleiding:** MeerK40t onderscheidt gecombineerd rasteren en een afzonderlijk verwerkte afbeelding. Dat maakt resampling een zichtbare procesbeslissing.

**Voorstel:** bewaar het bronbeeld, fysieke afmetingen, kleur-/alphabeleid, resamplingfilter, helderheid/gamma, rasterinterval en dithering als afzonderlijke data. Voer dithering pas uit nadat de fysieke schaal en rasterresolutie vaststaan. Voor vooraf geditherde beelden bestaat een expliciete modus met vaste pixelschaal en waarschuwing bij resampling.

De preview biedt bronbeeld, verwerkt beeld en echte scanruns. Een tijdschatting gebruikt scanlengten inclusief versnellingstrajecten. Maak verschil tussen binaire puntpatronen en vermogensmodulatie; het mogen geen verwisselbare UI-namen voor dezelfde export zijn.

**Acceptatie:** een grijsverloop op 50 mm breed heeft een reproduceerbare rasterhash. Vergroten naar 75 mm rekent opnieuw vanuit de bron. Het project opnieuw openen levert dezelfde pixels op bij dezelfde instellingen.

## I07 — Werkplaatsprofielen en materiaalrecepten met herkomst

**Aanleiding:** VisiCut deelt werkplaatsinstellingen; Rayforge heeft profielimport. Deelbaarheid wordt pas betrouwbaar wanneer recepten aan de juiste machine zijn gebonden.

**Voorstel:** een `WorkshopPackage` bundelt machineprofielen, materiaalrecepten, notities en calibratiereferenties. Vermeld auteur, revisie, machineoptiek, nominale buisconfiguratie, materiaal/dikte en testdatum. Een gebruiker ziet welke waarden uit het recept komen en welke lokaal zijn aangepast.

Een materiaalrecept wordt bij productie als snapshot in de job opgeslagen. Bijwerken van de bibliotheek wijzigt bestaande jobs niet. Maak onderscheid tussen een gekopieerde instelling en een nog gekoppelde bibliotheekwaarde. Toon een diff voor import: bedmaat, origin en mechanische waarden verdienen meer aandacht dan alleen een andere weergavenaam.

**Acceptatie:** twee werkplekken importeren hetzelfde pakket en maken hetzelfde plan. Een bibliotheekupdate wijzigt de opgeslagen vorige job niet. Een ontbrekend profiel veroorzaakt geen automatische terugval naar willekeurige defaults.

## I08 — Camera-overlay, registratie en herkenning afzonderlijk

**Aanleiding:** VisiCam ondersteunt positionering; RDVision onderscheidt herkennen en snijden. Het zijn verschillende problemen met verschillende bewijseisen.

**Voorstel:** bied drie werkstromen: visueel plaatsen op een bedfoto; markpunten meten om een ontwerp te registreren; modellen herkennen om meerdere producten te lokaliseren. Bewaar lenscalibratie, bedtransformatie en jobregistratie als aparte objecten. Een `RecognitionTemplate` bevat beeldfeatures; een `CutTemplate` bevat het gewenste pad.

Een detectie levert positie, rotatie, eventuele schaal en kwaliteitsinformatie op. Een similarity-score is algoritmeafhankelijk en mag niet als millimeterprecisie worden gepresenteerd. Meet registratiefout met onafhankelijke controlepunten. Toon materiaalhoogte, beeldtijd en geldigheidsgebied. Vereis beoordeling bij te weinig of dubbelzinnige matches.

**Acceptatie:** een scheef geplaatst gedrukt logo krijgt een snijcontour op vaste afstand. Een tweede gelijkend logo veroorzaakt een zichtbare keuze. Losse controlepunten rapporteren de fysieke afwijking en een gewijzigde cameraresolutie maakt eerdere calibratie ongeldig.

## I09 — Arrays en nesting als aparte modellen

**Aanleiding:** RDWorks behandelt herhalingen; RDNest behandelt onderdelen en platen. Die optimalisaties vragen andere invoer.

**Voorstel:** een `Array` bevat bronobject, aantallen, tussenruimte, rotatie en eventueel rijverschuiving. Een `NestRequest` bevat een order met aantallen, plaatcontouren, gaten, afstandsregels, materiaalrichting en toegestane rotaties/spiegeling. Een `NestResult` bewaart plaatsingen en niet-geplaatste aantallen.

Bied een snelle bedvulling voor identieke stukken en een aparte nestingtaak voor gemengde onderdelen. Rapporteer benutting met een expliciete noemer: netto onderdeeloppervlak gedeeld door bruikbaar plaatoppervlak. Rapporteer ook complete sets, rekentijd en te verwachten travels. Nesting mag herkenningstekens of materiaalrichting niet opofferen voor een mooi percentage.

**Acceptatie:** een order van twintig A- en tien B-onderdelen behoudt exact die aantallen. Een onderdeel met nerfrichting mag alleen toegestane rotaties krijgen. Geen plaatsing kruist een plaatgat of opgegeven marge.

## I10 — Capaciteiten per machine, firmware en verbinding

**Aanleiding:** Rayforge toont hoe algemene productfuncties en Ruida-drivermogelijkheden uiteenlopen. Ook LaserGRBL gebruikt firmwareafhankelijke opties.

**Voorstel:** een `Capability` heeft `supported`, `unsupported` of `unknown`, met bewijsbron en voorwaarden. Voorbeelden: native raster, controllerbestandslijst, rotary, Z-jog, camera, feeder en start/pause-status. Combineer driverkennis met modelprofiel en gerichte detectie. Een aanwezig configuratieveld is geen bewijs van fysieke hardware.

De UI toont waarom een functie niet beschikbaar is en welke verificatie ontbreekt. De compiler controleert dezelfde capabilities opnieuw, zodat een geïmporteerd project de UI-beperking niet omzeilt. Een unsupported stap mag niet stilzwijgend wegvallen.

**Acceptatie:** een project met rotary kan op een profiel zonder bewezen rotaryroute wel worden geopend, maar niet als vlakbedjob worden verstuurd. De fout verwijst naar de concrete bewerking.

## I11 — Transport en diagnostiek achter één adapter

**Aanleiding:** LibLaserCut en RPA maken uitvoer inspecteerbaar. BurnBurn heeft baat bij een duidelijke grens tussen planning, encoding en communicatie.

**Voorstel:** werk met `compile(plan) → rdBytes`, `inspect(rdBytes) → decodedJob` en een adapter voor controllerhandelingen. Geef controllergeheugen, directe bediening en upload ieder eigen opdrachten. Bewaar jobhash, pakketvolgorde, antwoordtype en fouten in een exporteerbaar diagnoserapport. Laat onbekende protocolelementen in de decoder bestaan.

Herstellen van een verbinding is iets anders dan hervatten van productie. Wissel alleen van USB naar Ethernet wanneer hetzelfde apparaat herkenbaar is en de jobtoestand bekend is. Stop onduidelijke retries van startopdrachten. Een replaymodus verwerkt eerder vastgelegde antwoorden zonder netwerkverbinding.

**Acceptatie:** een timeoutscenario is offline reproduceerbaar. De UI blijft reageren en een tweede startopdracht wordt niet verstuurd alleen omdat een antwoord verloren ging.

## I12 — Compacte operatorweergave bovenop dezelfde job

**Aanleiding:** de Mantis-familie suggereert verschillende clients voor voorbereiding en bediening. De exacte mogelijkheden zijn nog OPEN; dit is een eigen productrichting.

**Voorstel:** houd de desktopweergave uitgebreid en geef een operator een overzicht van machine, foto/preview, materiaal, aantal, bounds en actuele toestand. De kernacties verwijzen naar een opgeslagen jobrevisie. Bewerkingen van dezelfde job op een ander scherm mogen geen reeds voorbereide uitvoering veranderen.

Een netwerkclient praat met een lokale applicatieservice die de controllerverbinding beheert. De service bewaakt machine-identiteit, opdracht-ID's en gedeelde toestand. Een informatiescherm kan alleen status lezen; een bedieningsscherm krijgt expliciete acties die passen bij de huidige jobstatus.

**Acceptatie:** twee schermen zien dezelfde planhash. Een gewijzigd ontwerp verschijnt als nieuwe revisie. Alleen het bekijken van de status start geen job en reconnect herhaalt geen laatste bedieningsactie.

## Voorgestelde samenhang

```text
Ontwerp + materiaal + bewerkingen + machineprofiel
                         |
                 controle / registratie
                         |
                     JobPlan
                    /       \
                preview    RD-encoder
                               |
                        bestand + inspectie
                               |
                     productiewachtrij
                               |
                     controlleradapter
                               |
                 statusbewijs + jobhistorie
```

Dit voorstel sluit aan op de bestaande [herbouwarchitectuur](../lightburn/04-herbouwarchitectuur.md). De nieuwe nadruk is een herleidbare productiejobrevisie tussen editor en controller, met dezelfde betekenis in preview, opslag en operatorweergave.
