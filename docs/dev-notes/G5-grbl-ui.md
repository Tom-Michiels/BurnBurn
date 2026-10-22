# G5-grbl-ui: GRBL bedienbaar in het Mac-venster

Lane van golf 5. Basis: `d416f16`. Alle commando's met `CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/g5-grbl` en
`--offline` (de nieuwe regel `burnburn-gcode` bij `burnburn-presentation` vroeg een `Cargo.lock`-wijziging zonder nieuwe
externe dependency).

## Onbevestigd (lees dit eerst)

- **Er is geen enkele GRBL-controller gecontacteerd en er is geen seriële adapter.** Het venster praat tegen de
  `SerialPort`-trait; alle tests gebruiken een nagebootste controller (`tests/native/macos/common/grbl_fake.rs`, en een
  kopie in `crates/burnburn-presentation/tests/g5_grbl_laser.rs`). Wat de fake doet is uit de publieke GRBL-beschrijving
  nagebouwd, niet gemeten. Zie `docs/hardware/evidence.md` (rijen GRBL 1.1: onbevestigd).
- Start/upload van een job naar GRBL blijft getypeerd geweigerd (`GrblWhy::NotVerified`, `LaserWhy::NotVerified` bij
  Job voorbereiden); de knop `Start job…` staat altijd uit met het ontbrekende bewijs als tooltip.
- Jog-richting bij een bed met oorsprong rechts/boven (X en Y omgekeerd) is een aanname over de assen, niet gemeten.
- Windows: alleen menu-item en opslagdialoog voor Exporteer G-code (headless getest). Geen GRBL-venster op Windows.
  Win32-laag is hier niet gedraaid (alleen type-check van het draagbare deel).
- Niet gedaan: het lezen van een GRBL-profielbestand door een host (er is alleen `UiIntent::SetGrblMachineProfile`);
  een screenshot van het venster (`screencapture` gaf "could not create image from window": scherm vergrendeld of geen
  rechten); het OK-pad van het G-code-opslagsheet (alleen annuleren is native gedekt; het schrijven is headless gedekt);
  typen met een echt toetsenbord (de pijltjes zijn als `moveUp:`/`moveDown:` naar de veldeditor gestuurd).

## Wat er is

- Device Settings: keuze Ruida/GRBL + `$30`, lasermodus, M3/M4, lucht M8, werkcoördinaat G54-G59, decimalen
  (`DialogField::{Backend, Grbl*}`, `controller/device.rs`). Toepassen maakt een nieuwe profielversie met de GRBL-matrix
  (`grbl_capabilities`); terug naar Ruida zet de matrix conservatief. `layers.toml`: `burnburn-gcode` bij presentation.
- Preflight: `PlanViewModel.grbl: Vec<GrblFinding>` uit `burnburn_gcode::check` (reden, operatie, object); telt mee in
  `errors`. Mac: pull-down in de rij van de plan-id; een rij kiezen selecteert het object.
- `CommandKey::ExportGcode` (Archief ▸ Exporteer G-code, ook `menus/file.rs`), `UiIntent::ExportGcode`,
  `HostEffect::ShowGcodeExportPanel`, `TaskInput::ExportGcode`. Een geweigerd plan schrijft niets. Een GRBL-machine heeft
  geen RD-export en een Ruida-machine geen G-code-export (getypeerde melding).
- GRBL-laservenster (`controller/grbl.rs`, `grbl_panel.rs`, Mac `appkit/panels/grbl_window.rs`): console met invoer,
  geschiedenis op pijl omhoog/omlaag en transcript; benoemde macro's (uitvoeren regel voor regel, stopt bij de eerste
  `error:N`, nooit tijdens een job of andere macro; tekstvorm door de host bewaard via `HostEffect::GrblMacrosChanged`);
  jog, home, unlock, hold, resume, reset. Een job die de host zelf al streamt (`attach_grbl_driver`) blokkeert console,
  macro's en bewegingen; Hold en Reset blijven open.

## Cycli (rood -> groen)

