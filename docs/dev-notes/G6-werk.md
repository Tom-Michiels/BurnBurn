# G6-werk (golf 6): trace-voorbereiding, onderdelen en referenties, productienesting, restplaten

Omgeving: `export PATH="$HOME/.cargo/bin:$PATH" CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/g6-werk CARGO_INCREMENTAL=0`,
`cargo ... --locked`, macOS arm64, toolchain 1.99.1. Basis: 481966e (main na golf 5). Geen hardware, geen netwerk. Een red-fase
telt alleen als de test na een minimale stub op gedrag faalde (niet op een compilerfout). Waar een test na de implementatie is
geschreven of vanaf de eerste run groen was, staat dat expliciet in de tabel ("geen rode run").

## Wat er is

| Story | Wat bedienbaar is | Waar |
|---|---|---|
| C12 | Schikken-paneel: spiegelen als keuze (standaard verboden), aantal platen, rest per plaat (gebruikt/vrij in cm²), orderstatus per onderdeel, complete sets | `schikken.rs`, `nest_task.rs`, `controller/nest.rs`; paneel in `macos/panels/schikken.rs` (Windows idem via het gedeelde model) |
| T04 | Operatorbeslissingen van een centerline-trace (knooppunt, bevestigd primitief, verbinding) staan in het recept (`centerline.decisions`) en worden bij opnieuw tracen/heropenen toegepast; wat niet meer past wordt geteld en getoond | `burnburn-trace` (`centerline/decisions.rs`, `document.rs`), `controller/trace*.rs` |
| T03 | Paneel Beeld ▸ Beeld voorbereiden: lokale drempel (venster, verschil) naast de globale, vierpuntscorrectie (hoeken typen, vlaktransformatie + restfout getoond), maskers behouden/verwijderen. Alles in het recept; de bronpixels blijven ongemoeid | `trace_prep.rs`, `controller/trace_prep.rs`, `macos/panels/g6_werk.rs` |
| T05 | Lokale hertrace van één gebied met eigen drempel en vaste punten; correcties staan in het recept (`trace.corrections`), worden bij heropenen/opnieuw tracen toegepast; verloren bronkoppeling wordt gemeld en niet elders toegepast | `burnburn-trace` (`document.rs`, `recipes.rs::replay_corrections`), `task.rs` (`TraceCorrected`, `CorrectRegion`) |
| T06 | Rapport: fit-tolerantie, grootste afwijking + 95 %-waarde, de slechtste segmenten met plaats (mm), weggelaten onderdelen/gaten met plaats en oppervlak (ook een gat in een ring) | `burnburn-trace::deviation_summary`, `controller/trace_prep.rs::report_of` |
| S07 | Structuurpaneel: onderdeel maken uit de selectie (rechthoek/cirkel parametrisch, rest vast), exemplaar plaatsen op getypte positie, standaardwaarde per parameter, varianten toevoegen en waarden geven, exemplaar/onderdeel verwijderen (onderdeel pas zonder exemplaren) | `burnburn-document::structure::part_from_objects`, `controller/structure.rs` |
| S06 | Rand van het enige geselecteerde object kiezen en benoemen als referentie, referentie verwijderen, dubbelzinnige referentie opnieuw koppelen door een kandidaat te kiezen (nooit automatisch); gebroken koppeling blijft zichtbaar | idem |
| W08 | Paneel Venster ▸ Operatorweergave: machine (status uit bewijs), goedgekeurde jobs met revisie, afmeting op het bed en materiaal, orders met voltooide aantallen, de knoppen van de machineservice ongewijzigd doorgestuurd. `DocumentViewModel.operator` op elk venster | `view_model.rs`, `controller/{machine,view}.rs`, `macos/panels/g6_werk.rs` |
| W07 | Nesten op een restplaat: een gesloten object als plaat, de rest van het blijvende werk is uitsluitingsgebied | `nest_task.rs` (`take_sheet`), `controller/nest.rs` |

