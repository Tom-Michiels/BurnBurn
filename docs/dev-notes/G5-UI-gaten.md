# G5-UI-gaten (golf 5, lane: resterende software-gaten P, E, S, T, W, X)

Omgeving: `export PATH="$HOME/.cargo/bin:$PATH" CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/g5-gaten CARGO_INCREMENTAL=0`,
`cargo ... --locked`, macOS arm64. Basis: be65aea (golf 4). Tijdens het werk liep de schijf een keer vol (andere lanes);
de eigen `debug/incremental` is toen verwijderd en incrementeel bouwen staat uit. Een red-fase telt alleen als de test na een
minimale stub op gedrag faalde (niet op een compilerfout).

## TDD-log

| Test | Verwachte fout (red) | Red-commando en waargenomen resultaat | Green-commando en resultaat |
|---|---|---|---|
| P02 `burnburn-document/tests/undo_selection::{undo_returns_the_selection_from_before_and_redo_the_one_after, a_selection_set_after_an_undo_does_not_rewrite_the_older_record}` | undo/redo geven de selectie niet terug | stub: `DocumentSession::set_selection` doet niets, `CommandOutcome.selection` altijd `None`. `cargo test -p burnburn-document --locked --test undo_selection` → 2 FAILED (`left: None right: Some([GeometryId(1)])`) | zelfde → ok; `cargo test -p burnburn-document --locked` ok |
| P02 `presentation g5_edit::{undo_restores_the_selection_from_before_the_command_and_redo_the_one_after, a_selection_change_between_commands_is_part_of_the_next_undo_record}` | na undo van Duplicate is de selectie leeg i.p.v. het origineel | `cargo test -p burnburn-presentation --locked --test g5_edit` → 2 FAILED (`left: [] right: [GeometryId(1)]`) | zelfde → ok; hele presentation-suite ok |
| P04 notities `document/tests/project_notes::*` (3) | `SetNotes` deed niets (stub: `notes()` leeg) | `--test project_notes` → 3 FAILED | → 3 ok |
| P04 `io/tests/notes_roundtrip::{notes_survive_save_and_reopen_byte_for_byte, the_project_hash_sees_the_notes_and_a_project_without_notes_keeps_its_file_form}` | notities gaan verloren bij opslaan | `cargo test -p burnburn-io --locked --test notes_roundtrip` → 2 FAILED, 1 ok (`a_file_without_the_field_…` was vacuous groen omdat het onbekende veld nog geweigerd werd; wordt pas betekenisvol na de DTO) | → 3 ok; `cargo test -p burnburn-io --locked` ok |
| P04 `g5_edit::{show_notes_opens_the_stored_text_and_a_commit_is_one_undo_step, notes_beyond_the_limit_are_refused_without_touching_the_document}` | paneel/intents deden niets (stub) | `--test g5_edit` → 2 FAILED (`vm.notes` is None) | → ok |
| P04 LBRN2 als één stap: `document/tests/compound_commands::an_import_project_compound_targets_items_by_position_and_is_one_undo_step` (+ wacht `an_item_that_was_not_created_refuses_the_whole_compound`, die onder de stub al groen was) | `AddOperationForItems` was een stub (weigerde) | `--test compound_commands` → 1 FAILED | → 7 ok |
| P04 `g5_edit::a_lbrn2_import_with_a_bitmap_is_one_undo_step` | commit gaf meerdere undo-stappen / geen `ImportProject`-label | `--test g5_edit` → FAILED op de label-assert | → ok; `files_import` (9 tests) ok |
| T04 `burnburn-trace/tests/centerline_document::{the_centerline_recipe_survives_the_engine_parameter_form, a_centerline_result_becomes_one_trace_command, an_operator_edited_result_is_stored_with_the_edit_count_…}` (+ wacht `another_engine_a_missing_or_a_bad_value_is_refused`, onder de stub vacuous groen) | stubs gaven altijd `Err` | `cargo test -p burnburn-trace --locked --test centerline_document` → 3 FAILED, 1 ok | → 4 ok; hele trace-suite ok |
| T04 `g5_trace::{applying_a_centre_line_linked_stores_a_centreline_trace_with_an_open_path, a_centreline_trace_reopens_in_centreline_mode_and_retraces_in_place}` | `ApplyTraceLinked` was uit in centerline-modus | `cargo test -p burnburn-presentation --locked --test g5_trace` → 2 FAILED | → ok; `w4_trace`, `trace_panel` ok |
| T04/T08 `g5_trace::{an_ambiguous_junction_stays_open_until_the_operator_picks_a_pairing, an_accepted_circle_candidate_becomes_a_primitive}` | stub-intent deed niets; geen paneel | → 2 FAILED (`vm.centerline` is None) | → ok |
| E11 `io/tests/production_roundtrip::{the_counters_survive_save_and_reopen, inconsistent_counters_and_a_missing_table_are_refused}` (+ guard `a_project_without_counters_gains_no_entry_and_the_hash_ignores_them`, al groen) | tellers gingen niet mee in het bestand | `cargo test -p burnburn-io --locked --test production_roundtrip` → 2 FAILED | → 3 ok |
| E11 `g5_variable::{the_counter_policy_and_chosen_row_survive_save_and_reopen, changing_only_the_counters_makes_the_window_unsaved_until_the_next_save}` (+ guard `a_project_with_untouched_counters_gets_no_production_entry`) | na heropenen weer de standaardreeks; teller telt niet als ongeopgeslagen | `cargo test -p burnburn-presentation --locked --test g5_variable` → 2 FAILED (`left: current "1" … right: current "20" …`) | → 3 ok; hele presentation-suite ok |
| E05 `g5_edit::{the_offset_preview_is_a_task_that_never_changes_the_document, an_impossible_offset_preview_names_the_problem_and_shows_nothing, applying_an_offset_uses_the_chosen_options}` (+ `an_offset_preview_of_an_older_revision_is_dropped`, onder de stub vacuous groen) | stubs deden niets | → 3 FAILED | → 4 ok |
| E09 `g5_edit::{the_repair_preview_shows_what_closes_and_what_stays_without_changing_the_document, a_bad_tolerance_starts_no_repair_preview}` | stub deed niets | `--test g5_edit repair` → 2 FAILED | → ok |
| X03 `g5_edit::locale_does_not_change_physical_value` | — | geen rode run: het gedrag bestond al (veldparser + eenheden), de test ontbrak alleen (storydekking) | → ok |

