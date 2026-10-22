# G7-editor (golf 7): E05, E06, E07/E08, S07/P06, S01

Lane G7-editor. Basis 40c71b9. Alleen werkelijk uitgevoerde commando's. Toolchain 1.99.1, `--locked`,
`CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/g7-editor`, `CARGO_INCREMENTAL=0`. macOS arm64; geen hardware, geen
netwerk, Windows nooit gedraaid. Een red-fase telt alleen als de test na een minimale stub op gedrag faalde. Waar er geen
rode run is (test na de code geschreven, of een bewaker die vanaf het begin groen was) staat dat erbij.

## Wat er bedienbaar is

| Feature | macOS | Windows |
|---|---|---|
| E05 Verlengen van een Bézier-uiteinde langs de eigen kubiek tot een rechte of kromme grens (gereedschap Verlengen); Trim tegen een kromme snijlijn bestond al in de geometrie en is nu met een test vastgelegd | canvas (bestaand gereedschap) | idem |
| E05 Afrondingsstraal en afschuinafstanden/-hoek als gereedschapsopties (rij "Straal (mm)", "Afstand (mm)", "Tweede afstand", "Hoek"), onthouden bij gereedschapswissel; één undo per klik | Gereedschapsopties-sectie van het vormenpaneel | inspector-rijen |
| E06 Modifierstapel: ketens met stappen in volgorde, stap aan/uit, getal bewerken (aantal of afstand), omhoog/omlaag, stap en keten verwijderen, Toepassen (bakken: uitvoer wordt gewone objecten, keten weg, bron blijft), offsetketen op de selectie | sectie "Modifierstapel" in het Structuurpaneel (Venster ▸ Structuur) | idem (formulier) |
| S07/P06 Platen maken, alle velden bewerken, verwijderen, selectie toewijzen of losmaken, exemplaar per plaat kiezen; variant hernoemen/verwijderen; vormen van een onderdeel tonen, getal bewerken (mm of parameternaam), hernoemen, toevoegen (rechthoek/cirkel), verwijderen | secties "Platen beheren" en "Onderdeel bewerken" in het Structuurpaneel | idem |
| E07 Meerdere teksten tegelijk: één toolbar voor alle geselecteerde teksten, verschillende waarden leeg/gemarkeerd, een keuze geldt voor allemaal, één undo-stap | tekstpaneel (bestaand) | tekstformulier (bestaand) |
| E08 Tekst op pad als live binding: dialoog bindt standaard ("Tekst blijft het pad volgen", uit = gewone paden zoals voorheen), de tekst volgt het pad bij verplaatsen/vervormen, paneel "Tekst op pad" (start, afstand, omkeren, vervormen, losmaken) opent vanzelf voor een gebonden tekst | tekst-sheet + `G5Panel::TextPath` | idem |
| S01 Kleine as als sturende maat (ook via een naamparameter), punt-op-ellips en raaklijn-aan-ellips als relaties; gereedschap Maat heeft de optie "Kleine as van een ellips meten" | schetsmodus, constraintlijst ("Kleine as") | idem |

## E05 verlengen en afronden

Domein: `Path::extend_end` zette een kubiek-uiteinde altijd door als `NotExtendable`. Nu wordt het uiteinde langs hetzelfde
derdegraadspolynoom voorbij `t = 1` gevolgd (blossoming: de kubiek over `[0, 1 + δ]` is weer één kubiek) tot de eerste
kruising met een grens (rechte, boog of kromme). Het bereik is `BEZIER_REACH` = 3 parameterlengtes; daarna `NoBoundaryAhead`.

| Test | Rood | Groen |
|---|---|---|
| `geometry/tests/g7_extend.rs` (6: lijn tot cirkel, lijn getrimd door cirkel, Bézier getrimd bij kromme snijlijn, Bézier-einde en -begin verlengd tot muur en cirkel, niets vooruit) | `cargo test -p burnburn-geometry --locked --test g7_extend` -> 4 FAILED, 2 ok (`NotExtendable` i.p.v. `Ok`). De trimtest faalde eerst op een orakelfout van mij (de cirkel kruist de parabool tweemaal; een klik vóór de eerste kruising houdt het deel erna): orakel gecorrigeerd, daarna groen zonder codewijziging (bewaker) | 6 ok na `extended_cubic_subpath` |
| `editor extend::{a_bezier_end_extends_along_its_own_curve, a_bezier_end_with_nothing_ahead_is_a_typed_refusal}` | geen aparte rode run: vervangt `bezier_ends_are_reported_as_unavailable` (bewust gewijzigd gedrag, geometrie ging voor) | ok |
| `presentation g7_trim_fillet` (6) | `cargo test -p burnburn-presentation --locked --test g7_trim_fillet` -> 4 FAILED (rij ontbreekt), 2 ok (verlengen en trimmen via de UI waren door de geometriewijziging al groen). Orakelfout gevonden: een rechte hoek verliest `r²(1 − π/4)`, niet `r²(4 − π)` (394,63 vs 378,54) | 6 ok na `ParamKey::{FilletRadius, ChamferDistance, ChamferDistance2, ChamferAngle}` |
| `macos/tests/g7_editor_panels` (3), `windows/tests/g7_editor_forms` (2) | geen rode run: na de bekabeling geschreven | ok |

