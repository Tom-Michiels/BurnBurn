# Bronnen en onderzoeksmethode

Geraadpleegd op **17 oktober 2026**. Alle inhoudelijke conclusies gebruiken documentatie van makers, hun eigen repositories of door de maker geschreven handleidingen. Een mirror is expliciet gemarkeerd. Zoekresultaten van fora zijn gebruikt voor ontdekking, niet als bewijs voor functies of compatibiliteit.

## 1. Bronregister

| ID | Primaire bron / vindplaats | Gebruikt voor |
|---|---|---|
| R01 | [Ruida RDWorksV8-catalogus](https://www.rdacs.com/en/search?keyword=rdworksv8) | Actuele pakketvermelding 8.01.72.001 |
| R02 | [Thunder Laser RDWorks-handleidingpagina](https://www.thunderlaserusa.com/knowledge/rdworksv8-0-software-manual), [64-bladige PDF](https://maiyawuwbsbvimbjxytt.supabase.co/storage/v1/object/public/kb-assets/z/att-37d737c7917032fe87f2.pdf) | §§1.2, 2.13, 4.5–4.10: voorbereiding, preview, uitvoer, arrays en procesopties |
| R03 | [Ruida-downloadcentrum](https://www.rdacs.com/en/download) | Softwarefamilies en MantiSoft-pakket; geen universele compatibiliteitslijst |
| M01 | [MeerK40t README](https://github.com/meerk40t/meerk40t) | Directe Ruida versus emulator, uitbreidbaarheid, CLI |
| M02 | [MeerK40t Operations](https://github.com/meerk40t/meerk40t/wiki/Online-Help:-OPERATIONS) | Bewerkingsmodel, Raster versus Image |
| M03 | [MeerK40t Spooler](https://github.com/meerk40t/meerk40t/wiki/Online-Help-spooler) | Actieve jobs, historie, apparaatfilter en export |
| M04 | [MeerK40t Ruida device.py](https://github.com/meerk40t/meerk40t/blob/main/meerk40t/ruida/device.py) | Driver, spooler, sessie, pause/abort en timeoutgrens |
| V01 | [VisiCut-projectsite](https://visicut.org/) | Productrol, platformen, camera en Portable Laser Format |
| V02 | [VisiCut-handleiding](https://github.com/t-oster/VisiCut/wiki/VisiCut-manual) | Materiaal/mapping/profielen, editoroverdracht, positionering |
| V03 | [VisiCut Supported Hardware](https://github.com/t-oster/VisiCut/wiki/Supported-Hardware) | Concreet genoemde Ruida-machines, status “Well-Tested” |
| V04 | [LibLaserCut Ruida.java](https://github.com/t-oster/LibLaserCut/blob/master/src/main/java/de/thomas_oster/liblasercut/drivers/Ruida.java) | `connect`, `writeJobCode`, `getResolutions`; raster-naar-vectorroute |
| V05 | [VisiCut labinstellingen delen](https://github.com/t-oster/VisiCut/wiki/How-to-add-default-settings-for-your-lab) | `.vcsettings`/ZIP en vaste distributie-URL |
| Y01 | [Rayforge README](https://github.com/barebaric/rayforge) | Algemeen CAD/CAM-model en productbreedte |
| Y02 | [Rayforge firmwarecompatibiliteit](https://rayforge.org/docs/reference/firmware/) | Specifieke Ruida RPA-driver, interfaces, beperkingen, rotary ontbreekt |
| Y03 | [Rayforge changelog](https://github.com/barebaric/rayforge/blob/main/CHANGELOG.md) | Secties 1.12.0 en beta's; nieuwe driver, fixes en Command-beperking |
| Y04 | [Rayforge eerste configuratie](https://rayforge.org/docs/getting-started/first-time-setup/) | Adaptieve wizard en `.lbdev`-import |
| Y05 | [Rayforge apparatenpagina](https://rayforge.org/resources/devices/) | Expliciete experimentele status van Ruida |
| T01 | [MantiSoft-productsite](https://www.rdmantisoft.com/en) | Officiële productidentiteit en desktopplatformen |
| T02 | [MantiSoft-downloads](https://www.rdmantisoft.com/en/download) | Desktop/mobiel/web; handleidingversies en datum |
| T03 | [MantiSoft-support](https://www.rdmantisoft.com/en/support) | Bestaan van supportcategorieën; detailinhoud niet verkregen |
| C01 | [Ruida RDV6445G-productpagina](https://www.rdacs.com/en/product-detail/rdv6445g.html) | Speciale visionhardware, bijbehorende softwarefamilies |
| C02 | [RDVision Operation Manual, Ruida 2016, via mirror](https://www.ruidacontroller.com/wp-content/uploads/2021/10/Rdvision-Software-User-Manual-v1.0.pdf) | Hfdst. 4–5: calibratie, modellen, features, snijcontour en stitching |
| C03 | [Ruida eigen supporttutorials, Chinees](https://www.rdacs.com/support/faq) | RDVisionWorks templatebeheer, extractie en grootformaatwerkstroom |
| N01 | [Ruida RDNest-introductie, 28 april 2025](https://www.rdacs.com/en/news-detail/162.html) | Nestingmodel, platform en genoemde opties |
| P01 | [Ruida Protocol Analyzer README](https://github.com/StevenIsaacs/ruida-pa) | Analyse, driver, adapters en RPC |
| X01 | [LaserGRBL FAQ](https://lasergrbl.com/faq/) | GRBL-afbakening en firmwareafhankelijke opties |
| X02 | [jnweiger/ruida-laser](https://github.com/jnweiger/ruida-laser) | Aanvullende protocolreferentie |
| L | [Bestaande lokale LightBurn-studie](../lightburn/README.md) | Reeds onderzochte referentie, geen nieuwe appmeting |

De catalogus toont ook namen als RDWorksV8Lite, RDVnCut, MarkVision en RDCutist. Alleen aangetroffen worden in een catalogus is onvoldoende voor een featureprofiel. Daarom worden niet-onderzochte varianten niet als extra complete alternatieven meegeteld.

## 2. Onderzoeksstappen en bronkritiek

1. Bestaande lokale dossiers gelezen om overlap en ontbrekende productrichtingen te bepalen.
2. Gezocht op directe Ruida-ondersteuning en officiële Ruida-softwarefamilies.
3. Productclaims vergeleken met specifieke driverdocumentatie. Vooral bij Rayforge maakt dit verschil: brede rotaryclaims zijn niet toepasbaar op de huidige Ruida-route.
4. Gericht broncode gelezen bij MeerK40t en LibLaserCut om driverintegratie en uitvoerroutes te onderscheiden van alleen marketingtekst.
5. Functionele patronen vertaald naar eigen ontwerpvoorstellen, expliciet van bronclaims gescheiden.
6. Vervolgproeven gedefinieerd voor ontbrekend gedrag en compatibiliteit.

R02 is een oudere OEM-handleiding en bewijst de huidige interface van 8.01.72.001 niet. C02 is herkenbaar door Ruida geschreven, maar gehost op een mirror: de hostnaam wordt niet als bewijs van officiële fabrikantsstatus gebruikt. De actuele maker is via `rdacs.com` gecontroleerd. In C02 raakt de afgedrukte paginanummering later in de war; hoofdstukken en PDF-bladnummers zijn daarom betere locators.

De Chinese fabrikanttutorials zijn functioneel samengevat; vertaalde woorden als template, contour en vorm worden niet als exacte Engelse UI-labels gepresenteerd. Oude instructies en modelspecifieke functies zijn niet omgezet naar universele machineadviezen.

## 3. Versies en reproduceerbaarheid

Er is geen nieuwe applicatieversie geïnstalleerd. Het aangetroffen actuele catalogusnummer, de gebruikte handleiding en de geraadpleegde ontwikkelbranch zijn afzonderlijk benoemd. Versienummers uit release notes worden niet gelijkgesteld aan een lokaal geteste release. Ook bij een actuele pagina kan de inhoud ouder zijn.

De geraadpleegde main/master-links en wiki's zijn veranderlijk. Dit dossier bevat bronverwijzingen en eigen samenvattingen, geen volledige webarchieven of nieuw vastgepinde upstreamcode. Voor implementatie moeten de gekozen afhankelijkheden eerst op commit of release worden vastgezet, met licentiebestanden en hashes. De eerdere [protocolbronnen](../rudia/reference/protocol-sources.json) zijn wél afzonderlijk vastgepind; die bewijzen geen nieuwe features uit dit dossier.

## 4. Kwaliteitscontrole van het dossier

Lokale controle omvat: bestaan van alle relatieve bestandslinks, volledige verwijzingen vanuit de leesroute, twaalf unieke idee-ID's en twaalf proef-ID's. Dit is documentcontrole, geen software- of hardwaretest. Een aparte [validatienotitie](validation-results.txt) bevat de uitgevoerde controle.

Er zijn geen volledige softwarebronnen, proprietary assets of grote handleidingfragmenten overgenomen. Ideeën zijn zelfstandig uitgewerkt. Voor daadwerkelijk codehergebruik moeten de relevante licenties van de gekozen versie afzonderlijk worden beoordeeld.
