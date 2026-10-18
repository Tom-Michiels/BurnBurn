# Experimentele analyse van echte LightBurn-export

**RD-bewijs**, gegenereerd met LightBurn Core 2.1.04. Bestanden, hashes, JSON-decodes en testresultaten zijn onderdeel van dit dossier. De regels hieronder zijn werkelijk geobserveerde bytes, geen fictieve voorbeeldjob.

## Condities en bronbestanden

`fixtures/rectangle.svg`: 30 × 15 mm rechthoek. Na native import stond het object elders op het bed. Het opgeslagen Fill-project heeft `MirrorX="True"`, `MirrorY="True"`, een pad met lokale negatieve X-coördinaten en een eigen XForm. Die combinatie verklaart waarom SVG-coördinaten niet rechtstreeks als RD-coördinaten mogen worden gelezen.

De effectieve Line-hoekpunten liggen tussen **(771,687; 448,024)** en **(801,687; 463,024)** mm in de decode. Alle maten zijn positief en liggen binnen het onderzochte 1500 × 900-profiel. De posities zijn dus niet voor ieder machinebed bruikbaar.

Laag C00, speed 100 mm/s, min/max 20%, air aan, één pass. Voor Fill: 0,1 mm interval, hoek 0°, bidirectioneel, geen cross-hatch/flood/ramp. Het verschil Line/Fill is in de app ingesteld; de bestanden zijn niet achteraf door het onderzoeksscript aangepast.

## Vergelijking

| Maatstaf | Line | Fill |
|---|---:|---:|
| Bestandsgrootte | 525 bytes | 2572 bytes |
| Gesplitste commandorecords | 73 | 367 |
| Absolute travel 88 | 1 | 1 |
| Absolute cut A8 | 4 | 150 |
| Relatieve Y-travel 8B | 0 | 149 |
| Lengte één cutrun | 15/30 mm | 30 mm |
| Totale cutafstand | 90 mm | 4500 mm |
| Rijstap | n.v.t. | −100 µm |
| Eerste bodywerkmodus | CA 01 00 | CA 01 01 |
| Bestandssom | 24999 | 142815 |
| Laatste byte raw | D7 | D7 |

Commandorecords zijn geteld met de hoge-bit-splitter. Dat is voor deze fixtures consistent; het is geen garantie voor ieder toekomstig recordtype.

## Line-body met offsets

Offsets zijn raw/encoded gelijk, omdat scrambling één byte naar één byte mapt.

```text
018d  C9 02 00 00 06 0d 20       speed = 100000 µm/s
01aa  C6 01 19 4d                min power raw14 = 3277
01ae  C6 02 19 4d                max power raw14 = 3277
01ba  CA 03 01                   laser mask/select = 01
01bd  88 ...                     travel → (801.687,448.024)
01c8  A8 ...                     cut    → (801.687,463.024)
01d3  A8 ...                     cut    → (771.687,463.024)
01de  A8 ...                     cut    → (771.687,448.024)
01e9  A8 ...                     cut    → (801.687,448.024)
01f4  EB                         array end
01f5  E7 00                      block end
01f7  DA 01 06 20 ...            twee waarden 90
0205  E5 05 00 00 01 43 27       file-som 24999
020c  D7                         EOF
```

30 + 15 + 30 + 15 = 90 mm. De startpositie ligt rechtsboven in dit gedecodeerde kader; dat mag niet worden gegeneraliseerd naar alle startpuntpolicies.

## Fill-body en rijcentra

```text
017e  CA 01 01                   native scanmodus in dit corpus
018d  C9 02 00 00 06 0d 20       100 mm/s
01b5  88 ...                     travel → (801.687,462.974)
01c0  A8 ...                     cut    → (771.687,462.974)
01cb  8B 7f 1c                   dY = −100 µm
01ce  A8 ...                     cut    → (801.687,462.874)
01d9  8B 7f 1c                   dY = −100 µm
01dc  A8 ...                     cut    → (771.687,462.774)
... herhaal alternerend ...
09e6  A8 ...                     laatste cut naar (801.687,448.074)
09f1  E7 00                      block end
09f3  EB                         array end
09f4  E7 00                      block end
09f6  DA 01 06 20 ...            twee waarden 4500
0a04  E5 05 ...                  file-som 142815
0a0b  D7                         EOF
```

De eerste/laatste rij liggen 0,05 mm binnen de originele Y-randen: een halve interval. Dat is in deze rechthoek consistent met pixel-/scanrijcentra. Algemeen roundinggedrag bij niet-integrale rijhoogte, maskers, ronde contouren en afwijkende scanhoek is nog te meten.

De export bevat geen afzonderlijke overscanuitstap vóór en na iedere A8-run. De mode plus DSP-werking kan fysieke extra beweging veroorzaken. Een emulator die alleen deze punten tekent kan daarom geometrisch correct zijn en toch een onjuiste fysieke acceleratiesimulatie geven.

## Metadata en afronding

