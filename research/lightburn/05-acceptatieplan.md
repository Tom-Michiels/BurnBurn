# Acceptatieplan voor feature-pariteit

Dit is een **uit te voeren testplan**, geen lijst geslaagde tests. De daadwerkelijk uitgevoerde offline checks staan in `validation/results.txt` en de bijbehorende Python-tests. Verbind iedere implementatietaak met een menu-ID Mxxx en/of reference-topic-ID Fxxx.

## Functionele regressies

| ID | Corpus / handeling | Vereist resultaat |
|---|---|---|
| ED01 | select, invert, group, nested group, locked objects | voorspelbare selectie, bounds en transformaties; geen lock-bypass |
| ED02 | mm↔in, locale comma/point, 9 anchors | fysische maten gelijk; anchor blijft staan bij resize |
| ED03 | 100 bewerkingen, undo/redo, save/open | documentinhoud en relatiegraph identiek na roundtrip |
| ED04 | duplicate vs paste vs paste-in-place | eigen ID’s, correct plaatsingsbeleid en clipboardinhoud |
| GE01 | touching/overlapping polygons, holes, open curves | geldige boolean-topologie; fillregel expliciet |
| GE02 | self-intersection, 1µm edge, huge coordinate | geen crash, begrensde fout en duidelijke diagnose |
| GE03 | curvefit/flatten op cirkels en cusp | maximaal afgesproken geometriefout; sluiting behouden |
| GE04 | offset, kerf en fillet bij smalle nek | verdwenen delen of splitsingen correct gerapporteerd |
| GE05 | trim/extend/join met tolerantie net onder/op/boven grens | geen spookverbindingen; consistente threshold |
| AR01 | center/rand align, center/gap distribute, dock | exacte maten binnen modeltolerantie |
| AR02 | grid/circular/path arrays met groups en text | juiste count, oriëntatie, spacing en offsets |
| AR03 | nesting met gaten, locked rotations, padding | geen botsingen; report un-nested; deterministische seed |
| TX01 | kerning, ligatures, RTL, accents, fallback/SHX | preview, paths en output tonen dezelfde tekst |
| TX02 | CSV quoted commas/newlines, missing cell, serial overflow | expliciete fout/formatpolicy; geen stille verschuiving |
| TX03 | bake, next, auto-advance, failed send | geen duplicaat of verloren serienummer door mislukking |
| BC01 | barcode/QR min module, Unicode en lange payload | onafhankelijke decoder leest de gemaakte code |
| IM01 | alpha-gradient, zwart/wit, toontrap, éénpixelrand | voorspelbare compositie en fysieke pixelgrid |
| IM02 | alle tien image modes op hetzelfde bestand | golden image per algoritme en resamplingpolicy |
| IM03 | mask/crop/rotate/replace/refresh | clip en maat behouden volgens contract; undo werkt |
| TR01 | tracing met gaten, ruis, transparantie en open lijnen | contourhiërarchie en foutbudget kloppen |
| LY01 | tool/output/visible layers, sublayers en defaults | outputflag onafhankelijk van zichtbaarheid; sublayer-volgorde |
| CAM01 | geneste contouren over layers/groups/priorities | orderconstraints houden stand bij traveloptimalisatie |
| CAM02 | tabs + kerf + leads + overcut + meerdere passes | correcte volgorde en geen onbedoelde snijbruggen |
| CAM03 | parity fill met touching/overlapping shapes | zelfde/different-layergedrag aantoonbaar |
| CAM04 | flood/crosshatch/grouped fill en offset fill | alle gewenste gebieden één keer per geplande pass |
| CAM05 | selectie + origin + Print and Cut | preview en machinebounds horen bij dezelfde selectie/frame |
| PRE01 | preview/end artifact revisiehash | elke preview hoort exact bij artifactinhoud |
| PRE02 | looptijdmeting 10 korte/lange/snijdende/rasterjobs | foutstatistiek per profiel; geen universele nauwkeurigheidsclaim |
| LIB01 | art/material libraries verplaatsen, bundle import/export | verwijzingen en presetwaarden blijven geldig |
| CA01 | calibratiepatroon en gemeten targets op meerdere hoogtes | residuals en gemeten positioneringsfout binnen gekozen tolerantie |
| CA02 | camera resolutie/bron wijzigen | oude calibratie niet ongemerkt op andere bron toepassen |
| REG01 | twee targets, no-scale, scale en reset | bekende transform teruggevonden; near-coincident targets afgewezen |
| RO01 | chuck/roller, mirror, diameters en switch terug vlakbed | juiste omtrekmaat; profielherstel gecontroleerd |
| FE01 | repeat/feed-before/feed-after en pause | juiste aantallen en verplaatsingen; stoppen hervat coherent |
| MAT01 | material/interval/focus generators | tijdelijke instellingen en labels kloppen; geen brondocumentvervuiling |
| API01 | desktop UDP automation load/import/status/ping | replysemantiek, dirty checks en scope gedocumenteerd |
| PL01 | vertalingen, DPI, OS file dialogs, hotkeys | command-IDs blijven gelijk; locale tast fysieke uitvoer niet aan |