Beide hosts: één headless paneelmodel (`G5Panel::TracePrep`, uitgebreid `G5Panel::Structure`, uitgebreid Schikken-paneel), door de
Windows-crate via `#[path]` meegecompileerd. Nieuw menu-item `MenuItem::TracePrepPanel` (tag/id 1032) in het Beeld-menu van beide
hosts. Teksten: `l10n_g6_werk.rs` (NL/EN, nieuw bestand, ook door Windows meegecompileerd).

## TDD-log

| Test | Red (verwacht, echt gezien) | Red-commando | Green |
|---|---|---|---|
| C12 `presentation g6_nest_production` (7): `the_report_says_per_sheet_…`, `a_second_sheet_takes_what_the_first_cannot_hold_…`, `order_status_counts_ordered_and_placed_…`; plus 4 optie-/validatietests | stub gaf lege `sheets`/`parts`, één plaat: 3 FAILED (`left: 0 right: 1`, `(1, 2, 0)` i.p.v. `(2, 1, 2)`); 4 waren door de aanwezige velden al groen (optieplomp, geen rode run) | `cargo test -p burnburn-presentation --locked --test g6_nest_production` | 7 ok |
| C12 `nest_task::tests::a_part_that_fits_only_mirrored_is_placed_mirrored_…` (chirale driehoek; transform is een spiegeling) | geen rode run: na de implementatie geschreven | `cargo test -p burnburn-presentation --locked --lib nest_task` | ok |
| C12 `macos schikken_panel::the_nest_panel_offers_mirroring_and_sheets_…` | geen rode run: tegelijk met het model geschreven | `cargo test -p burnburn-macos --locked --test schikken_panel` | 12 ok |
| T04 `trace centerline_decisions` (5): junction in recept + herhaalbaar, primitief + verbinding in volgorde, onpassende beslissing overgeslagen, beschadigde lijst geweigerd | stubs (`decisions()` leeg, `replay` no-op, `from_engine` leeg): 4 FAILED, 1 ok (`…make_it_differ…` was al groen via `centerline.edits`) | `cargo test -p burnburn-trace --locked --test centerline_decisions` | 5 ok |
| T04 `centerline_decisions::a_join_of_two_stroke_ends_is_stored_and_replayed` | geen rode run: na de implementatie toegevoegd | idem | ok |
| T04 `presentation g6_trace_decisions` (4) | 4 FAILED (heropend: beslissing weg, `dropped` 0, `edits` 1 i.p.v. 2) | `cargo test -p burnburn-presentation --locked --test g6_trace_decisions` | 4 ok |
| T04 `macos g5_panels::a_stored_decision_that_no_longer_fits_is_said_in_the_panel` | geen rode run: bij het paneel geschreven | `cargo test -p burnburn-macos --locked --test g5_panels` | 20 ok |
| T03 `presentation g6_trace_prep` (8) | stub-intent deed niets: 8 FAILED (`p.perspective.on`, `issue` None, `("","")`…). Eerste fixture-oracle voor de globale drempel klopte niet (donkerder objecten nodig) en is aangepast vóór groen | `cargo test -p burnburn-presentation --locked --test g6_trace_prep` | 8 ok |
| T05 `trace correction_recipe` (6): bewaren + terugkijken, opnieuw tracen geeft dezelfde geometrie, verloren koppeling gemeld, beschadigde lijst | stubs: 5 FAILED, 1 ok (`a_plain_trace_stores_no_corrections`, vacuous) | `cargo test -p burnburn-trace --locked --test correction_recipe` | 6 ok |
| T05 `presentation g6_trace_correction` (5) | stub-intent: 5 FAILED (`NothingToCorrect` i.p.v. resultaat). Eén oracle (`assert_ne` van gelijke tellingen) was fout en is vervangen door een geometrievergelijking | `cargo test -p burnburn-presentation --locked --test g6_trace_correction` | 5 ok |
| T06 `trace deviation_summary` (3): gesorteerd en gelijk aan `deviation_map`, groter budget geeft grotere afwijking, `top = 0` | stub gaf nullen: 3 FAILED. De bewering "een budget van 0,02 mm wordt gehaald" bleek onjuist (waargenomen 0,05 mm op deze pixelgrootte) en is vervangen door wat het domein werkelijk levert | `cargo test -p burnburn-trace --locked --test deviation_summary` | 3 ok |
| T06 `presentation g6_trace_report` (4) | `report: None`: 3 FAILED, 1 ok (`…without_a_contour_result`, vacuous) | `cargo test -p burnburn-presentation --locked --test g6_trace_report` | 4 ok |
| T03/T05/T06 hosts: `macos g6_werk_panels` (6), `windows g6_forms` (4), native `g5_panels_native::trace_prep_panel_adds_a_mask_and_a_local_threshold` | geen rode run: tegelijk met het model geschreven | `cargo test -p burnburn-macos --locked --test g6_werk_panels` / `--test g5_panels_native`; `cargo test -p burnburn-windows --locked --test g6_forms` | ok |
| S07 `document part_from_objects` (4) | stub gaf `Err(NoObjects)`: 4 FAILED | `cargo test -p burnburn-document --locked --test part_from_objects` | 4 ok |
| S06/S07 `presentation g6_structure` (8) | stub deed niets: 8 FAILED (geen onderdeel, geen referentie, `problem` None) | `cargo test -p burnburn-presentation --locked --test g6_structure` | 8 ok |
| S06/S07 hosts: `macos g6_werk_structure` (4), `windows g6_forms::the_structure_form_…`, native `g5_panels_native::structure_panel_defines_places_and_references` | geen rode run: tegelijk met het model geschreven | zie boven | ok |
| W07 `presentation g6_nest_remnant` (5) | stub negeerde `sheet_object`: 4 FAILED, 1 ok (`another_object_that_stays_is_an_exclusion_zone…` was vacuous op een groot bed; daarna aangescherpt met "op de restplaat", niet opnieuw rood gezien) | `cargo test -p burnburn-presentation --locked --test g6_nest_remnant` | 5 ok |
| W07 hosts: `macos g6_werk_nest` (2) | geen rode run | `cargo test -p burnburn-macos --locked --test g6_werk_nest` | ok |
| W08 `presentation g6_operator` (4) + `controller::machine::tests::the_material_is_named_only_when_the_plates_agree` | stub gaf `bounds: None`/`material: None`: 1 FAILED (`the plan has bounds`) en de eenheidstest FAILED (`left: None right: Some("Berken")`); de andere 3 waren vacuous groen (bevroren job: eerst `before.bounds.is_some()` toegevoegd, dat was nog niet rood gezien) | `cargo test -p burnburn-presentation --locked --test g6_operator` en `--lib the_material_is_named` | 4 ok + 1 ok |
| W08 hosts: `macos g6_werk_operator` (4), `windows g6_operator_form` (2) | geen rode run: tegelijk met het model geschreven | `cargo test -p burnburn-macos --locked --test g6_werk_operator`; `cargo test -p burnburn-windows --locked --test g6_operator_form` | ok |

