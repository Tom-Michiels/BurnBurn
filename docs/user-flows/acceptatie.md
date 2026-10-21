# Acceptatie: het plan `research/lightburn/05-acceptatieplan.md` als uitvoerbare tests

Eigenaar: lane W4-acceptatie (golf 4). Alle tests staan in `crates/burnburn-scenarios/tests/acc_*.rs` (shared rigs:
`acc_rig/`, `acc_machine/`, `acc_cam/`, `common/`); fixtures en goldens in `test-data/acceptance/` (herkomst in de README
daar). Draaien: `cargo test -p burnburn-scenarios --locked` (de Ruida-diff-tests starten `python3 -I`; zet
`BURNBURN_REQUIRE_PYTHON=1` om het ontbreken van python een fout te maken in plaats van een melding).

Stand: **✔** headless getest tot aan de grens van dit pakket; **deels** wat er staat is getest, het genoemde ontbreekt;
**handmatig** alleen met een mens / echte OS-component; **hardware** alleen met een echte controller; **✗** (nog) niet
gebouwd, geen test mogelijk (staat in `docs/plan/integratie-wachtrij.md`). Een testnaam is `bestand::functie`; `acc_edit` =
`crates/burnburn-scenarios/tests/acc_edit.rs`. "Onafhankelijk" = de verwachting komt niet uit de implementatie.

## Functionele regressies