## E06 modifierstapel

Domein (additief): `StructureCommand::BakeModifier` (effect `Detach`: de uitvoer wordt gewone objecten met dezelfde id's,
de keten verdwijnt; `detach_owner_outputs` hergebruikt de tekst-"omzetten naar pad"). Presentatie: `StructureViewModel.modifiers`
(`ModifierRow/ModifierStepRow`, status per keten, mislukte stap genoemd) en `StructureEdit::{ToggleModifierStep,
MoveModifierStep, RemoveModifierStep, SetModifierStepValue, RemoveModifier, BakeModifier, CreateOffsetModifier}`. Het getal van een
stap is het aantal (lineair, pad, cirkel) of de offsetafstand (met eenheid); een raster en een boolean hebben geen enkel getal
(`StepHasNoValue`). De laatste stap blijft (`LastStep`).

| Test | Rood | Groen |
|---|---|---|
| `document/tests/g7_bake_modifier.rs` (4) | stub (`BakeModifier` gaf `NothingChanged`): 4 FAILED | 4 ok |
| `presentation/tests/g7_modifiers.rs` (12) | stub (edits geweigerd): 12 FAILED. Orakelfout in de breedtes (52 mm i.p.v. 92) vóór de implementatie gecorrigeerd | 12 ok |
| `macos/tests/g7_editor_panels` (5 in sectie E06), `windows/tests/g7_editor_forms::stack` (2) | geen rode run | ok |

Afwijking van de opdracht: de stapel staat in het **Structuurpaneel** (zwevend, gedeeld door beide hosts), niet in de
inspecteur van de Mac-app; dat vraagt geen AppKit-code en geeft Windows hetzelfde. Een stap *toevoegen* kan alleen via
Schikken ▸ Reeks… (arrays) en via "Offsetketen maken" op de selectie.

## S07/P06 platen, varianten, vormen van een onderdeel

Domein (additief): `StructureCommand::RenameVariant` (definitie en exemplaren in één transactie). Presentatie:
`StructureEdit::{CreatePlate, SetPlateField, RemovePlate, AssignSelectionToPlate, SetInstancePlate, RenameVariant, RemoveVariant,
AddPartShape, RemovePartShape, RenamePartShape, SetPartShapeField}`, `PartRow.shapes`, `InstanceRow.plate_id`. Een getal van een
vorm die de naam van een parameter is wordt `Dim::Param`, anders een lengte; een variant in gebruik wordt niet verwijderd
(`VariantInUse`); de omtrek van een onderdeel gaat niet zelf op een plaat (`PartOutputSelected`: zet het exemplaar erop).

| Test | Rood | Groen |
|---|---|---|
| `document/tests/g7_variant_rename.rs` (3) | stub: 3 FAILED | 3 ok |
| `presentation/tests/g7_plates_parts.rs` (15) | stub: 15 FAILED | 15 ok (een orakel gewijzigd: `NotPositive` is de bestaande `InvalidLength(NotPositive)`) |
| `macos/tests/g7_editor_panels` (6), `windows/tests/g7_editor_forms::plates_parts` (3) | geen rode run | ok |

## E07 meerdere teksten tegelijk

`selected_texts` (alle uitvoerobjecten van meerdere tekstobjecten, niets anders), `TextToolbarViewModel.{count, mixed}`
(`TextMixed`: welke waarden verschillen). Een bewerking wordt op een kopie van elke tekst toegepast; faalt het voor één, dan
verandert er niets (ook geen deelresultaat). Eén tekst die verandert is een `TextCommand::Set`, meerdere zijn één
`Compound` (`CompoundLabel::EditTexts`) = één undo-stap. Mac-paneel: gemengde velden zijn leeg, een lege invoer op een gemengd
veld is geen bewerking, een keuze op een gemengd keuzeveld geldt voor alle teksten.

| Test | Rood | Groen |
|---|---|---|
| `presentation/tests/g7_text_many.rs` (10) | stub (count 1, bewerking op één tekst): 9 FAILED, 1 ok (bewaker). Daarna een fixturefout: enkele-lijn-tekst kent alleen hoofdletters (de lowercase-uitvoer verdween) | 10 ok |
| `text_toolbar::two_texts_selected_share_one_toolbar_and_an_edit_changes_both` | de oude test (`..._show_no_toolbar_...`) beschreef het oude gedrag: bewust gewijzigd | ok |
| `macos/tests/g7_editor_panels::several_texts` (3), `windows/tests/g7_editor_forms::several_texts` (2) | stub van de velden: 3 FAILED | ok |

