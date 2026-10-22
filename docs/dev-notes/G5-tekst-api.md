# G5-tekst-api: bidi/RTL-tekst, desktop-automatisering, CLI X05, plugincontract X06

Basis be65aea. Toolchain 1.99.1, `CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/g5-tekst`. Dependency: `unicode-bidi`
0.3.18 (MIT OR Apache-2.0, `["std","hardcoded-data"]`), geregistreerd in `docs/design/dependency-register.md`. Het was
offline beschikbaar in de cargo-cache; `Cargo.lock` is daarom met `--offline` bijgewerkt (alleen die ene regel erbij).

| Test | Verwachte fout (red) | Red-commando en resultaat | Green-commando en resultaat | Refactor |
|---|---|---|---|---|
| `burnburn-text/tests/bidi.rs::{a_hebrew_word_reads_from_right_to_left, latin_runs_keep_their_order_around_a_hebrew_run, numbers_inside_right_to_left_text_stay_left_to_right, the_width_of_a_hebrew_word_is_the_sum_of_its_glyph_advances, a_hebrew_line_in_a_multi_line_text_is_laid_out_per_line}` | Hebreeuws links-naar-rechts: `[0,2,4,6]` i.p.v. `[6,4,2,0]`; pennen niet aansluitend | `cargo test --locked --offline -p burnburn-text --test bidi`: 5 FAILED, 2 passed (o.a. `left: [0, 2, 4, 6] right: [6, 4, 2, 0]`) | zelfde commando na `shape_line` met `BidiInfo`: alle groen; `cargo test -p burnburn-text`: alle suites groen | – |
| `burnburn-scenarios/tests/acc_text.rs::tx01_right_to_left_text_is_laid_out_from_the_right` (`#[ignore]` verwijderd) | idem (de test was rood onder `--ignored`) | zie bidi-red (zelfde gedrag) | `cargo test --locked --offline -p burnburn-scenarios --test acc_text`: 16 passed, 0 ignored | – |
| `burnburn-presentation/tests/app_automation_pref.rs::{desktop_automation_survives_a_restart_and_can_be_turned_off_again, a_value_that_is_not_on_or_off_is_refused_and_changes_nothing}` (+ `desktop_automation_is_off_until_the_user_turns_it_on`) | stub negeerde de waarde | `cargo test ... --test app_automation_pref`: 2 FAILED, 1 passed | zelfde: 3 passed; `--test app_prefs` blijft groen | – |
| `burnburn-runtime/tests/automation.rs` (16 tests, o.a. `load_never_overwrites_a_changed_document_and_says_so`, `machine_actions_are_forbidden_and_never_reach_the_host`, `a_disabled_service_does_not_answer_and_does_not_act`, `an_endpoint_refuses_a_socket_that_is_not_bound_to_loopback`, `exactly_one_reply_goes_back_to_the_sender_and_foreign_senders_are_dropped`, `bind_loopback_only_listens_on_127_0_0_1_and_answers_a_real_datagram`) | stub antwoordde nooit | `cargo test ... -p burnburn-runtime --test automation`: 15 FAILED, 1 passed | 15 passed, 1 failed (`only_clean_absolute_paths...`: een te lang pad gaf `MALFORMED` i.p.v. `PATH` omdat het verzoeklimiet eerst viel; limiet 1100 en test op 1050 bytes) → 16 passed | – |
| `burnburn-cli/tests/x05.rs::{an_upload_without_a_named_target_machine_or_intent_contacts_nothing, the_intent_must_match_the_command_and_the_machine_must_be_the_profiles, an_unknown_option_is_refused_and_writes_nothing, the_variant_option_is_refused_with_the_reason_instead_of_being_ignored}` | upload/start gingen door zonder `--machine`/`--intent`; onbekende opties genegeerd | `cargo test ... -p burnburn-cli --test x05`: 4 FAILED, 3 passed | 7 passed; `cli.rs`/`gcode.rs`-regels met `--machine`/`--intent` aangevuld; `cargo test -p burnburn-cli` groen | – |
| `burnburn-cli/tests/plugin_protocol.rs` (9 tests) | stub weigerde alles met `MissingHello` | `cargo test ... -p burnburn-cli --test plugin_protocol`: 7 FAILED, 2 passed | alle groen na `parse_plugin_output`/`access_summary` | – |

## Eerlijke kanttekeningen

- Karakteriseringstests die direct groen waren (geen rode fase mogelijk, gedrag bestond al): `x05::gui_and_cli_give_the_same_plan_id_and_the_same_rd_bytes_for_the_same_input`
  (vergelijkt plan-id, RD-hash en RD-bytes via de GUI-route `ExportRd` en de CLI), `x05::compile_preflight_inspect_and_history_never_touch_the_machine_connector`,
  `x05::a_named_target_and_intent_still_need_the_machine_contact_switch`, `plugin_protocol::the_schema_file_lists_exactly_the_fields_the_validator_accepts`
  (bewaakt schema-drift) en `plugin_protocol::plugin_failure_never_replaces_frozen_job`. De laatste is structureel: de applicatie heeft geen
  plugin-laadpad, en `PluginOutput` kan geen job benoemen; de test legt vast dat een mislukte run history en RD-bytes niet raakt.
- Bidi: UAX #9 via `unicode-bidi` (zonder volledige-tekst-embedding-UI). De alinearichting volgt de eerste sterke letter; de uitlijning
  (`Align`) blijft de gebruikerskeuze en wordt niet automatisch gespiegeld. Spiegeltekens (haakjes) worden door harfrust in RTL-runs
  gespiegeld; daar is geen eigen test voor. Cursiefe Arabische vormen (shaping van contextuele vormen) hangen van het lettertype af; alleen
  Hebreeuws is met de Liberation-fixture getest, geen Arabisch.
- SHX-fonts: geen permissieve Rust-parser gevonden; bewust niet gebouwd en open gelaten.
- API01: de lokale UDP-laag is volledig getest met een fake socket plus één echte 127.0.0.1-test, maar **niet aan een host gekoppeld**
  (geen luisterthread, geen `AutomationHost` voor macOS/Windows). Compatibiliteit met echte LightBurn-clients is niet geverifieerd.
- X05 `--variant`: het project kent geen document-brede varianten (alleen componentvarianten); de optie wordt daarom geweigerd met een
  duidelijke reden i.p.v. genegeerd. Het story-gat "variant-optie niet gecontroleerd" is dus als *niet beschikbaar* vastgelegd, niet als gebouwd.

## Handmatig / onbevestigd

- Weergave van RTL-tekst in de echte canvas en het tekstpaneel op macOS/Windows (handmatig).
- Voorkeursrij "Desktop-automatisering" in het echte Voorkeuren-venster op Mac/Windows: het model is headless getest; de native weergave niet.
- Windows-run niet uitgevoerd (alleen de gedeelde modellen). Geen menu-items toegevoegd, dus geen Windows-menu-pariteitswijziging nodig.
