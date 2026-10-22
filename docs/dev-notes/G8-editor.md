# G8-editor (golf 8): E05, E06, E07/E08, S07/P06, S01 (de restpunten van G7-editor)

Lane G8-editor. Basis 16f0f7d. Alleen werkelijk uitgevoerde commando's. Toolchain 1.99.1, `--locked`,
`CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/g8-editor`, `CARGO_INCREMENTAL=0`. macOS arm64; geen hardware, geen
netwerk, Windows nooit gedraaid (alleen type-check, clippy en headless formuliertests). Een red-fase telt alleen als de test na
een minimale stub op gedrag faalde; waar die ontbreekt staat dat erbij.

## Wat er bedienbaar is

| Feature | macOS | Windows |
|---|---|---|
| E05 Afronden/afschuinen tussen lijn en kromme, kromme en kromme (cirkelboog, ellipsboog, Bézier), afstanden langs de kromme | bestaande gereedschappen Afronden/Afschuinen op het canvas | idem |
| E05 Verlengen van een Bézier-uiteinde voorbij 3 parameterlengtes: rechte langs de eindraaklijn tot de grens | bestaand gereedschap Verlengen | idem |
| E06 Stap toevoegen aan een keten (offset, boolean met de andere geselecteerde objecten, raster, cirkel) en een keten maken met elke eerste stap | blok "Stap toevoegen" in het Structuurpaneel | idem (formulier) |
| E06 De stapel in de inspecteur: paneel "Stapel van de selectie" opent vanzelf zodra de selectie bron of resultaat van een keten is | zwevend paneel (`G5Panel::Stack`), niet ingebouwd in de vaste kolom van het venster | idem (`FormKind::G5(Stack)`) |
| E07 Omzetten naar pad en tekst typen met meerdere teksten geselecteerd, één undo-stap | Opmaak ▸ Omzetten naar pad, toetsenbord | idem |
| E08 Tekst van meerdere regels op een pad; variabele tekst op een pad; gebonden tekst verplaatsen = startafstand aanpassen | dialoog Tekst op pad, canvas-slepen, inspecteur-X/Y | idem |
| S07/P06 Vaste vormen verplaatsen/schalen; formules in vormgetallen; exemplaar draaien; automatisch verdelen over platen | Structuurpaneel ("Onderdeel bewerken", "Exemplaren op platen", "Automatisch verdelen") | idem |
| S01 Maat door a = b; ellips-cirkel- en ellips-ellipsraakrelatie | schetsmodus (Raakpunt-tool, maten) | idem |

## E05 afronden en afschuinen tussen krommen

Domein: `Path::cut_corner` weigerde elke zijde die geen rechte was (`CurvedSide`). Nu rekent `corner_curved.rs` het
algemene geval. Fillet: de cirkel met straal `r` raakt beide zijden; het middelpunt ligt aan de binnenkant van beide, dus
`P(s) + rσN(s) = Q(t) + rσN'(t)` met `σ` het teken van de draairichting. Dat zijn twee vergelijkingen in de parameters `(s, t)`;
gedempte Newton vanaf een raster van 6 x 6 startpunten, de oplossing die het minst van beide zijden wegneemt wint. Afstanden
(afschuinen) zijn booglengte (Simpson, 512 stappen) vanaf de hoek; de hoek-variant schiet een straal onder de gevraagde hoek
en neemt het snijpunt het dichtst bij de hoek. Een glad hoekpunt (raaklijn-continu, ook de naad van een cirkel) is geen hoek:
`Collinear`. Een straal die er niet op past: `TooLarge`. `CornerError::CurvedSide` wordt niet meer teruggegeven.

Verlengen: `extended_cubic_subpath` volgt de eigen kubiek tot 3 parameterlengtes (zoals voorheen). Ligt de grens verder, dan
gaat het uiteinde recht door langs de eindraaklijn (`tangent_continuation`): de getekende kromme blijft exact zoals ze was,
de overgang is raaklijn-continu, de rechte komt aan op de eerste kruising. Geen grens vooruit: `NoBoundaryAhead` (ook als de
raaklijn niets raakt). Reden voor deze keuze: een kubiek ver buiten `[0, 1]` zegt niets meer over de getekende vorm (de
controlepunten groeien met de derde macht), een rechte langs de raaklijn is voorspelbaar.