## E08 tekst op pad als live binding

Domein: `TextObject.path: Option<PathBinding { guide, start_mm, offset_mm, reverse, warp }>` (canonical alleen als gezet,
projectbestand `path_binding`, afwezig = ongebonden: bestaande hashes veranderen niet). Sessie: een tekst waarvan de gids in
`changes.{created, modified, deleted}` staat wordt in dezelfde transactie opnieuw geëvalueerd (`texts_following`), ook als de
gids door een modifier/onderdeel pas na de tekstevaluatie verandert (`evaluate_following_texts`). Een gids die weg is of de
tekst niet draagt: status `Failed`, laatste omtrek blijft, `production_blocked`. Een tekst kan zijn eigen omtrek niet volgen
(`TextError::GuideIsOwnOutput`). `burnburn_generators::BoundTextEvaluator` (vervangt `TextEvaluator` in de controller)
gebruikt `text_on_path`; ongebonden tekst gaat ongewijzigd door `TextEvaluator`.

| Test | Rood | Groen |
|---|---|---|
| `io/tests/g7_text_path_roundtrip.rs` (4) | stub (DTO kende het veld niet): 3 FAILED, 1 ok | 4 ok |
| `document/tests/g7_text_follows_guide.rs` (9) | 8 FAILED, 1 ok | 9 ok |
| `generators/tests/bound_text.rs` (9) | stub (delegeert): 7 FAILED, 2 ok (bewakers; de enkele-lijn-test is daarna versterkt met een straalcontrole). Cirkeltest: de evaluator vlakt de gids af op 0,01 mm, orakeltolerantie 1,2e-2 | 9 ok |
| `presentation/tests/g7_text_path.rs` (12) | stub: 9 FAILED, 3 ok (leeg/vacuous). Fixturefout: een cirkel van 5 mm draagt "HIT" nog; 3 mm niet | 12 ok |
| `text_toolbar::{rigid_text_on_a_circle_..., text_on_a_path_start_offset_...}` | bestaande tests gingen uit van "Toepassen maakt een gewoon pad": nu `Live(false)` in de test (de dialoog bindt standaard) | ok |
| `macos/tests/g7_editor_panels::text_on_path` (4) | stub: 4 FAILED | ok |
| `windows/tests/g7_editor_forms::text_on_path` (3) | geen rode run | ok |
| native `tests/native/macos/g7_editor_panels.rs` (2, `harness = false`) en aangepaste `text_panels.rs` | geen rode run (na de bekabeling); echte vensters: structuurpaneel (keten maken, stap uit, toepassen, plaat maken) en padpaneel (opent vanzelf, start 15, losmaken sluit het) | ok |
| `scenarios/tests/g7_bound_text_job.rs` (3) | geen rode run | ok: bent outline = document = RD-snijbeweging (4 µm), een gecompileerde job (snapshot revisie n) blijft byte-identiek terwijl de gids verschuift, de nieuwe revisie compileert naar de verschoven tekst |

**Gevonden gat (niet van deze lane):** `DocumentSnapshot::production_blocked` is "CAM must refuse to export it" maar noch
`burnburn_cam::compile` noch `preflight` leest het. Een tekst zonder gids (of elk ander mislukt afgeleid object) wordt dus nog
gesneden met de laatste omtrek. Zie de wachtrij-regel voor G7-cam/integratie; de scenariotest pint alleen de vlag.

## S01 kleine as, punt-op-ellips, raaklijn

De opgeloste solver (`sketch-solver` 0.1.0, vast) kent geen ellips. Een ellips wordt voor de solver zijn hoofdas (zoals in
golf 6). Een ellips die een kleine-as-maat, een punt of een raaklijn heeft krijgt hulpgeometrie (`EllipseAux`): een punt aan
het eind van de kleine as (loodrecht, lengte = kleine straal), de twee brandpunten (op de langste as, `|F1 B| = a` zodat
`c = √(a² − b²)`), en de spiegel van het hoofdeinde (zodat `2a` een lengte is). Een punt ligt op de ellips als `|PF1| + |PF2| = 2a`
(hulppunt Q op de straal F1→P op afstand `2a`, `|PQ| = |PF2|`); een rechte raakt als de spiegel van F1 in die rechte op afstand `2a` van
F2 ligt. Alleen bestaande solverconstraints (symmetrisch, loodrecht, evenwijdig, gelijk). Eén documentconstraint kan zo meerdere
solverconstraints worden; de diagnose neemt de slechtste toestand. Zonder maat blijft de getekende kleine straal vast (geen extra
vrijheidsgraad, dus bestaande schetsen veranderen niet). Is de kleine straal groter dan de grote, dan wisselen de assen van rol
(`swapped`). `SketchSolution.minors` bewaart de opgeloste kleine straal alleen als die van de getekende afwijkt (hash en bestand
ongewijzigd voor oude schetsen); de editor-overlay en de volgende solve beginnen daar.