Document- en partbounds zijn aanwezig in E7 50/51 en E7 52/53/61/62. In de Line-export zijn sommige headercoördinaten anders afgerond dan de daadwerkelijke motionpunten: bijvoorbeeld X-min 771690 µm versus motion X-min 771687 µm. Het dossier gaat daarom niet uit van byte-identieke bounds en motionextrema. Meet het afrondingsrooster en het toepassingsdoel met aanvullende fixtures.

Ondanks uitgeschakelde Laser 2 Controls worden tweede-kanaalpowers geschreven. Tegelijk staat de mask/selectwaarde op 01. Aanwezigheid van kanaal-2-powerrecords bewijst dus niet dat twee lasers fysiek aan staan.

Het huidige corpus identificeert één onbekend F2 01-record. De decoder toont het expliciet als UNKNOWN. Andere records dragen een OSS-naam zonder dat hun volledige LightBurn-semantiek hier is vastgesteld.

## Wat de twee exports wel en niet bewijzen

Bevestigd: scrambling bij magic 0x88; positieve absolute XY in µm; speed100 in µm/s; powerraw3277 voor UI20%; absolute cutruns; signed14 Y-stappen; verschil scan-/cutwerkmodus; partmetadata; file-somformule voor deze twee bestanden; EOF; cutlengtecontrole tegen apppreview.

Niet bevestigd: live UDP-envelope van LightBurn; USB; Start versus Send-headers; on-device bestandsnaam/uploadhandshake; andere referentieframes; scanhoeken; volledige grayscaleformule/ramp; PWM/tijdschalen bij niet-nulwaarden; dual-laser; alle partlimieten; volledige LBRN2; alle Ruida-firmwarevarianten.

Voor herbouw is dit een bruikbaar golden corpus voor een eerste Line/Fill-encoder. Voor volledige productieondersteuning moet het [oorspronkelijke acceptatieplan](../../lightburn/05-acceptatieplan.md) worden uitgevoerd.

## Aanvullend experiment: echte Grayscale-export

Na het Line/Fill-onderzoek is ook `gray-ramp.png` geïmporteerd en als **Grayscale** geëxporteerd. Dit is een eigen 16×4-pixeltoontrap. De app importeerde hem op circa **5,643739 × 1,410935 mm**. Cut Settings werd op 100 mm/s, min 1%, max 60%, interval 0,1 mm, bidirectioneel en Grayscale gezet. Het opgeslagen `gray-ramp-grayscale.lbrn2` bewaart het beeld en de feitelijke instellingen. Dit is een aanvullende derde fixture; bovenstaande Line/Fill-tabel blijft ongewijzigd.

De app gaf bij een eerste poging met min=max=20% een troubleshootingmelding over power scale. Die export is afgebroken; het bereik is daarna naar 1–60% gewijzigd. Er is dus geen bevestigd RD-bestand met die onjuiste gelijke instellingen.

De definitieve Grayscale-export is **7318 bytes**, met **2349** gesplitste records:

- 1 absolute travel (88).
- 13 relatieve Y-stappen (8B): samen met de eerste rij zijn er 14 scanrijen.
- 756 relatieve X-cuts (AA): 54 powerruns per rij.
- 756 records met opcode C7 en 756 met opcode C2; telkens als paar vóór een run.
- Bodywerkmodus CA 01 01, basispower laser1 min raw164 en max raw9830.

Voorbeeld vanaf raw offset 0x1c0:

```text
C7 7f 7f     modulation = 16383
C2 7f 7f     dezelfde modulation
AA 7e 38     X-delta = −200 µm
C7 7d 7d     modulation = 16125
C2 7d 7d     dezelfde modulation
AA 7f 1c     X-delta = −100 µm
C7 7b 3b     modulation = 15803
C2 7b 3b     dezelfde modulation
AA 7f 1c     X-delta = −100 µm
```

De publieke registry noemt C7 `IMD_POWER_1` en C2 `IMD_POWER_3`. **Die namen bewijzen geen aansturing van een fysieke derde laser.** Het echte bestand gebruikt deze paren met laserselectie 01. De concrete kanaal-/modulatiesemantiek moet afzonderlijk worden bevestigd.

De modulatie bereikt raw16383 terwijl het ingestelde maximum 60% is. Je kunt dit veld dus niet simpelweg als een zelfstandig absoluut 100%-lasercommando interpreteren. Het patroon past bij genormaliseerde beeldmodulatie bovenop ingestelde min/max, maar de exacte firmwareformule en de rol van C2 zijn **OPEN**. Voor herbouw moet de IR onderscheid maken tussen basispowerbeleid en per-run toon/modulatie.

Het uit 16 bronpixels geresamplede beeld gebruikt meer powerruns dan 16 per rij. Dit bevestigt dat bronpixelcount niet gelijk is aan het aantal machinepixels bij Pass-Through uit. De exacte resamplingkernel is niet geïdentificeerd. Vergelijk de volledige JSON-decode en het project om de geteste condities te reproduceren.
