# Ruida controller en modellen

## Wat de controller doet

Een Ruida-lasercontroller voert geplande bewegingen en lasercommando's uit, bewaart machinejobs en biedt een bedieningspaneel. De hostsoftware verzorgt ontwerp, rasterbewerking en jobgeneratie. Het RD-formaat bevat bewegingen, snelheid, vermogen, werkmodi en metadata; zie [de compilerketen](protocol/01-pipeline.md).

De [officiële RDC6445G-productpagina](https://www.rdacs.com/en/product-detail/rdc6445g.html) noemt vier assen, twee laserkanalen, Ethernet en USB, optionele wifi, RDWorksV8, zelfstandige werking en U-disk-verwerking. Zij beschrijft ook rotary, feeding, herstart na stroomuitval en firmware-upgrades. Dit zijn kenmerken van deze productfamilie; gebruik de handleiding van de concrete uitvoering voor aansluitingen en instellingen.

## Identificeer het volledige model

Noteer het label op de **hoofdprint**, de paneelversie en firmwareversie. Het schermmodel alleen identificeert niet noodzakelijk de hoofdcontroller. Leg ook werkbereik, nulpunthoek, assen, aantal lasers en gebruikte software vast. Een achtervoegsel zoals G, S, GZ, GT5 of een visionvariant kan relevant zijn.

| Familie | Betekenis voor dit onderzoek |
|---|---|
| RDC6442 en RDC6445 | Eerste doelgroep voor vergelijking met de bestaande RD-fixtures en publieke drivers |
| RDC6332, RDC6432 en oudere RDLC-modellen | Controleer de specifieke handleiding en softwareondersteuning; neem registeradressen niet automatisch over |
| RDC6445GZ, GT5, GT7 en MOD5 | Afzonderlijke handleidingen staan in de officiële 6445-zoekresultaten |
| RDC6563 en andere fiber/precision/visionfamilies | Eigen softwarepakketten en varianten; merknaam alleen is geen RDWorks-compatibiliteitsbewijs |
| RDC8445S en nieuwere generaties | Fabrikantdownloadcatalogus bevat aparte vermeldingen; toets ondersteuning expliciet aan actuele software |

De [downloadcatalogus van Ruida](https://www.rdacs.com/en/download?type=driver) maakt onderscheid tussen CO2, vision, fiber, precision, marking en andere series. De [LightBurn-typenlijst](https://docs.lightburnsoftware.com/2.2/Reference/DeviceTypes/) noemt Ruida als categorie met USB, netwerk en Bridge, maar is geen volledige matrix van alle firmwarevarianten.

## Drie soorten code

1. **Machinejob:** RD-instructies voor snijden of graveren. Dit is het normale uploadpad.
2. **Directe machinebesturing:** statusqueries, bestandsbeheer, jog, start, pauze en stop. Dit gebruikt protocolcommando's met andere gevolgen dan een opgeslagen job.
3. **Firmware:** programma van hoofdprint of paneel. De fabrikant noemt upgradeondersteuning, maar dat levert geen algemeen SDK-contract voor eigen firmware op. Een RD-job uploaden en firmware vervangen zijn verschillende procedures.

Eigen applicatielogica draait normaal op de computer of een bridge. De onderzochte bronnen onderbouwen geen mogelijkheid om een willekeurig Python-, C- of Arduino-programma als applicatie op deze DSP te installeren.

## Instellingen die uitvoer beïnvloeden

Werkbereik, machine origin, user origin, spiegeling, rotary, stappenmaat, snelheid, acceleratie, min/max power en lasertype bepalen samen de fysieke uitvoering. Ontwerpcoördinaten zijn daarom niet vanzelf machinecoördinaten. De [exportanalyse](protocol/04-exportanalyse.md) laat dit concreet zien.

Bewaar de bestaande machine-instellingen voordat je ze voor integratie wijzigt. Gebruik geen instellingenbestand van een ander model als kalibratiebron. Elektrische pinouts, signaalniveaus en interlocks horen bij de modelspecifieke hardwarehandleiding.
