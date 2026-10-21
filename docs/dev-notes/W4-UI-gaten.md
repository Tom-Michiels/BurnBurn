# W4-UI-gaten (golf 4, lane L1: presentation + macOS)

Omgeving: `export PATH="$HOME/.cargo/bin:$PATH" CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/w4-gaten`,
`cargo ... --locked`, macOS arm64. Basislijn voor de lane (commit 6dcdb60): `cargo test -p burnburn-presentation --locked
--no-fail-fast` gaf één falende test die niet van deze lane is: `barcode_dialog::all_29_types_are_listed_and_the_unsupported_ones_say_why`
(`25 of 29 are implemented`).

## TDD-log

| Test | Verwachte fout (red) | Red-commando en waargenomen resultaat | Green-commando en resultaat | Refactor |
|---|---|---|---|---|
| S08 `w4_sketch::ambiguous_autoconstrain_is_not_applied` | auto-constrain past de duidelijke relatie toe en negeert de dubbelzinnige lijn (2 graden) | `cargo test -p burnburn-presentation --locked --test w4_sketch` → FAILED: `the document is unchanged left: RevisionId(1) right: RevisionId(0)` (minimale stub: `ToolRejection::AmbiguousRelations` + `Rejection::AmbiguousRelations`, nog niet geproduceerd) | zelfde commando → ok; `cargo test -p burnburn-editor --locked` ok (293 + overige) | — |

Aanname S08 (LightBurn kent geen auto-constrain; conservatief): een lijn tussen 1 en 3 graden van een as, of drie punten op één plek,
is dubbelzinnig. Dan past auto-constrain niets toe (alles-of-niets) en meldt `ToolRejection::AmbiguousRelations`.
| S04 `w4_sketch::{restore_last_valid_sketch_undoes_back_to_the_last_solving_state, restore_last_valid_sketch_skips_every_failed_step, restore_without_an_earlier_valid_state_is_refused_and_keeps_the_history}` (+ `restore_last_valid_sketch_on_a_valid_sketch_changes_nothing` als regressiewacht) | knop "Herstel laatste geldige" doet niets | `cargo test -p burnburn-presentation --locked --test w4_sketch` → 3 FAILED (`left: Failed right: Valid`; refused-melding `None`); stub = `UiIntent::RestoreLastValidSketch` zonder gedrag | zelfde → 5 ok | `restore_last_valid` in `controller/sketch.rs` |
| S04 paneel `burnburn-macos/tests/w4_sketch_panel::the_restore_button_applies_only_to_a_failed_sketch_and_sends_the_restore_intent` | `sketch_restore_intent` gaf `None` | `cargo test -p burnburn-macos --locked --test w4_sketch_panel` → 2 FAILED (expect intent) | zelfde → 3 ok | AppKit-knop + `NativeEvent::SketchRestoreRequested` |
| S02 `w4_sketch::{parameter_with_a_decimal_comma_is_read_as_a_decimal, dimension_field_with_a_decimal_comma_is_read_as_a_decimal}` | `100,5 mm` geeft Failed (evaluator kent alleen punt) | zelfde commando → 4 FAILED (incl. de twee parameter-verwijder-tests), `a_comma_inside_a_function_call_stays_an_argument_separator` was al groen (wacht) | zelfde → ok; unit-tests `controller::sketch_param::tests::*` | `controller/sketch_param.rs` |
| S02 `w4_sketch::{an_unused_parameter_can_be_removed_and_the_removal_is_undoable, a_parameter_that_is_in_use_is_not_removed}`, paneel `removing_a_parameter_row_sends_its_name_and_a_bad_row_sends_nothing` | verwijderen deed niets | zie hierboven | ok | AppKit-knop "Verwijder parameter" |
| S02 `w4_sketch::setting_a_parameter_to_the_value_it_already_has_is_not_a_new_entry` | tweede identieke commit gaf tweede undo-stap (`left: Some("100.5 mm") right: Some("100 mm")`) | `--test w4_sketch` → 1 FAILED | ok | `set_sketch_parameter` |

Wijziging aan bestaande test (bewust): `burnburn-macos/tests/sketch_panel::a_parameter_is_set_with_its_raw_expression` verwachtte
dat `3,1 mm` letterlijk bewaard bleef; nu wordt `3.1 mm` bewaard (gedrag S02). Aanname: een komma tussen twee cijfers buiten
functie-aanroephaakjes is een decimale komma; binnen `max(…)`/`min(1,5)` blijft het een argumentscheider (evaluatortaal wint).
Aanname S04: "Herstel laatste geldige" = undo van de mislukte stappen tot de schets weer oplost (de stappen blijven redo);
zonder geldige eerdere stand wordt niets gewijzigd en komt `SketchNothingToRestore`. Ook andere bewerkingen die na de
eerste mislukte stap zijn gedaan worden meegeundo't (undo is chronologisch).
| P03 `burnburn-image/tests/decode_bmp_tiff::*` (5 tests: BMP pixels/dpi, TIFF pixels/dpi, afgekapt/liegend) | `decode` weigert BMP/TIFF (`UnsupportedFormat`) | `cargo test -p burnburn-image --locked --test decode_bmp_tiff` → 5 FAILED (unwrap op `UnsupportedFormat`) | zelfde → 5 ok; `cargo test -p burnburn-image --locked` volledig ok | handgemaakte bytes als oracle |
| P03 `w4_formats::{a_bmp_is_imported_with_its_own_resolution_and_travels_as_image_bmp, a_tiff_without_resolution_waits_for_a_dpi_and_travels_as_image_tiff}` | asset kreeg media `image/png` | `cargo test -p burnburn-presentation --locked --test w4_formats` → 2 FAILED (`left: "image/png" right: "image/bmp"` / tiff) | zelfde → 3 ok | `media_of` op signatuur |
| P03 `w4_formats::the_tracer_opens_a_bmp_and_proposes_its_resolution` | — | was al groen voor de openstaande delen; de media-assert onderaan is pas na de fix toegevoegd (geen eigen rode run) | ok | |

Dependency: `image`-features `bmp` en `tiff` aan; nieuwe lockregels `tiff`, `fax`, `weezl`, `quick-error 2.0.1` (allen MIT/Apache, offline uit
de cargo-cache; `cargo xtask layers` en register bijgewerkt). Beperking: TIFF alleen wat de `tiff`-crate in `image` 0.25 decodeert
(8/16-bit gray/RGB(A), baseline-compressies); BMP/TIFF-resolutie alleen uit de eerste directory/infoheader.