Beperking: loopt een maat of relatie tijdens één wijziging door `a = b` heen (de grote en kleine as wisselen), dan kan die ene
solve niet convergeren (status `NotConverged`); de gebruiker past de maat in twee stappen aan. Een cirkel (`a = b`) heeft
samenvallende brandpunten; de hulpgeometrie begint dan met een kleine afstand en dat convergeert in de tests, maar is niet bewezen
voor elke maat.

| Test | Rood | Groen |
|---|---|---|
| `sketch/tests/g7_ellipse_relations.rs` (20) | `cargo test -p burnburn-sketch --locked --test g7_ellipse_relations` -> 12 FAILED, 4 ok (3 refusals en een vacuous conflict). Eerste implementatie: 13 ok, 3 FAILED (nul als kleine as werd geaccepteerd; kleine as groter dan grote niet oplosbaar) -> `Negative` voor ≤ 0 en de rolwissel | 20 ok (opgeslagen kleine straal, bit-gelijk, conflict noemt constraints) |
| `sketch/tests/ellipse.rs::an_ellipse_is_never_equal_to_a_circle_and_only_a_line_is_tangent_to_it` | de oude test (`..._never_tangent`) beschreef het oude gedrag: bewust gewijzigd | ok |
| `editor constraint::an_ellipse_is_an_axis_and_a_centre_a_point_lies_on_it_a_line_touches_it` | `cargo test -p burnburn-editor --locked --lib constraint` -> 1 FAILED | ok |
| `editor dimension::with_the_minor_axis_option_...`, `overlays::the_minor_axis_dimension_runs_across_...` | geen aparte rode run voor de dimensietool (test en code in één stap); de overlay-test was rood (0 maatlijnen) | ok |
| `io/tests/g7_sketch_ellipse_relations_roundtrip.rs` (6) | de DTO-code stond er al vóór de test: niet rood gezien (zonder DTO zou de maat stil verdwijnen) | 6 ok |
| `presentation/tests/g7_ellipse_ui.rs` (5) | eerste run 4 FAILED, maar door een testfout (lijnen tekenen met klikken i.p.v. slepen) en ontbrekende bekabeling; de relaties raaklijn/punt-op-ellips zijn niet apart rood gezien | 5 ok |
| `macos/tests/g7_editor_panels::ellipse` (3), `windows/tests/g7_editor_forms::ellipse` (1) | geen rode run | ok |

## Wijzigingen buiten de lane (alleen additief)

- `burnburn-document`: `StructureCommand::{BakeModifier, RenameVariant}`, `CompoundLabel::EditTexts`, `TextObject.path`,
  `TextError::GuideIsOwnOutput`, `DimensionKind::MinorDiameter`, `SketchSolution.minors`; session: volgen van gidsen.
- `burnburn-io`: `text_dto::PathBinding`, `sketch_dto::Kind::MinorDiameter`, `Solution.minors`.
- gedeeld: `ParamKey::{FilletRadius, ChamferDistance, ChamferDistance2, ChamferAngle, MinorAxis}`, `StructureEdit`-varianten,
  `G5Panel::TextPath` (`G5Panel::ALL` is nu 9), `G5Control::Editor(EditorControl)`, `Slot::PathLive` (`Slot::ALL` 48),
  `HostText::Param*`, `SketchConstraintKind::MinorDiameter`; nieuwe bestanden `l10n_g7_editor.rs`, `panels/g7_editor.rs`.
- Bestaande tests bewust aangepast: `text_toolbar` (3), `sketch/tests/ellipse.rs` (1), editor `constraint` (1), native `text_panels`.

## Handmatig / onbevestigd

- Uiterlijk (licht/donker) van het langere Structuurpaneel (modifierstapel, platen, onderdeelvormen) en het nieuwe padpaneel; de
  gebogen tekst op het canvas; maatlijn van de kleine as.
- Windows: alleen type-check en clippy voor `x86_64-pc-windows-msvc` en de headless formuliertests; de Win32-controls zijn niet
  gedraaid (de nieuwe regels gebruiken bestaande `FormLine`-soorten, geen nieuwe Win32-code).
- Gereedschap Maat in de AppKit-werkbalk: geen aparte knop voor de kleine as, alleen de optie; geen sneltoets.

## Verificatie

Zie het eindrapport van de lane voor de uitkomst van `cargo fmt`, `cargo clippy --workspace --all-targets --locked -- -D warnings`
(schoon), clippy voor `x86_64-pc-windows-msvc` (schoon) en `cargo xtask verify`.
