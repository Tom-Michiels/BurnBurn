# Traceerbaarheidsmatrix LightBurn → BurnBurn (W1-L7)

Opgesteld 18-10-2026 op branch `claude/l7-matrix-4acf26` (basis `7c36842`). Bijgewerkt 20-10-2026 op branch `claude/w2-l7-docs-8620aa` tegen main-stand `8d0f019` (golf 1 + SolidCraft-golf gemerged). Hoort bij [lightburn-pariteit.md](lightburn-pariteit.md) en het dossier in `research/lightburn/`. Eén rij per menu-item (M001–M262), per instelling (tabel B) en per dialoogoptie (tabel C); tabel D geeft de featurefamilies uit `02-features.md`.

## Methode en beperkingen

- **Bronnen LightBurn:** `research/lightburn/01-menu-inventaris.md` (M-ID's), `03-instellingen.md` (tabel B), `06-dialoogcatalogus.md` (tabel C), `02-features.md` (tabel D, gekoppeld aan de familietabel in `lightburn-pariteit.md`).
- **Bewijs BurnBurn:** grep in `crates/*/src`, `crates/*/tests`, `tests/native/macos`. Voor het Mac-venster: menu `main_menu` in `crates/burnburn-macos/src/appkit/app.rs`; toolstrip `crates/burnburn-macos/src/appkit/toolstrip.rs` die het register `crates/burnburn-presentation/src/catalog.rs` gebruikt (het oude `TOOLBAR_TOOLS` bestaat niet meer); paneelfuncties in `crates/burnburn-macos/src/panels/*.rs`; commando's `CommandKey` in `crates/burnburn-presentation/src/intent.rs`.
- **Testnamen:** elke genoemde testnaam is gecontroleerd als bestaande `fn` (index van 4.151 fn-namen in `crates/*/src`, `crates/*/tests` en `tests/native/macos`). Native harnesses (`harness = false`) zijn met hun bestand genoemd. De tests zijn in deze update **niet gedraaid**; groen-status per story staat in `docs/user-flows/story-coverage.md` (run van 18-10-2026, 796 tests `ok`, macOS) en is niet opnieuw gegenereerd.
- **Waar het niet bewezen is:** `?` of `–` in de testkolom betekent: geen directe testnaam gevonden. Een ✔ in de UI-kolom betekent alleen dat de bediening in de Mac-code of een macOS-test bestaat; het betekent niet dat een handmatige acceptatie is gedaan.
- **Bron van waarheid:** de matrix volgt de code op main, niet eerdere notities. Een ✔ in de UI-kolom betekent een bediening in het Mac-venster; alleen domein of presentatie-commando's geven `–` of `deels`.

## Legenda

| Kolom | Waarden |
|---|---|
| UI (Mac-venster) | **✔** bedienbaar in `burnburn-macos` (menu, toolbar, veld of paneel). **deels** wel Mac-bediening, maar niet alle opties of onderdelen (zie opmerking). **–** niet in het Mac-venster; ook als presentatie-commando of domeincode bestaat, staat dat in de opmerking. **?** niet te bepalen met grep. |
| Pakket | ID uit `lightburn-pariteit.md`. **–** = afgerond of geen pakket nodig. **geen** = komt in het plan niet voor; zie de lijst onderaan. |
| Crate/bestand | Waar het domein zit; **–** = geen domeincode gevonden. |

## A. Menu-items (01-menu-inventaris.md, M001–M262)

### A1. LightBurn

| LightBurn-item | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| M001 About LightBurn | LightBurn | – | – | – | geen | Productinfo. `HostText::About` bestaat, maar geen menu-item in `main_menu`. |
| M002 Services ▸ Activity Monitor | LightBurn ▸ Services | – | – | – | geen | macOS-diagnose; OS-integratie, geen LightBurn-functie. |
| M003 Services ▸ Allocations & Leaks | LightBurn ▸ Services | – | – | – | geen | Idem. |
| M004 Services ▸ File Activity | LightBurn ▸ Services | – | – | – | geen | Idem. |
| M005 Services ▸ System Trace | LightBurn ▸ Services | – | – | – | geen | Idem. |
| M006 Services ▸ Time Profile Active Application | LightBurn ▸ Services | – | – | – | geen | Idem. |
| M007 Services ▸ Time Profile App Under Mouse | LightBurn ▸ Services | – | – | – | geen | Idem. |
| M008 Services ▸ Time Profile Entire System | LightBurn ▸ Services | – | – | – | geen | Idem. |
| M009 Services ▸ Toggle Instruments Recording | LightBurn ▸ Services | – | – | – | geen | Idem. |
| M010 Services ▸ Services Settings… | LightBurn ▸ Services | – | – | – | geen | Idem. |
| M011 Hide LightBurn | LightBurn | – | – | – | geen | Standaard AppKit-lifecycle; niet in `main_menu`. |
| M012 Hide Others | LightBurn | – | – | – | geen | Idem. |
| M013 Show All | LightBurn | – | – | – | geen | Idem. |
| M014 Quit LightBurn | LightBurn | `burnburn-macos/src/appkit/app.rs` (`terminate:`), `burnburn-macos/src/lifecycle.rs` | `tests/native/macos/quit_with_dirty_documents.rs` (harness) | ✔ | – | Afsluiten met dirty-check. |

### A2. File

| LightBurn-item | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| M015 New | File ▸ New | `burnburn-presentation/src/intent.rs` (`CommandKey::New`), `burnburn-macos/src/appkit/app.rs` | – | ✔ | – | Menu Cmd+N. |
| M016 New Window | File | – (tweede venster: `burnburn-macos/src/appkit/app.rs`) | `tests/native/macos/two_windows.rs` (harness) | – | geen | Meerdere documentvensters bestaan; geen menu-item. |
| M017 Recent Projects | File | `burnburn-io/src/search.rs` (zoeken, geen recent-lijst) | `project_search_finds_by_file_name_object_material_and_revision` (burnburn-io/tests/workshop.rs) | – | geen | Zoeken bestaat; recent-menu niet. |
| M018 Recent Templates | File | – | – | – | geen | Geen templates in plan (zie M024). |
| M019 Open | File ▸ Open | `burnburn-macos/src/appkit/app.rs` (`show_open_panel`), `burnburn-io/src/project.rs` | `tests/native/macos/native_file_panels.rs`, `tests/native/macos/finder_open.rs` (harness) | ✔ | – | Cmd+O. |
| M020 Import | File ▸ Importeer | `burnburn-io/src/import/mod.rs`, `burnburn-presentation/src/controller.rs` (import-preview) | `svg_import_reports_skipped_elements_and_commits_in_one_entry`, `cancelled_import_changes_nothing` (presentation/tests/import_flow.rs); `unitless_dxf_needs_explicit_scale_before_import_commit` (io/tests/import.rs) | ✔ | W1-L6a | Menu + eenheidskeuze. PDF/AI, HPGL, bitmap-DPI-flow ontbreken (`pdf_import_is_not_supported_yet`). |
| M021 Show Notes | File | – (`burnburn-history` heeft run-notities, geen job-notities) | – | – | geen | Niet in plan. |
| M022 Save | File ▸ Bewaar | `burnburn-io/src/project.rs`, `burnburn-presentation/src/controller.rs` | `failed_save_keeps_document_dirty_and_previous_file_readable`, `cancelled_save_panel_and_close_sheet_change_nothing` (presentation/tests/scenarios.rs) | ✔ | – | Cmd+S. |
| M023 Save As | File ▸ Bewaar als | zelfde als M022 | zelfde als M022 | ✔ | – | Cmd+Shift+S. |
| M024 Save As Template | File | – | – | – | geen | Sjabloon-concept ontbreekt; niet in plan (bibliotheken W2-L6b zijn iets anders). |
| M025 Save RD file | Production ▸ Exporteer RD | `burnburn-macos/src/appkit/app.rs` (`ExportRd`), `burnburn-machine/src/artifact.rs`, `burnburn-ruida/src/file.rs` | `export_sheet_ok_exports_rd_and_cancel_changes_nothing` (burnburn-macos/tests/adapter.rs), `headless_gate_b_same_revision_preview_and_rd` (presentation/tests/gate_b.rs) | ✔ | W1-HW | RD-bestand gemaakt. Ruida-dialect niet op hardware bewezen. |
| M026 Export | File ▸ Exporteer | `burnburn-io/src/export.rs` (SVG, DXF) | `svg_export_roundtrips_exactly_at_physical_scale`, `dxf_export_roundtrips_and_names_the_ellipse_approximation` (io/tests/export.rs) | – | W2-L1 | Export SVG/DXF als domein; geen exportdialoog in Mac. |
| M027 Preferences ▸ Import Prefs | File ▸ Voorkeuren | – | – | – | W3-L1 | Geen voorkeurenopslag. |
| M028 Preferences ▸ Export Prefs | File ▸ Voorkeuren | – | – | – | W3-L1 | Idem. |
| M029 Preferences ▸ Open Prefs Folder | File ▸ Voorkeuren | – | – | – | W3-L1 | Idem. |
| M030 Preferences ▸ Load Prefs Backup | File ▸ Voorkeuren | `burnburn-history/src/settings.rs` (backup van **machine**-instellingen, geen app-voorkeuren) | `settings_backup_is_journaled_and_read_back_per_machine` (history/tests/settings.rs) | – | W3-L1 | Alleen machine-backup bestaat. |
| M031 Preferences ▸ Edit Hotkeys | File ▸ Voorkeuren | `burnburn-editor/src/input.rs` (modifiers) | `modifier_flags_normalise_with_command_as_shortcut_modifier` (burnburn-macos/tests/adapter.rs) | – | W3-L1 | Vaste sneltoetsen; geen configureerbare editor. |
| M032 Bundles ▸ Import Bundle | File ▸ Bundels | – | – | – | W2-L6b | Bundels ontbreken. |
| M033 Bundles ▸ Export Bundle | File ▸ Bundels | – | – | – | W2-L6b | Idem. |
| M034 Print (Black Only) | File | – | – | – | geen | Printpijplijn staat niet in plan. |
| M035 Print (Keep Colors) | File | – | – | – | geen | Idem. |
| M036 Save Processed Bitmap | File | `burnburn-image/src/prepare.rs` (verwerkte pixels; export ontbreekt) | – | – | W2-L1f | |
| M037 Save Background Capture | File | `burnburn-macos/src/appkit/camera.rs` (capture) | `camera_asks_access_only_on_click_and_starts_the_chosen_device_in_bgra` (burnburn-macos/tests/presentation_wiring.rs) | – | W2-L5e | Cameracapture bestaat; achtergrond opslaan niet. |

### A3. Edit

| LightBurn-item | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| M038 Undo | Edit ▸ Undo | `burnburn-presentation/src/controller.rs`, `burnburn-document/src/engine.rs` | `headless_gate_a_rectangle_resize_undo_redo` (presentation/tests/scenarios.rs), `undo_redo_never_emits_machine_intent` (presentation/tests/gate_c.rs) | ✔ | – | Cmd+Z. |
| M039 Redo | Edit ▸ Redo | zelfde als M038 | `headless_gate_a_rectangle_resize_undo_redo` | ✔ | – | Cmd+Shift+Z. |
| M040 Undo History | Edit | `burnburn-history/src/history.rs` (journal) | – | – | W2-L1c | Historiepaneel ontbreekt. |
| M041 Select All | Edit ▸ Selecteer alles | `burnburn-macos/src/appkit/app.rs` (`select_all`), `burnburn-editor/src/selection.rs` | – | ✔ | – | Cmd+A via `selectAll:`. |
| M042 Invert Selection | Edit | – | – | – | W2-L2a | |
| M043 Cut | Edit | – | – | – | geen | Klembord ontbreekt; niet in plan. |
| M044 Copy | Edit | – | – | – | geen | Idem. |
| M045 Duplicate | Edit | – | – | – | geen | Idem (arrays W2-L1d zijn iets anders). |
| M046 Paste | Edit | – | – | – | geen | Idem. |
| M047 Paste in Place | Edit | – | – | – | geen | Idem. |
| M048 Delete | Edit ▸ Verwijder | `burnburn-document/src/edit.rs` (`GeometryCommand`) | `deleting_node_breaks_reference_to_merged_edge` (document/tests/modifiers.rs) | ✔ | – | Testnaam betreft knoopverwijdering; objectverwijdering via menu. |
| M049 Convert to Path | Edit | `burnburn-document/src/edit.rs` (`to_path`), `burnburn-document/src/structure/part.rs` | `convert_to_path_keeps_the_outline_drops_the_definition_and_is_undoable` (document/tests/parametric_shapes.rs) | – | W2-L1-tekenen | Domein en undo getest. Geen Edit-menu- of toolstrip-commando. |
| M050 Convert to Bitmap | Edit | – | – | – | geen | Niet in plan. |
| M051 Close Path | Edit | `burnburn-geometry/src/join.rs`; `burnburn-geometry/src/path.rs` | `a_single_open_shape_whose_ends_meet_is_closed` (geometry/tests/join_shapes.rs); `joining_one_shape_closes_it_in_place_and_far_pieces_are_refused` (document/tests/geometry_ops.rs) | – | W2-L1-tekenen | Sluiten op knoop- en objectniveau. Geen Mac-commando. |
| M052 Close Selected Paths With Tolerance | Edit | `burnburn-geometry/src/repair.rs` | `gaps_beyond_the_tolerance_stay_open` (geometry/tests/join_shapes.rs); `repair_closes_chosen_seam_but_keeps_intended_slot` (presentation/tests/edit_ops.rs) | – | W2-L1-tekenen | Reparatie sluit naden; tolerantie-dialoog ontbreekt. |
| M053 Auto-Join Selected Shapes | Edit | `burnburn-geometry/src/join.rs` | `two_lines_with_a_small_gap_become_one_polyline`, `closed_shapes_and_far_apart_designs_are_not_connected` (geometry/tests/join_shapes.rs); `joining_several_lines_replaces_them_by_one_object_and_undo_restores_them` (document/tests/geometry_ops.rs) | – | W2-L1-tekenen | Ruimtelijke join als domein en undo getest. Geen Mac-commando. |
| M054 Optimize Selected Shapes | Edit | `burnburn-geometry/src/simplify.rs` | `optimize_shapes_keeps_identity_and_reports_nothing_to_do` (document/tests/geometry_ops.rs); `a_nearly_straight_cubic_becomes_a_line_and_a_real_curve_stays` (geometry/tests/optimize.rs) | – | W2-L1-tekenen | Vereenvoudiging als domein en test. Geen Mac-commando. |
| M055 Delete Duplicates | Edit | – | – | – | geen | Niet in plan. |
| M056 Reverse Shape Direction | Edit | `burnburn-geometry/src/node_ops.rs`; `burnburn-geometry/src/path.rs` | `set_winding_reverses_only_when_needed_and_keeps_the_start_node` (geometry/tests/node_ops.rs); `winding_and_reverse_keep_start_nodes_and_open_ends_swap` (geometry/tests/path_ops.rs) | – | W2-L1-tekenen | Domein en test. Geen commando in Mac. |
| M057 Select Open Shapes | Edit | – | – | – | W2-L2a | |
| M058 Select Open Shapes Set to Fill | Edit | – | – | – | W2-L2a | |
| M059 Select All Shapes in Current Layer | Edit | `burnburn-presentation/src/controller/layers.rs` | – | – | W2-L2a | Laagselectie; geen commando. |
| M060 Flash Shapes in Current Layer | Edit | – | – | – | W2-L2a | |
| M061 Select Contained Shapes | Edit | – | – | – | W2-L2a | |
| M062 Select Shapes Smaller Than Selected | Edit | – | – | – | W2-L2a | |
| M063 Select Circles | Edit | – | – | – | W2-L2a | |
| M064 Image options ▸ Refresh Image | Edit | – | – | – | W2-L1f | |
| M065 Image options ▸ Replace Image | Edit | – | – | – | W2-L1f | |
| M066 Image options ▸ Replace Image to Fit | Edit | – | – | – | W2-L1f | |
| M067 Settings | Edit | – | – | – | W3-L1 | Voorkeurenvenster ontbreekt. |
| M068 AutoFill ▸ Contact… | Edit | – | – | – | geen | macOS-platform. |
| M069 AutoFill ▸ Passwords… | Edit | – | – | – | geen | macOS-platform; geen wachtwoordbeheer nodig. |
| M070 AutoFill ▸ Credit Card… | Edit | – | – | – | geen | macOS-platform; buiten scope. |
| M071 Start Dictation… | Edit | – | – | – | geen | macOS-platform. |
| M072 Emoji & Symbols | Edit | – | – | – | geen | macOS-platform. |

### A4. Tools

| LightBurn-item | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| M073 Select | Tools ▸ Selecteer | `burnburn-editor/src/tool/select.rs`; `burnburn-presentation/src/catalog.rs`; `burnburn-macos/src/appkit/toolstrip.rs` | `every_catalog_id_with_a_tool_activates_that_tool_and_is_marked_selected` (presentation/tests/toolbar.rs); `rubber_band_from_empty_canvas_selects_enclosed_objects` (editor/src/tool/select.rs) | ✔ | – | Toolstrip (`select`) en menu Gereedschap. |
| M074 Draw Lines | Tools ▸ Lijn | `burnburn-editor/src/tool/line.rs`; `burnburn-presentation/src/catalog.rs` | `ellipse_circle_line_polyline_and_pen_each_create_one_object_with_one_undo` (presentation/tests/im09_interaction.rs) | ✔ | – | Toolstrip: `sketch.line`, `sketch.polyline` en menu. |
| M075 Draw Shape ▸ Rectangle | Tools | `burnburn-editor/src/tool/draw.rs` | `typed_size_while_drawing_creates_an_exact_rectangle` (presentation/tests/im09_interaction.rs) | ✔ | – | Toolstrip: vier rechthoekvarianten (`sketch.rectangle.*`) en menu. |
| M076 Draw Shape ▸ Ellipse | Tools | `burnburn-editor/src/tool/draw.rs` | `ellipse_circle_line_polyline_and_pen_each_create_one_object_with_one_undo` | ✔ | – | Toolstrip: ellips (assen en vak) en menu. Cirkel (`sketch.circle.center`) staat ook in de toolstrip. |
| M077 Draw Shape ▸ Triangle | Tools | `burnburn-editor/src/tool/parametric.rs` (`ToolKind::Triangle`); `burnburn-geometry/src/shapes.rs` | `triangle_is_equilateral_with_closed_form_area_and_apex_up` (geometry/tests/param_shapes.rs) | – | W2-L1-tekenen | Domein en editor-gereedschap getest. Geen toolstrip-knop (niet in catalogus). |
| M078 Draw Shape ▸ Pentagon | Tools | `burnburn-editor/src/tool/parametric.rs` (`ToolKind::Pentagon`); `burnburn-geometry/src/shapes.rs` | `pentagon_and_octagon_presets_are_polygons_with_five_and_eight_sides` (geometry/tests/param_shapes.rs) | – | W2-L1-tekenen | Domein en editor-gereedschap getest. Geen toolstrip-knop. |
| M079 Draw Shape ▸ Polygon | Tools | `burnburn-editor/src/tool/polygon.rs`; `burnburn-presentation/src/catalog.rs` | `regular_polygons_have_vertices_on_the_circle_equal_sides_and_exact_area` (geometry/tests/param_shapes.rs) | ✔ | – | Toolstrip: veelhoek ingeschreven, omschreven en op zijde (`sketch.polygon.*`). |
| M080 Draw Shape ▸ Octagon | Tools | `burnburn-editor/src/tool/parametric.rs` (`ToolKind::Octagon`); `burnburn-geometry/src/shapes.rs` | `pentagon_and_octagon_presets_are_polygons_with_five_and_eight_sides` (geometry/tests/param_shapes.rs) | – | W2-L1-tekenen | Domein en editor-gereedschap getest. Geen toolstrip-knop. |
| M081 Draw Shape ▸ Star | Tools | `burnburn-editor/src/tool/parametric.rs` (`ToolKind::Star`); `burnburn-geometry/src/shapes.rs` | `star_alternates_tips_and_valleys_with_exact_area` (geometry/tests/param_shapes.rs) | – | W2-L1-tekenen | Domein en editor-gereedschap getest. Geen toolstrip-knop en geen parameterpaneel. |
| M082 Draw Shape ▸ Dual Star | Tools | `burnburn-editor/src/tool/parametric.rs` (`ToolKind::DoubleStar`); `burnburn-geometry/src/shapes.rs` | `double_star_is_two_stars_half_a_step_apart`, `double_star_of_three_points_has_its_six_tips_on_a_regular_hexagon` (geometry/tests/param_shapes.rs) | – | W2-L1-tekenen | Domein en editor-gereedschap getest. Geen toolstrip-knop. |
| M083 Edit Nodes | Tools ▸ Knopen | `burnburn-editor/src/tool/node.rs`; `burnburn-geometry/src/node.rs`, `node_ops.rs` | `node_edit_mode_selects_nodes_and_breaks_the_path_as_one_undoable_step` (editor/tests/editor_ops.rs); `smoothing_a_right_angle_corner_makes_collinear_handles_along_the_bisector` (geometry/tests/node_ops.rs) | deels | W2-L1-tekenen | Node-gereedschap in toolstrip (`node`); verplaatsen en breken werken. Smooth/corner, symmetrisch en curve↔lijn: geometrie getest, geen Mac-bediening. |
| M084 Trim Shapes | Tools | `burnburn-editor/src/tool/trim.rs`; `burnburn-document/src/edit.rs` (trim) | `trimming_a_circle_and_a_line_with_curve_aware_trim` (document/tests/modify_commands.rs); `trim_mode_click_removes_the_picked_piece_up_to_the_crossing` (presentation/tests/edit_ops.rs) | deels | W2-L1-tekenen | Toolstrip: Trim (`sketch.trim`, curve-aware). Trimmodus (`CommandKey::ToolTrim`) heeft geen Mac-koppeling. |
| M085 Add Tabs | Tools | `burnburn-cam/src/features.rs`, `burnburn-cam/src/line.rs` | `tabs_leave_evenly_spaced_bridges_and_reject_tabs_longer_than_the_contour` (cam/tests/process_features.rs); `tabs_by_spacing_get_their_count_from_each_contour_length`, `tabs_that_cannot_be_placed_are_refused_not_clamped` (cam/tests/line_extras.rs) | – | W2-L1-cut | Tab-domein in CAM getest; geen tabgereedschap of veld in Mac. |
| M086 Edit Text | Tools | `burnburn-document/src/text/model.rs`, `burnburn-text/src/` | `committed_text_is_one_text_command_and_extends_the_selected_text` (presentation/tests/gui_requests.rs); `text_is_placed_where_the_text_tool_was_clicked` (presentation/tests/tool_steps.rs) | deels | W1-L5b | Tekst-gereedschap in toolstrip (`sketch.text`); tekstinvoer via IME (`tests/native/macos/canvas_text_input.rs`). Geen tekstbewerking of tekstbalk. |
| M087 Position Laser | Tools | `burnburn-machine/src/laser.rs`; `burnburn-machine/src/frame.rs` | `position_laser_converts_a_canvas_click_in_bed_view_to_machine_coordinates` (machine/tests/laser_actions.rs) | – | W2-L1-laser | Domein getest. Geen Mac-commando of canvasklik in het laservenster. |
| M088 Measure | Tools ▸ Meten | `burnburn-presentation/src/task.rs` (measure-taak) | `measure_reports_count_and_union_bounds`, `cancelled_measure_returns_cancelled_not_partial_result` (presentation/src/task.rs) | ✔ | – | Tools-menu `Measure`. |
| M089 Create Bar Code | Tools | `burnburn-generators/src/barcode.rs` (Code 128), `burnburn-generators/src/qr.rs` | `code128_symbols_match_hand_computed_vectors`, `qr_geometry_is_one_square_per_dark_module_inside_the_quiet_zone` (generators/tests) | – | W2-L5d | Code 128 en QR als domein; 27 van 29 symbologieën ontbreken (tabel C). |
| M090 Offset Shapes | Tools | presentatie `Offset`-intent; `burnburn-editor/src/tool/offset.rs`; `burnburn-geometry` | `offset_panel_adds_one_offset_object_in_one_entry`, `impossible_offset_reports_location_and_keeps_document` (presentation/tests/edit_ops.rs) | deels | W2-L1-tekenen | Offset-gereedschap in toolstrip (`sketch.offset`). Richting, hoekstijl en bronobject verwijderen ontbreken in Mac. |
| M091 Weld Shapes | Tools | – | – | – | W2-L2c | |
| M092 Boolean Union | Tools | `burnburn-geometry/src/boolean.rs`; presentatie `BooleanUnion` | `applying_a_boolean_is_one_undo_entry` (presentation/tests/im09_interaction.rs); `boolean_fixture_cases_match_hand_derived_areas` (geometry/tests/boolean_fixtures.rs) | – | W2-L2c | Preview en toepassen in presentatie; geen Mac-menu. |
| M093 Boolean Subtract | Tools | zelfde als M092 | `applying_a_boolean_is_one_undo_entry` | – | W2-L2c | Operandvolgorde bepaalt uitkomst. |
| M094 Boolean Intersection | Tools | zelfde als M092 | `applying_a_boolean_is_one_undo_entry` | – | W2-L2c | |
| M095 Boolean Assistant | Tools | – | – | – | W2-L2c | |
| M096 Cut Shapes | Tools | `burnburn-geometry/src/cut.rs`; `burnburn-document/src/edit.rs` | `cut_shapes_replaces_the_subject_by_its_pieces_and_undo_restores_both_objects`, `cut_shapes_without_a_crossing_or_with_bad_operands_changes_nothing` (document/tests/geometry_ops.rs); `a_rectangle_cutter_splits_a_square_into_inside_and_outside_halves` (geometry/tests/cut_shapes.rs) | – | W2-L1-tekenen | Domein en undo getest. Geen Mac-commando. |
| M097 Adjust Image | Tools | `burnburn-image/src/adjust.rs` | – | – | W2-L1-beeld | Helderheid/contrast/gamma in domein; geen paneel. |
| M098 Trace Image | Tools | `burnburn-trace/src/lib.rs` (recept, resultaat); presentatie `ApplyTrace`/`CloseTracer` | `headless_gate_b_ring_trace_keeps_hole` (presentation/tests/gate_b.rs) | – | W2-L1g | Tracer-paneel ontbreekt in Mac-venster. |
| M099 Multi-File Trace (Labs) | Tools | – | – | – | W2-L1g | Labs-capability; geen batch. |
| M100 Apply Path to Text | Tools | `burnburn-generators/src/path_text.rs` | `text_on_path_reports_overflow_and_multiline`, `text_on_circle_sits_at_radius_minus_offset_and_follows_the_tangent` (generators/tests/patterns.rs) | – | W2-L1e | Domein en test; geen UI. |
| M101 Apply Mask to Image | Tools | `burnburn-image/src/mask.rs` | `mask_burns_only_inside_the_geometry` (cam/tests/image_modes.rs); `region_masks_include_and_exclude_without_touching_source` (image/tests/pipeline.rs) | – | W2-L1-beeld | |
| M102 Crop Image | Tools | `burnburn-image/src/adjust.rs` (crop) | `crop_stretches_only_the_window_over_the_frame`, `a_crop_outside_the_source_is_a_compile_error` (cam/tests/image_modes.rs); `crop_and_quarter_turns_map_pixels_and_swap_scale` (image/tests/pipeline.rs) | – | W2-L1-beeld | |
| M103 Resize Slots in Selection | Tools | – | – | – | W2-L1-tekenen | Sleufgeometrie-detectie ontbreekt. |
| M104 Warp Selection (4 points) | Tools | `burnburn-image/src/perspective.rs` (beeldperspectief; vectorwarp ontbreekt) | ? | – | W2-L2c | |
| M105 Deform Selection (16 points) | Tools | – | – | – | W2-L2c | |
| M106 Generate Tangent Circle | Tools | `burnburn-geometry/src/construct.rs`; `burnburn-editor/src/tool/circle.rs` | `circle_tangent_to_two_lines_with_given_radius`, `circle_tangent_to_three_lines_is_the_incircle_or_an_excircle_near_the_cursor` (geometry/tests/construct.rs) | deels | W2-L2 | Toolstrip: cirkel tangent aan twee of drie lijnen (`sketch.circle.two_tangent`, `three_tangent`). Tangent aan geselecteerde vormen ontbreekt. |

### A5. Arrange

| LightBurn-item | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| M107 Group | Arrange ▸ Groepeer | `burnburn-document/src/edit.rs` (`GeometryCommand::Group`) | `nested_groups_and_ungroup_restore_membership` (document/tests/geometry_commands.rs) | – | W2-L1c | Domein; geen menu. |
| M108 Ungroup | Arrange ▸ Groep opheffen | zelfde als M107 | `nested_groups_and_ungroup_restore_membership` | – | W2-L1c | |
| M109 Auto-Group | Arrange | – | – | – | W2-L1c | Insluiting-groepering ontbreekt. |
| M110 Flip Horizontal | Arrange | `burnburn-geometry/src/affine.rs` (`mirror_x`) | `mirroring_reverses_winding` (geometry/tests/paths.rs) | – | W2-L1c | |
| M111 Flip Vertical | Arrange | `burnburn-geometry/src/affine.rs` | `mirroring_reverses_winding` | – | W2-L1c | |
| M112 Mirror Across Line | Arrange | `burnburn-sketch` (symmetrie-relatie, geen objectspiegel) | `symmetric_points_mirror_about_a_line` (sketch/tests/solver_contract.rs) | – | W2-L1c | Spiegelen om lijn voor objecten ontbreekt; toolstrip-spiegel (`sketch.mirror`) is Deferred. |
| M113 Rotate 90° Clockwise | Arrange | `burnburn-presentation/src/controller.rs` (`rotate_selection`) | `rotation_field_turns_the_selection_about_its_centre_in_one_command` (presentation/tests/im09_interaction.rs) | – | W2-L1c | Rotatieveld ✔; 90°-commando ontbreekt. |
| M114 Rotate 90° Counter-Clockwise | Arrange | zelfde als M113 | zelfde als M113 | – | W2-L1c | |
| M115 Two-Point Rotate / Scale | Arrange | – | – | – | W2-L1c | |
| M116 Align ▸ Align Centers | Arrange ▸ Lijn uit | `burnburn-geometry/src/align.rs`; presentatie `AlignCenterX`/`AlignCenterY` | `align_and_distribute_of_many_objects_is_one_entry` (presentation/tests/im09_interaction.rs) | – | W2-L2a | Alleen X en Y apart; gecombineerd commando niet gevonden. |
| M117 Align ▸ Align Left | Arrange ▸ Lijn uit | `burnburn-geometry/src/align.rs`; presentatie `AlignLeft` | `align_left_moves_to_the_reference_in_one_entry_and_keeps_sizes` (presentation/tests/im09_interaction.rs) | – | W2-L2a | Geen Mac-menu. |
| M118 Align ▸ Align Right | Arrange ▸ Lijn uit | `burnburn-geometry/src/align.rs`; `AlignRight` | `align_and_distribute_of_many_objects_is_one_entry` | – | W2-L2a | |
| M119 Align ▸ Align Top | Arrange ▸ Lijn uit | `burnburn-geometry/src/align.rs`; `AlignTop` | `align_and_distribute_of_many_objects_is_one_entry` | – | W2-L2a | |
| M120 Align ▸ Align Bottom | Arrange ▸ Lijn uit | `burnburn-geometry/src/align.rs`; `AlignBottom` | `align_and_distribute_of_many_objects_is_one_entry` | – | W2-L2a | |
| M121 Align ▸ Align H-Center | Arrange ▸ Lijn uit | `burnburn-geometry/src/align.rs`; `AlignCenterX` | `align_and_distribute_of_many_objects_is_one_entry` | – | W2-L2a | |
| M122 Align ▸ Align V-Center | Arrange ▸ Lijn uit | `burnburn-geometry/src/align.rs`; `AlignCenterY` | `align_and_distribute_of_many_objects_is_one_entry` | – | W2-L2a | |
| M123 Distribute ▸ H-Spaced | Arrange ▸ Verdeel | `burnburn-geometry/src/align.rs`; `DistributeHorizontal` | `align_and_distribute_of_many_objects_is_one_entry` | – | W2-L2a | |
| M124 Distribute ▸ H-Centered | Arrange ▸ Verdeel | `burnburn-geometry/src/align.rs` | `align_and_distribute_of_many_objects_is_one_entry` | – | W2-L2a | Apart commando voor gecentreerd niet gevonden. |
| M125 Distribute ▸ V-Spaced | Arrange ▸ Verdeel | `burnburn-geometry/src/align.rs`; `DistributeVertical` | `align_and_distribute_of_many_objects_is_one_entry` | – | W2-L2a | |
| M126 Distribute ▸ V-Centered | Arrange ▸ Verdeel | `burnburn-geometry/src/align.rs` | `align_and_distribute_of_many_objects_is_one_entry` | – | W2-L2a | Zie M124. |
| M127 Move H-Together | Arrange ▸ Verdeel | – | – | – | W2-L2a | |
| M128 Move V-Together | Arrange ▸ Verdeel | – | – | – | W2-L2a | |
| M129 Nest Selected | Arrange | `burnburn-cam/src/nest.rs` | `applying_a_nest_moves_instances_onto_the_plate_in_one_undo_step` (document/tests/nest_on_plate.rs); `nesting_keeps_order_counts_and_respects_grain_rotation` (cam/tests/nest.rs) | – | W3-L1 | Nestingpaneel als taak ontbreekt. |
| M130 Quick Nest | Arrange | `burnburn-cam/src/nest.rs` | zelfde als M129 | – | W3-L1 | |
| M131 Dock ▸ Dock Left | Arrange ▸ Dock | – | – | – | W2-L2a | |
| M132 Dock ▸ Dock Right | Arrange ▸ Dock | – | – | – | W2-L2a | |
| M133 Dock ▸ Dock Up | Arrange ▸ Dock | – | – | – | W2-L2a | |
| M134 Dock ▸ Dock Down | Arrange ▸ Dock | – | – | – | W2-L2a | |
| M135 Move Selected Objects ▸ Move to Laser Position | Arrange | `burnburn-machine/src/frame.rs`, `burnburn-job/src/profile.rs` | `machine_frame_maps_each_origin_corner_and_back` (job/tests/plan.rs) | – | W1-L4a | |
| M136 Move Selected Objects ▸ Move to Page Center | Arrange | `burnburn-editor/src/arrange.rs` (geen positie-commando gevonden) | `current_position_puts_each_3x3_anchor_of_the_job_on_the_laser` (machine/tests/placement.rs) | – | W2-L1-laser | 3×3-referentie voor de job zit in `burnburn-machine/src/placement.rs` (test: `current_position_puts_each_3x3_anchor_of_the_job_on_the_laser`); het paginapositie-commando ontbreekt in het Mac-venster. |
| M137 Move Selected Objects ▸ Move to Upper Left | Arrange | zelfde als M136 | – | – | W2-L1c | |
| M138 Move Selected Objects ▸ Move to Upper Right | Arrange | zelfde als M136 | – | – | W2-L1c | |
| M139 Move Selected Objects ▸ Move to Lower Left | Arrange | zelfde als M136 | – | – | W2-L1c | |
| M140 Move Selected Objects ▸ Move to Lower Right | Arrange | zelfde als M136 | – | – | W2-L1c | |
| M141 Move Selected Objects ▸ Move to Left | Arrange | zelfde als M136 | – | – | W2-L1c | |
| M142 Move Selected Objects ▸ Move to Right | Arrange | zelfde als M136 | – | – | W2-L1c | |
| M143 Move Selected Objects ▸ Move to Top | Arrange | zelfde als M136 | – | – | W2-L1c | |
| M144 Move Selected Objects ▸ Move to Bottom | Arrange | zelfde als M136 | – | – | W2-L1c | |
| M145 Move Laser to Selection ▸ Move Laser to Selection Center | Arrange | – | – | – | W1-L4a | |
| M146 Move Laser to Selection ▸ Move Laser to Upper Left of Selection | Arrange | – | – | – | W1-L4a | |
| M147 Move Laser to Selection ▸ Move Laser to Upper Right of Selection | Arrange | – | – | – | W1-L4a | |
| M148 Move Laser to Selection ▸ Move Laser to Lower Left of Selection | Arrange | – | – | – | W1-L4a | |
| M149 Move Laser to Selection ▸ Move Laser to Lower Right of Selection | Arrange | – | – | – | W1-L4a | |
| M150 Move Laser to Selection ▸ Move Laser to Top of Selection | Arrange | – | – | – | W1-L4a | |
| M151 Move Laser to Selection ▸ Move Laser to Bottom of Selection | Arrange | – | – | – | W1-L4a | |
| M152 Move Laser to Selection ▸ Move Laser to Left of Selection | Arrange | – | – | – | W1-L4a | |
| M153 Move Laser to Selection ▸ Move Laser to Right of Selection | Arrange | – | – | – | W1-L4a | |
| M154 Move Laser to Selection ▸ Jog Laser Left | Arrange | `burnburn-machine/src/laser.rs` (`step_jog`); `burnburn-machine/src/actor/live.rs` | `step_jog_needs_a_known_position_and_follows_the_view_directions` (machine/tests/laser_actions.rs); `jog_press_and_release_are_two_requests_and_a_second_key_is_refused_while_one_is_held` (machine/tests/live.rs) | – | W2-L1-laser | Domein en Ruida-registry-bytes getest (niet op hardware). Geen jog-knop in het Mac-venster. |
| M155 Move Laser to Selection ▸ Jog Laser Right | Arrange | zelfde als M154 | zelfde als M154 | – | W2-L1-laser | |
| M156 Move Laser to Selection ▸ Jog Laser Down | Arrange | zelfde als M154 | zelfde als M154 | – | W2-L1-laser | |
| M157 Move Laser to Selection ▸ Jog Laser Up | Arrange | zelfde als M154 | zelfde als M154 | – | W2-L1-laser | |
| M158 Grid / Array | Arrange ▸ Grid/Array | `burnburn-cam/src/array.rs`, `burnburn-document/src/structure/modifier.rs` | `grid_array_modifier_outputs_every_copy_and_follows_source_edits` (document/tests/array_modifiers.rs); `grid_and_circular_arrays_generate_placements` (cam/tests/nest.rs) | – | W2-L1d | Domein en dialoogloze array getest. Toolstrip-Patroon-paneel is Deferred (uitgeschakeld met reden). |
| M159 Circular Array | Arrange | `burnburn-cam/src/array.rs`, `burnburn-document/src/structure/modifier.rs` | `circular_array_modifier_turns_copies_about_the_centre` (document/tests/array_modifiers.rs) | – | W2-L1d | Domein en dialoogloze array getest. Toolstrip-Patroon-paneel is Deferred (uitgeschakeld met reden). |
| M160 Copy Along Path | Arrange | – | – | – | W2-L1d | |
| M161 Create rubber band outline from selection | Arrange | – | – | – | W3-L1 | Convexe omhullende hoort bij nesting. |
| M162 Break Apart | Arrange | – | – | – | W2-L1c | Samengestelde geometrie splitsen ontbreekt. |
| M163 Push Forward in Draw Order | Arrange ▸ Tekenvolgorde | – | – | – | geen | Renderordening staat los van cut order; niet in plan. |
| M164 Push Backward in Draw Order | Arrange ▸ Tekenvolgorde | – | – | – | geen | Idem. |
| M165 Push to Front | Arrange ▸ Tekenvolgorde | – | – | – | geen | Idem. |
| M166 Push to Back | Arrange ▸ Tekenvolgorde | – | – | – | geen | Idem. |
| M167 Lock Selected Shapes | Arrange | – | – | – | W2-L2a | Vergrendeling ontbreekt. |
| M168 Unlock Selected Shapes | Arrange | – | – | – | W2-L2a | |

### A6. Laser Tools

| LightBurn-item | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| M169 Print and Cut ▸ Start Wizard | Laser Tools | – | – | – | W2-L2d | |
| M170 Print and Cut ▸ Set First Target Position | Laser Tools | – | – | – | W2-L2d | |
| M171 Print and Cut ▸ Set Second Target Position | Laser Tools | – | – | – | W2-L2d | |
| M172 Print and Cut ▸ Align Output to Targets | Laser Tools | – | – | – | W2-L2d | |
| M173 Print and Cut ▸ Align Output to Targets (no scaling) | Laser Tools | – | – | – | W2-L2d | |
| M174 Print and Cut ▸ Reset Print and Cut | Laser Tools | – | – | – | W2-L2d | |
| M175 Manage Cameras | Laser Tools | `burnburn-macos/src/appkit/camera.rs`, `burnburn-macos/src/camera.rs`, `burnburn-presentation/src/camera.rs` | `camera_starts_only_after_permission_and_shows_the_latest_frame_as_visual_placement` (presentation/tests/gui_requests.rs) | deels | W2-L5e | Cameraspaneel met start/stop; geen bronlijst of kalibratie. |
| M176 Rotary Setup | Laser Tools | – | – | – | W2-L4b | |
| M177 Feeder Setup | Laser Tools | – | – | – | W2-L3d | |
| M178 Focus Test | Laser Tools | – | – | – | W2-L3e | Vereist Z. |
| M179 Interval Test | Laser Tools | – | – | – | W2-L3e | |
| M180 Material Test | Laser Tools | – | – | – | W2-L3e | |
| M181 Center Finder | Laser Tools | – | – | – | W2-L2d | |
| M182 Taper Warp | Laser Tools | – | – | – | geen | Alleen DOC-index; Ruida niet gerealiseerd (02-features §11). |
| M183 Manage Devices | Laser Tools | `burnburn-job/src/profile.rs` | `capability_matrix_gates_each_feature_with_a_typed_error` (cam/tests/process_features.rs) | – | W2-L4b | Profielen zijn er; beheerdialoog niet. |
| M184 Device Settings | Laser Tools | `burnburn-job/src/profile.rs` | `machine_frame_maps_each_origin_corner_and_back` (job/tests/plan.rs) | – | W2-L4b | Zie tabel B. |
| M185 Machine Settings | Laser Tools | `burnburn-machine/src/settings.rs`, `burnburn-machine/src/controller_files.rs` | `controller_file_actions_need_explicit_intent_and_proven_capability` (machine/tests/management.rs) | – | geen | Read/Write-schema van controller ontbreekt; afhankelijk van W1-HW. Pakket toe te voegen. |
| M186 Get Controller Info | Laser Tools | `burnburn-machine/src/settings.rs` | – | – | geen | Zie M185. |

### A7. Web Tools

| LightBurn-item | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| M187 Cuttle | Web Tools | – | – | – | geen | Externe ontwerpservice; besluit nodig of dit in scope valt. |

### A8. Window

| LightBurn-item | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| M188 Reset to Default Layout | Window | – | – | – | W3-L1 | Paneellay-out. |
| M189 Preview | Window ▸ Preview | `burnburn-job/src/preview.rs`; plan-paneel in `burnburn-macos/src/panels/plan.rs` | `preview_moves_trace_to_source_and_estimate_time` (job/tests/preview.rs); `headless_gate_b_same_revision_preview_and_rd` (presentation/tests/gate_b.rs) | deels | W2-L1h | Plan met status en schatting (`plan_panel_names_the_compiled_plan_and_marks_it_stale_after_an_edit`); geen simulatie of scrubben. |
| M190 Zoom to Page | Window | `burnburn-editor/src/view.rs` | `zoom_keeps_anchor_world_point_fixed` (editor/src/view.rs) | – | W2-L1b | Geen menu-item. |
| M191 Zoom In | Window | `burnburn-editor/src/view.rs` | `zoom_keeps_anchor_world_point_fixed` | deels | W2-L1b | Zoomen via scroll/pinch (`trackpad_scroll_pans_wheel_lines_scale_and_pinch_zooms`, burnburn-macos/tests/adapter.rs); geen commando. |
| M192 Zoom Out | Window | `burnburn-editor/src/view.rs` | `zoom_keeps_anchor_world_point_fixed` | deels | W2-L1b | Idem M191. |
| M193 Frame Selection | Window | – | – | – | W2-L1b | |
| M194 View Style: Wireframe / Coarse | Window | – | – | – | W2-L1b | Weergavemodus ontbreekt. |
| M195 View Style: Wireframe / Smooth | Window | – | – | – | W2-L1b | |
| M196 View Style: Filled / Coarse | Window | – | – | – | W2-L1b | |
| M197 View Style: Filled / Smooth | Window | – | – | – | W2-L1b | |
| M198 Toggle Wireframe / Filled | Window | – | – | – | W2-L1b | |
| M199 Toggle Side Panels | Window | – | – | – | W3-L1 | |
| M200 Main | Window | `burnburn-macos/src/appkit/window.rs` | `tests/native/macos/native_canvas_surface.rs` (harness) | ✔ | – | Hoofdvenster met canvas. |
| M201 Numeric Edits | Window | `burnburn-presentation/src/view_model.rs` (`InspectorViewModel`); `burnburn-macos/src/panels/mod.rs` (`INSPECTOR_FIELDS`) | `inspector_has_x_y_width_height_and_each_commits_raw_text` (burnburn-macos/tests/panels.rs); `tests/native/macos/native_dimension_commit.rs` (harness) | ✔ | W2-L1c | X/Y/B/H/rotatie; %-schaal en aspectlock ontbreken. |
| M202 Creation Tools | Window | `burnburn-presentation/src/catalog.rs` (register); `burnburn-macos/src/appkit/toolstrip.rs` | `every_catalog_id_with_a_tool_activates_that_tool_and_is_marked_selected` (presentation/tests/toolbar.rs); `every_catalog_id_has_a_label_in_both_languages` (macos/tests/toolbar.rs); `a_tool_button_selects_that_catalog_tool_for_that_window` (macos/tests/panels.rs) | ✔ | W2-L1-tekenen | Toolstrip met zes panelen (Maak, Wijzig, Patroon, Beperkingen, Invoegen, Selecteer); 54 van 63 catalogusitems zijn actieve gereedschappen. Ster, driehoek, vijf-/achthoek, dubbele ster, afrondhoek en startpunt ontbreken in de catalogus. |
| M203 Text Options | Window | – | – | – | W1-L5b | Tekstbalk ontbreekt. |
| M204 Modifiers | Window | `burnburn-document/src/structure/modifier.rs` | `modifier_on_modifier_output_follows_and_cycles_are_refused` (document/tests/modifiers.rs) | – | W2-L1c | Modifierstapel niet in Mac-venster. |
| M205 Arrange | Window | `burnburn-editor/src/arrange.rs` | `arrange_and_boolean_commands_follow_the_selection` (presentation/tests/im09_interaction.rs) | – | W2-L2a | |
| M206 Arrange (long) | Window | zelfde als M205 | zelfde als M205 | – | W2-L2a | |
| M207 Color Palette | Window | `burnburn-presentation/src/controller/layers.rs`; `burnburn-macos/src/panels/operations.rs` (`palette_buttons`) | `palette_click_becomes_assign_layer_and_shift_adds` (burnburn-macos/tests/layer_panels.rs); `colour_click_with_selection_creates_a_layer_and_moves_objects_between_colours` (presentation/tests/layer_palette.rs) | ✔ | – | 30 laagkleuren C00–C29; T1/T2 niet gevonden. |
| M208 Docking | Window | – | – | – | W3-L1 | Panelen zijn vast. |
| M209 Modes | Window | `burnburn-macos/src/panels/cut_settings.rs` (`SHEET_MODES`, `sheet_mode`) | `blocked_mode_is_listed_disabled_with_the_missing_capability` (burnburn-macos/tests/layer_panels.rs) | deels | W1-L3b | Line/Fill/Offset Fill in Cut Settings-sheet. Image-modus staat niet in de sheet (`SHEET_MODES` telt 3). |
| M210 Art Library | Window | – | – | – | W2-L6b | |
| M211 Cameras | Window | `burnburn-macos/src/appkit/camera.rs`; `burnburn-macos/src/panels/camera.rs` (`camera_panel`) | `camera_starts_only_after_permission_and_shows_the_latest_frame_as_visual_placement` (presentation/tests/gui_requests.rs) | deels | W2-L5e | Cameraspaneel; geen overlay op canvas. |
| M212 Console | Window | – | – | – | W4 | G-code-backend. |
| M213 Cuts / Layers | Window | `burnburn-presentation/src/controller/layers.rs`; `burnburn-macos/src/panels/operations.rs` (`OperationLine`) | `layer_table_edits_power_min_passes_and_kerf_as_raw_text` (presentation/tests/layer_palette.rs); `visibility_and_output_toggle_independently` (presentation/tests/layer_palette.rs) | deels | W2-L3a | Laagtabel en Cut Settings-sheet; sublagen en tool-lagen ontbreken; Make Default ontbreekt. |
| M214 Laser | Window | `burnburn-presentation` (machinepaneel); `burnburn-machine/src/actor.rs`; `burnburn-macos/src/panels/machine.rs` (`machine_panel`) | `machine_panel_shows_status_message_progress_and_buttons`, `machine_buttons_send_their_action_unchanged_and_disabled_ones_send_nothing` (burnburn-macos/tests/production_panels.rs) | deels | W1-L4a → W2-L1i | Upload/Start/Cancel/Status/Reconnect-knoppen. Frame, home, jog, pauze en start-from: domein getest, geen intent of knop. |
| M215 File List | Window | – | – | – | W1-L4a | DSP-bestandslijst ontbreekt. Domein: `burnburn-machine/src/controller_files.rs`; test: `file_list_and_metadata_replies_are_kept_as_data` (machine/tests/live.rs). |
| M216 Macros | Window | – | – | – | W4 | |
| M217 Material Library | Window | – | – | – | W2-L6b | |
| M218 Move | Window | – | – | – | W1-L4a | Jogpaneel ontbreekt. |
| M219 Shape Properties | Window | `burnburn-presentation/src/view_model.rs` (`InspectorViewModel`) | `inspector_has_x_y_width_height_and_each_commits_raw_text` (burnburn-macos/tests/panels.rs) | deels | W1-L2b | Alleen selectiegeometrie. Schetsparameters ✔ in het sketch-paneel; vormparameters (ster, veelhoek) ontbreken in het Mac-venster. |
| M220 Variable Text | Window | `burnburn-generators/src/rows.rs` (CSV); `burnburn-presentation/src/production.rs` | `csv_parses_quotes_escapes_crlf_and_empty_cells`, `frozen_rows_are_reproducible_and_bound_by_hash` (generators/tests/personalisation.rs) | – | W1-L5c | Paneel en advance/bake ontbreken. |

### A9. Language

Alleen **Nederlands** en **English** bestaan als catalogus in `burnburn-macos/src/l10n.rs` (`Locale::Nl`, `Locale::En`); de overige 25 talen zijn er niet.

| LightBurn-item | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| M221 العربية (Arabic) | Language | – | – | – | W3-L1 | Geen catalogus; RTL ontbreekt. |
| M222 Čeština (Czech) | Language | – | – | – | W3-L1 | Geen catalogus. |
| M223 Dansk (Danish) | Language | – | – | – | W3-L1 | Geen catalogus. |
| M224 Deutsch (German) | Language | – | – | – | W3-L1 | Geen catalogus. |
| M225 Ελληνικά (Greek) | Language | – | – | – | W3-L1 | Geen catalogus. |
| M226 English | Language | `burnburn-macos/src/l10n.rs` | `locale_follows_the_first_supported_preferred_language` (burnburn-macos/tests/l10n.rs); `layer_message_keys_have_nl_and_en` (burnburn-macos/tests/layer_panels.rs) | deels | W3-L1 | Ondersteund; geen keuzemenu, volgt systeemtaal. |
| M227 Español (Spanish) | Language | – | – | – | W3-L1 | Geen catalogus. |
| M228 Español (Latin American) | Language | – | – | – | W3-L1 | Geen catalogus. |
| M229 Suomi (Finnish) | Language | – | – | – | W3-L1 | Geen catalogus. |
| M230 Français (French) | Language | – | – | – | W3-L1 | Geen catalogus. |
| M231 Magyar (Hungarian) | Language | – | – | – | W3-L1 | Geen catalogus. |
| M232 Italiano (Italian) | Language | – | – | – | W3-L1 | Geen catalogus. |
| M233 日本語 (Japanese) | Language | – | – | – | W3-L1 | Geen catalogus; CJK-font ontbreekt. |
| M234 한국어 (Korean) | Language | – | – | – | W3-L1 | Geen catalogus. |
| M235 Nederlands (Dutch) | Language | `burnburn-macos/src/l10n.rs` | `locale_follows_the_first_supported_preferred_language` (burnburn-macos/tests/l10n.rs) | deels | W3-L1 | Ondersteund; geen keuzemenu. |
| M236 Norsk (Norwegian) | Language | – | – | – | W3-L1 | Geen catalogus. |
| M237 Język Polski (Polish) | Language | – | – | – | W3-L1 | Geen catalogus. |
| M238 Português Brasileiro (Brazilian Portuguese) | Language | – | – | – | W3-L1 | Geen catalogus. |
| M239 Português (Portuguese) | Language | – | – | – | W3-L1 | Geen catalogus. |
| M240 Русский (Russian) | Language | – | – | – | W3-L1 | Geen catalogus. |
| M241 Slovenski Jezik (Slovenian) | Language | – | – | – | W3-L1 | Geen catalogus. |
| M242 Svenska (Swedish) | Language | – | – | – | W3-L1 | Geen catalogus. |
| M243 ภาษาไทย (Thai) | Language | – | – | – | W3-L1 | Geen catalogus. |
| M244 Türkçe (Turkish) | Language | – | – | – | W3-L1 | Geen catalogus. |
| M245 Українська (Ukrainian) | Language | – | – | – | W3-L1 | Geen catalogus. |
| M246 汉语 (Simplified Chinese) | Language | – | – | – | W3-L1 | Geen catalogus. |
| M247 漢語 (Traditional Chinese) | Language | – | – | – | W3-L1 | Geen catalogus. |

### A10. Help

| LightBurn-item | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| M248 End User License Agreement | Help | – | – | – | geen | Licentietekst; niet nodig voor functionele pariteit. |
| M249 LightBurn Support Forum | Help | – | – | – | W3-L1 | Externe link. |
| M250 Submit Feature Request | Help | – | – | – | W3-L1 | Externe link. |
| M251 Quick Help and Notes | Help | – | – | – | W3-L1 | Helpsysteem. |
| M252 Online Documentation | Help | – | – | – | W3-L1 | Externe link. |
| M253 PDF Documentation Link | Help | – | – | – | W3-L1 | Externe link. |
| M254 Online Video Tutorials | Help | – | – | – | W3-L1 | Externe link. |
| M255 Generate Support Data | Help | – | – | – | W3-L1 | Supportbundel; moet tonen welke gegevens gebundeld worden. |
| M256 Camera Selection Help | Help | – | – | – | W2-L5e | |
| M257 Check for Updates | Help | – | – | – | W4 | Installer/updates. |
| M258 License Management | Help | – | – | – | geen | Licentiesysteem mag eigen ontwerp krijgen (02-features §11). |
| M259 Enable Debug Log | Help | – | – | – | W3-L1 | Debuglog. |
| M260 Debug Drawing | Help | – | – | – | geen | Diagnostiek; niet in plan. |
| M261 Convert to Cut (Debug) | Help | – | – | – | geen | Diagnostiek; mag geen hardwarejob starten. |
| M262 Real-Time Statistics | Help | – | – | – | geen | Niet in plan. |

## B. Instellingen (03-instellingen.md)

### B1. Algemene voorkeuren

| Instelling (LightBurn) | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| S01 Default font en hoogte | Voorkeuren ▸ Editor / tekst | `burnburn-text/src/font.rs` | – | – | W1-L5b | |
| S02 RTL-ondersteuning aan | Voorkeuren ▸ Editor / tekst | – | – | – | W1-L5b | |
| S03 SHX-pad | Voorkeuren ▸ Editor / tekst | – | – | – | W2-L1e | |
| S04 Dubbelklik tekst bewerken | Voorkeuren ▸ Editor / tekst | – | – | – | W1-L5b | |
| S05 Auto-update | Voorkeuren ▸ Editor / bestanden | – | – | – | W3-L1 | |
| S06 Default layers laden bij New/Restart | Voorkeuren ▸ Editor / bestanden | – | – | – | W3-L1 | |
| S07 Autosave-interval (2 min) | Voorkeuren ▸ Editor / bestanden | `burnburn-macos/src/appkit/app.rs` (`start_timer`, `autosave_now`) | `autosave_writes_a_recovery_file_never_the_project_and_keeps_the_document_dirty` (presentation/tests/gui_requests.rs) | deels | W3-L1 | Autosave en herstel-sheet bestaan; interval is niet instelbaar. |
| S08 Interne/bypass system-dialogen | Voorkeuren ▸ Editor / bestanden | – | – | – | W3-L1 | |
| S09 Job log | Voorkeuren ▸ Editor / bestanden | – | – | – | W3-L1 | |
| S10 Default Cut Selected | Voorkeuren ▸ Editor / selectie-output | – | – | – | W1-L4a | |
| S11 Default Use Selection Origin | Voorkeuren ▸ Editor / selectie-output | – | – | – | W2-L1i | |
| S12 Save Job Origin to Project | Voorkeuren ▸ Editor / selectie-output | – | – | – | W2-L1i | |
| S13 Save Cut Selected/Selection Origin to Project | Voorkeuren ▸ Editor / selectie-output | – | – | – | W2-L1i | |
| S14 Use System Clipboard | Voorkeuren ▸ Editor / interactie | – | – | – | geen | Klembord ontbreekt. |
| S15 Invert wheel zoom | Voorkeuren ▸ Editor / interactie | `burnburn-editor/src/view.rs` | `trackpad_scroll_pans_wheel_lines_scale_and_pinch_zooms` (burnburn-macos/tests/adapter.rs) | – | W3-L1 | Zoomgedrag vast. |
| S16 Move/Rotate/Scale-switches bewaren | Voorkeuren ▸ Editor / interactie | – | – | – | W3-L1 | |
| S17 Na tekenen terug naar Select | Voorkeuren ▸ Editor / interactie | – | – | – | W3-L1 | |
| S18 Beginner Mode | Voorkeuren ▸ Editor / interactie | – | – | – | W3-L1 | |
| S19 Ignore out-of-bounds if possible | Voorkeuren ▸ Editor / uitvoering | `burnburn-cam/src/preflight.rs` | `overscan_outside_bed_is_a_laser_off_error_without_blaming_the_run`, `outside_bed_operation_links_error_to_source_geometry` (cam/tests) | – | W1-L4a | Buiten-bed-gedrag als preflight; instelling ontbreekt. |
| S20 Ignore Start when monitor sleeps | Voorkeuren ▸ Editor / uitvoering | – | – | – | W1-L4a | |
| S21 Start mag geen keyboardfocus houden | Voorkeuren ▸ Editor / uitvoering | – | – | – | W3-L1 | |
| S22 Foutgeluid bij mislukte DSP-send | Voorkeuren ▸ Editor / uitvoering | – | – | – | W3-L1 | |
| S23 Eenheden: Millimeters / Inches | Voorkeuren ▸ Units and Grids / units | `burnburn-presentation/src/field.rs` | `units_and_decimal_separators_give_the_same_physical_value` (presentation/src/field.rs) | deels | W3-L1 | Veldeenheid mm; inch-weergave niet gevonden. |
| S24 Snap to Objects | Voorkeuren ▸ Units and Grids / snap | `burnburn-editor/src/snap.rs`; presentatie `SnapToObjects` | `snap_names_endpoint_and_primary_modifier_suppresses` (presentation/tests/im09_interaction.rs) | – | W2-L1b | Geen Mac-menu. |
| S25 Snap to Grid | Voorkeuren ▸ Units and Grids / snap | `burnburn-editor/src/snap.rs`; presentatie `SnapToGrid` | `grid_snapping_is_a_window_toggle` (presentation/tests/im09_interaction.rs) | – | W2-L1b | |
| S26 Grid Snap Distance | Voorkeuren ▸ Units and Grids / snap | `burnburn-editor/src/snap.rs` | – | – | W2-L1b | |
| S27 Object Snap Distance (px) | Voorkeuren ▸ Units and Grids / snap | `burnburn-editor/src/snap.rs` | `grid_applies_only_when_no_object_target_is_near` (editor/src/snap.rs) | – | W2-L1b | |
| S28 Grid contrast | Voorkeuren ▸ Units and Grids / grid | – | – | – | W2-L1b | |
| S29 Grid visual spacing | Voorkeuren ▸ Units and Grids / grid | – | – | – | W2-L1b | |
| S30 Selection tolerance (px) | Voorkeuren ▸ Units and Grids / grid | `burnburn-editor/src/selection.rs` | `alt_click_cycles_overlapping_objects_and_reports_the_position` (presentation/tests/im09_interaction.rs) | – | W2-L2a | |
| S31 Keyboard increments (Ctrl/Arrow/Shift: 1/5/20 mm) | Voorkeuren ▸ Units and Grids / keyboard | – | – | – | W3-L1 | Pijltjes-verplaatsing niet gevonden. |
| S32 Thema | Voorkeuren ▸ Display | – | – | – | W3-L1 | |
| S33 Donkere achtergrond | Voorkeuren ▸ Display | – | – | – | W3-L1 | |
| S34 Lettertype- en pictogramgrootte | Voorkeuren ▸ Display | – | – | – | W3-L1 | |
| S35 Curve quality | Voorkeuren ▸ Display | – | – | – | W2-L1b | |
| S36 Cursor- en knoopgrootte | Voorkeuren ▸ Display | – | – | – | W2-L1b | |
| S37 Full-screen crosshair | Voorkeuren ▸ Display | – | – | – | W2-L1b | |
| S38 Bed-centerkruis | Voorkeuren ▸ Display | – | – | – | W2-L1b | |
| S39 Antialiasing | Voorkeuren ▸ Display / rendering | `burnburn-render` | – | – | W2-L1b | |
| S40 Filled rendering | Voorkeuren ▸ Display / rendering | `burnburn-render` | – | – | W2-L1b | |
| S41 True high-DPI workspace | Voorkeuren ▸ Display / rendering | – | – | ? | W2-L1b | |
| S42 Reduce Motion | Voorkeuren ▸ Display / rendering | – | – | – | W3-L1 | |
| S43 Startup display checks | Voorkeuren ▸ Display / rendering | – | – | – | W3-L1 | |
| S44 Palette labels | Voorkeuren ▸ Display / toggles | `burnburn-macos/src/panels/operations.rs` (`palette_buttons`) | – | – | W2-L1b | Palet heeft labels; instelling niet. |
| S45 Rotary enable (hoofdvenster) | Voorkeuren ▸ Display / toggles | – | – | – | W2-L4b | |
| S46 Cylinder correction enable | Voorkeuren ▸ Display / toggles | – | – | – | W4 | Niet door Ruida-backend (02 §11). |
| S47 Import: Select Imported | Voorkeuren ▸ Import / general | `burnburn-io/src/import/mod.rs` | – | – | W1-L6a | |
| S48 Import: Group Imported | Voorkeuren ▸ Import / general | `burnburn-io/src/import/mod.rs` | `groups_shared_lists_text_and_bitmaps_are_imported_or_reported` (io/tests/lbrn2.rs) | – | W1-L6a | Groepen uit LBRN2 worden gemeld; instelling ontbreekt. |
| S49 Import: AI hidden layers | Voorkeuren ▸ Import / general | `burnburn-io/src/import/mod.rs` | – | – | W1-L6a | |
| S50 Import: Allow Tool Layers | Voorkeuren ▸ Import / general | – | – | – | W2-L3a | |
| S51 Import: Center Imported | Voorkeuren ▸ Import / general | – | – | – | W1-L6a | |
| S52 Import SVG: 96 DPI / Inkscape | Voorkeuren ▸ Import / SVG | `burnburn-io/src/import/svg.rs` | `svg_inch_size_and_arc_flags_follow_the_spec` (io/tests/import.rs) | – | W1-L6a | |
| S53 Import SVG: 72 DPI / Illustrator | Voorkeuren ▸ Import / SVG | `burnburn-io/src/import/svg.rs` | `svg_inch_size_and_arc_flags_follow_the_spec` | – | W1-L6a | |
| S54 Import SVG: autodetect | Voorkeuren ▸ Import / SVG | `burnburn-io/src/import/svg.rs` | `svg_inch_size_and_arc_flags_follow_the_spec` | – | W1-L6a | |
| S55 Import DXF: auto-units | Voorkeuren ▸ Import / DXF | `burnburn-io/src/import/mod.rs` | `unitless_dxf_needs_explicit_scale_before_import_commit` (io/tests/import.rs) | – | W1-L6a | |
| S56 Import DXF: µm/mm/cm/m/km/in/ft | Voorkeuren ▸ Import / DXF | `burnburn-io/src/import/mod.rs` | `unitless_dxf_needs_explicit_scale_before_import_commit` | – | W1-L6a | |
| S57 Import DXF: ignore TEXT/MTEXT | Voorkeuren ▸ Import / DXF | `burnburn-io/src/import/mod.rs` | – | – | W1-L6a | |
| S58 Import DXF: auto-close tolerance (0,01 mm) | Voorkeuren ▸ Import / DXF | `burnburn-io/src/import/mod.rs` | – | – | W1-L6a | |
| S59 Export DXF: arcs behouden of lijnen | Voorkeuren ▸ Export / DXF | `burnburn-io/src/export.rs` | `dxf_export_roundtrips_and_names_the_ellipse_approximation` (io/tests/export.rs) | – | W2-L1 | |
| S60 Import HPGL: 1 inch = 1000 of 1016 | Voorkeuren ▸ Import / HPGL | – | – | – | W1-L6a | HPGL ontbreekt. |
| S61 Output print: Curve Tolerance (mm) | Voorkeuren ▸ Output / print | – | – | – | geen | Printpijplijn niet in plan. |
| S62 Output print: line width (mm) | Voorkeuren ▸ Output / print | – | – | – | geen | Idem. |
| S63 Camera capture: Recommended / Fallback | Voorkeuren ▸ Camera / capture | `burnburn-macos/src/appkit/camera.rs` | `camera_asks_access_only_on_click_and_starts_the_chosen_device_in_bgra` (burnburn-macos/tests/presentation_wiring.rs) | – | W2-L5e | |
| S64 Camera default mode / framerate | Voorkeuren ▸ Camera / capture | `burnburn-macos/src/appkit/camera.rs` | zelfde als S63 | – | W2-L5e | |
| S65 Camera overlay: Black and White / Full Color | Voorkeuren ▸ Camera / overlay | – | – | – | W2-L5e | Overlay op canvas ontbreekt. |
| S66 Camera overlay: preview quality | Voorkeuren ▸ Camera / overlay | – | – | – | W2-L5e | |
| S67 Camera overlay: preview FPS cap (2,5 fps) | Voorkeuren ▸ Camera / overlay | – | – | – | W2-L5e | |

### B2. Ruida Device Settings

| Instelling (LightBurn) | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| S68 Werkgebied Width/Height | Device Settings ▸ Dimensions / Units | `burnburn-job/src/profile.rs` | `machine_frame_maps_each_origin_corner_and_back` (job/tests/plan.rs) | – | W2-L4b | Profielwaarde; geen venster. |
| S69 Origin-keuze (vier hoeken) | Device Settings ▸ Dimensions / Units | `burnburn-job/src/profile.rs` | `machine_frame_maps_each_origin_corner_and_back` | – | W2-L4b | |
| S70 Machine Coordinates (G54) | Device Settings ▸ Dimensions / Units | – | – | – | W4 | G-code-functie; Ruida-datastroom ondersteunt G54 niet. |
| S71 Control Units (mm/sec) | Device Settings ▸ Dimensions / Units | – | – | – | W2-L4b | |
| S72 Link Control and Design Units | Device Settings ▸ Dimensions / Units | – | – | – | W2-L4b | |
| S73 Enable Z Axis | Device Settings ▸ Dimensions / Units | `burnburn-cam/src/features.rs` (Z-capability) | `passes_with_z_step_emit_focus_offsets_when_supported` (cam/tests/process_features.rs) | – | W2-L3d | |
| S74 Relative Z Moves Only | Device Settings ▸ Dimensions / Units | – | – | – | W2-L3d | |
| S75 Optimize Z Moves | Device Settings ▸ Dimensions / Units | – | – | – | W2-L3d | |
| S76 OMTech Polar | Device Settings ▸ Dimensions / Units | – | – | – | geen | Machinevariant; niet in plan. |
| S77 Enable pointer offset (X/Y) | Device Settings ▸ Dimensions / Units | – | – | – | W2-L4b | Hoort bij framing. |
| S78 Enable U Axis | Device Settings ▸ Dimensions / Units | – | – | – | W2-L4b | |
| S79 Frame Continuously | Device Settings ▸ Basic Settings / General | `burnburn-machine/src/frame.rs`; `burnburn-machine/src/frame.rs` (rubber band) | `frame_program_contains_no_laser_on_command`, `frame_rd_file_is_the_lowered_frame_plan_and_a_laser_off_artifact`, `rubber_band_is_a_strictly_convex_polygon_that_contains_every_point` (machine/tests/frame.rs) | – | W2-L1-laser | Domein getest, incl. rubber-band-kader en RD-frame. Geen frame-knop in het Mac-venster. |
| S80 Tab Pulse Width | Device Settings ▸ Basic Settings / General | – | – | – | W1-L3a | |
| S81 Overwrite Files on Device by Default | Device Settings ▸ Basic Settings / General | – | – | – | W1-L4a | |
| S82 Enable Job Checklist + Edit | Device Settings ▸ Basic Settings / General | – | – | – | W1-L4a | |
| S83 Save/Load Rotary Settings with Project Files | Device Settings ▸ Basic Settings / General | – | – | – | W2-L4b | |
| S84 Scanning Offset Adjust (enable, laser 1/2, tabel) | Device Settings ▸ Basic Settings / Scanning Offset Adjust | – | – | – | W1-L5a | Rasteroffset per snelheid; tabel ontbreekt. |
| S85 Start Delay | Device Settings ▸ Basic Settings / Delays | – | – | – | W2-L4b | |
| S86 End Delay | Device Settings ▸ Basic Settings / Delays | – | – | – | W2-L4b | |
| S87 Disable Start Button | Device Settings ▸ Basic Settings / Start/send | – | – | – | W1-L4a | |
| S88 Increment filename on Send | Device Settings ▸ Basic Settings / Start/send | – | – | – | W1-L4a | |
| S89 Check bounds when framing | Device Settings ▸ Basic Settings / Start/send | – | – | – | W1-L4a | |
| S90 Laser 1 naam | Device Settings ▸ Basic Settings / Laser 1 | – | – | – | W2-L4b | |
| S91 Laser 1 RF tube / Fiber | Device Settings ▸ Basic Settings / Laser 1 | – | – | – | W2-L4b | |
| S92 Laser 2 Enable Controls | Device Settings ▸ Basic Settings / Laser 2 | – | – | – | geen | Tweede laser; niet in plan. |
| S93 Laser 2 naam | Device Settings ▸ Basic Settings / Laser 2 | – | – | – | geen | Idem. |
| S94 Only allow one laser at a time | Device Settings ▸ Basic Settings / Laser 2 | – | – | – | geen | Idem. |
| S95 Laser 2 RF tube / Fiber | Device Settings ▸ Basic Settings / Laser 2 | – | – | – | geen | Idem. |
| S96 Enable Laser 2 Offset (X/Y) | Device Settings ▸ Basic Settings / Laser 2 | – | – | – | geen | Idem. |
| S97 Maximum speed X/Y | Device Settings ▸ Additional Settings | `burnburn-job/src/preview.rs` (tijdschatting) | `preview_moves_trace_to_source_and_estimate_time` (job/tests/preview.rs) | – | W2-L1h | Simulatietiming, geen controllerwaarde. |
| S98 Rapid speed | Device Settings ▸ Additional Settings | `burnburn-job/src/preview.rs` | zelfde als S97 | – | W2-L1h | |
| S99 Cut acceleration X/Y | Device Settings ▸ Additional Settings | `burnburn-job/src/profile.rs` | – | – | W2-L1h | Apart `SimulationProfile` nog niet gevonden. |
| S100 Engrave acceleration X/Y | Device Settings ▸ Additional Settings | `burnburn-job/src/profile.rs` | – | – | W2-L1h | |
| S101 Minimum corner speed | Device Settings ▸ Additional Settings | `burnburn-job/src/profile.rs` | – | – | W2-L1h | |
| S102 Corner accuracy | Device Settings ▸ Additional Settings | `burnburn-job/src/profile.rs` | – | – | W2-L1h | |
| S103 Global speed scale | Device Settings ▸ Additional Settings | `burnburn-job/src/profile.rs` | – | – | W2-L1h | |
| S104 Cut speed scale | Device Settings ▸ Additional Settings | `burnburn-job/src/profile.rs` | – | – | W2-L1h | |
| S105 Read From Controller | Device Settings ▸ Additional Settings | – | – | – | W2-L4b | Vereist controller-read; hardwareafhankelijk (W1-HW). |

### B3. Cut Settings

| Instelling (LightBurn) | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| S106 Naam van de laag | Cut Settings ▸ Gedeelde kop | `burnburn-presentation/src/controller/layers.rs` (`layer_label`) | – | – | W2-L3a | Lagen heten C00–C29; geen eigen naam. |
| S107 Output (Uitvoer) | Cut Settings ▸ Gedeelde kop | `burnburn-presentation/src/controller/layers.rs` | `visibility_and_output_toggle_independently` (presentation/tests/layer_palette.rs) | ✔ | – | Checkbox in laagtabel. |
| S108 Mode (Line/Fill/Image/Offset Fill) | Cut Settings ▸ Gedeelde kop | `burnburn-macos/src/panels/cut_settings.rs` (`sheet_mode`) | `blocked_mode_is_listed_disabled_with_the_missing_capability` (burnburn-macos/tests/layer_panels.rs) | deels | W1-L3b | Image ontbreekt in de sheet; capability-gating werkt (`blocked_mode_is_listed_disabled_with_the_missing_capability`). |
| S109 Speed | Cut Settings ▸ Gedeelde kop | `burnburn-presentation/src/controller/layers.rs` | `cut_settings_sheet_applies_raw_text_in_one_command_and_keeps_errors_open` (presentation/tests/layer_palette.rs) | ✔ | – | |
| S110 Min power (%) | Cut Settings ▸ Gedeelde kop | `burnburn-presentation/src/controller/layers.rs` | `layer_table_edits_power_min_passes_and_kerf_as_raw_text` (presentation/tests/layer_palette.rs) | ✔ | – | |
| S111 Max power (%) | Cut Settings ▸ Gedeelde kop | `burnburn-presentation/src/controller/layers.rs` | `cut_settings_sheet_applies_raw_text_in_one_command_and_keeps_errors_open` | ✔ | – | |
| S112 Air Assist | Cut Settings ▸ Gedeelde kop | – | – | – | W2-L3a | Controlleroutput ontbreekt. |
| S113 Sublagen (volgorde, naam, dupliceren, verwijderen, enable) | Cut Settings ▸ Sublagen | – | – | – | W2-L3a | |
| S114 Laatste ingeschakelde sublaag niet uit te zetten | Cut Settings ▸ Sublagen | – | – | – | W2-L3a | Hoort bij S113. |
| S115 Ruida Default Speed/Power | Cut Settings ▸ Gedeelde kop | – | – | – | W2-L4b | |

### B4. Line

| Instelling (LightBurn) | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| S116 Number of Passes | Cut Settings ▸ Line / Common | `burnburn-cam/src/compile.rs` | `passes_order_constraints_and_disabled_operations_shape_the_plan` (cam/tests/compile.rs); `layer_table_edits_power_min_passes_and_kerf_as_raw_text` | ✔ | – | Passes-veld in sheet en laagtabel. |
| S117 Z Offset | Cut Settings ▸ Line / Common | `burnburn-cam/src/compile.rs` | `passes_with_z_step_emit_focus_offsets_when_supported` (cam/tests/process_features.rs) | – | W2-L3d | |
| S118 Z step per pass | Cut Settings ▸ Line / Common | `burnburn-cam/src/compile.rs` | `passes_with_z_step_emit_focus_offsets_when_supported` | – | W2-L3d | |
| S119 Kerf offset | Cut Settings ▸ Line / Common | `burnburn-cam/src/features.rs` (kerf) | `kerf_offsets_outer_contours_outward_and_holes_inward`; `kerf_field_sets_line_kerf_in_one_command` (presentation/tests/layer_palette.rs) | ✔ | – | Kerf-veld in sheet; binnen/buiten-zijde niet visueel. |
| S120 Perforation Cut/Skip | Cut Settings ▸ Line / Common | `burnburn-cam/src/features.rs` | `perforation_alternates_on_and_off_from_the_entry` (cam/tests/process_features.rs); `perforation_phase_runs_through_the_corners` (cam/tests/line_extras.rs) | – | W2-L1-cut | Domein getest. Geen veld in Cut Settings. |
| S121 Tabs/Bridges: handmatig of automatisch | Cut Settings ▸ Line / Common | `burnburn-cam/src/features.rs` | `tabs_leave_evenly_spaced_bridges_and_reject_tabs_longer_than_the_contour` (cam/tests/process_features.rs); `tabs_by_spacing_get_their_count_from_each_contour_length` (cam/tests/line_extras.rs) | – | W2-L1-cut | Domein getest. Geen veld in Cut Settings. |
| S122 Tabs: min/max, spacing, size, skip inner, clear | Cut Settings ▸ Line / Common | `burnburn-cam/src/features.rs` | `tabs_by_spacing_get_their_count_from_each_contour_length`, `tabs_that_cannot_be_placed_are_refused_not_clamped` (cam/tests/line_extras.rs) | – | W2-L1-cut | |
| S123 Tab Cut Power | Cut Settings ▸ Line / Common | `burnburn-cam/src/features.rs` | – | – | W1-L3a | |
| S124 Overcut | Cut Settings ▸ Line / Advanced | `burnburn-cam/src/features.rs` | `overcut_continues_past_the_entry_along_the_contour`, `overcut_is_cut_on_the_last_pass_only` (cam/tests/line_extras.rs) | – | W2-L1-cut | Domein getest. Geen veld in Cut Settings. |
| S125 Start Pause (dwell ms) | Cut Settings ▸ Line / Advanced | – | – | – | W1-L3a | |
| S126 End Pause (dwell ms) | Cut Settings ▸ Line / Advanced | – | – | – | W1-L3a | |
| S127 Cut Through met eigen power | Cut Settings ▸ Line / Advanced | – | – | – | W1-L3a | |
| S128 Lead-In/Out (lijn/boog, lengte, hoek) | Cut Settings ▸ Line / Advanced | `burnburn-cam/src/features.rs`; `burnburn-cam/src/line.rs` | `lead_angle_tilts_the_lead_towards_the_contour_tangent`, `arc_leads_are_tangent_quarter_circles_on_the_waste_side`, `arc_lead_on_an_open_contour_is_refused_not_replaced` (cam/tests/line_extras.rs) | – | W2-L1-cut | Domein getest. Geen veld in Cut Settings. |
| S129 Override PWM Frequency | Cut Settings ▸ Line / Advanced | – | – | – | W1-L3a | Ruida-optie; eenheidsconversie niet geverifieerd. |
| S130 Dot Mode | Cut Settings ▸ Line / Advanced | – | `dot_mode_fires_one_pulse_per_spacing_and_is_gated` (cam/tests/line_extras.rs); `dot_events_belong_to_dot_settings_only` (job/tests/modes.rs) | – | W2-L1-cut | Domein getest. Geen veld in Cut Settings. |
| S131 U Offset | Cut Settings ▸ Line / Advanced | – | – | – | W2-L4b | |

### B5. Fill en Offset Fill

| Instelling (LightBurn) | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| S132 Bi-directional Fill | Cut Settings ▸ Fill / Common | `burnburn-cam/src/fill.rs` | – | – | W1-L3b | |
| S133 Cross-Hatch | Cut Settings ▸ Fill / Common | – | `cross_hatch_adds_a_perpendicular_pass_at_the_same_interval`, `cross_hatch_of_a_rotated_fill_adds_the_perpendicular_angle` (cam/tests/fill_extras.rs) | – | W2-L1-cut | Domein getest. Geen veld in Cut Settings. |
| S134 Line Interval / LPI | Cut Settings ▸ Fill / Common | `burnburn-cam/src/fill.rs` | – | – | W1-L3b | Veld niet in sheet. |
| S135 Scan Angle | Cut Settings ▸ Fill / Common | `burnburn-cam/src/fill.rs` | `scan_angle_rotates_rows_and_keeps_the_interval` (cam/tests/process_features.rs) | – | W1-L3b | |
| S136 Number of Passes (fill) | Cut Settings ▸ Fill / Common | `burnburn-cam/src/compile.rs` | `passes_order_constraints_and_disabled_operations_shape_the_plan` | ✔ | – | Passes-veld gedeeld met Line. |
| S137 Z Offset / Z step (fill) | Cut Settings ▸ Fill / Common | – | – | – | W2-L3d | |
| S138 Fill All Shapes at Once / Groups Together / Individually | Cut Settings ▸ Fill / Common | – | `fill_grouping_decides_which_shapes_share_the_even_odd_rule` (cam/tests/fill_extras.rs) | – | W2-L1-cut | Domein getest (even-odd per groep). Geen veld in Cut Settings. |
| S139 Ramp Length | Cut Settings ▸ Fill / Advanced | – | `ramp_length_turns_every_run_into_a_ramped_run` (cam/tests/fill_extras.rs) | – | W2-L1-cut | Domein getest. Geen veld in Cut Settings. |
| S140 Ramp Outer Edge | Cut Settings ▸ Fill / Advanced | – | – | – | W2-L1-cut | |
| S141 Flood Fill | Cut Settings ▸ Fill / Advanced | – | `flood_fill_scans_each_connected_region_completely_before_the_next`, `flood_fill_keeps_a_connected_shape_in_ordinary_row_order` (cam/tests/fill_extras.rs) | – | W2-L1-cut | Domein getest. Geen veld in Cut Settings. |
| S142 Override PWM Frequency (fill) | Cut Settings ▸ Fill / Advanced | – | – | – | W1-L3b | |
| S143 U Offset (fill) | Cut Settings ▸ Fill / Advanced | – | – | – | W2-L4b | |
| S144 Interval / LPI (offset fill) | Cut Settings ▸ Offset Fill / Common | `burnburn-cam/src/offset_fill.rs` | `default_offset_fill` (presentation/src/controller/layers.rs) | – | W1-L3b | |
| S145 Passes (offset fill) | Cut Settings ▸ Offset Fill / Common | `burnburn-cam/src/offset_fill.rs` | – | ✔ | – | Passes-veld. |
| S146 Z-offset / Z-step (offset fill) | Cut Settings ▸ Offset Fill / Common | `burnburn-cam/src/offset_fill.rs` | – | – | W2-L3d | |
| S147 Grouping (offset fill) | Cut Settings ▸ Offset Fill / Common | `burnburn-cam/src/offset_fill.rs` | – | – | W1-L3b | |
| S148 Override PWM (offset fill) | Cut Settings ▸ Offset Fill / Advanced | – | – | – | W1-L3b | |
| S149 PPI (controllergebonden, Trocen) | Cut Settings ▸ Offset Fill / Advanced | – | – | – | geen | Niet in plan; capability `supportsPPI` (DOC). |
| S150 U (offset fill) | Cut Settings ▸ Offset Fill / Advanced | – | – | – | W2-L4b | |

### B6. Image

| Instelling (LightBurn) | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| S151 Bidirectioneel (image) | Cut Settings ▸ Image | `burnburn-cam/src/image.rs` | – | – | W1-L5a | |
| S152 Negative Image | Cut Settings ▸ Image | – | `negative_image_burns_the_light_pixels` (cam/tests/image_modes.rs) | – | W2-L1-cut | Domein getest. Geen Mac-bediening (Image-operatie niet in de sheet). |
| S153 Overscan (controllergebonden) | Cut Settings ▸ Image | `burnburn-cam/src/preflight.rs` | `overscan_outside_bed_is_a_laser_off_error_without_blaming_the_run` (cam/tests/preflight.rs) | – | W1-L5a | |
| S154 Interval / DPI | Cut Settings ▸ Image | `burnburn-cam/src/image.rs`; `burnburn-image/src/scale.rs` | `missing_or_aspect_only_density_needs_an_explicit_dpi` (io/tests/raster.rs) | – | W1-L5a | |
| S155 Dot Width Correction | Cut Settings ▸ Image | – | `dot_width_correction_shortens_every_run_at_both_ends` (cam/tests/image_modes.rs) | – | W2-L1-cut | Domein getest. Geen Mac-bediening. |
| S156 Scan Angle (image) | Cut Settings ▸ Image | `burnburn-cam/src/image.rs` | `scan_angle_90_scans_columns_of_the_same_image`, `an_oblique_scan_angle_keeps_the_image_in_place_and_needs_the_scan_angle_feature` (cam/tests/image_modes.rs) | – | W2-L1-cut | Domein getest. Geen Mac-bediening. |
| S157 Angle Increment | Cut Settings ▸ Image | – | – | – | W1-L5a | |
| S158 Z-offset (image) | Cut Settings ▸ Image | – | – | – | W2-L3d | |
| S159 Cells per Inch | Cut Settings ▸ Image | – | `newsprint_is_the_45_degree_screen_and_ignores_the_screen_angle` (cam/tests/image_modes.rs) | – | W2-L1-cut | Domein getest. Geen Mac-bediening. |
| S160 Halftone Angle | Cut Settings ▸ Image | – | `halftone_screen_angle_rotates_the_dot_lattice` (image/tests/modes.rs) | – | W2-L1-cut | Domein getest. Geen Mac-bediening. |
| S161 Passes (image) | Cut Settings ▸ Image | `burnburn-cam/src/compile.rs` | – | – | W2-L1-cut | Passes-veld alleen voor Line/Fill/Offset Fill; image-operatie heeft geen sheet-veld. |
| S162 Ramp Length (image) | Cut Settings ▸ Image | – | `ramp_runs_belong_to_binary_scans_and_are_validated` (job/tests/modes.rs) | – | W2-L1-cut | Domein getest. Geen Mac-bediening. |
| S163 Pass-Through | Cut Settings ▸ Image | `burnburn-cam/src/image.rs`; `burnburn-image/src/scale.rs` | `pass_through_replicates_source_pixels_without_filtering`, `pass_through_skips_pixels_when_shrinking_instead_of_averaging` (cam/tests/image_modes.rs) | – | W2-L1-cut | Resamplingcontract getest. Geen Mac-bediening. |
| S164 Beeldmodus (10 modi) | Cut Settings ▸ Image | `burnburn-cam/src/image.rs`; `burnburn-document/src/process/settings.rs` (`DitherMode`) | `atkinson_gradient_matches_independent_reference`, `floyd_steinberg_gradient_matches_independent_reference`, `jarvis_judice_ninke_gradient_matches_independent_reference`, `stucki_gradient_matches_independent_reference`, `halftone_and_newsprint_and_sketch_modes_reach_the_kernels` (image/tests/modes.rs); `ordered_dither_accepts_only_bayer_sizes` (image/tests/raster.rs) | – | W2-L1-cut | Alle modi als domein en test (Threshold, Ordered, Atkinson, FS, Jarvis, Stucki, Newsprint, Halftone, Sketch, Grayscale). Geen Mac-keuze van de modus: Image-operatie niet in de sheet. |
| S165 Grouping (image) | Cut Settings ▸ Image | – | – | – | W1-L5a | |
| S166 PWM override (image) | Cut Settings ▸ Image | – | – | – | W1-L5a | |
| S167 U (image) | Cut Settings ▸ Image | – | – | – | W2-L4b | |

### B7. Adjust Image en Trace

| Instelling (LightBurn) | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| S168 Contrast | Adjust Image | `burnburn-image/src/adjust.rs` | – | – | W2-L1-beeld | |
| S169 Brightness | Adjust Image | `burnburn-image/src/adjust.rs` | – | – | W2-L1-beeld | |
| S170 Gamma | Adjust Image | `burnburn-image/src/adjust.rs` | – | – | W2-L1-beeld | |
| S171 Enhance Amount | Adjust Image | – | – (geen enhance-code) | – | W2-L1-beeld | Enhance-filter bestaat niet in de code. |
| S172 Enhance Radius | Adjust Image | – | – (geen enhance-code) | – | W2-L1-beeld | Enhance-filter bestaat niet in de code. |
| S173 EnhanceDenoise (opgeslagen beeldveld) | Adjust Image | – | – (geen enhance-code) | – | W2-L1-beeld | Enhance-filter bestaat niet in de code. |
| S174 Invert Display (alleen preview) | Adjust Image | – | – | – | W1-L5a | |
| S175 Adjust presets | Adjust Image | – | – | – | W1-L5a | |
| S176 Trace Cutoff | Trace Image | `burnburn-trace/src/recipe.rs` | ? | – | W2-L1g | |
| S177 Trace Threshold | Trace Image | `burnburn-trace/src/recipe.rs` | `local_threshold_separates_ink_from_shadow_where_the_global_threshold_fails` (trace/tests/recipes.rs) | – | W2-L1g | |
| S178 Ignore Less Than | Trace Image | `burnburn-trace/src/recipe.rs` | ? | – | W2-L1g | |
| S179 Smoothness | Trace Image | `burnburn-trace/src/fit.rs` | ? | – | W2-L1g | |
| S180 Optimize | Trace Image | `burnburn-trace/src/fit.rs` | ? | – | W2-L1g | |
| S181 Transparency / Sketch Trace / previewopties | Trace Image | `burnburn-trace/src/preview.rs` | ? | – | W2-L1g | |
| S182 Boundary | Trace Image | `burnburn-trace/src/contour.rs` | ? | – | W2-L1g | |

## C. Dialoogoptie (06-dialoogcatalogus.md)

### C1. Offset

| Optie | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| D01 Offset Distance | Offset | presentatie `Offset`-intent (`burnburn-presentation/src/intent.rs`) | `offset_panel_adds_one_offset_object_in_one_entry` (presentation/tests/edit_ops.rs) | – | W1-L2a | Geen Mac-paneel. |
| D02 Direction: Outward / Inward / Both | Offset | – | – | – | W1-L2a | "Both" levert twee kanten. |
| D03 Corner Style: Round / Bevel / Corner | Offset | – | – | – | W1-L2a | |
| D04 Outer Shapes Only | Offset | – | – | – | W1-L2a | |
| D05 Select Resulting Objects | Offset | – | – | – | W1-L2a | |
| D06 Delete Original Objects | Offset | presentatie `Offset`-intent | `offset_panel_adds_one_offset_object_in_one_entry` (presentation/tests/edit_ops.rs) | – | W1-L2a | Test controleert alleen het toevoegen van één object; verwijderen van het bron-object is niet getest. |
| D07 Optimize / Simplify Results | Offset | – | – | – | W1-L2a | |
| D08 OK / Cancel | Offset | presentatie | `impossible_offset_reports_location_and_keeps_document` (presentation/tests/edit_ops.rs) | – | W1-L2a | Onmogelijke offset laat document ongewijzigd; Cancel-knop niet apart getest. |

### C2. Create Grid Array

| Optie | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| D09 X Columns | Grid Array (horizontaal) | `burnburn-document/src/structure/modifier.rs` | `grid_array_modifier_outputs_every_copy_and_follows_source_edits` (document/tests/array_modifiers.rs) | – | W2-L1d | Domein; geen dialoog. |
| D10 X Spacing | Grid Array (horizontaal) | zelfde als D09 | zelfde als D09 | – | W2-L1d | |
| D11 X Column Shift | Grid Array (horizontaal) | zelfde als D09 | zelfde als D09 | – | W2-L1d | |
| D12 Total Width | Grid Array (horizontaal) | zelfde als D09 | zelfde als D09 | – | W2-L1d | Aantal kolommen wordt afgeleid. |
| D13 Distance Center-to-Center vs. Edge-padding | Grid Array (algemeen) | `burnburn-cam/src/array.rs` | `grid_and_circular_arrays_generate_placements` (cam/tests/nest.rs) | – | W2-L1d | |
| D14 Reverse Direction | Grid Array (horizontaal) | `burnburn-cam/src/array.rs` | – | – | W2-L1d | |
| D15 Shift by Half | Grid Array (horizontaal) | `burnburn-cam/src/array.rs` | – | – | W2-L1d | |
| D16 Mirror Alternate Columns in X/Y | Grid Array (horizontaal) | – | – | – | W2-L1d | |
| D17 Y Rows | Grid Array (verticaal) | zelfde als D09 | zelfde als D09 | – | W2-L1d | |
| D18 Y Spacing | Grid Array (verticaal) | zelfde als D09 | zelfde als D09 | – | W2-L1d | |
| D19 Y Row Shift | Grid Array (verticaal) | zelfde als D09 | zelfde als D09 | – | W2-L1d | |
| D20 Total Height | Grid Array (verticaal) | zelfde als D09 | zelfde als D09 | – | W2-L1d | |
| D21 Mirror Alternate Rows in X/Y | Grid Array (verticaal) | – | – | – | W2-L1d | |
| D22 Create Virtual Array | Grid Array (algemeen) | `burnburn-document/src/structure/modifier.rs` (virtuele array = modifier) | `grid_array_modifier_outputs_every_copy_and_follows_source_edits` | – | W2-L1d | Bronrelatie bewaard. |
| D23 Random Orientation (+ numeriek veld) | Grid Array (algemeen) | – | – | – | W2-L1d | LightBurn-randombeleid niet getest. |
| D24 Auto-Increment Variable Text (+ increment) | Grid Array (algemeen) | – | – | – | W1-L5c | |
| D25 Select Results / Group Results | Grid Array (algemeen) | `burnburn-presentation/src/controller.rs` | – | – | W2-L1d | |

### C3. Circular Array

| Optie | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| D26 Center X / Y | Circular Array | `burnburn-document/src/structure/modifier.rs` | `circular_array_modifier_turns_copies_about_the_centre` (document/tests/array_modifiers.rs) | – | W2-L1d | |
| D27 Start / End (hoek) | Circular Array | `burnburn-cam/src/array.rs` | – | – | W2-L1d | Eindpuntpolicy (0°/360°) niet getest. |
| D28 Step | Circular Array | `burnburn-cam/src/array.rs` | – | – | W2-L1d | |
| D29 Copies | Circular Array | `burnburn-cam/src/array.rs` | `grid_and_circular_arrays_generate_placements` (cam/tests/nest.rs) | – | W2-L1d | |
| D30 Rotate Object Copies | Circular Array | `burnburn-document/src/structure/modifier.rs` | `circular_array_modifier_turns_copies_about_the_centre` | – | W2-L1d | |
| D31 Use Last Selected Object Position as Center | Circular Array | – | – | – | W2-L1d | Selectievolgorde is onderdeel van de interactie. |
| D32 Auto-Increment Variable Text (+ increment) | Circular Array | – | – | – | W1-L5c | |
| D33 Select Results / Group Results | Circular Array | `burnburn-presentation/src/controller.rs` | – | – | W2-L1d | |

### C4. Bar Code Properties

| Optie | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| D34 Raw Content | Bar Code Properties | `burnburn-generators/src/barcode.rs`, `qr.rs` | `code128_symbols_match_hand_computed_vectors` (generators/tests) | – | W2-L5d | Eigenschappenvenster ontbreekt. |
| D35 Evaluated Text | Bar Code Properties | `burnburn-generators/src/evaluator.rs` | – | – | W2-L5d | |
| D36 Text evaluation: Normal / Variable | Bar Code Properties | `burnburn-generators/src/evaluator.rs` | – | – | W1-L5c | |
| D37 Variable Offset | Bar Code Properties | – | – | – | W1-L5c | |
| D38 Input Mode: Data / Unicode / GS1 | Bar Code Properties | – | – | – | W2-L5d | |
| D39 Process Control Characters | Bar Code Properties | – | – | – | W2-L5d | Niet vervangen door gelijk uitziende tekst. |
| D40 Error Correction Level (QR) | Bar Code Properties | `burnburn-generators/src/qr.rs` (`QrEcc`) | `automatic_mask_choice_agrees_with_the_reference_encoder` (generators/tests/qr.rs) | – | W2-L5d | |
| D41 Tab WIFI | Bar Code Properties | – | – | – | W2-L5d | |
| D42 Tab Contact | Bar Code Properties | – | – | – | W2-L5d | |

### C5. Bar Code Type (29 keuzes)

| Optie | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| D43 2 of 5 | Bar Code Type ▸ 2 of 5 | – | – | – | W2-L5d | |
| D44 2 of 5 IATA | Bar Code Type ▸ 2 of 5 | – | – | – | W2-L5d | |
| D45 2 of 5 Industrial | Bar Code Type ▸ 2 of 5 | – | – | – | W2-L5d | |
| D46 AprilTag 16h5 | Bar Code Type ▸ fiducials | – | – | – | W2-L5d | Markerfamilie, geen barcode. |
| D47 AprilTag 25h9 | Bar Code Type ▸ fiducials | – | – | – | W2-L5d | |
| D48 AprilTag 36h10 | Bar Code Type ▸ fiducials | – | – | – | W2-L5d | |
| D49 AprilTag 36h11 | Bar Code Type ▸ fiducials | – | – | – | W2-L5d | |
| D50 ArUco MIP 36h11 | Bar Code Type ▸ fiducials | – | – | – | W2-L5d | |
| D51 ArUco OpenCV 7x7 | Bar Code Type ▸ fiducials | – | – | – | W2-L5d | |
| D52 Aztec | Bar Code Type ▸ matrix | – | – | – | W2-L5d | |
| D53 Chinese Sensible Code | Bar Code Type ▸ matrix | – | – | – | W2-L5d | |
| D54 DataMatrix | Bar Code Type ▸ matrix | – | – | – | W2-L5d | |
| D55 Grid Matrix | Bar Code Type ▸ matrix | – | – | – | W2-L5d | |
| D56 CodaBar | Bar Code Type ▸ lineair | – | – | – | W2-L5d | |
| D57 Code 128 | Bar Code Type ▸ lineair | `burnburn-generators/src/barcode.rs` | `code128_symbols_match_hand_computed_vectors` (generators/tests) | – | W2-L5d | Subsets B en C. |
| D58 Code 128 GS1 | Bar Code Type ▸ lineair | `burnburn-generators/src/barcode.rs` | – | – | W2-L5d | GS1-variant niet gevonden. |
| D59 Code 93 | Bar Code Type ▸ lineair | – | – | – | W2-L5d | |
| D60 EAN | Bar Code Type ▸ lineair | – | – | – | W2-L5d | |
| D61 ISBN | Bar Code Type ▸ lineair | – | – | – | W2-L5d | |
| D62 Plessey | Bar Code Type ▸ lineair | – | – | – | W2-L5d | |
| D63 UPC A | Bar Code Type ▸ lineair | – | – | – | W2-L5d | |
| D64 UPC E | Bar Code Type ▸ lineair | – | – | – | W2-L5d | |
| D65 VIN | Bar Code Type ▸ lineair | – | – | – | W2-L5d | |
| D66 PDF417 | Bar Code Type ▸ PDF417 | – | – | – | W2-L5d | |
| D67 PDF417 Compact | Bar Code Type ▸ PDF417 | – | – | – | W2-L5d | |
| D68 PDF417 Micro | Bar Code Type ▸ PDF417 | – | – | – | W2-L5d | |
| D69 QR Code | Bar Code Type ▸ QR | `burnburn-generators/src/qr.rs` (`qr_encode`) | `qr_geometry_is_one_square_per_dark_module_inside_the_quiet_zone` (generators/tests) | – | W2-L5d | |
| D70 QR Micro | Bar Code Type ▸ QR | – | – | – | W2-L5d | Micro-QR niet gevonden. |
| D71 QR Micro Rectangular | Bar Code Type ▸ QR | – | – | – | W2-L5d | Idem. |

### C6. Offset Fill (dialoog; disabled in het onderzochte profiel)

| Optie | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| D72 Scan Angle | Offset Fill / Common | `burnburn-cam/src/offset_fill.rs` | – | – | W1-L3b | Disabled in dit profiel. |
| D73 Bi-directional | Offset Fill / Common | – | – | – | W1-L3b | Disabled. |
| D74 Cross-Hatch | Offset Fill / Common | – | – | – | W1-L3b | Disabled. |
| D75 Grouping (3 radioknoppen) | Offset Fill / Common | `burnburn-cam/src/offset_fill.rs` | – | – | W1-L3b | Disabled. |
| D76 Flood Fill | Offset Fill / Advanced | – | – | – | W1-L3b | Disabled. |
| D77 Ramp Length / Ramp Outer Edge | Offset Fill / Advanced | – | – | – | W1-L3b | Disabled. |

### C7. Material Test, Interval Test, Camera manager

| Optie | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| D78 Material Test: presetbeheer en titel | Material Test | – | – | – | W2-L3e | |
| D79 Material Test: Vertical/Rows (parameter, min, max, count, maat) | Material Test | – | – | – | W2-L3e | |
| D80 Material Test: Horizontal/Columns | Material Test | – | – | – | W2-L3e | |
| D81 Material Test: X/Y Center | Material Test | – | – | – | W2-L3e | |
| D82 Material Test: Enable Text / Border | Material Test | – | – | – | W2-L3e | |
| D83 Material Test: Edit Material/Text/Border Settings | Material Test | – | – | – | W2-L3e | |
| D84 Interval Test: Simple Fill / Dithered Image | Interval Test | – | – | – | W2-L3e | |
| D85 Interval Test: steps, min/max interval | Interval Test | – | – | – | W2-L3e | |
| D86 Interval Test: maat, snelheid, power | Interval Test | – | – | – | W2-L3e | |
| D87 Interval Test: Preview / Frame / Start / Send / Save | Interval Test | – | – | – | W2-L3e | |
| D88 Camera manager: cameralijst | Manage Cameras | `burnburn-macos/src/appkit/camera.rs` | – | deels | W2-L5e | Lijst was leeg in het onderzoek; bronnenlijst bestaat niet als dialoog. |

## D. Featurefamilies (02-features.md, familietabel in lightburn-pariteit.md)

| Familie | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| F01 Werkruimte: zoom/pan/fit, wireframe/filled, grid, rulers, guides | Window / Display | `burnburn-editor/src/view.rs`, `burnburn-editor/src/snap.rs` | `zoom_keeps_anchor_world_point_fixed` (editor/src/view.rs) | deels | W2-L1b | Zoom/pan via scroll; fit, linialen, hulplijnen ontbreken. |
| F02 Selectie: gebied, omkeren, per laag, open paden, ingesloten, lock/hide | Edit / Tools | `burnburn-editor/src/selection.rs`, `tool/select.rs` | `rubber_band_selects_only_objects_fully_inside` (editor/src/selection.rs) | deels | W2-L2a + W1-L1 | Gebied en Alt-klik werken; selectiefilters, lock en hide ontbreken. |
| F03 Transform: X/Y/B/H/rotatie, %-schaal, aspectlock, shear, 3×3-referentie | Window ▸ Numerieke bewerkingen | `burnburn-presentation/src/controller.rs`; `burnburn-geometry/src/affine.rs` | `rotation_field_turns_the_selection_about_its_centre_in_one_command` (presentation/tests/im09_interaction.rs) | deels | W2-L1c | X/Y/B/H en rotatie ✔; %-schaal, aspectlock, shear en referentiepunt ontbreken. |
| F04 Groepen, break apart, auto-group, undo-historie | Arrange / Edit | `burnburn-document/src/edit.rs` | `nested_groups_and_ungroup_restore_membership` | – | W2-L1c | Domein; geen UI. |
| F05 Basisvormen (driehoek, veelhoek, ster, …) parametrisch | Tools ▸ Draw Shape | `burnburn-geometry/src/shapes.rs`; `burnburn-editor/src/tool/parametric.rs`; `burnburn-document` (parametric shapes) | `star_alternates_tips_and_valleys_with_exact_area` (geometry/tests/param_shapes.rs); `created_shapes_are_named_objects_of_their_kind_with_their_definition` (document/tests/parametric_shapes.rs) | deels | W2-L1-tekenen | Parametrische vormen als domein en editor-gereedschap. Toolstrip: alleen veelhoeken en afgeronde rechthoek; ster, driehoek, vijf-/achthoek en dubbele ster ontbreken. |
| F06 Lijnen, pen, nodes: break, join, close, smooth/corner, curve↔line, trim/extend | Tools | `burnburn-editor/src/tool/node.rs`, `burnburn-geometry/src/node.rs` | `node_edit_is_one_undo_entry_and_other_spans_stay_identical` | deels | W1-L2a → L1 | Knoopverplaatsing ✔; break/join/curve↔line/extend ontbreken. |
| F07 Offset, weld/booleans, cut shapes, round corners, optimize shapes, start point, tangent circle, warp/deform, resize slots, measure | Tools / Edit | `burnburn-geometry/src/boolean.rs`, `repair.rs`, `burnburn-presentation/src/task.rs` (measure) | `round_corners_is_one_undoable_step_that_keeps_identity`, `cut_shapes_replaces_the_subject_by_its_pieces_and_undo_restores_both_objects`, `optimize_shapes_keeps_identity_and_reports_nothing_to_do` (document/tests/geometry_ops.rs); `applying_a_boolean_is_one_undo_entry` (presentation/tests/im09_interaction.rs); `measure_reports_count_and_union_bounds` (presentation/src/task.rs) | deels | W2-L1-tekenen, W2-L2c | Measure ✔ in het Mac-menu. Toolstrip: offset, trim, tangent-cirkel ✔; afrondhoek (`ToolKind::RoundCorner`) en startpunt staan alleen in de editor. Cut shapes, optimize en booleans: domein of presentatie zonder Mac-koppeling. Resize slots ontbreekt. |
| F08 Align/distribute/dock/move together | Arrange | `burnburn-geometry/src/align.rs` | `align_and_distribute_of_many_objects_is_one_entry` | – | W2-L2a | Presentatie-commando's (`CommandKey::Align*`, `Distribute*`) zonder Mac-koppeling; geen menu of toolstrip. Dock en move together ontbreken. |
| F09 Grid/circular array, copy along path, virtuele arrays | Arrange | `burnburn-cam/src/array.rs`, `burnburn-document/src/structure/modifier.rs` | `grid_array_modifier_outputs_every_copy_and_follows_source_edits`; `circular_array_modifier_turns_copies_about_the_centre` | – | W2-L1d | Domein met preview; geen dialoog; copy along path ontbreekt. |
| F10 Nest selected / quick nest | Arrange | `burnburn-cam/src/nest.rs` | `nesting_keeps_order_counts_and_respects_grain_rotation` (cam/tests/nest.rs) | – | W3-L1 | Domein; nestingpaneel ontbreekt. |
| F11 Tekst: font, bold/italic, uitlijning, spatiëring, welded, op pad, convert to path, SHX/RTL | Tools ▸ Edit Text / Window ▸ Text Options | `burnburn-text/src/`, `burnburn-generators/src/path_text.rs` | `fallback_chain_draws_missing_characters_from_the_next_font`, `text_on_path_reports_overflow_and_multiline` | deels | W1-L5b, W2-L1e | Tekstinvoer via IME; tekstbalk, fontkeuze en tekst-op-pad-UI ontbreken. |
| F12 Variabele tekst: serial, datum, CSV, advance, bake | Window ▸ Variable Text | `burnburn-generators/src/rows.rs`, `burnburn-presentation/src/production.rs` | `csv_parses_quotes_escapes_crlf_and_empty_cells` | – | W1-L5c, W3-L1 | CSV-domein; paneel en advance/bake ontbreken. |
| F13 Barcodes/QR (29 symbologieën) | Tools ▸ Create Bar Code | `burnburn-generators/src/barcode.rs`, `qr.rs` | `code128_symbols_match_hand_computed_vectors`; `qr_geometry_is_one_square_per_dark_module_inside_the_quiet_zone` | – | W2-L5d, W3-L1 | 2 van 29 symbologieën. |
| F14 Beeld: adjust, crop, mask, replace, save processed | Tools / Edit | `burnburn-image/src/adjust.rs`, `mask.rs`, `prepare.rs` | `region_masks_include_and_exclude_without_touching_source`; `crop_and_quarter_turns_map_pixels_and_swap_scale` | – | W1-L5a, W2-L1f | Domein; beeldpaneel ontbreekt. |
| F15 Image modes: threshold, ordered, Atkinson, dither, Stucki, Jarvis, newsprint, halftone, sketch, grayscale; negative, dot width, pass-through | Cut Settings ▸ Image | `burnburn-cam/src/image.rs`; `burnburn-document/src/process/settings.rs` (`DitherMode`); `burnburn-image/src/threshold.rs`, `gray.rs` | `atkinson_gradient_matches_independent_reference`, `floyd_steinberg_gradient_matches_independent_reference`, `jarvis_judice_ninke_gradient_matches_independent_reference`, `stucki_gradient_matches_independent_reference`, `halftone_and_newsprint_and_sketch_modes_reach_the_kernels` (image/tests/modes.rs); `negative_image_burns_the_light_pixels`, `pass_through_replicates_source_pixels_without_filtering` (cam/tests/image_modes.rs) | – | W2-L1-cut | Alle modi, negatief, dot width en pass-through als domein met tests. Geen Mac-bediening: Image-operatie niet in de Cut Settings-sheet. |
| F16 Trace image (+ multi-file) | Tools ▸ Trace Image | `burnburn-trace/src/` | `headless_gate_b_ring_trace_keeps_hole` (presentation/tests/gate_b.rs) | – | W2-L1g | Tracer-paneel ontbreekt in Mac. |
| F17 Lagen C00–C29, T1/T2, sublagen, defaults per device | Window ▸ Cuts/Layers / Color Palette | `burnburn-presentation/src/controller/layers.rs` | `colour_click_with_selection_creates_a_layer_and_moves_objects_between_colours` (presentation/tests/layer_palette.rs) | deels | W2-L3a + W2-L1 | C00–C29 ✔; T1/T2, sublagen, Make Default ontbreken. |
| F18 Line-mode: passes, kerf, tabs/bridges, perforation, lead-in/out, overcut, dot/dwell | Cut Settings ▸ Line | `burnburn-cam/src/features.rs`, `line.rs` | `perforation_alternates_on_and_off_from_the_entry`, `overcut_continues_past_the_entry_along_the_contour` (cam/tests/process_features.rs); `tabs_by_spacing_get_their_count_from_each_contour_length`, `lead_angle_tilts_the_lead_towards_the_contour_tangent`, `dot_mode_fires_one_pulse_per_spacing_and_is_gated` (cam/tests/line_extras.rs) | deels | W2-L1-cut | Passes en kerf ✔ in Mac. Tabs, perforatie, lead-in/out, overcut en dot: domein en CAM-tests ✔; geen velden in Cut Settings. |
| F19 Fill/offset fill: grouping, flood fill, cross-hatch, ramp, scan angle, bidirectional, overscan | Cut Settings ▸ Fill | `burnburn-cam/src/fill.rs`, `offset_fill.rs` | `scan_angle_rotates_rows_and_keeps_the_interval` (cam/tests/process_features.rs); `cross_hatch_adds_a_perpendicular_pass_at_the_same_interval`, `flood_fill_scans_each_connected_region_completely_before_the_next`, `ramp_length_turns_every_run_into_a_ramped_run` (cam/tests/fill_extras.rs) | deels | W2-L1-cut | Passes en output ✔ in Mac. Cross-hatch, flood fill, ramp, grouping en scan angle: domein en tests ✔; geen velden in Cut Settings. |
| F20 Optimalisatie: order by layer/group/priority, inner-first, startpunt, richting, remove overlapping, backlash | Optimization Settings | `burnburn-cam/src/compile.rs` | `inner_contours_are_cut_before_the_contour_that_contains_them` (cam/tests/compile.rs) | – | W2-L3c | Inner-first in domein; optimalisatie-instellingen en dialoog ontbreken. |
| F21 Preview: simulatie, tijdschatting, travel tonen, scrubben | Window ▸ Preview | `burnburn-job/src/preview.rs` | `preview_moves_trace_to_source_and_estimate_time` (job/tests/preview.rs) | deels | W2-L1h | Plan met schatting ✔; simulatie en scrubben ontbreken. |
| F22 Laservenster: start/pause/stop, frame, home, go to origin, start from, 3×3 job origin, cut selected, selection origin | Window ▸ Laser | `burnburn-presentation`; `burnburn-machine/src/laser.rs`; `burnburn-machine/src/placement.rs`; `burnburn-machine/src/frame.rs` | `frame_program_contains_no_laser_on_command` (machine/tests/frame.rs); `go_to_origin_machine_is_zero_and_user_origin_needs_to_be_set`, `position_laser_converts_a_canvas_click_in_bed_view_to_machine_coordinates` (machine/tests/laser_actions.rs); `current_position_puts_each_3x3_anchor_of_the_job_on_the_laser`, `start_from_user_origin_without_a_user_origin_is_refused` (machine/tests/placement.rs); `lost_start_reply_does_not_cause_second_start_after_reconnect` (machine/tests/execution.rs) | deels | W2-L1-laser | Start, Annuleren, Status en Upload ✔ in Mac. Frame, home, go-to-origin, start-from, 3×3 en cut-selected: domein en tests ✔; geen intent of knop in presentatie of Mac. |
| F23 Move/jog, position laser, save/run RD, file list (DSP) | Window ▸ Move / Laser | `burnburn-macos/src/appkit/app.rs` (`ExportRd`); `burnburn-machine/src/actor/live.rs`; `burnburn-ruida/src/live.rs` | `export_sheet_ok_exports_rd_and_cancel_changes_nothing` (burnburn-macos/tests/adapter.rs); `jog_press_and_release_are_two_requests_and_a_second_key_is_refused_while_one_is_held` (machine/tests/live.rs); `a_jog_press_reaches_the_fake_controller_scrambled_with_a_valid_checksum_and_is_acked` (runtime/tests/live_dialect.rs) | deels | W2-L1-laser | Save RD ✔ in Mac. Jog, file list en pauze: domein en registry-bytes ✔, niet op hardware (W1-HW); geen Mac-knoppen. |
| F24 Machinebewijs: verified wire-dialect op echte Ruida | Laser | `burnburn-ruida/src/`; `docs/hardware/` | `upload_ack_is_not_shown_as_stored`, `identity_mismatch_uploads_nothing` (presentation/tests/gate_c.rs) | – | W1-HW | Handmatig met hardware; nog niet gedaan. Upload wordt geweigerd tot bewijs. |
| F25 Device settings, rotary (chuck/roller), auto-feed, Z/focus | Laser Tools / Device Settings | `burnburn-job/src/profile.rs` | `capability_matrix_gates_each_feature_with_a_typed_error` (cam/tests/process_features.rs) | – | W2-L4b, W2-L3d | Profiel-domein; venster, rotary en feeder ontbreken. |
| F26 Material test, interval test, focus test | Laser Tools | – | – | – | W2-L3e | |
| F27 Material library, art library, bundles, presets | File / Window | `burnburn-io/src/std_fs.rs` (lezen); `burnburn-document/src/process/layer.rs` | – | – | W2-L6b, W3-L1 | Geen bibliotheek met schema-migratie of paneel. |
| F28 Camera: lenskalibratie, workspace alignment, overlay, opacity, capture, meerdere camera's | Laser Tools / Window | `burnburn-macos/src/calib.rs`, `burnburn-macos/src/camera.rs` | `radial_lens_distortion_is_modelled_and_estimated`, `calibration_is_invalid_after_camera_resolution_changes` (burnburn-macos/tests/calibration.rs) | deels | W2-L5e, W3-L1 | Cameraspaneel en kalibratie in domein; overlay op canvas en kalibratiewizards ontbreken. |
| F29 Print and cut, center finder | Laser Tools | – | – | – | W2-L2d, W3-L1 | Registratie-algoritmen ontbreken. |
| F30 Import: SVG, DXF, AI/PDF, HPGL/PLT, bitmaps, LBRN2, templates | File ▸ Import | `burnburn-io/src/import/` | `svg_import_reports_skipped_elements_and_commits_in_one_entry`; `import_reports_unknown_lbrn2_options_without_silent_drop` (io/tests/lbrn2.rs) | deels | W1-L6a | SVG, DXF, LBRN2 ✔ (domein); PDF/AI, HPGL en bitmap-DPI-flow ontbreken. |
| F31 Export: SVG, DXF, RD, processed bitmap | File ▸ Export / Production ▸ Exporteer RD | `burnburn-io/src/export.rs`; `burnburn-machine/src/artifact.rs` | `svg_export_roundtrips_exactly_at_physical_scale`; `export_sheet_ok_exports_rd_and_cancel_changes_nothing` | deels | W2-L1 | RD ✔ in Mac; SVG/DXF-export zonder dialoog; processed bitmap ontbreekt. |
| F32 Voorkeuren, sneltoetsen-editor, taal, help, supportbundel, debuglog | File ▸ Voorkeuren / Help | `burnburn-macos/src/l10n.rs` (taal) | `locale_follows_the_first_supported_preferred_language` (burnburn-macos/tests/l10n.rs) | deels | W3-L1 | Alleen NL/EN-taal en vaste sneltoetsen. |
| F33 G-code-backend en galvo | Window ▸ Console / Macros | – | – | – | W4 | Na Ruida-pariteit. |
| F34 Windows-host op pariteit | – | `burnburn-windows/` | `wheel_pans_and_ctrl_wheel_zooms_around_the_cursor` (burnburn-windows/tests/adapter.rs) | – | W3–W4-L8 | Alleen typecheck; geen Windows-run in deze matrix. |
| F35 BurnBurn-extra: sketches en constraints | Window / sketch | `burnburn-sketch/src/`; `burnburn-document/src/sketch/`; `burnburn-macos/src/appkit/panels/sketch.rs`; Schets-menu in `burnburn-macos/src/appkit/app.rs` | `the_schets_menu_commands_all_have_tags_and_titles_that_follow_the_selection`, `the_panel_lists_each_constraint_with_its_kind_and_status_in_words` (macos/tests/sketch_panel.rs); `a_drawn_rectangle_becomes_a_sketch_in_one_undo_entry_and_keeps_its_place` (presentation/tests/sketch_mode.rs); `freedom_shows_as_a_hollow_ring_a_filled_dot_and_a_pinned_triangle` (render/tests/sketch_overlays.rs) | ✔ | – | BurnBurn-extra: Schets-menu, sketch-paneel (beperkingen, maten, conflicten) en toolstrip-panelen Beperkingen en Patroon (patroon deferred). |

## Telling en items zonder pakket

Geteld over alle 567 datarijen (A: menu-items, B: instellingen, C: dialoogopties, D: featurefamilies). Getallen zijn met een script over de markdown-tabellen berekend (ID-prefix `M`/`S`/`D`/`F`); de vorige stand (W1-L7, 18-10-2026) was ✔ 27, deels 37, – 501, ? 2.

### Telling per kolomwaarde: UI (Mac-venster)

| Tabel | ✔ | deels | – | ? | totaal |
|---|---:|---:|---:|---:|---:|
| A. Menu-items (M001–M262) | 21 | 16 | 225 | 0 | 262 |
| B. Instellingen (S01–S182) | 8 | 3 | 170 | 1 | 182 |
| C. Dialoogopties (D01–D88) | 0 | 1 | 87 | 0 | 88 |
| D. Featurefamilies (F01–F35) | 1 | 17 | 17 | 0 | 35 |
| **Totaal** | **30** | **37** | **499** | **1** | **567** |

Verschil met W1-L7 (27 / 37 / 501 / 2): ✔ +3, deels +0, – -2, ? -1. Dat komt vooral door de toolstrip uit de SolidCraft-golf (catalogus `catalog.rs`), het Schets-menu en het sketch-paneel. Image-modi, laser-jog en de meeste CAM-opties zijn domein-ready maar niet in het Mac-venster.

### Telling per kolomwaarde: pakket

| Waarde | Aantal rows | Toelichting |
|---|---:|---|
| pakket-ID | 486 | Eén of meer pakketten uit `lightburn-pariteit.md`; bij meerdere pakketten telt de eerste. |
| – | 26 | Afgerond (UI ✔) of geen pakket nodig. |
| geen | 55 | Komt in het plan niet voor; zie onderstaande lijst. |

Verdeling eerste pakket-ID (rijen): W1-HW 2, W1-L2a 9, W1-L2b 1, W1-L3a 6, W1-L3b 15, W1-L4a 21, W1-L5a 10, W1-L5b 6, W1-L5c 6, W1-L6a 14, W2-L1 3, W2-L1-beeld 9, W2-L1-cut 24, W2-L1-laser 9, W2-L1-tekenen 19, W2-L1b 24, W2-L1c 23, W2-L1d 27, W2-L1e 2, W2-L1f 4, W2-L1g 10, W2-L1h 10, W2-L1i 3, W2-L2 1, W2-L2a 32, W2-L2c 7, W2-L2d 8, W2-L3a 7, W2-L3c 1, W2-L3d 9, W2-L3e 14, W2-L4b 22, W2-L5d 38, W2-L5e 11, W2-L6b 5, W3-L1 67, W3–W4-L8 1, W4 6.

Pakketnamen: W1/W2-pakketten zoals in de golven van `lightburn-pariteit.md`. Voor rijen waarvan het domein in golf 1 is gelanded maar de Mac-bediening nog ontbreekt, staat het golf-2-pakket dat de UI doet (`W2-L1-tekenen`, `W2-L1-cut`, `W2-L1-beeld`, `W2-L1-laser`, `W2-L2`, `W2-L5`, `W2-L6`) — in plaats van het golf-1-domeinpakket.

### Items zonder pakket (55) — moeten nog in het plan

Deze items hebben in `lightburn-pariteit.md` geen pakket. Per groep een voorstel; de plannende sessie beslist.

**Menu (45)**

- *Klembord en bewerking (niet in plan):* M043 Cut, M044 Copy, M045 Duplicate, M046 Paste, M047 Paste in Place, M055 Delete Duplicates, M050 Convert to Bitmap.
- *Bestanden, sjablonen, print (niet in plan):* M016 New Window, M017 Recent Projects, M018 Recent Templates, M021 Show Notes, M024 Save As Template, M034 Print (Black Only), M035 Print (Keep Colors).
- *Tekenvolgorde (niet in plan):* M163 Push Forward, M164 Push Backward, M165 Push to Front, M166 Push to Back. Renderordening staat los van cut order; voorstel: nieuw pakket of W2-L1c.
- *Machine (schema ontbreekt, koppelen aan W1-HW):* M185 Machine Settings, M186 Get Controller Info, M182 Taper Warp.
- *Platform (OS-integratie; beslissing of buiten scope):* M001 About LightBurn, M002–M010 Services (diagnose-items), M011–M013 Hide/Hide Others/Show All, M068 AutoFill Contact, M069 AutoFill Passwords, M070 AutoFill Credit Card, M071 Start Dictation, M072 Emoji & Symbols. (M069 en M070 gaan om gevoelige gegevens; niet implementeren.)
- *Extern, licentie, diagnose:* M187 Cuttle, M248 EULA, M258 License Management, M260 Debug Drawing, M261 Convert to Cut (Debug), M262 Real-Time Statistics.

**Instellingen (10)**

- S14 Use System Clipboard (hangt aan M043–M047).
- S61 Output print: Curve Tolerance, S62 Output print: line width (hangen aan M034–M035).
- S76 OMTech Polar (machinevariant).
- S92–S96 Laser 2 (tweede laser: enable, naam, één-laser-regel, RF/fiber, offset).
- S149 PPI (Trocen-specifiek; capability `supportsPPI`).

### Bevindingen

1. **Toolbar is de catalogus-toolstrip.** `TOOLBAR_TOOLS` in `panels.rs` bestaat niet meer. `crates/burnburn-presentation/src/catalog.rs` is het enige register (63 items: 54 actieve gereedschappen, 4 zonder domein, 5 Deferred). `appkit/toolstrip.rs` bouwt daaruit zes panelen. Daardoor staan cirkel, polylijn, trim, offset, fillet/chamfer en tangent-cirkel nu in de toolbar; het oude toolbar-test `toolbar_offers_the_six_drawing_tools_in_order` bestaat niet meer en is vervangen.
2. **Menu.** Het Mac-menu heeft App, File, Edit, Tools, Schets (BurnBurn-extra) en Production. Arrange-, Window- en Laser-Tools-menu's ontbreken nog. Het Gereedschap-menu mist Cirkel, Polylijn en Trim; die zitten in de toolstrip.
3. **Commando's zonder Mac-koppeling.** In `CommandKey` (`crates/burnburn-presentation/src/intent.rs`) hebben Align (6), Distribute (2), Boolean (3), Snap (2), `ApplyPreview`, `ApplyTrace`, `CloseTracer`, `ToolTrim`, `PrepareJob` en `ApproveJob` presentatietests maar geen menu- of toolstrip-item. `CommitImport`/`CancelImport` zijn wel in het Mac-venster. Koppelen is een snelle winst voor de UI-lane; Align/Distribute horen nog in een paneel.
4. **Image-modi: domein aanwezig, Mac niet.** De modi (Threshold, Ordered, Atkinson, Floyd-Steinberg, Jarvis, Stucki, Newsprint, Halftone, Sketch, Grayscale) en de opties negatief, dot width, pass-through, scan angle, crop en mask zijn in `burnburn-image` en `burnburn-cam` gedekt; per modus staat in S164 de testnaam die er is (Threshold heeft geen eigen testnaam hier). Maar `SHEET_MODES` in `crates/burnburn-macos/src/panels/cut_settings.rs` telt drie modi (Line, Fill, Offset Fill); een image-operatie heeft geen sheet. Dit is de grootste UI-lacune van L5 (W2-L1-cut).
5. **Cut Settings-velden voor L3 ontbreken.** Tabs, leads, perforatie, overcut, dot, cross-hatch, flood fill, ramp en grouping zijn in CAM getest (`cam/tests/line_extras.rs`, `fill_extras.rs`). De sheet heeft alleen speed, power min/max, passes, kerf en output (`CutSettingsPanel`).
6. **Laservenster: domein en registry-bytes, geen intents.** Frame, home, jog, go-to-origin, user origin, start-from, cut-selected en het 3×3-anker bestaan in `burnburn-machine` met tests. `MachineAction` kent alleen Upload/Start/Cancel/QueryStatus/Reconnect (plus ConfirmStored/RecordOutcome). De Ruida-live-bytes (`burnburn-ruida/src/live.rs`) zijn alleen uit de registry afgeleid en in tests gecontroleerd; **niet op hardware bewezen** (W1-HW). De matrix markeert ze daarom niet als machinebewijs.
7. **Parametrische vormen en knoopbewerkingen: domein ✔, toolstrip gedeeltelijk.** Ster, driehoek, vijf-/achthoek en dubbele ster zijn editor-tools (`ToolKind`) maar niet in de catalogus. Afronden (`RoundCorner`) en startpunt ook niet. Smooth/corner, symmetrisch en curve↔lijn: geometrie getest, geen Mac-bediening.
8. **Schetsmodus (BurnBurn-extra, F35) is nu ✔.** Schets-menu, sketch-paneel met beperkingen en maten, en de constraint-panelen in de toolstrip zijn in tests gedekt. Het patroon-paneel (`sketch.pattern.*`) is Deferred.
9. **Geen Enhance-filter.** S171–S173 verwijzen naar een Enhance-filter die in de code niet bestaat (zoekactie op `enhance` leeg). Ze blijven `–`.
10. **Testnamen gecontroleerd, niet gedraaid.** Voor groen-status verwijst de matrix naar `docs/user-flows/story-coverage.md`, dat in deze update niet is hergenereerd. Hardware-, native- en UI-verificatie (`tests/native/macos`) is in deze sessie niet uitgevoerd.

### Vervolg

- Dit bestand hoort bij W1-L7 en W2-L7. Bijwerken na elke golf (Haiku), of wanneer een pakket een menu-item of instelling raakt.
- Items zonder pakket: de plannende sessie beslist of ze een pakket krijgen, buiten scope vallen, of naar W1-HW gaan.
- Voor golf 2 (W2-L1-cut, W2-L1-laser, W2-L1-beeld): de Mac-koppeling van de domeinrijen in dit bestand is de eerste controle na de merge.
