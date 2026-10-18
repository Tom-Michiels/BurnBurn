# Hoe LightBurn Ruida-uitvoer genereert

## Wat we kunnen aantonen

**RD:** LightBurn Core 2.1.04 heeft hier drie binaire `.rd`-bestanden geëxporteerd zonder aangesloten controller. Na inverse scrambling leveren ze een herkenbare Ruida-commandostroom op. De Line-uitvoer bevat één laser-uit-positionering en vier snijsegmenten. Fill bevat een rasterwerkmodus, 150 aan-lijnen en 149 Y-stappen.

Daaruit volgt aantoonbaar dat ten minste deze offline uitvoer geheel aan de hostzijde beschikbaar is. Het bewijst niet hoe LightBurn intern geprogrammeerd is, noch dat iedere Start-actie exact dezelfde envelop gebruikt als Save RD file.

Een Ruida-lasercontroller is hier een DSP-motioncontroller voor een laser, geen generieke CNC die willekeurige G-code ontvangt. G0/G1/M3-generatie vormt geen Ruida-backend.

## Functionele compilerketen

```text
Project + selectie + laag/sublaaginstellingen + deviceprofiel
→ tekst/generators oplossen
→ transformaties en uitvoerfilters
→ bewerkingsgeometrie / scanruns
→ volgorde, startpunten en referentieframe
→ Ruida-onderdelen met bounds en parameters
→ commandorecords (ongescrambeld)
→ file-som + einde
→ byte-scrambling
→ .rd bestand OF transportverpakking
```

De middenfasen zijn **ONTWERP/afleiding** uit de benodigde functionaliteit en de geobserveerde records. De fasevolgorde en datastructuren van LightBurn zijn gesloten.

## Editorlaag is geen Ruida-part

Een projectlaag kan meerdere sublagen, passes en objectgroeperingen bevatten. De planner kan daarom meerdere uitvoeronderdelen maken. Ieder onderdeel krijgt concrete speed/power/mode en bounds. Een encoder mag niet veronderstellen dat C00 altijd part 0 is of dat alle 30 projectlagen één-op-één in het controllerformaat moeten worden gegoten.

In de drie fixtures is er één part met index 0. De globale metadata bevat part-speed, part-powers en partbounds. De body herhaalt werkparameters vóór bewegingen. Dat ondersteunt een zelfstandige job die het controllerpaneel kan begrijpen; exacte betekenis van ieder metadatarecord is niet volledig bewezen.

## Host versus controller

| Taak | Verantwoordelijkheid bij vergelijkbare implementatie | Bewijs |
|---|---|---|
| fonts, boolean, kerf, afbeeldingfilters | host editor/compiler | nodig voor uitvoer; geen dergelijke primitieve records in de fixtures |
| vectorfill naar rijen | host maakt scanlijnen | RD: 150 lijnen en 0,1 mm stappen |
| kies cut/travel | host encodeert verschillende opcodes | RD: 88/A8 en 8B |
| native rasterwerkmodus | host kiest mode, controller voert passend uit | RD: verschil CA 01 00 versus CA 01 01 |
| rasteroverscan en acceleratie | DSP bij native asgerichte scan | DOC; niet hardwaregetest |
| motorpulsen, accel/decel en I/O | controller | geen step-pulsestream in RD |
| laser min/max opgeven | host in header en body | RD |
| min/max ramp naar feitelijke snelheid | DSP-gedrag; modelafhankelijk | DOC; geen exacte rampformule afgeleid |
| bestand uploaden en ACK afhandelen | transportmanager | OSS; geen livecapture in dit onderzoek |
| materiaalresultaat | combinatie optiek, materiaal, bron en controller | buiten een bestandsanalyse |

## Bewegingen en compactheid

Absolute XY gebruikt twee vijfbytewaarden, dus één beweging is elf bytes. Signed14-relative XY gebruikt twee tweebytewaarden, dus vijf bytes; éénassig relative gebruikt drie bytes. Relatieve waarden passen van −8192 tot +8191 µm. Gebruik de grenzen nauwkeurig: +8192 past niet.

In de Line-fixture zijn alle segmenten langer dan de relatieve limiet en gebruikt de app A8. In de Fill-fixture zijn de graveerlijnen 30 mm en dus A8, terwijl de 0,1-mm rijstap in 8B past. Dit verklaart de geobserveerde keuze zonder kennis van de gehele optimalisatieheuristiek. Of de app onder alle omstandigheden altijd de kortste encoding kiest is OPEN.

Gebruik voor een eigen writer de eerste XY-positionering absoluut en reset de relatieve accumulator bij een expliciete absolute move. Reken relatieve verschillen tussen reeds gequantiseerde absolute doelpunten; afzonderlijk afgeronde delta’s kunnen anders accumulatief afwijken.

## Native raster versus geëmuleerde hatch

Een gewone vectorcut en een native scan kunnen dezelfde A8-geometrie gebruiken, maar een ander mode-record hebben. Daarom is een decoder die uitsluitend bewegingen reconstrueert onvoldoende om uitvoeringsgedrag te vergelijken.

Voor veelvouden van 90° documenteert LightBurn native DSP-scanning. Andere scanhoeken worden als normale cutbewegingen geëmuleerd; neem dan geen automatische DSP-rasteroverscan aan. Een herbouw kan deze capability scheiden als `nativeRasterAxes={X,Y}` en overige hatch als vectorbewerkingen uitvoeren.

De aanvullende Grayscale-fixture bevestigt C7/C2-modulatieparen met AA-runs, naast ingestelde basispowers. De precieze formule, kanaalbetekenis, ramp, PWM override en dual-channel-aansturing moeten verder worden onderzocht. Zie de aanvullende exportanalyse.

## Preview als controle

In de app gaf Fill 4500 mm snijlengte; de decoder vindt 150 × 30 mm. Dat is een onafhankelijke controle van geometrie en interval. De preview gaf circa 1442 mm rapid moves en 1:04 totaal. Die rapidafstand is niet af te leiden als alleen de 149 rijstappen uit de RD-body worden geteld: approach-/return-/DSP-simulatiebewegingen en hun referentiecontext spelen mee. Claim dus geen volledige timing-equivalentie op basis van de vijf motion-opcodes alleen.

## Interface met opslag en uitvoering

Een `.rd` is een job-artifact. Lokale project-XML bevat bewerkbare vormen en is iets anders. Een Ethernet-datagram is weer een transportverpakking rond gescrambelde payload, met eigen checksum. Een opgeslagen bestand met een geldig file-sum is nog geen bevestigde upload. Een bevestigde upload is nog geen gestarte job.

Voor de herbouw: compileer één immutable artifact, valideer het offline, toon die versie in preview, upload met een eigen job-ID, en laat de jobcontroller expliciet kiezen tussen bewaren en uitvoeren.

Bronnen: de meegeleverde RD-fixtures en project-XML; [officiële Ruida-inrichting](https://docs.lightburnsoftware.com/2.1/Guides/ConfigureRuidaController/); de commitgebonden publieke encoder/driverbronnen in `reference/protocol-sources.json`.