| Test | Rood | Groen |
|---|---|---|
| `geometry/tests/g8_curve_corners.rs` (10: lijn-cirkel, boog-boog, lijn-Bézier, afstanden langs de kromme, gelijke afschuining tussen bogen, hoek-afschuining, typed failures, 3x verlengen voorbij het bereik) | `cargo test -p burnburn-geometry --locked --test g8_curve_corners` -> 9 FAILED, 1 ok (bewaker: grens binnen bereik volgt de kubiek nog); `CurvedSide`/`NoBoundaryAhead` i.p.v. `Ok` | 10 ok na `corner_curved.rs` + `tangent_continuation` |
| `geometry/tests/modify.rs::corner_cuts_report_typed_failures...` | bewust gewijzigd: de cirkelnaad is nu `Collinear` i.p.v. `CurvedSide` | ok |
| `editor tool::fillet::tests::{a_circle_seam_is_smooth_so_it_is_no_corner, a_corner_between_a_line_and_an_arc_is_rounded_tangent_to_both}` | de oude test (`a_line_arc_corner_is_not_available_and_says_so`) faalde op het nieuwe gedrag; de nieuwe lijn-boogtest is daarna geschreven (geen aparte rode run) | ok |
| editor `constraint::...` | zie S01 | ok |

Orakels: de tangentiecirkel van lijn en cirkel (`(cy - 10)^2 = 100 - 20 r`), van twee cirkels (`|c| = |c - (12,0)| = 10 + r`),
een onafhankelijke afstandssampler voor de Bézier (de test herhaalt de Newton-iteratie niet), raakcontinuïteit op beide
voegen, booglengte `s/R` voor afstanden op een cirkel.

## E06 stappen toevoegen en de stapel van de selectie

Presentatie: `StructureEdit::{AddModifierStep, CreateModifierChain}` met `NewModifierStep::{Offset, Boolean, Grid, Circle}` en
`BooleanKind`. Een boolean-stap neemt de overige geselecteerde objecten als operand (niet de bron, niet de eigen uitvoer van
de keten); zonder operand `NeedsOperands`. Raster/cirkel valideren via `grid_array`/`circular_array` (aantal 0 of te groot:
`InvalidCount`, hoek 0 of > 360: `InvalidSweep`); de cirkel draait de kopieën mee (`rotate_items`) om het getypte middelpunt.
`ModifierRow.in_selection` markeert de ketens waarvan de selectie bron of uitvoer is. Panelen: blok "Stap toevoegen" (soort,
alle getallen, een knop per keten, "Keten maken van de selectie") in het Structuurpaneel én een nieuw `G5Panel::Stack`
("Stapel van de selectie") dat vanzelf opent voor zo'n selectie en dezelfde regels toont.

| Test | Rood | Groen |
|---|---|---|
| `presentation/tests/g8_modifier_add.rs` (9) | stub (`CommandRejected`): 9 FAILED | 9 ok |
| `macos/tests/g8_editor_panels.rs` (8) | stub van de regels: `cargo test -p burnburn-macos --locked --test g8_editor_panels` -> 5 FAILED, 3 ok | 8 ok |
| `windows/tests/g8_editor_forms.rs` (3) | geen rode run (na de Mac-bekabeling geschreven) | ok |

**Afwijking van de opdracht:** het Stapelpaneel is een zwevend paneel dat vanzelf opent, geen ingebouwd blok in de vaste
inspecteurkolom van het documentvenster. De kolom heeft een vaste AppKit-opbouw zonder dynamische secties; een nieuw dynamisch
onderdeel erin vraagt een eigen scrollende sectie met herbouw, en dat is niet bekeken of gebouwd.

## E07 meerdere teksten tegelijk: omzetten naar pad en typen

