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
| T01 `w4_trace::image_scale_keeps_original_pixels` | — | geen rode run: het gedrag bestond al, de test ontbrak alleen (storydekking) | `cargo test -p burnburn-presentation --locked --test w4_trace` → ok | |
| T02 `w4_trace::{applying_linked_stores_the_recipe_as_a_trace_in_one_undo_entry, a_linked_trace_reopens_in_the_tracer_with_its_recipe_and_retraces_in_place}` (+ wacht `the_plain_apply_still_makes_ordinary_editable_shapes`) | `CommandKey::ApplyTraceLinked`/`TraceEdit` bestonden alleen als stub (enablement false) | zelfde commando → 2 FAILED (panics op enabled-assert en op `ApplyTraceLinked` knop) | zelfde → 4 ok | `controller/trace_link.rs`, `TaskInput::DecodeImage` |

Ontwerpkeuze T02: de bestaande "Pas trace toe" (gewone bewerkbare vormen via `Import`) blijft; daarnaast "Pas toe als trace"
(`TraceCommand::Create`, bij heropenen via "Bewerk trace…" `Replace`, objecten behouden hun id). Reden: afgeleide uitvoer is niet direct
bewerkbaar (C-8) en het domein heeft geen "ontkoppel"; vervanging van de standaard zou knoop-bewerken van traceresultaten afsluiten.
Centerline blijft `Import` (geen centerline-TraceCommand in het domein; T04 open).
Merge: 23bcd3a (golf 3 op main, barcodetests) is in de branch gemerged.
| P01 `w4_project::a_project_from_a_newer_version_says_so_and_stays_untouched` | generieke `OpenFailed` i.p.v. eigen melding | `cargo test -p burnburn-presentation --test w4_project` → FAILED (geen `OpenNewerSchema`-effect; stub = variant zonder productie) | zelfde → ok | dev-deps `serde_json`, `zip` (al in de lock) voor de testfixture |
| E08 `w4_text::single_line_text_stays_open_and_unfilled` | — | geen rode run: gedrag bestond, test ontbrak (open streken, vul-operatie scant niets, waarschuwing) | ok | |
| E07 `w4_text::missing_font_shows_stored_outline_and_replacement_choice` | `missing_font` ontbrak; bij openen zonder lettertype bleef de status `Valid` zodat `font_missing` false was | `--test w4_text` → FAILED `assertion failed: bar.font_missing` (stub `missing_font: None`) | zelfde → ok; `text_toolbar` en macOS `text_panels` ok | familie-afwezigheid wordt nu ook als ontbrekend gemeld |
| E03 `w4_edit::{select_by_kind_picks_every_object_of_that_kind, the_type_filter_skips_hidden_and_locked_objects, select_same_type_extends_the_selection_to_the_kinds_selected, isolate_hides_the_others_in_one_undo_step_and_show_all_restores_them}` | stubs deden niets | `--test w4_edit` → 4 FAILED; na eerste implementatie nog 1 FAILED (`the isolated object stays selected`) | → 4 ok | `SelectByKind`, `SelectSameType`, `IsolateSelection` + menu-items |
| E04 `burnburn-editor snap::tests::the_cycle_picks_a_later_candidate_and_wraps` | `snap_point` negeerde `cycle` (stub: veld `SnapSettings.cycle`, `CanvasKey::Tab`) | `cargo test -p burnburn-editor --lib snap` → FAILED `assertion left == right failed: cycle 1` | → ok | |
| E04 `w4_edit::tab_steps_through_the_snap_candidates_and_the_gesture_uses_the_one_shown` | — | geen eigen rode run: de eerste run faalde op een compilefout (ontbrekend veld), daarna op een fout in mijn testvolgorde; de presentatie-assert is dus alleen groen waargenomen | → ok | Tab in `AppController::key`, `Session.last_pointer` |

Handmatig/onbevestigd E04: of de Tab-toets het canvas bereikt (AppKit key-view loop) is niet in de native test bewezen; de adaptervertaling (`key_code::TAB` → `CanvasKey::Tab`) wel hoofdloos gebouwd.
| P07 `burnburn-io/tests/profile_library::*` (4 tests: roundtrip, ontbrekend, nieuwer schema/beschadigd/gewijzigde waarde, atomair + dubbele machine) | `profile_library` was een stub (`decode` gaf `NotAProject`, `save` schreef niets) | `cargo test -p burnburn-io --locked --test profile_library` → 3 FAILED (unwrap op `NotAProject`/`Ok`-zonder-bestand), 1 ok | → 4 ok | nieuw io-bestand `profiles.json` (`burnburn-profile-library`, schema 1) |
| P07 `w4_profiles::profiles_from_a_bundle_are_kept_outside_the_session` (+ `a_damaged_profile_library_is_kept_and_never_overwritten`) | geen `profiles.json` na import | `cargo test -p burnburn-presentation --locked --test w4_profiles` → 1 FAILED (`the profile library file was written`), de beschadigd-test was al groen (wacht) | → 2 ok; `files_bundle` ok | `FilesTask::SaveProfileLibrary`, `Loaded` profielen in `LoadLibraries` |

Beperking P07: alleen profielen die een bundel *toevoegt* gaan naar `profiles.json`. Een bundelprofiel dat het actieve apparaat
*vervangt* werkt in de sessie (bestaand gedrag) maar wordt niet bewaard: de host bepaalt het actieve profiel (`SetMachineProfile`).
Het native selecteren van een bewaard profiel als actief apparaat (host) is niet gebouwd.
| S01 `burnburn-editor sketch::draw::tests::{an_arc_adds_centre_start_end_and_the_arc_in_the_drawn_direction, an_arc_ending_on_an_existing_point_shares_it}` (+ wacht `curves_that_are_not_circular_arcs_cannot_be_drawn_into_a_sketch`) | `plan_commit` weigerde `CreatePath` (`Unsupported`) | `cargo test -p burnburn-editor --locked --lib sketch::draw` → 2 FAILED (unwrap op `Unsupported`) | → 14 ok | `circular_arc` in `sketch/draw.rs`; alleen cirkelbogen, ellips/vrije curve blijft geweigerd (geen sketch-entity) |
| S01 `w4_sketch::an_arc_drawn_in_sketch_mode_becomes_an_arc_entity_that_dimensions_can_drive` | — | geen eigen rode run (de editor-wijziging maakte hem direct groen) | ok | |
| E01 `w4_edit::the_arc_tools_draw_open_arcs_as_one_undoable_object_each` | — | geen rode run: de boogtools bestonden al (catalogus + editor); alleen de presentatietest ontbrak | ok | gap "boog-gereedschap" in de storydekking was achterhaald |
| E01 `w4_edit::numeric_fields_take_expressions_and_refuse_broken_ones` | rekenkundige expressie in een veld (`100/4 + 5`) gaf een fout | `--test w4_edit numeric` → FAILED `assertion failed: (width_of_first - 30.0).abs() < 1e-9` | → ok; volledige `--lib` ok | `field::parse_expression` hergebruikt de schets-expressieparser (eenheden, decimale komma) |

Ellips in een schets (S01) blijft open: het schets-domein kent geen ellips-entity.
