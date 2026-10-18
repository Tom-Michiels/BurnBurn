# Aanvullende rechtstreeks bekeken dialogen

Dit hoofdstuk vult de hoofdfuncties aan met **UI-waargenomen** velden. Het onderscheid tussen een zichtbaar, enabled en daadwerkelijk uitgevoerd veld blijft belangrijk. De meeste dialogen zijn met Cancel afgesloten; er zijn geen array-/offsetresultaten naar een machine gestuurd.

## Offset

- Offset Distance.
- Direction: Outward, Inward, Both.
- Corner Style: Round, Bevel, Corner.
- Outer Shapes Only.
- Select Resulting Objects.
- Delete Original Objects.
- Optimize / Simplify Results.
- OK / Cancel.

De simplify-optie vermeldt curvefitting van het resultaat. **Herbouwcontract:** genereer eerst de offsettopologie en vereenvoudig daarna binnen foutmarge; een delete-original-keuze hoort pas bij OK te worden gecommit. Both produceert twee kanten en kan meer dan één contour per bron opleveren.

## Create Grid Array

Horizontaal: X Columns, X Spacing, X Column Shift, Total Width; Distance from Center to Center of Padding Between Edges; Reverse Direction; Shift by Half; Mirror Alternate Cols in X/Y.

Verticaal: Y Rows, Y Spacing, Y Row Shift, Total Height; Distance from Center to Center of Padding Between Edges; Reverse Direction; Shift by Half; Mirror Alternate Rows in X/Y.

Algemeen: Create Virtual Array; Random Orientation plus numeriek veld; Auto-Increment Variable Text plus increment; Select Results; Group Results; total size/count; OK/Cancel.

De UI-tooltip geeft een belangrijk contract: Total Width/Height berekent het aantal kolommen/rijen, maar een virtuele array blijft die buitenmaat niet afdwingen wanneer de bronvorm later wijzigt. Dit is geen permanente constraint-solver op de totale rechthoek.

**Herbouw:** maak `VirtualArray(sourceIds, rows, columns, spacingPolicy, shifts, alternation, orientationPolicy, textIncrement)`. Expand uitsluitend voor rendering, hit-test of compile wanneer nodig. Bewaar instantie-ID’s of provenance zodat een CSV-offset aan een deterministische rij/kolom blijft gekoppeld. Een materialized array levert zelfstandige kopieën; een virtual array bewaart de bronrelatie. Stel een limiet op de uitgebreide operationele count en voorkom cycli. Random Orientation moet een opgeslagen seed of opgeslagen uitkomsten hebben voor reproduceerbare preview en export; het daadwerkelijke LightBurn-randombeleid is nog niet getest.

## Circular Array

Waargenomen: center X/Y, Start, End, Step, Copies; Rotate Object Copies; Use Last Selected Object Position As Center; Auto-Increment Variable Text en increment; Select Results; Group Results; OK/Cancel.

De sessie toonde 8 kopieën, 45° stap en 360° einde. Dat bewijst niet welke telpolicy geldt voor overlappende 0°/360° eindpunten bij iedere combinatie. Test inclusief en exclusief eindpunt apart. De last-selected-centerkeuze maakt selecteerorde onderdeel van de interactie.

## Bar Code Properties

De Create Bar Code-menuactie activeert eerst een tekengereedschap. Pas na slepen opent het eigenschappenvenster. Het venster bevat raw content, evaluated text, text evaluation Normal/variable modes, Variable Offset, Input Mode en Error Correction Level. De QR-weergave had een correctieslider met Low-label. Er zijn inhoudstabbladen Raw Content, WIFI en Contact.

Input Mode beschrijft Data, Unicode en GS1. Process Control Characters zet getypte controltokens zoals record/group separators om; volgens de tooltip wordt dit automatisch uitgezet bij GS1. Ondersteun deze als echte encodings; vervang ze niet door visueel gelijke tekst.

Alle in de geopende Bar Code Type-lijst geziene keuzes:

| Familie | Waargenomen keuzes |
|---|---|
| 2 of 5 | 2 of 5; 2 of 5 IATA; 2 of 5 Industrial |
| fiducials | AprilTag 16h5; AprilTag 25h9; AprilTag 36h10; AprilTag 36h11; ArUco MIP 36h11; ArUco OpenCV 7x7 |
| matrix | Aztec; Chinese Sensible Code; DataMatrix; Grid Matrix |
| lineair | CodaBar; Code 128; Code 128 GS1; Code 93; EAN; ISBN; Plessey; UPC A; UPC E; VIN |
| PDF417 | PDF417; PDF417 Compact; PDF417 Micro |
| QR | QR Code; QR Micro; QR Micro Rectangular |

Dit register telt 29 keuzes. Typeafhankelijke extra parameters en de WIFI/Contact-formulieren zijn niet volledig doorlopen. Een eigen implementatie moet fiducials als markerfamilies behandelen en niet verwachten dat een generieke barcodelezer alle aangeboden typen ondersteunt. De tijdelijke barcodeproef is niet aan het opgeslagen grayscaleproject toegevoegd.

## Offset Fill

Common gebruikt de gedeelde layout van Fill, maar in dit onderzochte profiel waren Scan Angle, bidirectional, cross-hatch en alle drie grouping-radioknoppen **disabled**. Interval/LPI en passes bleven beschikbaar. Z bleef disabled door de profielinstelling.

Advanced: U Offset en Override PWM Frequency waren beschikbaar; Flood Fill, Ramp Length en Ramp Outer Edge waren disabled. Daarom moet een backend de actieve capability/instelling van een optie vastleggen, niet alleen het feit dat de labeltekst voorkomt in de dialoog of handleiding.

De modekeuze is alleen tijdelijk in de dialoog gebruikt en met Cancel verworpen. De bewaarde rechthoekfixture blijft Fill.

## Andere workflows

Material Test: presetbeheer, titel, Vertical/Rows en Horizontal/Columns met parameter/min/max/count/maat, X/Y Center, Enable Text/Border, Edit Material/Text/Border Settings en een eigen laserbedieningsblok. De subeditors zijn niet allemaal geopend.

Interval Test: Simple Fill / Dithered Image, steps, min/max interval, maat, snelheid/power en Preview/Frame/Start/Send/Save.

Rotary en Auto-feed: zie `03-instellingen.md` en `evidence/ui-observations.md`. Machine Settings en controllerinfo tonen de concrete verbindingsgrens. Camera manager had een lege lijst, zodat werkelijk camerabronnen en calibraties niet konden worden geïnspecteerd.