## Ontwerpkeuzes en aannames

- **P02 selectie in het undo-record.** `DocumentSession::set_selection(&[GeometryId])` (platte ids, geen editor/native types). Een commando
  bewaart de selectie van dat moment als `selection_before`; de laatste selectie na het commando wordt `selection_after` zolang het
  nieuwste record "live" is (na een undo hoort een nieuwe selectie bij geen record). `CommandOutcome.selection` is `Some` bij undo/redo.
  De presentation meldt de selectie vóór en na elke intent aan de sessies; records zonder bekende `selection_after` vallen terug op
  de oude afleiding uit `changed`.
- **P04 notities** zijn documentdata in de proceslaag (`ProcessCommand::SetNotes`, max 20 000 tekens, één undo-stap, label
  `ProcessLabel::SetNotes`), bewaard als optioneel veld `notes` in `document.json` (afwezig = geen notities; oudere bestanden openen
  ongewijzigd) en alleen in de canonieke hash als ze gevuld zijn. Een settings-only template neemt ze niet mee.
- **P04 LBRN2-import** gebruikt nu één `CompoundCommand` (`CompoundLabel::ImportProject`) met de nieuwe stap
  `AddOperationForItems { draft, items }` (items = positie in de volgorde waarin het compound objecten maakt, zodat een import lagen
  aan objecten kan koppelen voordat de ids bestaan). Mislukt één stap, dan verandert er niets.
