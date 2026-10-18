# Ruida-binaire codering en bestandstructuur

Dit is een **interop-specificatie met bewezen subset**. Bytevoorbeelden hieronder zijn **ongescrambeld**, tenzij expliciet anders vermeld. De drie LightBurn-fixtures zijn de primaire meetdata. `command-registry.json` bevat het ruimere **OSS**-vocabularium; de namen zijn identificatielabels en geen bewijs van universele semantiek.

## 1. Byte-scrambling

Voor de onderzochte exports werkt magic **0x88**. Verwissel bit 0 en bit 7 van iedere logische byte, XOR met magic, tel 1 op modulo 256:

```text
swap07(b) = (b & 0x7e) | ((b & 1) << 7) | ((b >> 7) & 1)
wire(b)   = ((swap07(b) XOR magic) + 1) & 0xff
raw(w)    = swap07(((w - 1) & 0xff) XOR magic)
```

Voorbeelden bij 0x88: raw CC → wire C6; raw CD → wire 46; raw CF → wire 48; raw 00 → wire 89. De scrambling is een omkeerbare bytepermutatie, geen beveiligde versleuteling. Magic kan firmware-/apparaatgebonden zijn; maak het configureerbaar en bevestig via bekende records/statusantwoord, niet alleen via een heuristische gok.

De hulpmiddelen testen de inverse voor alle 256 bytewaarden. De gescrambelde bytes vormen het RD-bestand; analyseer opcodes dus pas ná unscrambling.

## 2. Zevenbitswaardevelden

Payloadbytes dragen doorgaans 7 bits; hun hoge bit is nul. Commando’s beginnen met een byte met hoog bit één. Bij de fixtures is splitsen op zulke bytes bruikbaar. Voor een productieparser zijn exacte lengtes en aparte string-/responsegrammatica’s nodig: een onbekend record kan niet betrouwbaar op semantiek worden geïnterpreteerd alleen omdat een grens herkenbaar is.

```text
pack7(value, n): schrijf 7*n bits als n big-endian groepen van 7 bits
unpack7(bytes): value = (((b0 << 7) | b1) << 7 | ...)
```

| Veld | Lengte | Onderzoekscontract |
|---|---:|---|
| unsigned14 | 2 | bereik 0..16383 |
| relative coordinate | 2 | signed14 two’s complement, µm, −8192..8191 |
| absolute coordinate | 5 | 35 payloadbits; positieve µm in fixtures bevestigd |
| snelheid | 5 | µm/s: UI mm/s ×1000 |
| power | 2 | 14-bit schaal; exacte procentafronding apart bepalen |
| part/index/switch | meestal 1 | subtypeafhankelijk; valideer bereik op schema |
| tijd/frequentie/kleur | subtypeafhankelijk | publieke encoders gebruiken vijfbytesvelden; unit en bitindeling per command bevestigen |

**Absolute negatieve waarden zijn OPEN.** Publieke code heeft zowel signed32- als signed35-decodefuncties. Het bestaan van vijf zevenbitsbytes betekent niet automatisch dat het gehele 35-bit signed bereik op elk model een fysieke coördinaat is. De eigen decoder toont daarom alleen de positieve waarden van het gemeten corpus als bevestigd.

Voorbeelden:

- 100 mm/s → 100000 → `00 00 06 0d 20`.
- 30 mm → 30000 → `00 00 01 6a 30`.
- −100 µm → signed14 `0x3f9c` → `7f 1c`.
- +8191 → `3f 7f`; −8192 → `40 00`; +8192 vereist absolute encoding of splitsing.

## 3. Powerafronding: relevant verschil

De appwaarde 20% werd **3277**, dus `19 4d`, zowel voor min als max. Publieke encoderimplementaties gebruiken onder andere truncatie van `pct*16383/100`; dat zou bij 20% 3276 geven. De gemeten LightBurnwaarde past bijvoorbeeld bij afronden van een 16383- of 16384-schaal, maar twee identieke powerwaarden identificeren die formule niet uniek.

