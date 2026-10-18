# Featurevergelijking en toepasbaarheid

Onderzoeksdatum: **17 oktober 2026**. Dit is een brongebonden vergelijking; geen benchmark of gebruikerstest. Zie de [profielen](01-softwareprofielen.md) voor uitleg en het [bronregister](05-bronnen-en-methode.md) voor de URLs.

## 1. Hoofdvergelijking

**D** = gedocumenteerd; **C** = code bekeken; **S** = speciaal systeem/model nodig; **O** = onvoldoende vastgesteld; **N** = expliciet niet ondersteund voor deze route. Een O betekent niet “ontbreekt”. L = bestaande LightBurn-studie, R/M/V/Y = broncodes.

| Onderwerp | LightBurn (referentie) | RDWorks V8 | MeerK40t | VisiCut | Rayforge |
|---|---|---|---|---|---|
| Ruida-uitvoer | D + eerdere RD-exports [L] | D [R02] | D/C [M01,M04] | D/C [V03,V04] | D, experimenteel [Y02] |
| Primaire productrol | Editor + CAM + bediening | Voorbereiden + bediening | Bewerkingen + bediening | Extern ontwerp voorbereiden | CAD + processtappen |
| Materiaal en bewerkingen apart | D [L] | O | D, operations [M02] | D, expliciete profielen [V02] | D, stock en stappen [Y01] |
| Handmatige padcontrole | D [L] | D [R02] | O in deze studie | D, sorting-verwijzing [V02] | O voor exacte Ruida-opties |
| Jobwachtrij + historie | File List D [L]; andere rol | Controllerdownload D [R02] | D, appspooler [M03] | O | O |
| Bestandsexport voor Ruida | D [L] | D [R02] | O voor exacte exportflow | C, `.rd` [V04] | O voor exacte exportflow |
| Camera-positionering | D [L] | Specialistische familie [C01] | O voor Ruida-precisie | D [V01] | D algemeen [Y01/Y03] |
| Rotary met Ruida | D [L] | D, Y-vervanging [R02] | O | O | N volgens specifieke driverdocs [Y02] |
| Herhaalindeling | D [L] | D, array [R02] | O voor precieze variant | O | O voor precieze variant |
| Parametrisch tekenen | Variabele tekst D [L] | O | O | Externe tekenapp | D, schetsconstraints [Y01] |
| Automatiseerbare route | App-UDP D [L] | O | CLI D [M01] | Externe extensie D [V02] | Command-stap heeft Ruida-grens [Y03] |

“File List”, appwachtrij en controllergeheugen betekenen verschillende dingen. De matrix gebruikt die niet als synoniemen. Camera-ondersteuning betekent evenmin dat automatisch print-and-cut met voldoende nauwkeurigheid is bewezen.

## 2. Platform en beschikbaarheid

| Product | Aangetroffen platforminformatie | Beschikbaarheid/versiegrens |
|---|---|---|
| RDWorks V8 | Oudere handleiding beschrijft Windows [R02] | Catalogus 8.01.72.001 [R01]; actuele OS-matrix niet getest |
| MeerK40t | Python-app, GUI en CLI [M01] | Open broncode; geen geïnstalleerde release getest |
| VisiCut | Linux/macOS/Windows [V01] | Java-runtime volgens projectsite; geen package getest |
| Rayforge | Linux/macOS/Windows volgens project [Y01] | 1.12-release notes; Ruida blijft experimenteel [Y02,Y03] |
| MantiSoft | Desktop op drie OS-families [T01,T02] | Officiële 1.1.8-vermelding; modelcompatibiliteit OPEN |
| RDVision-familie | Windows vermeld bij RDV6445G [C01] | Verschillende camera-/controllerproducten; geen universeel pakket |
| RDNest | Windows 10+ [N01] | Nestingvoorbereiding; directe controllerroute OPEN |
| RPA | Python-project [P01] | Driver/diagnose; eigen integratie nodig |

Geen prijstabel: actuele aanschaf-, autorisatie- en gebruiksvoorwaarden zijn niet voldoende onderzocht. Open source zegt niets over de ondersteuning van een specifieke firmware. Oude OS-vermeldingen worden hier niet als actueel installatieadvies gebruikt.

## 3. Keuze per werkstroom — eigen beoordeling

| Situatie | Eerste referentie om verder te beproeven | Reden | Nog aantonen |
|---|---|---|---|
| Bestaande Ruida-job vergelijken | RDWorks naast LightBurn | Fabrikantgerichte voorbereiding en padinstellingen | Identiek proefontwerp op exact model |
| Open eigen app ontwikkelen | MeerK40t + LibLaserCut + RPA | Drie inspecteerbare architectuurvoorbeelden | Licenties, foutpaden, rasterkwaliteit |
| Fablab met extern tekenen | VisiCut | Heldere materiaal-/mappingworkflow | Importtrouw en bedcoördinaten |
| Processtappen en parametrische schetsen | Rayforge | Moderne scheiding tussen bron en bewerking | Ruida-dekking per benodigde stap |
| Bestaand drukwerk uitsnijden | RDVision/MarkVision plus eigen registratieontwerp | Scheiding herkenning en snijpad | Camera, controller en millimeterfout |
| Veel verschillende onderdelen plaatsen | RDNest als aparte voorstap | Onderdeel-/plaatmodel | Exportoverdracht, rotatieregels en opbrengst |
| Desktop/mobiel/web combineren | MantiSoft-familie nader onderzoeken | Officiële multi-clientrichting | Exacte ondersteunde boards en joboverdracht |

Voor BurnBurn is **correcte planning en herleidbare uitvoer** de eerste prioriteit. Nieuwe tekenfuncties, camera en nesting leveren pas betrouwbaar productiewinst als oorsprong, eenheden, bewerkingsvolgorde en verzendstatus kloppen. Dit is een eigen afweging op basis van de beschreven werkstromen.
