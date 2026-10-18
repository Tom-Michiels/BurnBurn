# Validatie, open vragen en bouwvolgorde

## 1. Voorstel voor uitvoering

Dit is een **ONTWERP**-roadmap, geen wijziging van de applicatie en geen reeds uitgevoerde hardwaretest. De afhankelijkheden zijn belangrijker dan het aantal functies.

| Fase | Te realiseren resultaat | Ideeën | Waarom nu? |
|---|---|---|---|
| P0 — correct plan | Eenheden, origins, operations, bounds, deterministische preview en RD-export | I01,I02,I04,I05,I10 | Iedere latere functie steunt hierop |
| P0 — betrouwbare overdracht | Eén transportadapter, duidelijke opslag/start-intentie, herstel en diagnose | I03,I11 | Verbinding is geen bewijs van uitvoering |
| P1 — dagelijks werk | Materiaalrecepten, beeldketen, historie en herhaalindeling | I06,I07,I09-array | Verbetert herhaalbaarheid en productiviteit |
| P2 — beter materiaalgebruik | Mixed-part nesting met orderaantallen en richtingregels | I09-nesting | Vereist correct materiaal- en contourmodel |
| P2 — registratie | Calibratie en handmatige markregistratie | I08 | Eerst millimeterfout meetbaar maken |
| P3 — geavanceerd | Automatische herkenning, feeder en operatorclient | I08,I12 | Hardware, toestand en foutafhandeling moeten bewezen zijn |

Geen kunstmatige deadline of kostenschatting: de repository bevat op het moment van dit onderzoek onderzoeksbestanden, geen beoordeelde applicatiecodebasis waaruit ontwikkeltijd kan worden afgeleid.

## 2. Vergelijkingsproef met dezelfde invoer

Gebruik per toepassing hetzelfde bronbestand en leg alle effectieve instellingen vast. Vergelijk fysieke betekenis, niet alleen screenshots of bestandsgrootte. Verschillende planners mogen andere maar correcte paden produceren.

| ID | Proefontwerp/situatie | Te meten | Relevant vergelijkingspunt |
|---|---|---|---|
| T01 | 40 × 30 mm rechthoek met twee gaten | Units, bounds, volgorde, sluiting | Alle vier hoofdapps |
| T02 | Eén contour met markering plus snijden | Objectreferenties en bewerkingsvolgorde | MeerK40t, VisiCut |
| T03 | DXF met losse segmenten, naad en duplicaat | Reparatietoleranties en behoud detail | RDWorks, eigen planner |
| T04 | Grijsverloop en vooraf geditherd beeld op twee maten | Pixels, scaninterval, overscan, powerruns | MeerK40t, RDWorks, Rayforge |
| T05 | Plaatsing in verschillende origins | Geen spiegeling/verschuiving; dezelfde bedoelde bounds | Alle apps met bewezen route |
| T06 | Upload onderbreken op 10%, 50%, na laatste pakket | Status, retry, controllerbestandsnaam | Transportadapter en hoofdapps |
| T07 | Start verstuurd, antwoord verloren | Geen automatische tweede start | Wachtrij en controllerstatus |
| T08 | Homing/jog met een ongeldige aspositie | Geen onverwachte beweging op andere as | Rayforge-geïnspireerde randvoorwaarde |
| T09 | Array van één onderdeel versus order van twee soorten | Aantallen, marge, spiegelen, rotatie | RDWorks-array en RDNest |
| T10 | Gedrukt patroon met gemeten scheefstand | Registratiefout op onafhankelijke punten | RDVision en eigen registratiewerkstroom |
| T11 | Dezelfde job op twee computers | Profielrevisie, planhash, bronnen | VisiCut-geïnspireerd werkplaatspakket |
| T12 | Library/profiel wijzigt na jobvoorbereiding | Oude uitvoer blijft reproduceerbaar | Eigen jobmodel |

Begin offline met import, planning en decodevergelijking. Bij hardwarevalidatie eerst identiteit, bedmaat en coördinaten vastleggen; daarna beweging zonder laser beoordelen voordat materiaalbewerking wordt vergeleken. De bestaande fixtures zijn analysebestanden voor een bepaald profiel en worden niet universeel hergebruikt als machineproef.

### Vast te leggen per proef

- Appversie, OS, controllerlabel en firmware; transport en relevante instellingen.
- Hash van invoer, project, profiel en geëxporteerd RD-bestand.
- Effectieve eenheden, origin, laagnamen en procesinstellingen; eventuele defaults van controllerpaneel.
- Preview-uitkomst en gedecodeerde paden; tijden als schatting of meting gelabeld.
- Waargenomen uitkomst, fouten en ruwe antwoorden; geen “geslaagd” als alleen upload is waargenomen.

Voor rasterkwaliteit zijn onder meer lijnregelmaat, banding, detailbehoud en gelijkmatigheid relevant. Het aantal verstuurde bytes alleen is onvoldoende. Voor nesting horen complete orders en bruikbaar restmateriaal naast benuttingspercentage te staan.

## 3. Open vragen per product

| Product | Wat ontbreekt? | Wat sluit de vraag? |
|---|---|---|
| RDWorks V8 | Verschillen actuele release tegenover oudere handleiding; OEM-varianten | Appinspectie van exact pakket met T01–T05 |
| MeerK40t | Directe Ruida-dekking per interface, firmware en speciale operation | Driverinspectie op vastgepinde commit plus T01,T04,T06–T08 |
| VisiCut | Gedrag rasteromzetting, serial/USB-route op doelmachine | Exportinspectie en fysieke scanproef |
| Rayforge | Huidige driverdekking en experimentele grenzen op eigen model | Geïnstalleerde release + T01,T04,T06,T08; rotary niet aannemen |
| MantiSoft | Ondersteunde standaardcontrollers, licentie, transport en clientfuncties | Werkende supportmatrix/handleiding en concrete modeltest |
| Visionfamilie | Welke controller, camera, driver en software samen horen | Geverifieerde hardware-BOM plus T10 |
| RDNest | Exacte formaten, parameters, exportoverdracht en rekenprestatie | Invoer/uitvoerproef met bekende order |
| RPA | API-stabiliteit en gedrag bij onbekende/partiële replies | Commit vastpinnen, adaptertest en replaycorpus |

## 4. Wat deze studie wel en niet afsluit

De studie geeft een navolgbare selectie, productprofielen, vergelijkingsmatrix en twaalf concrete ontwerpvoorstellen. Voor ieder hoofdonderwerp zijn primaire vindplaatsen opgenomen. Niet ieder pakket heeft evenveel publiek toegankelijke documentatie; vooral MantiSoft en familiespecifieke visioncompatibiliteit blijven open.

Geen onafhankelijke uitspraken over betrouwbaarheid, snelheid, nauwkeurigheid, supportkwaliteit of gemak zijn door praktijktests bewezen. Woorden als “interessant” en prioriteiten zijn eigen afwegingen. De actuele webdocumentatie kan veranderen en is niet volledig lokaal gearchiveerd. Codeverwijzingen naar main/master zijn in deze studie niet op commit vastgepind.
