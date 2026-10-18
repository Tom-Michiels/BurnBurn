# Eigen software voor Ruida

## Verdeel encoder en transport

De eigen backend heeft afzonderlijke onderdelen nodig voor geometrieplanning, RD-encoding, transport, bestandsbeheer en uitvoering. Een sender die bytes verstuurt vervangt geen encoder en bepaalt niet vanzelf of die bytes als bestand worden opgeslagen of direct uitgevoerd.

De [binaire specificatie](protocol/02-binaire-specificatie.md) beschrijft de onderzochte encoding; [transport](protocol/03-transport.md) beschrijft de publieke implementaties en herstelproblemen. De meegeleverde decoder is een onderzoeksinstrument en accepteert onbekende records; dat maakt hem geen validator voor iedere productiejob.

## Het UDP jobkanaal

In de onderzochte openbare drivers:

```text
computer UDP 40200  →  controller UDP 50200
datagram = checksum als uint16 big endian + gescrambelde payload
checksum = som(payloadbytes) modulo 65536
```

Een gangbare bovengrens is 1470 payloadbytes plus twee checksumbytes. Een `.rd` bevat al de gescrambelde payload; opnieuw scrambling toepassen beschadigt de job. De bestandssom binnen RD en de checksum buiten elk UDP-pakket zijn verschillende berekeningen.

Bij magic `0x88` is een ACK op de wire `C6`, logisch na unscrambling `CC`. ERR is respectievelijk `46` en `CD`; NAK `48` en `CF`. Interpreteer een antwoord één keer in de juiste bytewereld. Voor querycommando's kan naast ACK een inhoudelijk antwoord volgen.

Deze details komen uit de [vastgepinde MeerK40t-driver](https://github.com/meerk40t/meerk40t/blob/d511944e08788e88077727f682957759884ff5ca/meerk40t/ruida/udp_connection.py) en de bestaande [protocolanalyse](protocol/03-transport.md). De openbare implementatie noemt ook 50207/40207 voor een ander besturingskanaal; beschouw dat niet als alternatieve uploadpoort.

## Opslaan en uitvoeren zijn verschillende contracten

De oudere [protocolbeschrijving](https://github.com/jnweiger/ruida-laser/blob/a1e7b9b93b10d5cac79c875bc3efec46f7397a11/doc/protocol.md) noemt logisch `E8 02 E7 01` met een nulgetermineerde bestandsnaam als voorbereiding van een benoemde transfer. Behandel dit als modelgebonden observatie. Exacte naamcodering, limieten, overschrijven en timing moeten op de doelcontroller worden bevestigd.

Een los geëxporteerd RD-bestand naar UDP sturen is daarom geen bewezen universele implementatie van **Send**. De onderzochte bestanden bevatten bovendien `D8 00`, publiek benoemd als start-processfamilie. De betekenis hangt af van live- of bestandscontext. Voeg geen los startcommando toe op basis van alleen een constantenaam.

Een bruikbare eigen API houdt deze operaties apart:

```text
connect(profile) -> model, firmware, capabilities
readStatus() -> status met tijdstempel
uploadFile(name, rd) -> ontvangstbewijs en verificatiestatus
listFiles() -> bestanden op controller
runFile(id) -> startverzoek
pause(), resume(), stop()
disconnect()
```

## Uploadprocedure als pseudocode

```text
controleer modelprofiel en RD-job
open exclusieve sessie op hostpoort 40200
bevestig controllercommunicatie
blokkeer concurrerende statusqueries op dit kanaal
bereid modelspecifieke bestandsopslag voor
voor iedere chunk van het al gescrambelde bestand:
    bereken twee checksumbytes en verstuur één datagram
    wacht op geldig antwoord van de doelcontroller
    bij ACK: ga naar de volgende chunk
    bij fout of timeout: stop en markeer overdracht als onzeker
bevestig opslag via ondersteunde bestandsquery
rapporteer Uploaded of StoredVerified
voer alleen via een afzonderlijke runFile-operatie uit
```

Dit is een implementatieontwerp, geen hardwaregevalideerde driver. Serialiseer verzoeken, valideer antwoordbron en voorkom dat een laat statusantwoord als uploadbevestiging telt. Een verloren ACK kan betekenen dat data wel is verwerkt. Zonder gevalideerd herstelpad mag een sender zo'n chunk niet blind blijven herhalen.

## Pakketten offline bekijken

`tools/prepare_udp.py` verdeelt een bestand in chunks en schrijft de checksum, bestandsoffset en volledige datagramhex naar JSON. Het bouwt geen bestandsopslagenvelop en verstuurt niets. Gebruik het voor vergelijking met een capture van dezelfde job:

```sh
python3 research/rudia/tools/prepare_udp.py research/rudia/fixtures/rectangle-fill.rd --output /tmp/rudia-packets.json
```

De keuze om op vaste bytegrenzen te splitsen volgt oudere publieke implementaties. Andere drivers groeperen liever op commandogrenzen; de doelcontroller en lange records moeten de keuze bevestigen.

## Encoder bouwen

Begin met absolute travel en rechte snijsegmenten, één part en vaste parameters. Voeg headers, bounds, referentieframe, partmetadata, eindrecords en bestandssom toe voordat een job als compleet wordt beschouwd. Rasterwerk vereist scanplanning en bijbehorende werkmodus; Grayscale vereist modulatie. De [drie exportanalyses](protocol/04-exportanalyse.md) zijn een kleine referentieset.

Gebruik zevenbitsgroepen met grondtal **128**, geen 127. Relatieve coördinaten in de onderzochte subset zijn signed14 in micrometer; snelheid staat in micrometer per seconde. Houd powerafronding en de scrambling-magic configureerbaar. De varianten en onzekerheden staan naast de velden in de binaire specificatie.

## Publieke bouwstenen

| Project | Bruikbaar voor | Grens |
|---|---|---|
| [MeerK40t](https://github.com/meerk40t/meerk40t/tree/d511944e08788e88077727f682957759884ff5ca/meerk40t/ruida) | Encoder, transport, jobcontroller en emulator | Onderzochte broncode is geen certificatie voor jouw firmware |
| [jnweiger/ruida-laser](https://github.com/jnweiger/ruida-laser) | Eenvoudige encoder en protocolobservaties | Oudere interpretaties en beperkte modeldekking |
| [kkaempf/ruida](https://github.com/kkaempf/ruida) | Decoder en commandotabel | Controleer formules tegen fixtures en onafhankelijke code |
| [StevenIsaacs/ruida-pa](https://github.com/StevenIsaacs/ruida-pa) | Captureanalyse, RD-decode/generatie en een eigen driver-API | Snel veranderend project; README-claims over UDP/TCP/USB vergen inspectie van backend en doelmodel |

Een TCP-backend in een extern project bewijst geen native TCP-jobpoort op alle Ruida-controllers. Controleer of het gaat om een bridge of een specifiek model. Bewaar de toepasselijke licenties wanneer je broncode hergebruikt; zie [bronnotices](reference/THIRD-PARTY-NOTICES.md).