`ShapesConvertToPath` is aan voor een selectie die uit uitvoer van één of meer teksten bestaat. Eén tekst is een
`TextCommand::ConvertToPath`; meerdere zijn één `Compound` (`CompoundLabel::ConvertTexts`). Typen (`InsertText`) verlengt elke
geselecteerde tekst (één `Set`, of één `Compound` met `CompoundLabel::EditTexts`); een tekst die de letter niet kan tekenen
laat het hele commando mislukken. Geen derde tekst meer door typen bij een meervoudige selectie.

| Test | Rood | Groen |
|---|---|---|
| `presentation/tests/g8_text_many.rs` (5) | `cargo test -p burnburn-presentation --locked --test g8_text_many` -> 2 FAILED (omzetten uit; typen maakte een derde tekst), 3 ok (bewakers) | 5 ok |

## E08 meerregelig, variabel, verplaatsen

Generators: `text_on_path` plaatst nu meerdere regels. Regel k ligt op `baseline_k - baseline_0` langs de linkernormaal
(onder de eerste), het blok begint met de meest linkse regel bij `start` en de regels houden hun uitlijning onderling
(`block_x = min start_x`). De passenseis is de breedste regel. Nieuw: `guide_frame_at` (punt en richting van de gids op een
booglengte, ook achterwaarts, ook voorbij het eind rechtdoor) en `text_room` (de grootste startafstand waarop de tekst nog past).
Variabele tekst: de gebonden evaluator werkt al op de opgeloste tekst van `bind()`; er was geen code nodig, wel een test dat de
job van de gebonden variabele tekst byte-gelijk is aan die van de gebonden letterlijke tekst (en dat het volgnummer verspringt).

Verplaatsen: `text::retarget_move` in de gemeenschappelijke commandotrechter (`apply_command`, naast `generator::retarget`).
Een zuivere verschuiving van alleen de uitvoer van een gebonden tekst wordt een `TextCommand::Set` met een nieuwe `start_mm`
(verplaatsing langs de gidsrichting op de huidige startplek, begrensd op `[0, text_room]`); wordt de gids mee verplaatst, dan
valt de tekst uit de doelen (hij volgt vanzelf); het overige blijft wat het was en wordt door het document geweigerd. Een
verplaatsing precies dwars op de gids verandert niets (en geeft de gewone weigering).

| Test | Rood | Groen |
|---|---|---|
| `generators/tests/g8_multiline_path.rs` (6) | stub (`Err(EmptyGuide)`, nog `MultiLineText`): 6 FAILED. Eerste implementatie: 4 ok, 2 FAILED: het teken van de regelverschuiving was fout (de rechte-gidstest met het orakel "gewone tekst opgeschoven" ving dat) en twee toleranties van het orakel (de cirkel is op 0,01 mm vlak gemaakt: raaklijn 0,02 rad) | 6 ok |
| `generators/tests/{bound_text,patterns}.rs`, `presentation/tests/text_toolbar.rs` | beschreven het oude "meerdere regels faalt": bewust gewijzigd | ok |
| `presentation/tests/g8_text_path.rs` (7) | `cargo test -p burnburn-presentation --locked --test g8_text_path` -> 4 FAILED (3 verplaatstests, 1 met een eigen fout orakel: hoogte > 15 mm), 3 ok (bewakers: niet gebonden, dwars op de gids, variabele job). Na de code nog 2 FAILED door mijn orakel (5,0 mm, terwijl de gids als veelhoek 4,9988 geeft: richting van de eerste koorde) | 7 ok |

## S07/P06 formules, vaste vormen, draaien, verdelen