| ID | Stand | Tests | Oracle | Wat ontbreekt |
|---|---|---|---|---|
| ED01 | ✔ | `acc_edit::ed01_nested_group_is_picked_whole_and_its_bounds_and_move_follow_the_members`, `…ed01_invert_selection_skips_locked_objects_and_the_object_list_cannot_move_a_locked_object`, `…ed01_undo_of_a_selection_dependent_edit_restores_the_exact_bounds` | rekenwerk op getekende vierkanten; lock-bypass via inspector, spiegelen en verwijderen | – |
| ED02 | ✔ | `acc_edit::ed02_comma_and_point_and_units_give_the_same_physical_width`, `…ed02_each_of_the_nine_anchors_stays_put_when_the_size_changes`, proptest `…ed02_property_unit_locale_and_anchor_are_independent` | mm/cm/in/" × komma/punt × 9 ankers, exacte maten | OS-regio-instelling (locale van het systeem) wordt in de host vertaald; daar is de invoer al tekst |
| ED03 | ✔ | `acc_edit::ed03_a_hundred_fixed_operations_roundtrip_through_undo_redo_save_open`, proptest `…ed03_property_any_hundred_operations_roundtrip` (100 willekeurige bewerkingen: verplaats, schaal, groepeer, spiegel, draai, kopieer, plak, vergrendel, verberg, verwijder) | structuur-vingerafdruk + byte-gelijke projectbestanden na undo-alles, redo-alles en opslaan-openen-opslaan | – |
| ED04 | ✔ | `acc_edit::ed04_duplicate_paste_in_place_and_paste_each_have_their_own_ids_and_placement` | eigen id's, plaatsing, klembord onaangeroerd bij Dupliceer, één undo per stap | – |
| GE01 | ✔ | `acc_geometry::ge01_*` (6 tests) | inclusie-exclusie van oppervlakten; exacte cirkeloppervlakte uit de boogrepresentatie; fillregel EvenOdd/NonZero expliciet | – |
| GE02 | ✔ | `acc_geometry::ge02_a_bow_tie_…`, `ge02_a_one_micrometre_edge_…`, `ge02_a_huge_coordinate_…` | bow-tie = 2 × 25 mm²; 1 µm-rand gemeld; 1e3…1e100 mm: juiste oppervlakte of `OutOfRange`, nooit paniek | `Path::signed_area` verliest cijfers boven ~1e8 mm (zie dev-note, geen praktisch pad) |
| GE03 | ✔ | `acc_geometry::ge03_flattened_circle_stays_within_the_budget_and_closed` (proptest), `ge03_a_cusp_…`, `ge03_optimizing_a_flattened_circle_…` | afstand exacte kromme → koorden ≤ budget (4000 steekproeven); sluiting behouden | – |
| GE04 | ✔ | `acc_geometry::ge04_*` (4 tests) | halsbreedte 2 mm: bij 0,9 mm blijft 1 contour, bij 1,1 mm twee van 316,84 mm²; uitgroei kerf = 100+8·2+π·4 | – |
| GE05 | ✔ | `acc_geometry::ge05_join_threshold_is_inclusive_and_the_same_for_every_pair`, `…ge05_property_join_decision_is_gap_le_tolerance`, `…ge05_a_joined_piece_snaps_exactly_…`, `…ge05_trim_and_extend_…` | grens inclusief (exact representeerbare waarden); trim/extend op exacte kruising | – |
| AR01 | ✔ | `acc_arrange::ar01_each_align_command_…`, `ar01_distribute_horizontally_…`, `ar01_distribute_by_centres_…`, `ar01_dock_slides_to_the_reference_edge_…` | uitlijnen via de harness (6 commando's), verdeel-gaten, verdeel-middens en dock op documentniveau | geen UI-route voor verdelen op middens |
| AR02 | deels | `acc_arrange::ar02_grid_array_…`, `ar02_circular_array_…` (rotatie per kopie, deelboog incl. beide uiteinden), `ar02_invalid_counts_…` | positie van elk deelcontour van een 2-delige bron; hoeken met trigonometrie | pad-array bestaat niet; geen UI-route; tekst/groep als bron niet getest (bron = één object) |
| AR03 | ✔ | `acc_arrange::ar03_parts_with_holes_…`, `ar03_a_small_part_may_sit_inside_the_hole_…`, proptest `ar03_property_no_collisions_and_complete_accounting` | botsing en randafstand met een onafhankelijke boolean/offset-oracle; verwerkt = geplaatst + gerapporteerd; deterministisch (geen seed in het algoritme) | – |
| TX01 | deels | `acc_text::tx01_preview_paths_and_output_…`, `tx01_ligatures_…`, `tx01_accents_…`, `tx01_a_fallback_font_…`, `tx01_single_line_text_…`, `tx01_the_derived_outlines_…`; **rood/ignored**: `tx01_right_to_left_text_is_laid_out_from_the_right` | shaper-bounds = documentobjecten = RD-snijbeweging (±4 µm); fontmetrics uit OFL-fonts | RTL (bidi) niet gebouwd; `.shx`-fonts niet ondersteund (alleen de ingebouwde enkellijns set) |
| TX02 | ✔ | `acc_text::tx02_*` (6 tests + proptest) | RFC-4180-gevallen met handgeschreven verwachting; ware regelnummers; BOM/UTF-8; seriële overloop Stop/Wrap/i64-rand; geen afgekapte cijfers | – |
| TX03 | ✔ | `acc_text::tx03_next_bake_and_literal_text_…`, `tx03_a_failed_send_never_consumes_a_number_…`, `tx03_auto_advance_is_only_by_the_chosen_event_…` | RD-bytes variabel = RD-bytes letterlijk; faal-upload verbruikt geen nummer; bake = 1 undo | – |
| BC01 | ✔ | `acc_barcode::bc01_*` (6 tests) | `rxing` (ZXing-port) leest QR (4 ECC-niveaus, Unicode, langste payload 271 B), EAN-13, Code 128, Code 93 uit de scene-geometrie; RD-snijbeweging = symboolgrootte | andere symbologieën: `burnburn-generators/tests/symbology*.rs`; fysiek scannen: handmatig |
| IM01 | ✔ | `acc_image::im01_*` (4 tests) | alfa over wit, toontrap (stapwig), toonkromme, 1-px rand bij 1:1, 2× en 1,5× | – |
| IM02 | ✔ | `acc_image::im02_every_image_mode_matches_the_python_reference_at_one_and_two_times_scale` (11 modi × 2 schalen), `im02_grayscale_power_modulation_is_refused_…` | `test-data/acceptance/image-modes-golden.txt` uit `image_modes_reference.py` (Python, geen Rust) | Grayscale (vermogensmodulatie) geweigerd zolang geen profiel het bewijst; resamplingbeleid alleen opschalen ×2 en ×1,5 |
| IM03 | ✔ | `acc_image::im03_rotate_replace_refresh_and_undo_…`, `im03_mask_clips_to_the_shape_…`, `im03_refresh_after_replace_and_undo_reads_the_original_file_again` (**vond een bug, opgelost**, zie dev-note) | verwerkte bitmap per stap, framegrootte, undo geeft dezelfde pixels | Crop zelf: `burnburn-presentation/tests/image_flow.rs` |
| TR01 | ✔ | `acc_trace::tr01_*` (3 tests) | getekende bitmap met gat, eiland, dunne lijn, ruis, alfa; oppervlakte ±2 %, rand ≤ 0,05 mm van de tekening | – |
| LY01 | deels | `acc_process::ly01_output_and_visibility_are_independent_…`, `ly01_sublayers_run_in_layer_order_…` | het RD-bestand bevat een verborgen laag met output wel en een zichtbare laag zonder output niet | "tool"-lagen bestaan niet |
| CAM01 | ✔ | `acc_process::cam01_property_order_keys_and_inner_first_hold_under_travel_optimisation`, `cam01_a_hard_order_constraint_…` | Order-by-hiërarchie (laag/prioriteit), binnen-eerst, harde ordebeperking + cyclus geweigerd | sleutel "Groep" niet in de property-test (dekt `burnburn-cam/tests/optimization.rs`) |
| CAM02 | ✔ | `acc_process::cam02_tabs_kerf_leads_overcut_and_passes_…` | contourlengte − tabs + leads (+ overcut op de laatste pass), 4 gaten van 2 mm in elke pass | – |
| CAM03 | ✔ | `acc_process::cam03_parity_fill_xors_overlaps_…` | scanlengte = oppervlakte/interval (xor, aanrakend, genest; zelfde of andere laag) | – |
| CAM04 | ✔ | `acc_process::cam04_every_wanted_area_is_scanned_exactly_once_…`, `cam04_offset_fill_rings_…` | 728 mm per pass (364 mm²/0,5); ringen op halve staplijnen 72+56+40+24+8 | – |
| CAM05 | ✔ | `acc_process::cam05_the_placed_plan_its_preview_and_the_header_bounds_agree_…` | plaatsing met selectie-oorsprong; preview, RD-kop en snijbeweging dezelfde doos | Print-and-Cut zie REG01 |
| PRE01 | ✔ | `acc_process::pre01_preview_export_and_job_name_one_plan_and_one_rd_hash_…` | plan-id in preview, export, jobkaart; `rd_hash` = hash van de bestandsbytes; snijlengte preview = snijlengte in bestand; bewerking → stale, nieuw id | – |
| PRE02 | deels | `acc_process::pre02_the_estimate_is_length_over_speed_for_ten_job_shapes_…` | tijd = lengte/snelheid (+ verplaatsing/400) voor 10 jobvormen | foutstatistiek per profiel tegen een stopwatch: **hardware**; de schatting is expliciet "geen gemeten duur" |
| LIB01 | ✔ | `acc_machine_setup::lib01::lib01_moving_the_library_files_…`, `lib01_a_bundle_into_an_empty_library_…` | verplaatst bestand: zelfde presets, projectlink `UpToDate`, art-geometrie gelijk; bundle → alles identiek | – |
| CA01 | deels | `acc_machine_setup::ca01_positioning_error_stays_inside_…`, `ca01_a_mark_that_contradicts_…` | pinhole-camera in gesloten vorm, ruis ±0,3 px, 3 hoogtes + geïnterpoleerd, tolerantie 0,5 mm | echte camera/patroon: **handmatig/hardware**; lenskalibratie zelf: `burnburn-image/tests/calib_lens.rs` |
| CA02 | ✔ | `acc_machine_setup::ca02_every_single_change_of_the_camera_configuration_…`, proptest `ca02_property_only_the_identical_configuration_hits` | elke enkele veldwijziging (resolutie, formaat, fps, lens, bron) → geweigerd met de juiste reden | – |
| REG01 | ✔ | `acc_machine_setup::reg01_property_the_known_rotation_scale_and_offset_are_found`, `reg01_targets_that_are_near_coincident_…`, `reg_app::*` (2) | bekende gelijkvormigheid teruggevonden; no-scale vs scale; reset | – |
| RO01 | deels | `acc_machine_setup::ro01_property_circumference_scale_and_mirror`, `ro01_switching_back_to_the_flat_bed_…` | RD-Y-beweging = oppervlak × stappen/(omtrek × stappen/mm), spiegelen, roller = chuck, terug naar vlak = byte-gelijk | werkelijke omtrek/stappen op een machine: **hardware** |
| FE01 | ✗ | – | – | Feeder (repeat, feed-before/after, pauze) bestaat niet; wachtrij-regel |
| MAT01 | ✔ | `acc_machine_setup::mat01_property_cells_carry_exactly_their_axis_values_…`, `mat01_an_interval_test_scans_every_cell_…` | celwaarden = lineaire verdeling van de assen; snelheid in RD; brondocument-hash ongewijzigd | labelinhoud (streeklettertekst) niet inhoudelijk gecontroleerd |
| API01 | ✗ | – | – | Desktop-UDP-automatisering (LOADFILE/IMPORT/STATUS/PING) bestaat niet; wachtrij-regel |
| PL01 | deels | `acc_platform::pl01_every_command_and_shortcut_id_of_golf_3_still_exists`, `pl01_command_ids_do_not_depend_on_the_language`, `pl01_language_units_decimal_separator_and_window_scale_do_not_change_a_single_output_byte`, `pl01_the_canvas_camera_…` | golden van 138 commando- en 227 sneltoets-id's (regressieslot, geen oracle); RD-bytes gelijk onder taal × eenheid × decimaalteken × venster-schaal | echte OS-bestandsdialogen, toetsenbordevents en DPI: native tests (`tests/native/`) en **handmatig**; Windows-run: **handmatig** |

## Transport / emulator (foutinjectie)

Alles tegen de echte `ExecutionActor` + `MachineService` over een `ScriptedPeer` met `FakeClock`; elk geval eindigt met
dezelfde controle (`assert_untrusted`): de weergegeven status is nooit Stored/StartRequested/Running/Completed zonder
bewijs, er is geen Start-knop, er is geen Start naar de controller gegaan. Onderlaag-tests blijven bestaan:
`burnburn-machine/tests/execution.rs`, `burnburn-runtime/tests/udp.rs` (echte 127.0.0.1-sockets), `burnburn-ruida/tests/udp.rs`.

| Faultcase | Stand | Test(s) | Opmerking |
|---|---|---|---|
| ACK kwijt | ✔ | `acc_transport::tr_ack_lost_makes_the_upload_unknown_and_the_chunk_is_never_resent` | chunk 1 precies één keer verstuurd, geen chunk 2 |
| dubbele ACK | ✔ | `tr_duplicate_ack_does_not_confirm_the_next_chunk` | |
| vertraagd inhoudelijk antwoord | ✔ | `tr_delayed_ack_after_the_timeout_does_not_revive_the_upload`, `tr_delayed_content_reply_is_accepted_only_when_it_really_arrives` | binnen de timeout: Stored; nooit beantwoord: niet Stored |
| wrong source | ✔ | `tr_wrong_source_replies_are_ignored` | |
| corrupte checksum | deels | `tr_a_corrupted_datagram_never_validates` (proptest, elke enkele bitflip), `tr_no_single_bit_corruption_of_an_ack_byte_reads_as_an_ack`, `tr_the_checksum_is_the_big_endian_sum_…` | codec-niveau; inkomende antwoorden hebben in de productiepoort geen checksum (alleen het ACK-byte en een dialect): onbekende bytes = geen antwoord |
| truncation | deels | `tr_a_truncated_datagram_is_caught_unless_it_only_lost_zero_bytes` | een som-checksum ziet het afkappen van nul-bytes niet: dat is protocol, geen bug; gemeld |
| disconnect midden in chunk | ✔ | `tr_disconnect_in_the_middle_of_a_chunk_is_unknown_and_a_reconnect_sends_nothing` | |
| busy | deels | `tr_busy_controller_refuses_a_chunk_and_the_upload_fails_without_further_bytes` (NAK en ERR) | er is geen geverifieerde draadvorm voor "busy"; hardwarebewijs nodig |
| disk full | deels | `tr_disk_full_on_the_last_chunk_or_at_read_back_…` | idem: ERR op de laatste chunk of `Stored(false)` bij read-back |
| duplicate filename / overwrite weigeren | ✔ (door afwezigheid) | `tr_duplicate_filename_and_overwrite_are_refused_because_no_named_upload_is_wired` | benoemd uploaden naar de controller heeft geen draadvorm: er wordt nooit iets overschreven of gewist; echte bestandsnaamconflicten: **hardware** |
| app herstart | ✔ | `tr_app_restart_in_the_middle_of_an_upload_resumes_nothing_and_offers_no_start` (+ gate-C-tests in presentation) | |
| identiteit ≠ profiel | ✔ | `tr_identity_of_another_model_uploads_nothing` | |
| beperkte queue, annuleerbare transfers | verwezen | `burnburn-runtime/tests/udp.rs::flood_beyond_the_queue_is_dropped_without_blocking`, `burnburn-machine/tests/execution.rs::cancel_does_not_delete_implicit_controller_file` | niet opnieuw getest |

## Differentieel Ruida-corpus

Python-oracle: `research/lightburn/tools/rd_inspect.py` onder `python3 -I` (Python 3.13.7 op de ontwikkelmachine, geen
installaties); de getallen worden uit zijn hex-uitvoer gelezen volgens `02-binaire-specificatie.md`, niet met BurnBurns
decoder. Tests: `acc_ruida_diff.rs`. De hardwarebank (de controller accepteert en brandt het) is **hardware**.

| # | Factor | Stand | Test | Opmerking |
|---|---|---|---|---|
| 1 | Richtingen X±, Y±, diagonalen, gesloten | ✔ | `rd_diff_01_directions_…` | getekende volgorde behouden (best-start/-richting uit) |
| 2 | relative14 8190/8191/8192 µm en negatief | ✔ | `rd_diff_02_relative14_boundaries_…` | `AA` binnen −8192..=8191, anders `A8`; verplaatsing exact |
| 3 | Origins: 4 hoeken; 9 anchors | ✔ | `rd_diff_03_each_bed_corner_origin_…`, `rd_diff_03_all_nine_job_anchors_…` | Absolute / User Origin / Current Position: `burnburn-machine/tests/placement.rs` + CAM05 |
| 4 | Power 0/0,1/1/10/20/50/99,9/100 %, min≠max | deels | `rd_diff_04_power_values_…` | alleen 20 % → 3277 is door LightBurn bewezen; de rest is dezelfde schaling (hypothese) tot een banktest |
| 5 | Snelheid | ✔ | `rd_diff_05_speed_…` | alleen mm/s; mm/min = /60 vooraf |
| 6 | Fill 0/90/180/270/45, uni/bi, interval | deels | `rd_diff_06_fill_rows_…`, `rd_diff_06_vertical_and_angled_fills_are_refused_…` | RD-export kent alleen X-scans: Y-as en hoeken ≠ 0 worden netjes geweigerd (`Unsupported`); 180° ≡ 0° (modulo 180); wachtrij-regel |
| 7 | Geometrie: gaten, eilanden, groepen, flood/crosshatch | verwezen | CAM03/CAM04 (plan-niveau) | niet als RD-corpus; crosshatch/flood zijn Y-scan of plan-eigen |
| 8 | Images | deels | `rd_diff_08_a_dithered_image_…` | Atkinson via bestand teruggelezen; andere modi staan op plan-niveau in IM02 |
| 9 | Lijn: kerf, tabs, leads, overcut | verwezen | CAM02 (plan-niveau) | |
| 10 | Lagen: kleuren, sublagen, output uit, prioriteit | verwezen | LY01, CAM01 | |
| 11 | Assen Z/U, passes | deels | `rd_diff_11_a_focus_offset_is_refused_…` | Z-offset niet exporteerbaar (typed weigering); passes ✔ |
| 12 | Job: array-herhaling, zeer groot bestand, checksum | deels | `rd_diff_12_the_file_checksum_…` | bestandssom herberekend uit Python-bytes (>65 535); start/end delay en array-repeat-records worden niet geschreven |
| 13 | Uitvoerroute | deels | `rd_diff_13_save_and_send_differ_in_envelope_but_not_in_payload` | Save = bestand; Send = dezelfde bytes in chunks met 2-byte checksum; Start / Run RD / USB-stick: **hardware**, geen draadvorm |

## Handmatig / hardware (niet automatisch bewezen)

Echte camera en afdrukpatroon (CA01), stopwatch-afwijking van de tijdschatting (PRE02), omtrek/stappen van een rotary
(RO01), controller-gedrag bij busy / disk full / bestandsnamen (Transport), de Ruida-bank (corpus 1–13), echte
OS-dialogen en hotkeys (PL01), Windows-run (hele plan op Windows: de scenario-crate is platformonafhankelijk maar is hier
alleen op macOS gedraaid), fysiek scannen van barcodes (BC01).