| Test(s) | Rood: verwachte fout | Rood-commando en waargenomen resultaat | Groen-commando en resultaat |
|---|---|---|---|
| `presentation/tests/g5_grbl_device.rs` (7) | dialoog kent geen backend/GRBL-velden; `SetGrblMachineProfile` was een no-op stub | `cargo test -p burnburn-presentation --offline --test g5_grbl_device`: 0 passed, 7 failed | 7 passed; bestaande `cam_device`, `cam_rd_refusal`, `g5_machine_save` bleven groen |
| `presentation/tests/g5_grbl_export.rs` (9) | geen GRBL-bevindingen, geen export, commando uit | `--test g5_grbl_export`: 2 passed (de twee "geen bevindingen"-tests slagen triviaal), 7 failed | eerst 8 passed, 1 failed: de testhelper las de synchrone weigering niet (alleen `run_tasks`-effecten). Helper aangepast (mijn testfout, geen gedragswijziging), daarna 9 passed |
| `presentation/tests/g5_grbl_laser.rs` (25) | controller-stub: `grbl_view()` is `None`, intents doen niets | `--test g5_grbl_laser`: 0 passed, 23 failed; de tests `right_is_toward…` en `macros_can_be_loaded_before_a_machine_exists…` kwamen later: de laatste was rood (1 failed: `LoadMacros` werd geweigerd zonder GRBL-machine) | 25 passed |
| `macos/tests/cam_panels.rs::every_row_choice_and_note_of_every_dialog_is_said_in_both_languages` (bestaand) | `Backend Nl` had geen bijschrift | `cargo test -p burnburn-macos --offline --test l10n --test cam_panels --test adapter`: dit was de enige falende | 9 passed in `cam_panels`. `tests/l10n.rs` heb ik niet apart rood gezien |
| `macos/tests/g5_grbl_host.rs` (5) | sheet gaf `Nothing`, plan-paneel zonder rijen, lege teksten | `--test g5_grbl_host`: 2 passed (menu-item, Ruida zonder rijen), 3 failed | 5 passed |
| `macos/tests/g5_grbl_window_model.rs` (10) | stubs gaven `None`/lege teksten | `--test g5_grbl_window_model`: 1 passed, 9 failed | 10 passed |
| `tests/native/macos/grbl_window.rs` | `Voer uit` bleef uit na een selectie in de macrotabel (geen refresh na `tableViewSelectionDidChange:`) | `cargo test -p burnburn-macos --offline --test grbl_window`: 1 failed ("Voer uit is on for a selected macro") | na een refresh in de selectie-delegate: 1 passed |
| `tests/native/macos/cam_dialogs.rs::inspector_priority_field_commits_and_fits_the_window` (bestaand) | mijn pull-down onder de bevindingen duwde de inspector 5 pt onder het venster | `cargo test -p burnburn-macos --offline` : 1 failed ("lowest y = -5") | pull-down deelt de rij van de plan-id: 1 passed |
| `windows/tests/g5_grbl_menu.rs` (2) | geen menu-item, geen dialoog voor het effect | `cargo test -p burnburn-windows --offline --test g5_grbl_menu`: 2 failed | 2 passed; hele `burnburn-windows`-suite groen |

Testnamen per feature:

- Device Settings: `g5_grbl_device::{a_ruida_profile_opens_with_the_ruida_backend_and_greyed_grbl_fields,
  choosing_grbl_enables_its_fields_with_the_stock_laser_defaults,
  applying_grbl_makes_a_new_revision_with_the_grbl_matrix_and_the_typed_fields,
  the_dialog_shows_the_stored_grbl_values_again_and_ruida_can_be_chosen_back,
  unusable_grbl_fields_keep_the_dialog_open_and_the_profile_as_it_was,
  a_grbl_only_change_is_a_new_revision_and_an_untouched_dialog_is_not,
  a_loaded_grbl_profile_is_the_active_one_and_a_loaded_ruida_profile_replaces_it}`.
- Preflight en export: `g5_grbl_export::{a_plan_the_grbl_backend_accepts_has_no_gcode_findings,
  air_assist_without_the_output_is_a_finding_with_its_reason_and_the_object,
  a_profile_without_laser_mode_is_refused_for_the_plan_as_a_whole, a_ruida_machine_has_no_gcode_findings,
  changing_the_grbl_profile_makes_the_findings_stale_with_the_plan, export_gcode_writes_the_plan_the_preview_shows,
  a_refused_plan_writes_no_file_and_says_so, the_command_asks_for_a_location_only_for_a_compiled_grbl_plan,
  export_gcode_on_a_ruida_machine_and_export_rd_on_a_grbl_machine_are_refused}`; Mac
  `g5_grbl_host::{export_gcode_is_a_menu_item_that_is_on_for_a_compiled_grbl_plan_only,
  the_gcode_sheet_sends_the_export_and_a_cancel_changes_nothing,
  the_plan_panel_lists_each_grbl_refusal_in_words_and_choosing_one_selects_its_object, …}`; Windows
  `g5_grbl_menu::{the_file_menu_has_export_gcode, the_gcode_effect_asks_for_a_gcode_file_and_the_answer_is_the_export}`.
  De oracle voor de geëxporteerde tekst is een eigen G0/G1-lezer in de test (verbrande lengte 120 mm voor 40 x 20) en de
  kop met de plan-id, niet de lowering zelf.
- Laservenster: `g5_grbl_laser::*` (console, geschiedenis, geweigerde regels, macro's in volgorde en stop bij fout, namen,
  tekstvorm, job-blokkade, jog, home/unlock, realtime, start geweigerd, prepare geweigerd, poortfout, verouderde sessie),
  Mac `g5_grbl_window_model::*`, native `grbl_window_end_to_end_against_a_fake_controller`.

## Ontwerpkeuzes

- `Machine.grbl: Option<GrblProfile>` naast het Ruida-profiel; `SetMachineProfile` zet het weer op `None`.
  Een plan is ook verouderd als alleen de GRBL-instellingen veranderden terwijl de profielrevisie gelijk bleef
  (`Compiled.grbl` onthoudt waarvoor de bevindingen berekend zijn).
- De standaardoperaties vragen luchtondersteuning; met het stock-GRBL-profiel (geen M8) wordt dus elk standaardplan
  geweigerd (`AirAssistUnavailable`). Dat is de eerlijke uitkomst van `check()`; de gebruiker zet lucht aan in
  Apparaatinstellingen of uit in Cut Settings.
- Het venster is app-breed (zoals het cameravenster) en vervangt het Ruida-laservenster alleen voor een GRBL-machine.
- Vragen aan andere lanes: geen. Wijzigingen buiten de lane: additieve varianten in `intent.rs`, `view_model.rs`,
  `controller.rs`, `task.rs`, `macos/l10n.rs`, `macos/adapter.rs`, `appkit/window.rs`, `appkit/app.rs`, `panels/plan.rs`,
  `windows/host_panels/{mod,files}.rs`, `docs/design/layers.toml`.