Document: `Dim::Expr(String)` met `structure/formula.rs` (`parse_formula`, `Formula::{names, eval}`: getallen met decimale komma en
optioneel `mm`, namen = parameters, `+ - * /`, haakjes, tekens; `FormulaError::{Syntax, UnknownName, DivisionByZero}`).
`DefinePart` weigert een formule die niet parset (`InvalidValue{veld}`) of een onbekende parameter noemt (`UnknownParameter`);
een formule die geen grootte geeft laat die instantie mislukken en noemt de eerste parameter van de formule (`InvalidParameters`).
Canonical: de tag `expr` komt alleen voor als er een formule is (bestaande hashes en bestanden ongewijzigd, getest). Project:
`structure_dto::Dim::Expr { text }`.
Verdelen: `distribute_over_plates(platen, voetafdrukken, tussenruimte)`: instanties zonder plaat in rijen, hoogste eerst (dan
breedste, dan id), op de eerste plaat waar ze passen, ook een kwartslag gedraaid; wat al op een plaat staat telt eerst mee en
blijft staan. Alleen de keuze van de plaat; waar het op de plaat ligt is aan de nest.
Presentatie: `SetPartShapeField` neemt een formule (alleen als de tekst geen getal is en eruitziet als een formule; een los
woord blijft `InvalidLength`, een formule met een onbekende naam of een syntaxfout is `BadFormula`); een vaste vorm toont X, Y,
breedte en hoogte van zijn doos en verplaatst/schaalt zijn pad (`Fixed(path)` blijft; geen parameters of formules erin).
`StructureEdit::RotateInstance { instance, degrees, relative }` draait om het midden van de uitvoer (`InstanceRow.rotation`);
`DistributeOverPlates { gap }` is één `Compound` (`CompoundLabel::DistributeOverPlates`) of een weigering zonder wijziging
(`NoPlates`, `NothingToDistribute`, `NoRoomOnPlates`).

| Test | Rood | Groen |
|---|---|---|
| `document/tests/g8_part_formulas.rs` (7) | stub (`parse_formula` -> `Err(Syntax)`): 6 FAILED, 1 ok | 7 ok |
| `io/tests/g8_part_formula_roundtrip.rs` (2) | stub (`Expr` -> `Mm(0)` in de DTO): 1 FAILED, 1 ok | 2 ok |
| `document/tests/g8_distribute.rs` (8) | stub (alles onverdeeld): 7 FAILED, 1 ok (de invariantentest is met een stub leeg waar). Daarna 1 FAILED door mijn orakel (met 12 mm tussenruimte blijven twee over, niet één) | 8 ok |
| `presentation/tests/g8_parts.rs` (11) | stub (`CommandRejected`/geen velden): `cargo test -p burnburn-presentation --locked --test g8_parts` -> 10 FAILED, 1 ok | 11 ok |
| `macos/tests/g8_editor_panels_parts.rs` (5), `windows/tests/g8_editor_forms_parts.rs` (2) | Mac: 4 FAILED, 1 ok (de oude catch-all `NoCandidate` maakte de weigeringstest eerst groen; nu zegt de test dat een nieuwe weigering eigen woorden moet hebben). Windows: geen rode run | ok |

## S01 maat door a = b, ellips raakt cirkel en ellips

Solver (`burnburn-sketch`, solver zelf ongewijzigd): de rol van de assen (welke as de brandpunten draagt) wordt bepaald uit de
doelmaat van de hoofdas (driving straal of diameter) in plaats van de huidige afstand, zodat één bewerking die `a` door `b`
stuurt in één solve klopt. Zonder zo'n maat (bijvoorbeeld een afstandsmaat tussen de twee punten) probeert `evaluate_sketch`
na `NotConverged`/`Conflict` dezelfde schets nog eens met de rollen omgekeerd en neemt die als hij geldig is.
Raken: een contactpunt `T` op beide krommen en een lijn ter lengte 1 vanaf `T` loodrecht op beide normalen. Cirkel: straal-
lijn middelpunt-`T` en `PointOn`. Ellips: de normaal in `T` is de som van de eenheidsvectoren naar de brandpunten
(reflectie-eigenschap), gebouwd als parallellogram zodat de diagonaal nooit lengte nul heeft. **Gevonden onderweg:** de
spiegelbeeldconstructie die G7 voor een vrije raaklijn gebruikt is tweede orde zodra het raakpunt vastligt (het maximum van een
functie van de richting): de solver rapporteerde dan 1 vrijheidsgraad te veel en convergeerde niet. De normaalconstructie is
eerste orde. Het startpunt van het contact is het dichtstbijzijnde punt van de ellips (cirkel) of het dichtstbijzijnde punt van
de tweede ellips (ellips-ellips; zo convergeerde 18, 19, 20 en 21 mm bij 17 mm doel, en een lichte overlap).
Document: `Tangent(ellips, cirkel)` en `(ellips, ellips)` zijn toegestaan, `Equal(ellips, cirkel)` en boog blijven geweigerd.

