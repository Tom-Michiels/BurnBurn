# Onderzoeksdekking en resterende onzekerheid

## Werkelijk voltooid

| Onderdeel | Bewijsniveau | Afbakening |
|---|---|---|
| Menubalk | UI | alle 10 hoofdmenu’s, 262 zichtbare actie-/lijstitems incl. OS-invoegingen |
| Algemene instellingen | UI | alle 5 tabbladen |
| Ruida-device settings | UI | alle 3 tabbladen van huidig profiel |
| Cut Settings | UI | Line, Fill en Offset Fill Common/Advanced; Image en tien modekeuzes |
| Offset / arrays / barcode | UI | dialoogvelden; 29 barcodekeuzes; virtual-arrayoptie |
| Adjust/Trace Image | UI | dialogen met eigen toontrap bekeken |
| Tests en machinehulpmiddelen | UI | dialogen bekeken; focus door Z-voorwaarde beperkt |
| Preview | UI + RD | Fill-cutafstand gelijk aan offline decode |
| Officiële referentie-index | DOC | 158 referentiepagina’s + 5 aanvullende guides |
| Vector Line-export | RD | één rechthoek, 100 mm/s, 20%, absolute frame |
| Vector Fill-export | RD | één rechthoek, 150 rijen, 0,1 mm, 0° bidirectioneel |
| Grayscale-export | RD | 16×4-bronbeeld, 14 scanrijen, C7/C2-modulatie en AA-runs |
| Scrambling/waarden | RD + OSS + tests | 0x88 bevestigd voor fixtures; boundaries offline getest |
| File-som | RD + OSS | alle drie echte exports gecontroleerd |
| Protocolvocabularium | OSS | 200 constanten en 8 geheugenadressen uit vastgepinde bron |
| Transportontwerp | OSS + DOC | geen livecapture; aanbevolen state machine apart gemarkeerd |
| Architectuur/acceptatie | ONTWERP | zelfstandig herbouwplan; geen implementatie van gehele app |

## Niet volledig afgerond door offline-/profielgrenzen

| Open vraag | Waarom belangrijk | Vereist vervolg |
|---|---|---|
| alle contextmenu’s/selectietoestanden | menu’s wijzigen per object en groepscontext | screenshots/AX-corpus voor node, text, image, layer, library, file list |
| alle modal subdialogen en numeric ranges | enable-toggles tonen extra velden | systematic UI-state matrix, inclusief typeafhankelijke barcode-/generatorvelden en nesting |
| volledige machineparameterboom | systeem toonde communicatiefout | model/firmware uitlezen op aangesloten controller; backups en typed registermapping |
| LightBurn live UDP | packetisering en retries niet uit RD bekend | passieve capture van connect/query/send/start/stop met eigen testjobs |
| directe USB | endpoints/driver/framing niet vastgesteld | hardware-identificatie en USB-capture |
| alle Ruida-modellen | varianten kunnen anders encoderen | compatibiliteitsmatrix per controller/firmware |
| powerrounding/defaultbeleid | raw3277 bij 20% bepaalt formule niet uniek | exporttoontrap en controller-defaultproeven |
| absolute signed coord | publieke 32/35-bit interpretaties verschillen | gecontroleerde negatieve coördinatenexport en decodervergelijking |
| rasterangles/overscan | native en vector-emulatie verschillen | vier assen + 45°-exports en fysieke motionobservatie |
| grayscaleformule/ramp/PWM/delays | toontrap toont modulatie; formule en overige schalen nog onvoldoende bepaald | één-factor-per-exportcorpus en niet-nulwaarden |
| multi-layer/sub-layer/channels | projectlaag is mogelijk geen controllerpart | exports met meerdere bewerkingen en kanalen |
| volledige LBRN2 | één XML-voorbeeld dekt alleen een Path/Scan | schemaonderzoek met iedere objectsoort, relaties en unknown-field-retention |
| font-/dither-/nestheuristiek | gesloten implementatie niet uit UI afleidbaar | eigen expliciet algoritme + geometrische/visuele equivalentietests |
| camera/rotary/feed | geen apparatuur aanwezig/getest | gemeten calibratie- en productiebench |
| G-code en galvo-pariteit | Ruida-profiel dekt die backends niet | afzonderlijke UI/protocol/hardwarecorpora |

De gebruiker hoeft voor de reeds uitgevoerde offline research niets te bevestigen. Deze tabel beschrijft technische bewijsgrenzen, geen aanvullende toestemmingseisen. Het dossier levert een brede, traceerbare basis voor een vergelijkbare implementatie, maar verklaart geen onbewezen complete feature- of hardwarepariteit.

## Prioriteiten voor verdere research

1. Meer gecontroleerde offline exports: kleine relative moves, powerladder, tweede laag, beeld/grayscale, andere oorsprong en niet-nuldelay.
2. Context-UI vastleggen met de genoemde objecttypen.
3. Eén werkelijk Ruida-model koppelen en passieve UDP/USB-metingen maken.
4. Pas daarna overige Ruida-modellen en productbrede G-code/galvo-ondersteuning kwalificeren.

De huidige artifacts en scripts blijven bruikbaar als baseline en hoeven niet opnieuw gemaakt te worden.
