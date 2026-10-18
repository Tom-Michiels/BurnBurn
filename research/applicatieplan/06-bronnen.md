# Bronnen en onderbouwing

Geraadpleegd op **17 oktober 2026**. Dit plan combineert bestaande lokale onderzoeken met aanvullend documentatieonderzoek naar 2D-sketching en tracing. Productvoorstellen, prestatiebudgetten en releasegrenzen zijn eigen ontwerpkeuzes. Er zijn voor dit plan geen nieuwe appbenchmarks of fysieke lasertests uitgevoerd.

## Bestaande analyses

| Bron | Gebruikt voor |
|---|---|
| [LightBurn-dossier](../lightburn/README.md) en [feature-index](../lightburn/reference/feature-index.md) | Bestaande editor-, CAM- en machinefuncties als functionele referentie |
| [LightBurn architectuur](../lightburn/04-herbouwarchitectuur.md) | Eerdere modulegrenzen, eenheden en compiled revision |
| [Echte RD-exportanalyse](../lightburn/ruida/04-exportanalyse.md) | Line/Fill/Grayscale-fixtures, concrete bytes en nog open semantiek |
| [LightBurn acceptatieplan](../lightburn/05-acceptatieplan.md) | Differentiële uitvoerproeven, emulator en hardwarebench |
| [Ruida open vragen](../rudia/05-validatie-en-open-vragen.md) | Ontbrekende gegevens van de doelmachine en transportvalidatie |
| [Andere software profielen](../andere-software/01-softwareprofielen.md) | RDWorks, MeerK40t, VisiCut, Rayforge, vision, nesting en operatorrichtingen |
| [Vergelijkingsmatrix](../andere-software/02-featurevergelijking.md) | Bewijsniveaus en specifieke Ruida-beperkingen |
| [Twaalf ontwerpideeën](../andere-software/03-beste-ideeen-voor-burnburn.md) | Operationmodel, bevroren JobPlan, wachtrij, calibratie en adapters |
| [Oorspronkelijk bronregister](../andere-software/05-bronnen-en-methode.md) | Primaire herkomst van bestaande vergelijkingsclaims |

De lokale dossiers zijn de basis voor de laservergelijking. Hun oorspronkelijke bewijslabels blijven gelden; het overnemen van een conclusie in een productvoorstel levert geen extra compatibiliteitsbewijs op.

## Aanvullende primaire bronnen

| ID | Primaire bron | Concreet ondersteunde observatie |
|---|---|---|
| B01 | [LightBurn Layer Modes](https://docs.lightburnsoftware.com/2.2/Explainers/LayerModes/) | Vier modi: Line, Fill, Offset Fill en Image |
| B02 | [LightBurn Trace Image](https://docs.lightburnsoftware.com/2.2/Reference/TraceImage/) | Traceopties omvatten onder meer centerline, Sketch Trace en transparantie |
| B03 | [Onshape Working with Constraints](https://cad.onshape.com/help/Content/Sketch/working_with_constraints.htm) | Geometrische relaties, inferentie, statusweergave en constraintmanager |
| B04 | [Onshape Dimension](https://cad.onshape.com/help/Content/Sketch/dimension.htm) | Sturende en refererende bemating; afstand, hoek, diameter en radius |
| B05 | [Fusion Fully define and constrain sketches](https://help.autodesk.com/cloudhelp/ENU/Fusion-Sketch/files/SKT-FULLY-DEFINE-CONSTRAIN-SKETCH.htm) | Volledig bepaald via maten en constraints; status van geometrie |
| B06 | [Fusion Create or edit parameters](https://help.autodesk.com/cloudhelp/ENU/Fusion-Model/files/SLD-MODIFY-CHANGE-PARAMETERS.htm) | Benoemde parameters, expressies en berekende waarden |
| B07 | [Fusion Project and include geometry](https://help.autodesk.com/cloudhelp/ENU/Fusion-Sketch/files/SKT-SKETCH-CREATE-PROJECT-INCLUDE.htm) | Associatieve projectie met Projection Link |
| B08 | [Fusion AutoConstrain](https://help.autodesk.com/cloudhelp/ENU/Fusion-Sketch/files/REF-AUTO-CONSTRAIN.htm) | Resultaatvoorstellen, aantallen relaties en gecontroleerde toleranties voor geometriewijziging |
| B09 | [Potrace projectsite](https://potrace.sourceforge.net/) | Bitmap naar gladde vectorcontouren, curveoptimalisatie en licentie-informatie |
| B10 | [AutoTrace repository](https://github.com/autotrace/autotrace) | Expliciete contour- en centerline-route en gebruik als library |
| B11 | [VTracer repository](https://github.com/visioncortex/vtracer) | Kleurvectorisatie en beschreven trace-/fittingstappen; kandidaat voor benchmark |
| B12 | [SolveSpace library-interface](https://solvespace.com/library.pl) | Constraintsolver kan via een library-interface worden ingebed |

De gelinkte documentatie en ontwikkelbranches zijn veranderlijk. Voor implementatie worden gekozen releases/commits, licenties, configuratie en hashes vastgepind. Actuele VTracer-README-inhoud beschrijft ook ontwikkeling richting 1.0; die tekst wordt niet gelijkgesteld aan een lokaal geteste stabiele release.

Inkscape is als aanvullende trace-referentie gezocht, maar de tutorialpagina kon niet betrouwbaar volledig worden opgehaald. Het plan gebruikt daarom de toegankelijke makerbronnen van de trace-engines voor concrete algoritmekandidaten; geen ongeverifieerde Inkscape-kwaliteitsclaim.

## Feit, voorstel en te meten resultaat

| Uitspraak | Status |
|---|---|
| De bestaande lokale exports bevatten Line, Fill en een Grayscale-experiment | Reeds aanwezig bestandsbewijs uit het oorspronkelijke dossier |
| Alle Ruida-modellen ondersteunen dezelfde uitvoer en terugmelding | Niet vastgesteld; geen uitgangspunt van het plan |
| LightBurn heeft al centerline en Sketch Trace | Gedocumenteerd in B02 |
| Optionele sketches naast vrije vectoren passen bij dit product | Eigen productkeuze op basis van de werkstromen |
| Lokale correcties en topologiecontrole kunnen herstelwerk verminderen | Ontwerphypothese, te toetsen met taakbenchmark |
| 30% minder voorbereiding en genoemde tijd-/foutbudgetten | Voorgestelde acceptatiedoelen, niet gemeten prestaties |
| De beschreven volledige applicatie bestaat al | Nee; dit dossier is een product- en bouwplan |

## Controle van dit dossier

De lokale controle telt unieke story-ID's, controleert lokale Markdown-links en releaseverwijzingen, en beoordeelt of scope, architectuur en validatie bij elkaar passen. De uitkomst staat in [validatie](validation.txt). Dit is controle van het plan en geen geslaagde test van nieuwe applicatiecode.