| Test | Rood | Groen |
|---|---|---|
| `sketch/tests/g8_ellipse_swap_tangent.rs` (11) | `cargo test -p burnburn-sketch --locked --test g8_ellipse_swap_tangent` -> 11 FAILED (`WrongEntityKind`; en de maat door a = b: de oude bewerking liet de eerdere uitvoer staan). Eerste implementatie: 8 ok, 3 FAILED met het raakpunt op het uiteinde van de hoofdas (dof 1 te veel of `NotConverged`: tweede orde). Na de normaalconstructie 10 ok, 1 FAILED (twee ellipsen einde op einde, `NotConverged`); parallellogram met gemeenschappelijk middelpunt in plaats van `Parallel`+`Equal` (die lieten een onechte oplossing toe: 16,265 i.p.v. 17); startpunt op de tweede ellips | 11 ok |
| `sketch/tests/{ellipse,g7_ellipse_relations}.rs`, editor `constraint::...` | beschreven "ellips nooit raak aan cirkel": bewust gewijzigd | ok |

## Wijzigingen buiten de lane (alleen additief tenzij vermeld)

- `burnburn-document`: `Dim::Expr`, `structure::{Formula, FormulaError, parse_formula, Footprint, PlateRoom, Distribution,
  distribute_over_plates}`, `CompoundLabel::{ConvertTexts, DistributeOverPlates}`, sketchmodel staat `Tangent` ellips-cirkel/ellips toe.
- `burnburn-generators`: `guide_frame_at`, `text_room`; `text_on_path` accepteert meerdere regels (`GenError::MultiLineText`
  betekent nu "geen regel").
- `burnburn-io`: `structure_dto::Dim::Expr`.
- gedeeld: `StructureEdit::{AddModifierStep, CreateModifierChain, RotateInstance, DistributeOverPlates}`, `NewModifierStep`,
  `BooleanKind`, `ModifierRow.in_selection`, `InstanceRow.rotation`, `StructureProblem::{NeedsOperands, InvalidSweep,
  BadFormula, NoPlates, NothingToDistribute, NoRoomOnPlates}`, `G5Panel::Stack` (`G5Panel::ALL` is nu 13), `EditorControl`-
  varianten, `G7Text`-teksten in `l10n_g7_editor.rs`, undo-teksten in `l10n.rs`.
- Bestaande tests bewust aangepast: zie de tabellen (modify, fillet, constraint, bound_text, patterns, text_toolbar, ellipse,
  g7_ellipse_relations).

## Handmatig / onbevestigd

- Uiterlijk van het Stapelpaneel (zwevend, opent vanzelf) en van de langere Structuurpanelen; het slepen van een gebonden tekst
  met de muis (getest via de inspecteur-X/Y, die dezelfde `Transform` stuurt); gebogen meerregelige tekst op het canvas.
- Windows: alleen type-check, clippy en de headless formuliertests; nieuwe Win32-code is er niet.
- De native AppKit-test (`tests/native/macos/g7_editor_panels.rs`) is niet uitgebreid voor het Stapelpaneel.

## Verificatie

Op f6fe45e: `cargo fmt --all` schoon; `cargo clippy --workspace --all-targets --locked -- -D warnings` schoon (de eerste verify-run faalde op een ongebruikte import in `editor tool/fillet.rs`, verwijderd); `cargo clippy -p burnburn-windows --target x86_64-pc-windows-msvc --all-targets --locked -- -D warnings` schoon; `cargo xtask verify` exit 0 (4513 tests geslaagd, 0 gefaald, 11 genegeerd).
