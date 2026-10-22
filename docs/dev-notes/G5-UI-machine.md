# G5-UI-machine (partial: save with previous version, X07)

Done: step 1, first half only. Not done: update gate, profiles (P07), GRBL UI, story gaps M01-M09/W01-W03/W09.

Tests (`crates/burnburn-presentation/tests/g5_machine_save.rs`):
- `saving_a_migrated_project_keeps_the_previous_version_and_says_where`
- `saving_over_a_current_file_keeps_no_second_copy`
- `a_newer_file_on_disk_is_never_overwritten_and_is_said_in_words`

Red (task.rs temporarily using plain `save_project`): `cargo test -p burnburn-presentation --test g5_machine_save` gave 3 failed (assertion failures, not compile errors).
Green: same command gave 3 passed; `cargo test -p burnburn-presentation -p burnburn-macos --locked` all ok; clippy -D warnings on both ok.
Not run: `cargo xtask verify`, burnburn-windows tests, native/manual checks.

## machine-rest

Werkdirectory: branch `worktree-agent-ac9d5d3a67ffeb6bb` (basis d416f16). Rode fase = echte assertiefouten na een minimale stub (geen compilerfouten).

1. Updatepoort (`burnburn_runtime::update_gate::decide_update`): `AppController::machine_activity()` (presentation), `macos::quit_guard` + alert in `applicationShouldTerminate:` (modaal, eerste knop = blijven).
   Tests: `presentation/g5_machine_rest::{an_idle_machine_has_no_activity_and_a_stored_job_is_reported, without_a_machine_there_is_no_activity}`; `macos/g5_machine_rest::{quitting_is_free_when_the_machine_is_idle, quitting_during_a_transfer_a_run_or_live_control_asks_with_the_most_urgent_reason, every_reason_has_its_own_words_in_both_languages_and_the_buttons_are_named, the_first_button_stays_and_anything_unexpected_keeps_the_app_open}`.
   Rood: `cargo test -p burnburn-presentation --test g5_machine_rest` 2 failed; `cargo test -p burnburn-macos --test g5_machine_rest` 4 failed (stub). Groen: dezelfde commando's, alles ok. Het alert zelf (runModal) is handmatig, niet getest.
2. `HistoryTrimmed`: `MessageKey::HistoryTrimmed(n)` in de statusregel (`after_change`), tekst in `l10n.rs`. Test `forgotten_undo_steps_are_announced_with_their_number` en `macos::the_trim_notice_says_how_many_steps_can_no_longer_be_undone` (rood: geen melding).
3. P07-rest: `controller/library/profile_keep.rs` (vervangen actief profiel -> bibliotheek, na het lezen van het bestand samengevoegd, beschadigd bestand blijft ongemoeid), `UiIntent::ChooseMachineProfile`. Ook bundelvervanging van een niet-actief profiel schrijft nu de bibliotheek (bestond niet). Tests `presentation/g5_machine_profiles` (8; rood: 6 failed op de stub).
4. Machinehub (`presentation/machine_hub.rs`, `controller/machine_hub.rs`; `macos/hub_panel.rs`, `l10n_machine.rs`, `appkit/panels/hub.rs`): apparaten + ondersteuning (M01/M02), uploadstatus met aparte stappen en "Stop upload" (M04), wachtrij/aantallen (W02), historie met schatting en gemeten duur apart, herhaling met receptverschillen (W03), kalibratieproeven die plannen verouderen (W09). Tests `presentation/g5_machine_hub` (20; rood: 16 failed op de stub) en `macos/g5_machine_hub_panel` (12; rood: 10 failed).
   `job_settings_do_not_touch_vendor_parameters` (M09) is een bewakingstest van bestaand gedrag: geen rode fase.

Eerlijk open: volle opslag/naamconflict zijn niet te onderscheiden (de controller meldt geen reden; de tekst zegt dat); kalibratiewaarden worden nog niet door de compiler gebruikt en zijn alleen in-sessie; "approved" (W01) niet duurzaam; M09-instellingen schrijven/herstellen niet in de UI (geen register-kaart); het hub-venster en het quit-alert zijn niet handmatig bekeken; Windows-venster/menu-item niet gemaakt (`menus.rs` blijft groen, het Mac-item is een AppAction in het Venster-menu).