## Ontwerpkeuzes en aannames

- **Centerline-beslissingen in het recept.** Elke beslissing krijgt een plek (positie in mm van het knooppunt, de eerste punt van de
  keten, de twee uiteinden) en wordt alleen daar toegepast; bij een andere schaal of afbeelding valt ze weg en wordt geteld
  (`CenterlineEditViewModel.dropped`), zodat ze nooit op een ander knooppunt landt. De lijst in gebruik (ook onpassende) blijft in het
  tracervenster, zodat een tijdelijk ander getal (bij het typen van de dpi) niets definitief weggooit; "Pas toe" bewaart alleen de
  toegepaste beslissingen. Positie-tolerantie 1e-4 mm: de engine is deterministisch.
- **Correcties in het recept.** `trace.corrections` bewaart gebied, lokaal recept, ankers, beschermde contouren en de bindingen waarop
  de correctie gemaakt is. Bij terugkijken is `base_inputs` een nephash, zodat altijd de bindingen gecontroleerd worden
  (`reapply_correction`); bij een andere basis wordt een correctie die haar contouren kwijt is gemeld (`NeedsRelink`) en niet toegepast.
  "Opnieuw koppelen" is: verwijderen en opnieuw maken (er is geen herkoppelroute in het domein).
- **Voorbereiding is typen, niet wijzen.** Hoeken, maskers en gebieden worden als pixelgetallen getypt (DR-05); wijzen in het beeld
  (kwast, hoekpunten klikken) is niet gebouwd. Maskers zijn rechthoeken (`Behouden` = Include, `Verwijderen` = Exclude).
