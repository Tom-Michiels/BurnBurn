# LightBurn — featureonderzoek en herbouwspecificatie

Onderzoeksdatum: **17 oktober 2026**. Taal: Nederlands; oorspronkelijke UI-termen behouden voor herkenbaarheid.

Dit dossier combineert directe inspectie van **LightBurn Core 2.1.04 op macOS**, de officiële **2.1**-handleiding, drie daadwerkelijk door deze app gegenereerde Ruida-bestanden en publieke protocolimplementaties. Het doel is een functioneel vergelijkbare laserontwerp- en besturingstool te specificeren.

**Belangrijke grens:** dit is een brede functionele en technische researchbasis, geen bewezen volledige replica-specificatie van de gesloten LightBurn-code. Alle tien hoofdmenu’s en zichtbare submenu’s zijn geïnventariseerd. Niet iedere contextafhankelijke dialoog, controllerfamilie en randgeval is experimenteel getest. Zonder aangesloten controller zijn uitvoering, USB, timing en firmwarevarianten niet bevestigd. De ontbrekende bewijzen en experimenten staan expliciet in [dekking en vervolgvalidatie](validation/coverage.md).

## Leesroute

| Bestand | Inhoud |
|---|---|
| [01-menu-inventaris.md](01-menu-inventaris.md) | Alle waargenomen hoofdmenu’s en submenu’s; betekenis en implementatieverdeling |
| [02-features.md](02-features.md) | Editor, geometrie, tekst, beelden, CAM, bibliotheken, camera en productie |
| [03-instellingen.md](03-instellingen.md) | Algemene voorkeuren, Ruida-device, cut modes, voorwaarden en eenheden |
| [04-herbouwarchitectuur.md](04-herbouwarchitectuur.md) | Datamodel, modules, algoritmen, API-contracten en bouwvolgorde |
| [06-dialoogcatalogus.md](06-dialoogcatalogus.md) | Barcodevarianten, virtuele arrays, offsets en aanvullende dialoogopties |
| [05-acceptatieplan.md](05-acceptatieplan.md) | Functionele acceptatiecriteria en regressiecorpus |
| [ruida/01-pipeline.md](ruida/01-pipeline.md) | Van project naar Ruida-job; verantwoordelijkheden van host en DSP |
| [ruida/02-binaire-specificatie.md](ruida/02-binaire-specificatie.md) | Bytecodering, waarden, opcodes, headers, footer en onbekenden |
| [ruida/03-transport.md](ruida/03-transport.md) | UDP, ACK, checksum, toestandsmachine, USB en jobbeheer |
| [ruida/04-exportanalyse.md](ruida/04-exportanalyse.md) | Reproduceerbare analyse van echte Line-, Fill- en Grayscale-export |
| [ruida/command-registry.json](ruida/command-registry.json) | 200 publieke commandoconstanten en 8 geheugenadressen; geen supportgarantie |
| [reference/feature-index.md](reference/feature-index.md) | Brongebonden onderwerpen- en optie-index van 158 officiële referentiepagina’s |
| [reference/documentation-index.json](reference/documentation-index.json) | URLs, koppen, links en SHA-256 van geraadpleegde pagina’s |
| [reference/protocol-sources.json](reference/protocol-sources.json) | Publieke protocolbronnen, vastgepinde commits en hashes |
| [evidence/ui-observations.md](evidence/ui-observations.md) | Rechtstreekse appwaarnemingen en onderzoekscondities |
| [validation/coverage.md](validation/coverage.md) | Wat is bewezen, wat is afgeleid en wat nog ontbreekt |

## Bewijslabels

- **UI**: rechtstreeks gezien in deze geïnstalleerde app; bestaan van een optie bewijst de interne implementatie niet.
- **DOC**: beschreven in de officiële 2.1-documentatie; beschikbaarheid kan afhangen van machine, editie, platform en instellingen.
- **RD**: rechtstreeks vastgesteld in de meegeleverde LightBurn-exportbestanden.
- **OSS**: zichtbaar in een publieke, vastgepinde implementatie; geen uitspraak dat LightBurn dezelfde code gebruikt.
- **ONTWERP**: aanbevolen eigen implementatie voor functionele gelijkwaardigheid.
- **OPEN**: onvoldoende bewijs; concrete verificatie nodig.

## Zelf reproduceren

Alle hulpmiddelen werken offline, zonder pakketten te installeren of een controller aan te sturen:

```sh
python3 research/lightburn/tools/rd_inspect.py research/lightburn/fixtures/rectangle-default.rd
python3 research/lightburn/tools/rd_inspect.py research/lightburn/fixtures/rectangle-fill.rd --json
python3 -m unittest discover -s research/lightburn/validation -p 'test_*.py' -v
python3 research/lightburn/tools/validate_dossier.py
```

`fixtures/rectangle.svg` is de eigen testgeometrie. `rectangle-default.rd` en `rectangle-fill.rd` zijn echte app-exports; `gray-ramp-grayscale.rd` voegt een Grayscale-test toe; `rectangle-fill.lbrn2` is het door de app opgeslagen project met de feitelijke plaatsing en instellingen. De SVG is bij import door de app verplaatst: vergelijk dus afmetingen met de SVG en absolute coördinaten met het projectbestand.

De inspectietool is bewust een onderzoeksdecoder. Hij bewaart onbekende records en herkent bewegingen en enkele parameters; hij is geen productieklare Ruida-driver. Stuur de fixtures niet blind naar een machine: hun opgeslagen absolute plaatsing hoort bij het onderzochte profiel van 1500 × 900 mm.

Uitgevoerde verificatie: **14 offline tests geslaagd**, plus controle van lokale links, JSON-indexen, unieke IDs en fixturehashes. Resultaten: [validation/results.txt](validation/results.txt).

## Kernbevindingen

LightBurn verenigt drie grote producten: een 2D-vector/beeldeditor, een CAM-planner en een verzameling machinebackends. Voor Ruida genereert het een binaire, gescrambelde RD-commandostroom. Dat is geen G-code. Ontwerpobjecten, projectlagen en controlleronderdelen moeten afzonderlijke modellen krijgen.

De echte Fill-export bevat 150 graveerlijnen en 149 relatieve Y-verplaatsingen van −100 µm. De Line-export bevat vier absolute snijsegmenten. Speed en power staan zowel in onderdeelmetadata als in de uitvoeringssectie. Een vergelijkbare tool heeft dus meer nodig dan een geometrie-naar-bewegingsconverter: ook werkmodus, deelmetadata, bounds, referentiestelsel, eindrecords en transportstatus horen bij het contract.

Reproduceerbare gelijkwaardigheid moet in geometrie, materiaalresultaat en controllergedrag worden gemeten. Byte-identieke uitvoer voor alle projecten is niet aangetoond en vereist aanvullende gecontroleerde export- en hardwareproeven.
