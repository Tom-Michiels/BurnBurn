# Code naar een Ruida controller sturen

## Kies de route

| Route | Invoer | Wat de computer doet | Wat je nodig hebt |
|---|---|---|---|
| LightBurn of RDWorks via Ethernet | Ontwerp of ondersteund jobbestand | Compileert en verstuurt de job | Passend profiel en netwerkconfiguratie |
| LightBurn of RDWorks via computer-USB | Ontwerp of ondersteund jobbestand | Compileert en gebruikt de deviceverbinding | PC-USB-aansluiting en passende driver |
| USB-stick | Machinebestand `.rd` | Exporteert het bestand | U-disk-poort en geschikt medium |
| Eigen software via UDP | Geldige RD-commandostroom | Verpakt chunks, bewaakt ACK en jobstatus | Encoder of bestaande RD-export en gevalideerde transportdriver |

Een `.lbrn2`, SVG of DXF is bronmateriaal voor de hostsoftware. Een bestand hernoemen naar `.rd` converteert het niet.

## Ethernet met LightBurn

1. Noteer model en firmware en maak een Ruida-deviceprofiel met het echte werkbereik en de correcte nulpunthoek.
2. Verbind de Ethernetpoort van de controller met het netwerk of een aparte computernetwerkinterface.
3. Kies een vrij statisch IPv4-adres in een passend subnet. Voorbeeld van een apart netwerk: computer `192.168.50.10/24`, controller `192.168.50.20`. Dit is een voorbeeld, geen fabrieksinstelling.
4. Stel het controlleradres in via het paneel. Bij veel modellen loopt dit via **Z/U** of **Menu**, vervolgens **IP config+**. De paneelprocedure verschilt per model.
5. Kies in LightBurn **Devices → Create Manually → Ruida → Ethernet/UDP** en vul het adres in. Sta op macOS 15 of hoger lokale netwerktoegang toe.
6. Controleer verbinding, plaatsing en jobparameters. Gebruik **Send** voor een benoemde job die je daarna op het paneel selecteert. Kies **Start** alleen wanneer de machine de job meteen mag uitvoeren.

De [officiële Ethernetprocedure](https://docs.lightburnsoftware.com/2.1/Guides/RuidaEthernet/) beschrijft adresconfiguratie en device-instelling. Een antwoord op ping bewijst alleen IP-bereikbaarheid; controleer ook of de besturingssoftware reageert.

**Send** slaat een benoemde job op. **Start** kan al beginnen zodra voldoende data is aangekomen. **Shift + Send** start na volledige overdracht. **Run RD File** voert een eerder geëxporteerd bestand uit; het is geen uploadknop met uitsluitend opslag als gevolg. Zie de [knopbeschrijvingen](https://docs.lightburnsoftware.com/2.1/Reference/LaserWindow/).

## USB vanaf de computer

Gebruik de **PC-USB**-aansluiting, niet de USB-hostpoort voor een stick. Installeer de bij model en besturingssysteem passende driver en configureer het device in LightBurn of RDWorks. Daarna blijft het onderscheid tussen verzenden en uitvoeren hetzelfde.

Ruida publiceert [USB-drivers](https://www.rdacs.com/en/download?type=driver), waaronder afzonderlijke modelvermeldingen. Ga voor een eigen driver niet uit van een universele COM-poort, baudrate, VID/PID of endpoint. Deze gegevens moeten voor de aangesloten hardware worden vastgesteld. UDP-checksumbytes horen niet automatisch bij USB-payloads.

## USB-stick zonder directe computerverbinding

1. Configureer de job voor de juiste machine en exporteer via **Save RD File** in LightBurn of de overeenkomstige machine-export in RDWorks.
2. Zet de `.rd` op een USB-stick die door jouw controller wordt ondersteund.
3. Steek de stick in de **U-disk**-poort en gebruik de paneelfunctie voor bestanden vanaf USB. Kopieer of selecteer de job volgens de modelhandleiding.
4. Controleer naam, plaatsing en parameters en start vanaf het paneel.

LightBurn beschrijft [RD-export voor zelfstandig gebruik](https://docs.lightburnsoftware.com/2.1/Reference/LaserWindow/); de RDC6445G ondersteunt volgens de [fabrikant](https://www.rdacs.com/en/product-detail/rdc6445g.html) U-disk-verwerking. Bestandssysteem, maximale capaciteit, naamlimieten en precieze menuvolgorde blijven modelspecifiek.

## RDWorks

Gebruik het pakket dat de fabrikant voor jouw model vermeldt. De [officiële softwarecatalogus](https://www.rdacs.com/en/download?type=software) bevat RDWorksV8 en andere families zoals RDCutist en RDVision. Voeg in de toepasselijke software de USB- of netwerkverbinding toe, stel de machineparameters in en verstuur de gecompileerde job met de modelspecifieke downloadfunctie. Controleer in de bijbehorende handleiding of een optie alleen opslaat of ook start.

## Eigen applicatie

Begin met een geldige export voor dezelfde machine. Onderzoek het bestand offline, bouw daarna transport en opslag en voeg pas vervolgens een eigen encoder toe. [Eigen software](03-eigen-software.md) beschrijft de noodzakelijke lagen. Een upload-ACK bewijst ontvangst van een pakket, niet dat de hele job correct is opgeslagen of uitgevoerd.

LightBurn heeft daarnaast een [tekstinterface voor automatisering via UDP](https://docs.lightburnsoftware.com/latest/Guides/AutomationWithUDP/), op 19840 met antwoorden op 19841. Deze interface bestuurt de desktopapp; de Ruida-controller gebruikt een ander binair protocol.