Gebruik dus niet de naam `encode_power` uit een extern project als bewijs van byte-identieke LightBurn-compatibiliteit. Test 0, 0,1, 1, 10, 20, 50, 99,9 en 100%, inclusief halfwaardes en min/max/defaultbeleid. Bewaar raw14 in de decoder. Eén kwantisatiestap is ongeveer 0,0061 procentpunt; dit is commandresolutie, geen optische vermogensnauwkeurigheid.

## 4. Kernopcodes voor interoperabiliteit

| Opcode | Parameters na opcode | Betekenis / status |
|---|---|---|
| 88 | X5 Y5 | absolute travel; RD bevestigd |
| 89 | dX2 dY2 | relative travel XY; OSS |
| 8A / 8B | delta2 | relative travel X/Y; 8B RD bevestigd |
| A8 | X5 Y5 | absolute snij-/markeerbeweging; RD bevestigd |
| A9 | dX2 dY2 | relative cut XY; OSS |
| AA / AB | delta2 | relative cut X/Y; OSS |
| C9 02 | speed5 | actieve lasersnelheid; 100 mm/s RD bevestigd |
| C9 04 | part1 speed5 | partsnelheid in metadata; RD |
| C6 01 / 02 | power2 | laser 1 min/max; RD |
| C6 21 / 22 | power2 | laser 2 min/max; aanwezig in RD, kanaalactivatie afzonderlijk |
| C6 31 / 32 | part1 power2 | part min/max laser 1; RD |
| C6 41 / 42 | part1 power2 | part min/max laser 2; RD |
| C6 12 / 13 | time5 | laser-on/offdelay; nul in huidige RD; schaal niet uit nul afleidbaar |
| C6 50 / 51 | power2 | through-powerfamilie; `00 01` in Line-fixture; precies effect OPEN |
| CA 01 00 | geen | door publieke registry Layer End genoemd; staat ook vóór Line-body |
| CA 01 01 | geen | rasterwerkmodus voor onderzochte horizontale Fill; RD |
| CA 01 12 / 13 | geen | air off/on volgens OSS; on aanwezig in RD |
| CA 02 | part1 | partkeuze; RD |
| CA 03 | mask1 | laser enable/select; `01` in RD; meerkanaalbetekenis nog testen |
| CA 06 | part1 color5 | partkleur; RD met zwart |
| CA 41 | part1 mode1 | metadata voor partwerkmodus; RD |
| E6 01 | geen | absolute-setfamilie; RD |
| D8 10 / 11 / 12 | geen | reference machine/user/current volgens OSS; D8 10 in absolute fixture |
| E7 03 / 07 | X5 Y5 | processbounds; RD |
| E7 50 / 51 | X5 Y5 | documentbounds; RD |
| E7 52 / 53 | part1 X5 Y5 | partbounds; RD |
| E7 61 / 62 | part1 X5 Y5 | uitgebreide partbounds; RD |
| E7 00 | geen | block end; RD |
| EA / EB | index1 / geen | array begin/endfamilie; RD |
| E5 05 | sum5 | bestandssom; RD en eigen controle |
| D7 | geen | einde bestand; RD |

Een nulwaarde kan aanwezigheid en veldlengte bevestigen, maar niet de schaal of betekenis van een niet-nulparameter. Hetzelfde geldt voor identieke powers bij uitgeschakelde laser 2-controls.

De registry bevat 200 commandoconstanten, inclusief extra assen, parts, powerchannels, immediate/end power, arrays, feeders, documentopslag en frontpanel-keyevents. Een gelijk bytepatroon kan meerdere labels hebben; de decoder kiest de langste bekende prefix en bewaart onbekende records.

## 5. Geobserveerde bestandsopbouw

Voor alle drie app-exports:

1. Reference-/absolute-setup en jobinitialisatie.
2. Feed en bounds: process, document en arraybereik.
3. Partmetadata: speed, powers, color, mode en bounds.
4. Offset-/element-/arraymetadata.
5. Body: workmode, partselectie, laserselectie, air, speed/powers, delay en motion.
6. Array/block-eindes, metriekrecord, file-som en D7.

