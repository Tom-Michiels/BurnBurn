# G5-UI-machine (partial: save with previous version, X07)

Done: step 1, first half only. Not done: update gate, profiles (P07), GRBL UI, story gaps M01-M09/W01-W03/W09.

Tests (`crates/burnburn-presentation/tests/g5_machine_save.rs`):
- `saving_a_migrated_project_keeps_the_previous_version_and_says_where`
- `saving_over_a_current_file_keeps_no_second_copy`
- `a_newer_file_on_disk_is_never_overwritten_and_is_said_in_words`

Red (task.rs temporarily using plain `save_project`): `cargo test -p burnburn-presentation --test g5_machine_save` gave 3 failed (assertion failures, not compile errors).
Green: same command gave 3 passed; `cargo test -p burnburn-presentation -p burnburn-macos --locked` all ok; clippy -D warnings on both ok.
Not run: `cargo xtask verify`, burnburn-windows tests, native/manual checks.
