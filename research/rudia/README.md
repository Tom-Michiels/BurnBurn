# Ruida controller en code versturen

Onderzoeksdatum: **17 oktober 2026**. De gevraagde map heet `rudia`; de fabrikant en zoekterm zijn **Ruida / RuiDa**. Op basis van het bestaande laseronderzoek gaat dit dossier over Ruida-lasercontrollers, vooral de RDC6442/6445-familie. Het exacte model van jouw machine is nog onbekend.

Ruida ontvangt een binaire **RD-commandostroom**. Een ontwerp wordt op de computer naar een machinejob gecompileerd; G-code, Python, SVG of een LightBurn-project kun je niet rechtstreeks als uitvoerbare code uploaden. Voor gebruik via bestaande software zijn Ethernet, computer-USB en USB-stick de relevante routes. Voor een eigen applicatie is de openbare Ethernet/UDP-implementatie een concreet vertrekpunt.

## Leesroute

| Bestand | Inhoud |
|---|---|
| [01-controller-en-modellen.md](01-controller-en-modellen.md) | Hardware, modellen, functies, identificatie en firmware |
| [02-code-versturen.md](02-code-versturen.md) | Concrete procedures voor LightBurn, RDWorks, USB en eigen software |
| [03-eigen-software.md](03-eigen-software.md) | Encoder, upload, bestandsopslag, status, herstel en pseudocode |
| [04-bronnen.md](04-bronnen.md) | Fabrikantdownloads, handleidingen, software en openbare implementaties |
| [05-validatie-en-open-vragen.md](05-validatie-en-open-vragen.md) | Bevestigde kennis, modelafhankelijkheden en hardwareproeven |
| [protocol/01-pipeline.md](protocol/01-pipeline.md) | Compilerketen van ontwerp naar RD-job |
| [protocol/02-binaire-specificatie.md](protocol/02-binaire-specificatie.md) | Scrambling, zevenbitsvelden, opcodes en bestandssom |
| [protocol/03-transport.md](protocol/03-transport.md) | UDP-poorten, checksums, ACK, retries en USB-grenzen |
| [protocol/04-exportanalyse.md](protocol/04-exportanalyse.md) | Drie echte RD-exports met Line, Fill en Grayscale |
| [protocol/command-registry.md](protocol/command-registry.md) | 200 publieke commandoconstanten; ook als JSON aanwezig |

De technische hoofdstukken, decoder en fixtures zijn overgenomen uit het bestaande [LightBurn-dossier](../lightburn/README.md), zodat dit onderzoek zelfstandig leesbaar en offline bruikbaar is. De nieuwe hoofdstukken voegen fabrikantbronnen, verzendprocedures en implementatiekeuzes toe. `RD`, `DOC`, `OSS`, `ONTWERP` en `OPEN` betekenen respectievelijk exportbewijs, officiële documentatie, openbare implementatie, eigen ontwerp en nog te verifiëren.

## Direct bruikbaar

Voor handmatig werken: configureer een passend Ruida-profiel, verbind via Ethernet of USB en gebruik **Send** om een benoemde job op te slaan. **Start** en **Run RD File** zijn uitvoeracties. LightBurn beschrijft deze verschillen in de [Laser Window-handleiding](https://docs.lightburnsoftware.com/2.1/Reference/LaserWindow/). De [huidige typenlijst](https://docs.lightburnsoftware.com/2.2/Reference/DeviceTypes/) noemt **Pro** voor Ruida; de naam Core in de historische exportwaarnemingen is geen licentieadvies.

Voor eigen code: lees eerst [het implementatiehoofdstuk](03-eigen-software.md). Het UDP-jobkanaal gebruikt in de onderzochte implementaties controllerpoort **50200** en hostpoort **40200**. Een `.rd` is al gescrambeld: voeg bij transport een checksum toe, scramble het bestand niet opnieuw.

Offline analyseren en netwerkpakketten voorbereiden:

```sh
python3 research/rudia/tools/rd_inspect.py research/rudia/fixtures/rectangle-default.rd
python3 research/rudia/tools/rd_inspect.py research/rudia/fixtures/rectangle-fill.rd --json
python3 research/rudia/tools/prepare_udp.py research/rudia/fixtures/rectangle-fill.rd --output /tmp/rudia-packets.json
```

`prepare_udp.py` schrijft uitsluitend pakketdata naar JSON en opent geen netwerkverbinding. De fixtures bevatten absolute coördinaten voor een profiel van 1500 × 900 mm en zijn analysemateriaal, geen universele proefjobs.

Dit is een brede onderzoeksbasis, geen volledige officiële protocolspecificatie. Er is geen controller aangesloten of aangestuurd. Met modelnummer, firmwareversie en interface kan de resterende compatibiliteit gericht worden vastgesteld.