De init bevat in deze fixtures `D8 00`, publiek gelabeld START_PROCESS. Dit staat in het bestand zelf. Trek daaruit niet de conclusie dat elk willekeurig los verzonden D8 00-record slechts onschuldige metadata is: live-commandocontext en fileparsercontext kunnen verschillen.

De Line-footer bevat `DA 01 06 20` met twee vijfbytewaarden van 90. Fill heeft daar twee keer 4500. Dit correleert exact met de snijlengte in mm in deze twee bestanden. De algemene registerbetekenis, andere eenheden of totaliseringspolicy zijn nog niet vastgesteld; stuur dit register niet los als configuratieschrijfopdracht naar een machine.

`F2 01 00` blijft UNKNOWN in de decoder. `E7 37` heeft een publieke naam, maar daarmee is de precieze LightBurn-semantiek niet verklaard. Behoud zulke records in de analyse in plaats van ze stilzwijgend weg te laten.

## 6. Twee verschillende checksums

**Bestandssom (RD-bevestigd):** in alle drie fixtures is de vijfbytewaarde na E5 05 gelijk aan de som van alle ongescrambelde bytes vóór dit record plus `0xD7`. De bytes van E5 05 en de eigen somwaarde worden niet meegeteld.

```text
fileSum = sum(raw_before_E5_05) + 0xD7
footer  = E5 05 + pack7(fileSum,5) + D7
```

Line: 24784 + 215 = **24999**. Fill: 142600 + 215 = **142815**. Grayscale: 741329 + 215 = **741544**. Dit is geen CRC en geen UDP-som. Grote bestanden en overflowgedrag moeten apart worden getest; in de huidige fixtures is geen overflow.

**UDP-som (OSS):** som van reeds gescrambelde payloadbytes modulo 65536; twee gewone big-endian bytes vóór de payload. Deze twee bytes worden niet opnieuw gescrambeld en zijn niet zevenbitsvelden.

## 7. Parser- en writervereisten

- Bewaar raw offset, raw bytes, herkende commandoprefix, gedecodeerde parameters en provenance.
- Houd parserstate bij voor part, mode, power, speed, positie, referentie en arrays.
- Controleer parameterlengtes vóór lezen; onderscheid truncated, unknown en malformed.
- Stel negatieve abscoords, strings, geheugenreacties en lange payloads apart op schema.
- Valideer ranges vóór packen: masking zonder rangecheck kan een waarde stilzwijgend omklappen.
- Voorkom dubbele scrambling bij transport van een al gecodeerd `.rd`.
- Behoud onbekende bytes bij inspectie/roundtrip; genereren van productiejobs vereist een gevalideerde whitelist.

## Primaire implementatiebronnen

[MeerK40t RD-jobencoder op onderzochte commit](https://github.com/meerk40t/meerk40t/blob/d511944e08788e88077727f682957759884ff5ca/meerk40t/ruida/rdjob.py), [ruida-laser referentie-encoder](https://github.com/jnweiger/ruida-laser/blob/a1e7b9b93b10d5cac79c875bc3efec46f7397a11/src/ruida.py). De commit- en hashmanifesten maken vergelijking met gewijzigde upstreamversies mogelijk. De onafhankelijke code en geobserveerde fixtures zijn belangrijker dan ongedateerde losse protocoltabellen.

## Aanvullend bevestigd modulatievocabularium

De derde fixture bevestigt C7 + twee modulationbytes en C2 + twee modulationbytes als paarsgewijze records vóór relatieve X-run AA. Het bereik bevat 0..16383; de ingestelde basispowers staan daar los van. Zie `04-exportanalyse.md`. De logische veldnaam `modulation_raw14` in de inspectietool is bewust terughoudend: we claimen daarmee geen derde fysieke laser of absolute outputpower. De extra basiswaarden min1%=164 en max60%=9830 passen eveneens bij afronding; ze identificeren nog geen volledige procentformule.