- **T04 centerline** is een eigen engine (`burnburn-trace/centerline-1`) met `centerline.*`-parameters naast de `image.*`-parameters;
  de uitvoer zijn de streken (open paden), sleutel = `CenterlineRequest::key`. Operatorbeslissingen (knooppunten, bevestigde
  primitieven) zitten niet in het recept: hun aantal staat als `centerline.edits` in de definitie, en opnieuw tracen vanuit het recept
  begint zonder die beslissingen (eerlijk beperkt; niet stil weggegooid).
- **T04/T08 bediening**: `UiIntent::CenterlineEdit` met `ResolveJunction{node,candidate}` en `ConfirmPrimitive{chain}`; niets wordt
  vanzelf beslist of vervangen. De paneel-gegevens staan in `DocumentViewModel.centerline` (niet in `TracerViewModel`, om vaste
  fixtures van de hosts niet te breken).
- **E11 tellers** staan als `production.json` in de container (`Project.production`): reeks (start/einde/stap/breedte/wrap/huidig),
  gekozen rij, beleid en de gekozen CSV als asset (`text/csv`, plus de `frozen_at`-tijdwaarde). Het is boekhouding, geen ontwerp: geen
  undo-stap en niet in de projecthash. Een wijziging van alleen de tellers maakt het venster "niet bewaard"
  (`Session::unsaved`). Templates nemen de tellers niet mee. Op laden wordt de reeks gevalideerd (stap ≠ 0, einde aan de goede kant,
  breedte ≤ 20, huidige waarde in de reeks, CSV is een asset); een beschadigde tabel laat de standaard staan.
- **E05 offsetvoorbeeld** is een taak (`TaskKind::OffsetPreview`, revisiegevoelig, nieuwere taak annuleert de oudere); het resultaat
  verschijnt als scene-overlay (`Overlay::ResultPreview`) en als maat + oppervlak in `DocumentViewModel.offset.preview`. Opties:
  hoekvorm (Round/Miter/Bevel) en "bron houden". Aanname (LightBurn-gedrag onbekend): standaard Round en bron houden, zoals het paneel
  tot nu toe deed.
- **E09 voor/na**: `UiIntent::PreviewRepair` rekent op een scratch-kopie wat `CloseGaps` zou geven; "voor" = het bestaande
  probleemoverlay, "na" = de gerepareerde omtrekken als overlay; het paneel zegt hoeveel gaten sluiten en hoeveel open blijven.
- **X03**: de test bestond niet; het gedrag wel.

## Wat gedekt is / eerlijk open

Zie het eindrapport; per story:

- P02 gedekt (document + presentatie). P04: notities en LBRN2-als-één-stap gedekt in domein + presentatie; **native notitiepaneel en
  menu-item (Mac + Windows) niet gebouwd** (geen dood menu-item toegevoegd). P06, P08: niet gedaan.
- E05: offsetvoorbeeld + opties in presentatie gedekt; extend/fillet-UI niet; **geen native offsetpaneel** (bestond niet).
  E09: voor/na in presentatie; geen native knop. E10 (generatorpaneel dozen/scharnieren, barcode als object): niet gedaan.
  E11: gedekt voor reeks/rij/beleid/CSV; Auto-Advance-gebeurtenis van LightBurn blijft onbevestigd.
- S01 ellips in een schets, S05–S07: niet gedaan. T03, T05, T06: niet gedaan. T04 en T08 (centerline als TraceCommand, knooppunt-
  en primitiefbeslissingen): gedekt in presentatie; geen native paneel voor de centerline-beslissingen.
- W04–W08: niet gedaan (camera/orders zijn hardware- of cam-gebonden of hoorden bij andere lanes). X01: niet gedaan. X02: niets
  nieuws. X03: gedekt met een test.