- **Rapport tolerantie.** Een fit-tolerantie onder een halve pixel is op de pixelgrens niet te halen; het rapport zegt wat het is en
  meldt "groter dan de tolerantie" in plaats van het budget te herhalen. Berekend bij het afleveren van een resultaat op de
  hoofddraad (O(bronpunten), veel lichter dan de trace zelf).
- **Onderdeel uit selectie.** Rechthoek/cirkel worden alleen parametrisch als de omtrek echt een rechthoek/cirkel is (oppervlak-
  vergelijking, 1e-3), anders `Fixed`. Het frame begint op de linkeronderhoek van de selectie; een exemplaar op `origin` valt op de
  objecten. De objecten blijven staan (niets vervangen).
- **Referentie kiezen uit een lijst** van de randen van het ene geselecteerde object (lengte in mm), niet door op het canvas te wijzen.
- **Restplaat = gesloten object.** Geen duurzame restenbibliotheek; materiaal/dikte zouden daar thuishoren. Meerdere platen naast
  elkaar liggen in documentcoördinaten (10 mm tussenruimte) en vallen dus buiten het bed; de gebruiker schuift ze naar de machine.
- `UiIntent::TracePrep` blijft klein door de hoeken te boxen (clippy `large_enum_variant`).

## Open / onbevestigd

- **Handmatig**: uiterlijk van de nieuwe panelen; hoe het aanvoelt om hoeken/gebieden te typen; Windows-run van de formulieren en het
  menu-item (alleen headless + typecheck/clippy `--target x86_64-pc-windows-msvc`).
- C12: geen aantal exemplaren per onderdeel (elk object is één onderdeel), dus "niet-geplaatste aantallen" is per object; common-line
  cutting en korrelrichting niet in de UI.
- T03: geen kwast/wijzen op het beeld; geen lensvervorming; T05: beschermde contouren en knooppuntdoel niet in de UI; T06: beschermde
  gaten/hoeken niet te kiezen (alleen het "negeer kleiner dan"-filter), verwijderde delen worden gemeld maar niet teruggezet per stuk.
- S06/S07: platen aanmaken/toewijzen, vormen van een onderdeel bewerken, variant hernoemen/verwijderen, exemplaar draaien: niet in de UI.
- W04 (kalibraties bewaren: vraagt een DTO voor `LensCalibration`/`WorkspaceAlignment`, de acceptatie van W04 zelf was al gedekt) en W06 (herkenning: template-localisatie met beoordeling van meerdere treffers; vraagt een eigen algoritme, aanleerflow en kaartkoppeling) zijn niet gebouwd. W05 blijft wat het was (hardwarebewijs handmatig).
- W08: tweede scherm/proces (IPC) niet gebouwd; het paneel is lokaal. Materiaal komt uit de platen van het bevroren project en is leeg bij een mix of zonder platen. Native AppKit-run van dit paneel apart is niet gedaan (alleen de algemene G5-paneelroute is native getest, via de andere panelen).
- W07: duurzame restenbibliotheek (materiaal, dikte), open aantallen/exemplaren in de nest, orderkoppeling met een restplaat.