## Differentieel Ruida-exportcorpus

Maak per experiment een project, screenshots van relevante instellingen, raw `.rd`, decode-JSON en een manifest met profiel/modelversie. Verander **één factor tegelijk** en vergelijk vervolgens combinaties.

1. Richtingen: X+, X−, Y+, Y−, diagonalen en gesloten contours.
2. Lengtes rond relative14: 8,190; 8,191; 8,192 mm en negatieve tegenhangers.
3. Origins: vier bedhoeken; Absolute, User Origin, Current Position; alle 9 jobanchors.
4. Power: 0 / 0,1 / 1 / 10 / 20 / 50 / 99,9 / 100%; min≠max; default power.
5. Snelheid: lage en hoge waarden, mm/sec versus mm/min, default speed.
6. Fill: 0°/90°/180°/270°/45°, uni-/bidirectioneel, interval dat wel/niet exact in hoogte past.
7. Geometrie: gaten, meerdere eilanden, overlappende contouren, groepen, flood/crosshatch.
8. Images: threshold versus grayscale, toontrap, alpha, ramp en dot correction.
9. Line: kerf, tabs/partial tabs, perforation, leads, pauses, dot mode en overcut.
10. Layers: twee kleuren, sublagen, disabled output, priority, laser1/laser2/offset.
11. Assen: Z en U met positieve/negatieve offset, passes en restorebeleid.
12. Job: start/end delay, rotary/feed, array repeat, zeer groot bestand en checksumoverflow.
13. Uitvoerroute: Save RD, Send, Start, Run RD en USB-stick — wat verschilt in envelop en payload?

Vergelijk semantische events én binaire records. Een equivalente tool hoeft niet dezelfde startpuntheuristiek te kiezen, maar moet wel dezelfde geometrie, energie-intentie, volgordeconstraints en machineframe leveren. Bytepariteit is een aanvullende compatibiliteitsdoelstelling.

## Transport/emulator

Fault injection: ACK kwijt, dubbele ACK, vertraagd inhoudelijk antwoord, wrong source, corrupted checksum, truncation, disconnect midden in chunk, busy controller, disk full, duplicate filename, overwrite weigeren en app herstart. Geen test mag een uncertain upload als completed behandelen. Dwing beperkte queues en annuleerbare transfers af.

Een emulator bewijst encode-/state-machineconsistentie maar geen controllercompatibiliteit. Gebruik een onafhankelijke decoder of referentietool voor de golden jobs om niet uitsluitend encoder en decoder tegen dezelfde fout te testen.

## Hardwarebench

Gebruik een expliciet geïdentificeerde controller met model/firmware en actuele configuratiebackup. Begin met status en laser-uit-beweging op passend werkgebied; ga dan naar de gecontroleerde materiaaltests van de machine-eigenaar. Meet afmetingen, oorsprong, overscan, min/max-cornergedrag, stop/pause/resume, rotary, feed en I/O. Noem pas modellen ondersteund nadat de matrix daarop is uitgevoerd.

## Gereedcriteria

Een feature is ready wanneer de UI-optie een gespecificeerd effect heeft in document/IR/uitvoer, de relevante tests slagen, opslagroundtrip werkt en capability-/foutgedrag getest is. Een menu-item dat alleen opent is `inventoried`, geen `implemented`. Een codec-roundtrip is geen geslaagde laserjob. Een equivalente architectuur is geen bewezen volledige LightBurn-replica.
