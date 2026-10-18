# Bronnen en downloads voor Ruida

Geraadpleegd op **17 oktober 2026**. Officiële productinformatie beschrijft functies en bediening; openbare code biedt aanvullende protocolkennis. Alleen de bestaande implementatiereferenties in `reference/protocol-sources.json` zijn op commit vastgepind. De andere webpagina's kunnen wijzigen.

## Fabrikant

| Bron | Gebruik |
|---|---|
| [Ruida RDC6445G productpagina](https://www.rdacs.com/en/product-detail/rdc6445g.html) | Assen, laserkanalen, interfaces, offlinewerking en software |
| [Officiële handleidingenzoeker voor 6445](https://www.rdacs.com/en/search?idx=2&keyword=6445) | G/S V1.6, GZ, GT5, GT7 en MOD5 handleidingen |
| [Officiële drivers](https://www.rdacs.com/en/download?type=driver) | USB-drivers en catalogus per controllerfamilie |
| [Officiële software](https://www.rdacs.com/en/download?type=software) | RDWorksV8 en familiespecifieke programma's |

De officiële catalogus vermeldde **RDWorksV8 8.01.72.001**, de **RDWorks V8 software user manual**, **RDC6445G (S) Control System V1.6** en **RDC644XG(S) Control System V1.3**. Dit zijn vindplaatsen en catalogusvermeldingen, geen verklaring dat het meest recente pakket voor iedere machine geschikt is. De dynamische downloadknoppen leverden in de geraadpleegde tekstweergave geen bruikbare directe bestandslinks op; de PDF-inhoud van deze fabrikantvermeldingen is hier niet integraal onderzocht of lokaal gearchiveerd.

Gebruik `rdacs.com` als eerste fabrikantbron. Het eveneens gevonden `ruidacontroller.com` bevat downloads en mirrors, maar de naam alleen bewijst geen officiële fabrikantsstatus.

## LightBurn documentatie

| Bron | Onderwerp |
|---|---|
| [Ethernet instellen](https://docs.lightburnsoftware.com/2.1/Guides/RuidaEthernet/) | Statisch IPv4, paneelinstelling, device toevoegen en macOS-toegang |
| [Laser Window](https://docs.lightburnsoftware.com/2.1/Reference/LaserWindow/) | Send, Start, Save RD File en Run RD File |
| [Device Types 2.2](https://docs.lightburnsoftware.com/2.2/Reference/DeviceTypes/) | Ruida-verbindingen en Pro-licentie |
| [LightBurn Bridge](https://docs.lightburnsoftware.com/2.1/Reference/LightBurnBridge/) | Bridge als netwerkroute voor Ruida |
| [Appautomatisering via UDP](https://docs.lightburnsoftware.com/latest/Guides/AutomationWithUDP/) | Tekstcommando's naar de app, los van het controllerprotocol |

De nieuwe gebruikersprocedures verwijzen hoofdzakelijk naar versie 2.1, passend bij de bestaande exports. De geraadpleegde 2.2-typenlijst wordt alleen gebruikt voor de actuele verbindingen- en licentievermelding.

## Protocol en implementaties

| Bron | Inhoud |
|---|---|
| [MeerK40t RD-jobencoder](https://github.com/meerk40t/meerk40t/blob/d511944e08788e88077727f682957759884ff5ca/meerk40t/ruida/rdjob.py) | Scrambling, waarden en commandoconstanten |
| [MeerK40t UDP](https://github.com/meerk40t/meerk40t/blob/d511944e08788e88077727f682957759884ff5ca/meerk40t/ruida/udp_connection.py) | Poorten, checksum en ACK/reply-toestanden |
| [MeerK40t controller](https://github.com/meerk40t/meerk40t/blob/d511944e08788e88077727f682957759884ff5ca/meerk40t/ruida/controller.py) | Spooling en synchronisatie |
| [Oudere protocolnotities](https://github.com/jnweiger/ruida-laser/blob/a1e7b9b93b10d5cac79c875bc3efec46f7397a11/doc/protocol.md) | RD over UDP, ACK en bestandsnaamvoorbereiding |
| [Referentie-encoder](https://github.com/jnweiger/ruida-laser/blob/a1e7b9b93b10d5cac79c875bc3efec46f7397a11/src/ruida.py) | Opbouw van binaire jobs |
| [Ruby decoder](https://github.com/kkaempf/ruida) | RD-decodegereedschap |
| [Commandotabel](https://github.com/kkaempf/ruida/blob/1503ba9cf6fd3d73a8a2b4eb64cd06f331e6a49b/doc/commands.md) | Reverse-engineered opcodes; geen officiële garantie |
| [Ruida Protocol Analyzer](https://github.com/StevenIsaacs/ruida-pa) | Netwerkcapturen analyseren, RD en scripts, driverintegratie |

De commandotabel bevat een verdachte formule met 127 als grondtal. De meegeleverde specificatie en codec gebruiken zevenbitsgroepen met grondtal 128. Neem losse tabellen dus niet zonder vergelijking over.

## Lokale bewijzen

- [Bronmanifest met commits en SHA-256](reference/protocol-sources.json).
- [Commandoregistry](protocol/command-registry.json).
- [Exportanalyse](protocol/04-exportanalyse.md) en [fixturemanifest](evidence/fixtures-manifest.json).
- [Oorspronkelijk LightBurn-onderzoek](../lightburn/README.md).
- [Licenties en bronvermelding](reference/THIRD-PARTY-NOTICES.md).

De upstream bronbestanden zijn gelinkt, niet als volledige softwarebibliotheken meegeleverd. De opgenomen decoder en de packetiser werken offline.
