# Traceerbaarheidsmatrix LightBurn → BurnBurn (W1-L7)

Bijgewerkt 22-10-2026 (G6-docs) tegen main 9335726 (na golf 6): rijen die in golf 3–6 bedienbaar zijn geworden staan op ✔ of deels met crate en testnaam. Alle testnamen komen uit de dev-notes en zijn als `fn` (of native label) in de repo gevonden; er zijn in deze ronde geen tests gedraaid. Een ✔ betekent: bedienbaar in het Mac-venster en getest tegen een fake of headless; het betekent niet dat een handmatige of hardware-controle is gedaan. Opgesteld 18-10-2026 op branch `claude/l7-matrix-4acf26` (basis `7c36842`). Bijgewerkt 20-10-2026 op branch `claude/w3-l7-docs-47370e` tegen main-stand `147f7ff` (golf 1, SolidCraft en golf 2 gemerged). Hoort bij [lightburn-pariteit.md](lightburn-pariteit.md) en het dossier in `research/lightburn/`. Eén rij per menu-item (M001–M262), per instelling (tabel B) en per dialoogoptie (tabel C); tabel D geeft de featurefamilies uit `02-features.md`.

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
| M015 New | File ▸ New | `burnburn-presentation/src/intent.rs` (`CommandKey::New`), `burnburn-macos/src/appkit/app.rs` | – | deels | – | Menu Cmd+N. Geen directe testnaam gevonden (`new_doc` in de tests is een helper, geen test). |
| M016 New Window | File | – (tweede venster: `burnburn-macos/src/appkit/app.rs`) | `tests/native/macos/two_windows.rs` (harness) | – | geen | Meerdere documentvensters bestaan; geen menu-item. |
| M017 Recent Projects | File | `burnburn-io/src/search.rs` (zoeken, geen recent-lijst) | `choosing_a_recent_entry_opens_that_path_and_an_unknown_index_does_nothing` (burnburn-macos/tests/edit_menu.rs), `an_opened_project_becomes_the_most_recent_and_the_host_is_told_to_persist_the_list` (burnburn-presentation/tests/recent_projects.rs) | ✔ | W2-L6 | Submenu Recente projecten (JSON-lijst in de gebruikersmap). |
| M018 Recent Templates | File | – | – | – | geen | Geen templates in plan (zie M024). |
| M019 Open | File ▸ Open | `burnburn-macos/src/appkit/app.rs` (`show_open_panel`), `burnburn-io/src/project.rs` | `tests/native/macos/native_file_panels.rs`, `tests/native/macos/finder_open.rs` (harness) | ✔ | – | Cmd+O. |
| M020 Import | File ▸ Importeer | `burnburn-io/src/import/mod.rs`, `burnburn-presentation/src/controller.rs` (import-preview) | `svg_import_reports_skipped_elements_and_commits_in_one_entry`, `cancelled_import_changes_nothing` (presentation/tests/import_flow.rs); `unitless_dxf_needs_explicit_scale_before_import_commit` (io/tests/import.rs) | ✔ | W1-L6a | Menu + eenheidskeuze. PDF/AI, HPGL, bitmap-DPI-flow ontbreken (`pdf_import_is_not_supported_yet`). |
| M021 Show Notes | File | – (`burnburn-history` heeft run-notities, geen job-notities) | – | – | geen | Niet in plan. |
| M022 Save | File ▸ Bewaar | `burnburn-io/src/project.rs`, `burnburn-presentation/src/controller.rs` | `failed_save_keeps_document_dirty_and_previous_file_readable`, `cancelled_save_panel_and_close_sheet_change_nothing` (presentation/tests/scenarios.rs) | ✔ | – | Cmd+S. |
| M023 Save As | File ▸ Bewaar als | zelfde als M022 | `failed_save_keeps_document_dirty_and_previous_file_readable`, `cancelled_save_panel_and_close_sheet_change_nothing` (presentation/tests/scenarios.rs) | ✔ | – | Cmd+Shift+S; dezelfde opslag-route als M022. |
| M024 Save As Template | File | – | – | – | geen | Sjabloon-concept ontbreekt; niet in plan (bibliotheken W2-L6b zijn iets anders). |
| M025 Save RD file | Production ▸ Exporteer RD | `burnburn-macos/src/appkit/app.rs` (`ExportRd`), `burnburn-machine/src/artifact.rs`, `burnburn-ruida/src/file.rs` | `export_sheet_ok_exports_rd_and_cancel_changes_nothing` (burnburn-macos/tests/adapter.rs), `headless_gate_b_same_revision_preview_and_rd` (presentation/tests/gate_b.rs) | ✔ | W1-HW | RD-bestand gemaakt. Ruida-dialect niet op hardware bewezen. |
| M026 Export | File ▸ Exporteer | `burnburn-io/src/export.rs` (SVG, DXF); `burnburn-io/src/export.rs`; `burnburn-presentation/src/files_export.rs` | `svg_export_roundtrips_exactly_at_physical_scale`, `dxf_export_roundtrips_and_names_the_ellipse_approximation` (io/tests/export.rs); `export_lists_lost_constraints_before_writing`; native `file_panels_native` | ✔ | – | Exportdialoog met verliesrapport en opties (W3-UI-bestanden, `files_export`, 11 tests). Bewaarpaneel (NSSavePanel) handmatig. |
| M027 Preferences ▸ Import Prefs | File ▸ Voorkeuren | – | – | – | W3-L1 | Geen voorkeurenopslag. |
| M028 Preferences ▸ Export Prefs | File ▸ Voorkeuren | – | – | – | W3-L1 | Idem. |
| M029 Preferences ▸ Open Prefs Folder | File ▸ Voorkeuren | – | – | – | W3-L1 | Idem. |
| M030 Preferences ▸ Load Prefs Backup | File ▸ Voorkeuren | `burnburn-history/src/settings.rs` (backup van **machine**-instellingen, geen app-voorkeuren) | `settings_backup_is_journaled_and_read_back_per_machine` (history/tests/settings.rs) | – | W3-L1 | Alleen machine-backup bestaat. |
| M031 Preferences ▸ Edit Hotkeys | File ▸ Voorkeuren | `burnburn-editor/src/input.rs` (modifiers); `burnburn-presentation` (sneltoetsregister, `app_shortcuts.rs`) | `modifier_flags_normalise_with_command_as_shortcut_modifier` (burnburn-macos/tests/adapter.rs); `shortcut_lines_are_named_like_the_menus_sorted_and_filterable`; native `shortcut_editor_records_resolves_conflicts_and_resets` | ✔ | – | Sneltoetseneditor met conflictmelding, gereserveerde toetsen en terugzetten (W3-UI-app). |
| M032 Bundles ▸ Import Bundle | File ▸ Bundels | `burnburn-io/src/bundle.rs`; `burnburn-presentation` (`files_bundle`) | `package_import_shows_profile_diff_before_replace` | ✔ | – | Bundelsheets voor export en import met profielverschil (W3-UI-bestanden, 7 tests). NSOpenPanel/NSSavePanel handmatig. |
| M033 Bundles ▸ Export Bundle | File ▸ Bundels | `burnburn-io/src/bundle.rs`; `burnburn-presentation` (`files_bundle`) | `package_import_shows_profile_diff_before_replace` | ✔ | – | Bundelsheets voor export en import met profielverschil (W3-UI-bestanden, 7 tests). NSOpenPanel/NSSavePanel handmatig. |
| M034 Print (Black Only) | File | – | – | – | geen | Printpijplijn staat niet in plan. |
| M035 Print (Keep Colors) | File | – | – | – | geen | Idem. |
| M036 Save Processed Bitmap | File | `burnburn-image/src/prepare.rs` (verwerkte pixels; export ontbreekt) | `the_save_panel_suggests_the_document_title_as_png` (burnburn-macos/tests/image_panels.rs) | ✔ | W2-L1-beeld | Menu Beeld ▸ "Verwerkte bitmap bewaren…" (PNG van de gescande pixels). |
| M037 Save Background Capture | File | `burnburn-macos/src/appkit/camera.rs` (capture) | `camera_asks_access_only_on_click_and_starts_the_chosen_device_in_bgra` (burnburn-macos/tests/presentation_wiring.rs) | – | W2-L5e | Cameracapture bestaat; achtergrond opslaan niet. |

### A3. Edit

| LightBurn-item | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| M038 Undo | Edit ▸ Undo | `burnburn-presentation/src/controller.rs`, `burnburn-document/src/engine.rs` | `headless_gate_a_rectangle_resize_undo_redo` (presentation/tests/scenarios.rs), `undo_redo_never_emits_machine_intent` (presentation/tests/gate_c.rs) | ✔ | – | Cmd+Z. |
| M039 Redo | Edit ▸ Redo | zelfde als M038 | `headless_gate_a_rectangle_resize_undo_redo` | ✔ | – | Cmd+Shift+Z. |
| M040 Undo History | Edit | `burnburn-history/src/history.rs` (journal); `burnburn-presentation` (`app_history.rs`) | `a_click_on_an_earlier_step_goes_back_and_the_later_steps_stay_listed_as_undone`; native `history_panel_click_goes_back_and_forward` | ✔ | – | Ongedaan-maakgeschiedenis als paneel (W3-UI-app). |
| M041 Select All | Edit ▸ Selecteer alles | `burnburn-macos/src/appkit/app.rs` (`select_all`), `burnburn-editor/src/selection.rs` | – | deels | – | Cmd+A via `selectAll:`. Geen testnaam gevonden; niet bevestigd. |
| M042 Invert Selection | Edit | `burnburn-presentation` (`edit_arrange.rs`) | `invert_selection_flips_between_selected_and_not_selected` | ✔ | – | Selectie omkeren (W2-L1-bewerken). |
| M043 Cut | Edit | – | `cut_removes_the_selection_in_one_undo_entry_and_paste_brings_it_back` (burnburn-presentation/tests/edit_clipboard.rs) | ✔ | W2-L1-bewerken | Menu Wijzig ▸ Knip (klembord via de responder chain). |
| M044 Copy | Edit | – | `copy_hands_the_selection_to_the_host_and_changes_no_document` (burnburn-presentation/tests/edit_clipboard.rs), `copy_with_nothing_selected_does_nothing` (burnburn-presentation/tests/edit_clipboard.rs) | ✔ | W2-L1-bewerken | Menu Wijzig ▸ Kopieer. |
| M045 Duplicate | Edit | – | `duplicate_copies_in_place_and_leaves_the_clipboard_alone` (burnburn-presentation/tests/edit_clipboard.rs) | ✔ | W2-L1-bewerken | Menu Wijzig ▸ Dupliceer. |
| M046 Paste | Edit | – | `paste_sends_the_pasteboard_bytes_when_we_own_the_type_and_the_controllers_clipboard_otherwise` (burnburn-macos/tests/edit_menu.rs), `a_copy_then_a_paste_of_the_pasteboard_bytes_duplicates_the_object` (burnburn-macos/tests/edit_menu.rs) | ✔ | W2-L1-bewerken | Menu Wijzig ▸ Plakken (eigen pasteboardtype). |
| M047 Paste in Place | Edit | – | – | – | geen | Idem. |
| M048 Delete | Edit ▸ Verwijder | `burnburn-document/src/edit.rs` (`GeometryCommand`) | `deleting_node_breaks_reference_to_merged_edge` (document/tests/modifiers.rs) | ✔ | – | Testnaam betreft knoopverwijdering; objectverwijdering via menu. |
| M049 Convert to Path | Edit | `burnburn-document/src/edit.rs` (`to_path`), `burnburn-document/src/structure/part.rs` | `convert_to_path_removes_the_shape_section_and_is_one_undo_entry` (burnburn-presentation/tests/draw_tools.rs) | ✔ | W1-L2b | Mac-menu "Omzetten naar pad" (alleen als alle selectie parametrisch is). |
| M050 Convert to Bitmap | Edit | – | – | – | geen | Niet in plan. |
| M051 Close Path | Edit | `burnburn-geometry/src/join.rs`; `burnburn-geometry/src/path.rs` | `close_and_join_ends_close_an_open_path` (burnburn-presentation/tests/node_tools.rs) | ✔ | W1-L2a | Knop "Pad sluiten" in de knooppunt-werkbalk. |
| M052 Close Selected Paths With Tolerance | Edit | `burnburn-geometry/src/repair.rs` | `join_asks_for_a_tolerance_and_joins_the_selected_paths_in_one_entry` (burnburn-presentation/tests/draw_tools.rs) | ✔ | W1-L2a | Sheet met toleranties-veld (mm) voor de geselecteerde paden. |
| M053 Auto-Join Selected Shapes | Edit | `burnburn-geometry/src/join.rs` | `join_asks_for_a_tolerance_and_joins_the_selected_paths_in_one_entry` (burnburn-presentation/tests/draw_tools.rs) | ✔ | W1-L2a | Zelfde sheet als M052; meerdere objecten worden één object. |
| M054 Optimize Selected Shapes | Edit | `burnburn-geometry/src/simplify.rs` | `optimize_removes_a_node_that_is_on_the_line_and_says_so_when_nothing_is_left_to_do` (burnburn-presentation/tests/draw_tools.rs) | ✔ | W1-L2a | Dialoog met foutbudget (mm); knopentelling/preview ontbreekt (open). |
| M055 Delete Duplicates | Edit | – | `delete_duplicates_keeps_the_first_of_identical_stacked_shapes_in_one_entry` (burnburn-presentation/tests/edit_clipboard.rs) | ✔ | W2-L1-bewerken | Menu Wijzig ▸ Verwijder dubbelen. |
| M056 Reverse Shape Direction | Edit | `burnburn-geometry/src/node_ops.rs`; `burnburn-geometry/src/path.rs` | `direction_commands_reverse_or_set_the_winding_in_one_entry` (burnburn-presentation/tests/draw_tools.rs) | ✔ | W1-L2a | Menu "Richting omkeren" en "Richting wijzigen". |
| M057 Select Open Shapes | Edit | `burnburn-presentation` (`edit_arrange.rs`) | `select_open_shapes_picks_only_unclosed_paths` | ✔ | – | Open vormen selecteren (W2-L1-bewerken). |
| M058 Select Open Shapes Set to Fill | Edit | – | – | – | W2-L2a | |
| M059 Select All Shapes in Current Layer | Edit | `burnburn-presentation/src/controller/layers.rs` | – | – | W2-L2a | Laagselectie; geen commando. |
| M060 Flash Shapes in Current Layer | Edit | – | – | – | W2-L2a | |
| M061 Select Contained Shapes | Edit | `burnburn-presentation` (`edit_arrange.rs`) | `select_contained_picks_what_lies_inside_the_selection` | ✔ | – | Ingesloten vormen selecteren (W2-L1-bewerken). |
| M062 Select Shapes Smaller Than Selected | Edit | – | – | – | W2-L2a | |
| M063 Select Circles | Edit | – | – | – | W2-L2a | |
| M064 Image options ▸ Refresh Image | Edit | `burnburn-presentation` (`image_flow.rs`) | `im03_refresh_after_replace_and_undo_reads_the_original_file_again` | ✔ | – | Vernieuwen leest het oorspronkelijke bestand (bugfix W4-acceptatie). `origins` is sessiestaat: na heropenen werkt Vernieuw niet. |
| M065 Image options ▸ Replace Image | Edit | `burnburn-presentation` (`image_flow.rs`) | `replacing_keeps_the_frame_and_the_settings_and_changes_the_pixels` | ✔ | – | Beeld vervangen met behoud van frame en instellingen. Replace to Fit (M066) ontbreekt. |
| M066 Image options ▸ Replace Image to Fit | Edit | – | – | – | W2-L1f | |
| M067 Settings | Edit | `burnburn-presentation` (`app_prefs.rs`) | `preferences_show_every_row_and_send_the_text_the_id_or_on_off_as_the_mac_does`; `preferences_are_never_part_of_a_project_file` | ✔ | – | Voorkeurenvenster met eenheden, autosave, desktop-automatisering (W3-UI-app, G5-tekst-api). Import/export/backup van voorkeuren (M027–M030) niet gebouwd. |
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
| M077 Draw Shape ▸ Triangle | Tools | `burnburn-editor/src/tool/parametric.rs` (`ToolKind::Triangle`); `burnburn-geometry/src/shapes.rs` | `a_dragged_star_is_a_named_shape_with_parameters_in_the_inspector` (burnburn-presentation/tests/draw_tools.rs) | ✔ | W2-L1-tekenen | Toolstrip via de catalogus (vormengroep). |
| M078 Draw Shape ▸ Pentagon | Tools | `burnburn-editor/src/tool/parametric.rs` (`ToolKind::Pentagon`); `burnburn-geometry/src/shapes.rs` | `one_polygon_tool_has_all_the_constructions_as_variants_of_one_group` (burnburn-presentation/src/catalog.rs) | ✔ | W2-L1-tekenen | Toolstrip via de catalogus (vaste zijden). |
| M079 Draw Shape ▸ Polygon | Tools | `burnburn-editor/src/tool/polygon.rs`; `burnburn-presentation/src/catalog.rs` | `regular_polygons_have_vertices_on_the_circle_equal_sides_and_exact_area` (geometry/tests/param_shapes.rs) | ✔ | – | Toolstrip: veelhoek ingeschreven, omschreven en op zijde (`sketch.polygon.*`). |
| M080 Draw Shape ▸ Octagon | Tools | `burnburn-editor/src/tool/parametric.rs` (`ToolKind::Octagon`); `burnburn-geometry/src/shapes.rs` | `one_polygon_tool_has_all_the_constructions_as_variants_of_one_group` (burnburn-presentation/src/catalog.rs) | ✔ | W2-L1-tekenen | Toolstrip via de catalogus (vaste zijden). |
| M081 Draw Shape ▸ Star | Tools | `burnburn-editor/src/tool/parametric.rs` (`ToolKind::Star`); `burnburn-geometry/src/shapes.rs` | `a_dragged_star_is_a_named_shape_with_parameters_in_the_inspector` (burnburn-presentation/tests/draw_tools.rs) | ✔ | W2-L1-tekenen | Toolstrip; parameterpaneel (punten, verhouding). |
| M082 Draw Shape ▸ Dual Star | Tools | `burnburn-editor/src/tool/parametric.rs` (`ToolKind::DoubleStar`); `burnburn-geometry/src/shapes.rs` | `a_wrong_shape_parameter_is_shown_in_its_row_and_changes_nothing` (burnburn-presentation/tests/draw_tools.rs) | ✔ | W2-L1-tekenen | Toolstrip via de catalogus; parameters als bij de ster. |
| M083 Edit Nodes | Tools ▸ Knopen | `burnburn-editor/src/tool/node.rs`; `burnburn-geometry/src/node.rs`, `node_ops.rs` | `smooth_symmetric_and_corner_change_the_handles_of_the_selected_node` (burnburn-presentation/tests/node_tools.rs), `line_and_curve_buttons_act_on_the_segment_between_two_selected_nodes` (burnburn-presentation/tests/node_tools.rs), `delete_and_break_work_on_the_selected_nodes_in_one_entry_each` (burnburn-presentation/tests/node_tools.rs) | ✔ | W2-L1-tekenen | Knooppunt-werkbalk (smooth/symmetric/corner, lijn↔kromme, sluiten, verbinden, verwijderen, breken, startpunt). |
| M084 Trim Shapes | Tools | `burnburn-editor/src/tool/trim.rs`; `burnburn-document/src/edit.rs` (trim) | `trimming_a_circle_and_a_line_with_curve_aware_trim` (document/tests/modify_commands.rs); `trim_mode_click_removes_the_picked_piece_up_to_the_crossing` (presentation/tests/edit_ops.rs); `trim_mode_click_removes_the_picked_piece_up_to_the_crossing` | ✔ | – | Trimmodus werkt in het venster (W2-L1-tekenen/bewerken). |
| M085 Add Tabs | Tools | `burnburn-cam/src/features.rs`, `burnburn-cam/src/line.rs` | `tabs_by_count_and_by_spacing_are_one_command_each_and_cut_the_bridges_out` (burnburn-presentation/tests/cut_settings_options.rs) | deels | W2-L1-cut | Geen tabgereedschap; tabs via Cut Settings ▸ Line. |
| M086 Edit Text | Tools | `burnburn-document/src/text/model.rs`, `burnburn-text/src/` | `committed_text_is_one_text_command_and_extends_the_selected_text` (presentation/tests/gui_requests.rs); `text_is_placed_where_the_text_tool_was_clicked` (presentation/tests/tool_steps.rs) | deels | W1-L5b | Tekst-gereedschap in toolstrip (`sketch.text`); tekstinvoer via IME (`tests/native/macos/canvas_text_input.rs`). Geen tekstbewerking of tekstbalk. |
| M087 Position Laser | Tools | `burnburn-machine/src/laser.rs`; `burnburn-machine/src/frame.rs` | `position_laser_turns_a_canvas_click_into_a_move_and_selects_nothing` (burnburn-presentation/tests/laser_window.rs) | deels | W2-L1-laser | Canvasklik in Position-Laser-modus; geweigerd (NotVerified) tot MoveAbsolute op hardware bewezen. |
| M088 Measure | Tools ▸ Meten | `burnburn-presentation/src/task.rs` (measure-taak) | `measure_reports_count_and_union_bounds`, `cancelled_measure_returns_cancelled_not_partial_result` (presentation/src/task.rs) | ✔ | – | Tools-menu `Measure`. |
| M089 Create Bar Code | Tools | `burnburn-generators/src/barcode.rs` (Code 128), `burnburn-generators/src/qr.rs`; `burnburn-generators/src/barcode.rs`, `symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `code128_symbols_match_hand_computed_vectors`, `qr_geometry_is_one_square_per_dark_module_inside_the_quiet_zone` (generators/tests); `create_makes_one_barcode_object_in_the_middle_of_the_view_in_one_undo_step`; `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Barcodedialoog met 29 typen, 27 geïmplementeerd (alleen ArUco MIP 36h11 en Grid Matrix niet); streepjescode is een generatorobject met stabiele key (G6-editor). Leesproef handmatig. |
| M090 Offset Shapes | Tools | presentatie `Offset`-intent; `burnburn-editor/src/tool/offset.rs`; `burnburn-geometry`; `burnburn-presentation` (`g5_edit.rs`); paneel Venster ▸ Offset en reparatie… | `offset_panel_adds_one_offset_object_in_one_entry`, `impossible_offset_reports_location_and_keeps_document` (presentation/tests/edit_ops.rs); `offset_panel_adds_one_offset_object_in_one_entry`; `the_offset_preview_is_a_task_that_never_changes_the_document`; `applying_an_offset_uses_the_chosen_options` | ✔ | – | Offset met voorbeeldtaak, hoekvorm en bron houden (G5-UI-gaten, G5-native). Extend/fillet-UI open. |
| M091 Weld Shapes | Tools | – | – | – | W2-L2c | |
| M092 Boolean Union | Tools | `burnburn-geometry/src/boolean.rs`; presentatie `BooleanUnion` | `applying_a_boolean_is_one_undo_entry` (presentation/tests/im09_interaction.rs); `boolean_fixture_cases_match_hand_derived_areas` (geometry/tests/boolean_fixtures.rs); `applying_a_boolean_is_one_undo_entry`; `boolean_preview_is_revision_checked_and_never_changes_the_document` | ✔ | – | Boolean-commando's volgen de selectie; preview als taak, één undo-stap. Operandvolgorde bepaalt de uitkomst. |
| M093 Boolean Subtract | Tools | zelfde als M092; `burnburn-geometry/src/boolean.rs`; presentatie | `applying_a_boolean_is_one_undo_entry`; `applying_a_boolean_is_one_undo_entry`; `boolean_preview_is_revision_checked_and_never_changes_the_document` | ✔ | – | Boolean-commando's volgen de selectie; preview als taak, één undo-stap. Operandvolgorde bepaalt de uitkomst. |
| M094 Boolean Intersection | Tools | zelfde als M092; `burnburn-geometry/src/boolean.rs`; presentatie | `applying_a_boolean_is_one_undo_entry`; `applying_a_boolean_is_one_undo_entry`; `boolean_preview_is_revision_checked_and_never_changes_the_document` | ✔ | – | Boolean-commando's volgen de selectie; preview als taak, één undo-stap. Operandvolgorde bepaalt de uitkomst. |
| M095 Boolean Assistant | Tools | – | – | – | W2-L2c | |
| M096 Cut Shapes | Tools | `burnburn-geometry/src/cut.rs`; `burnburn-document/src/edit.rs` | `cut_shapes_cuts_the_first_selected_by_the_last_and_undo_restores_both` (burnburn-presentation/tests/draw_tools.rs) | ✔ | W2-L1-tekenen | Mac-menu "Snij vormen" (snijvorm = laatst geselecteerde); keep-cutter staat vast uit. |
| M097 Adjust Image | Tools | `burnburn-image/src/adjust.rs` | `adjust_panel_shows_the_typed_values_source_size_gray_method_and_state` (burnburn-macos/tests/image_panels.rs), `adjust_panel_puts_the_gamma_issue_on_the_gamma_row` (burnburn-macos/tests/image_panels.rs) | ✔ | W2-L1-beeld | Adjust Image-paneel met schuiven; preview in het paneel, niet op het canvas. |
| M098 Trace Image | Tools | `burnburn-trace/src/lib.rs` (recept, resultaat); presentatie `ApplyTrace`/`CloseTracer` | `the_image_menu_items_ask_for_the_right_panels` (burnburn-macos/tests/image_panels.rs), `tracer_panel_mirrors_the_form_and_the_summary` (burnburn-macos/tests/image_panels.rs) | ✔ | W2-L1-beeld | Tracer-paneel; multi-file (M099) en A/B ontbreken. |
| M099 Multi-File Trace (Labs) | Tools | – | – | – | W2-L1g | Labs-capability; geen batch. |
| M100 Apply Path to Text | Tools | `burnburn-generators/src/path_text.rs` | `text_on_path_reports_overflow_and_multiline`, `text_on_circle_sits_at_radius_minus_offset_and_follows_the_tangent` (generators/tests/patterns.rs); `text_on_a_path_start_offset_and_direction_reach_the_generator` | ✔ | – | Tekst op pad met startoffset en richting (W3-UI-tekst). Resultaat is een gebakken pad, geen live binding. |
| M101 Apply Mask to Image | Tools | `burnburn-image/src/mask.rs`; `burnburn-image/src/mask.rs`; presentatie (`image_flow.rs`) | `image_flow.rs::mask::*` (burnburn-presentation/tests/image_flow.rs); `masking_targets_the_shape_and_keeps_the_picture_in_place` | ✔ | – | Mask to shape in het venster; één undo-stap (compound, W4-UI-machine). |
| M102 Crop Image | Tools | `burnburn-image/src/adjust.rs` (crop) | `typing_in_the_four_crop_fields_is_one_set_image_adjust_with_every_field` (burnburn-macos/tests/image_panels.rs), `adjust_crop_issues_name_the_crop_row_but_no_field_suffix` (burnburn-macos/tests/image_panels.rs) | ✔ | W2-L1-beeld | Crop-velden (pixels) in het Adjust-paneel; crop-handles op het canvas ontbreken. |
| M103 Resize Slots in Selection | Tools | `burnburn-presentation` (`schikken_slots.rs`) | `ok_widens_every_matching_slot_in_one_undo_entry_and_closes_the_dialog` | ✔ | – | Resize Slots in Selection (W3-UI-schikken). |
| M104 Warp Selection (4 points) | Tools | `burnburn-image/src/perspective.rs` (beeldperspectief; vectorwarp ontbreekt); `burnburn-editor/src/tool/reshape.rs`; presentatie (`schikken_reshape.rs`) | ?; `dragging_a_handle_previews_and_enter_applies_one_warp_as_one_undo_entry` | ✔ | – | Warp 4 punten (W3-UI-schikken); eigen contract, LightBurn-mapping onbevestigd. |
| M105 Deform Selection (16 points) | Tools | `burnburn-editor/src/tool/reshape.rs`; presentatie (`schikken_reshape.rs`) | `deform_has_sixteen_handles_reset_and_apply` | ✔ | – | Deform 16 punten (W3-UI-schikken); eigen bicubische tensorpatch. |
| M106 Generate Tangent Circle | Tools | `burnburn-geometry/src/construct.rs`; `burnburn-editor/src/tool/circle.rs`; `burnburn-editor/src/tool/tangent.rs`; `burnburn-geometry/src/construct.rs` | `circle_tangent_to_two_lines_with_given_radius`, `circle_tangent_to_three_lines_is_the_incircle_or_an_excircle_near_the_cursor` (geometry/tests/construct.rs); `editor::tool::tangent::tests::*`; presentatie `schikken_*` | ✔ | – | Tangent Circle: alle oplossingen, punten en cirkels (W3-UI-schikken). |

### A5. Arrange

| LightBurn-item | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| M107 Group | Arrange ▸ Groepeer | `burnburn-document/src/edit.rs` (`GeometryCommand::Group`) | `group_and_ungroup_are_single_entries_and_ungroup_takes_one_level_off` (burnburn-presentation/tests/edit_arrange.rs) | ✔ | W2-L1-bewerken | Menu Schikken ▸ Groepeer. |
| M108 Ungroup | Arrange ▸ Groep opheffen | zelfde als M107 | `group_and_ungroup_are_single_entries_and_ungroup_takes_one_level_off` (burnburn-presentation/tests/edit_arrange.rs) | ✔ | W2-L1-bewerken | Menu Schikken ▸ Groep opheffen. |
| M109 Auto-Group | Arrange | – | `auto_group_groups_shapes_that_lie_inside_each_other_and_leaves_the_rest` (burnburn-document/tests/arrange_commands.rs) | ✔ | W2-L1-bewerken | Menu Schikken ▸ Groepeer automatisch. |
| M110 Flip Horizontal | Arrange | `burnburn-geometry/src/affine.rs` (`mirror_x`) | `flips_mirror_about_the_selection_centre_and_keep_the_bounds` (burnburn-presentation/tests/edit_arrange.rs), `the_controls_are_off_without_a_selection_and_the_flip_buttons_invoke_the_flip_commands` (burnburn-macos/tests/inspector_controls.rs) | ✔ | W2-L1-bewerken | Menu Schikken ▸ Spiegel horizontaal; ook inspectorknop. |
| M111 Flip Vertical | Arrange | `burnburn-geometry/src/affine.rs` | `flips_mirror_about_the_selection_centre_and_keep_the_bounds` (burnburn-presentation/tests/edit_arrange.rs), `the_controls_are_off_without_a_selection_and_the_flip_buttons_invoke_the_flip_commands` (burnburn-macos/tests/inspector_controls.rs) | ✔ | W2-L1-bewerken | Menu Schikken ▸ Spiegel verticaal; ook inspectorknop. |
| M112 Mirror Across Line | Arrange | `burnburn-sketch` (symmetrie-relatie, geen objectspiegel) | `symmetric_points_mirror_about_a_line` (sketch/tests/solver_contract.rs) | – | W2-L1c | Spiegelen om lijn voor objecten ontbreekt; toolstrip-spiegel (`sketch.mirror`) is Deferred. |
| M113 Rotate 90° Clockwise | Arrange | `burnburn-presentation/src/controller.rs` (`rotate_selection`) | `rotation_field_turns_the_selection_about_its_centre_in_one_command` (presentation/tests/im09_interaction.rs) | – | W2-L1c | Rotatieveld ✔; 90°-commando ontbreekt. |
| M114 Rotate 90° Counter-Clockwise | Arrange | zelfde als M113 | zelfde als M113 | – | W2-L1c | |
| M115 Two-Point Rotate / Scale | Arrange | – | – | – | W2-L1c | |
| M116 Align ▸ Align Centers | Arrange ▸ Lijn uit | `burnburn-geometry/src/align.rs`; presentatie `AlignCenterX`/`AlignCenterY` | `align_and_distribute_of_many_objects_is_one_entry` (presentation/tests/im09_interaction.rs) | – | W2-L2a | Alleen X en Y apart; gecombineerd commando niet gevonden. |
| M117 Align ▸ Align Left | Arrange ▸ Lijn uit | `burnburn-geometry/src/align.rs`; presentatie `AlignLeft` | `align_and_distribute_are_one_entry_and_keep_sizes` (burnburn-document/tests/geometry_commands.rs), `the_control_has_the_six_align_and_two_distribute_commands_in_menu_order` (burnburn-macos/tests/arrange_toolbar.rs) | ✔ | W2-L1-bewerken | Uitlijnen-werkbalk en menu Schikken ▸ Lijn uit. |
| M118 Align ▸ Align Right | Arrange ▸ Lijn uit | `burnburn-geometry/src/align.rs`; `AlignRight` | `align_and_distribute_are_one_entry_and_keep_sizes` (burnburn-document/tests/geometry_commands.rs), `the_control_has_the_six_align_and_two_distribute_commands_in_menu_order` (burnburn-macos/tests/arrange_toolbar.rs) | ✔ | W2-L1-bewerken | Uitlijnen-werkbalk en menu Schikken ▸ Lijn uit. |
| M119 Align ▸ Align Top | Arrange ▸ Lijn uit | `burnburn-geometry/src/align.rs`; `AlignTop` | `align_and_distribute_are_one_entry_and_keep_sizes` (burnburn-document/tests/geometry_commands.rs), `the_control_has_the_six_align_and_two_distribute_commands_in_menu_order` (burnburn-macos/tests/arrange_toolbar.rs) | ✔ | W2-L1-bewerken | Uitlijnen-werkbalk en menu Schikken ▸ Lijn uit. |
| M120 Align ▸ Align Bottom | Arrange ▸ Lijn uit | `burnburn-geometry/src/align.rs`; `AlignBottom` | `align_and_distribute_are_one_entry_and_keep_sizes` (burnburn-document/tests/geometry_commands.rs), `the_control_has_the_six_align_and_two_distribute_commands_in_menu_order` (burnburn-macos/tests/arrange_toolbar.rs) | ✔ | W2-L1-bewerken | Uitlijnen-werkbalk en menu Schikken ▸ Lijn uit. |
| M121 Align ▸ Align H-Center | Arrange ▸ Lijn uit | `burnburn-geometry/src/align.rs`; `AlignCenterX` | `align_and_distribute_are_one_entry_and_keep_sizes` (burnburn-document/tests/geometry_commands.rs), `the_control_has_the_six_align_and_two_distribute_commands_in_menu_order` (burnburn-macos/tests/arrange_toolbar.rs) | ✔ | W2-L1-bewerken | Uitlijnen-werkbalk en menu Schikken ▸ Lijn uit. |
| M122 Align ▸ Align V-Center | Arrange ▸ Lijn uit | `burnburn-geometry/src/align.rs`; `AlignCenterY` | `align_and_distribute_are_one_entry_and_keep_sizes` (burnburn-document/tests/geometry_commands.rs), `the_control_has_the_six_align_and_two_distribute_commands_in_menu_order` (burnburn-macos/tests/arrange_toolbar.rs) | ✔ | W2-L1-bewerken | Uitlijnen-werkbalk en menu Schikken ▸ Lijn uit. |
| M123 Distribute ▸ H-Spaced | Arrange ▸ Verdeel | `burnburn-geometry/src/align.rs`; `DistributeHorizontal` | `align_and_distribute_of_many_objects_is_one_entry` (burnburn-presentation/tests/im09_interaction.rs), `the_control_has_the_six_align_and_two_distribute_commands_in_menu_order` (burnburn-macos/tests/arrange_toolbar.rs) | ✔ | W2-L1-bewerken | Uitlijnen-werkbalk en menu Schikken ▸ Verdeel horizontaal/verticaal. |
| M124 Distribute ▸ H-Centered | Arrange ▸ Verdeel | `burnburn-geometry/src/align.rs` | `align_and_distribute_of_many_objects_is_one_entry` | – | W2-L2a | Apart commando voor gecentreerd niet gevonden. |
| M125 Distribute ▸ V-Spaced | Arrange ▸ Verdeel | `burnburn-geometry/src/align.rs`; `DistributeVertical` | `align_and_distribute_of_many_objects_is_one_entry` (burnburn-presentation/tests/im09_interaction.rs), `the_control_has_the_six_align_and_two_distribute_commands_in_menu_order` (burnburn-macos/tests/arrange_toolbar.rs) | ✔ | W2-L1-bewerken | Uitlijnen-werkbalk en menu Schikken ▸ Verdeel horizontaal/verticaal. |
| M126 Distribute ▸ V-Centered | Arrange ▸ Verdeel | `burnburn-geometry/src/align.rs` | `align_and_distribute_of_many_objects_is_one_entry` | – | W2-L2a | Zie M124. |
| M127 Move H-Together | Arrange ▸ Verdeel | `burnburn-presentation` (`schikken_dock.rs`) | `move_together_closes_the_gaps_towards_the_chosen_end_in_one_entry` | ✔ | – | Move Together (W3-UI-schikken). LightBurn-gedrag onbevestigd. |
| M128 Move V-Together | Arrange ▸ Verdeel | `burnburn-presentation` (`schikken_dock.rs`) | `move_together_closes_the_gaps_towards_the_chosen_end_in_one_entry` | ✔ | – | Move Together (W3-UI-schikken). LightBurn-gedrag onbevestigd. |
| M129 Nest Selected | Arrange | `burnburn-cam/src/nest.rs`; `burnburn-cam/src/nest.rs`; presentatie (`schikken_nest.rs`) | `applying_a_nest_moves_instances_onto_the_plate_in_one_undo_step` (document/tests/nest_on_plate.rs); `nesting_keeps_order_counts_and_respects_grain_rotation` (cam/tests/nest.rs); `quick_nest_places_every_part_inside_the_bed_with_the_padding_in_one_undo_entry`; `nest_selected_moves_only_the_selection_and_stays_clear_of_the_rest` | ✔ | – | Nest als taak: annuleerbaar, verouderd resultaat valt weg (W3-UI-schikken). Productienesting met spiegelen/platen/rest in G6-werk. |
| M130 Quick Nest | Arrange | `burnburn-cam/src/nest.rs`; `burnburn-cam/src/nest.rs`; presentatie (`schikken_nest.rs`) | zelfde als M129; `quick_nest_places_every_part_inside_the_bed_with_the_padding_in_one_undo_entry`; `nest_selected_moves_only_the_selection_and_stays_clear_of_the_rest` | ✔ | – | Nest als taak: annuleerbaar, verouderd resultaat valt weg (W3-UI-schikken). Productienesting met spiegelen/platen/rest in G6-werk. |
| M131 Dock ▸ Dock Left | Arrange ▸ Dock | `burnburn-presentation` (`schikken_dock.rs`) | `dock_left_slides_the_other_objects_against_the_first_selected_in_one_undo_entry` | ✔ | – | Dock links/rechts/boven/onder (W3-UI-schikken). LightBurn-gedrag onbevestigd. |
| M132 Dock ▸ Dock Right | Arrange ▸ Dock | `burnburn-presentation` (`schikken_dock.rs`) | `dock_left_slides_the_other_objects_against_the_first_selected_in_one_undo_entry` | ✔ | – | Dock links/rechts/boven/onder (W3-UI-schikken). LightBurn-gedrag onbevestigd. |
| M133 Dock ▸ Dock Up | Arrange ▸ Dock | `burnburn-presentation` (`schikken_dock.rs`) | `dock_left_slides_the_other_objects_against_the_first_selected_in_one_undo_entry` | ✔ | – | Dock links/rechts/boven/onder (W3-UI-schikken). LightBurn-gedrag onbevestigd. |
| M134 Dock ▸ Dock Down | Arrange ▸ Dock | `burnburn-presentation` (`schikken_dock.rs`) | `dock_left_slides_the_other_objects_against_the_first_selected_in_one_undo_entry` | ✔ | – | Dock links/rechts/boven/onder (W3-UI-schikken). LightBurn-gedrag onbevestigd. |
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
| M154 Move Laser to Selection ▸ Jog Laser Left | Arrange | `burnburn-machine/src/laser.rs` (`step_jog`); `burnburn-machine/src/actor/live.rs` | `holding_a_key_jogs_and_losing_focus_releases_it` (burnburn-presentation/tests/laser_window.rs) | deels | W2-L1-laser | Pijltoetsen in het laservenster (ingedrukt houden); niet op hardware bewezen. |
| M155 Move Laser to Selection ▸ Jog Laser Right | Arrange | zelfde als M154 | `holding_a_key_jogs_and_losing_focus_releases_it` (burnburn-presentation/tests/laser_window.rs) | deels | W2-L1-laser | Pijltoetsen in het laservenster (ingedrukt houden); niet op hardware bewezen. |
| M156 Move Laser to Selection ▸ Jog Laser Down | Arrange | zelfde als M154 | `holding_a_key_jogs_and_losing_focus_releases_it` (burnburn-presentation/tests/laser_window.rs) | deels | W2-L1-laser | Pijltoetsen in het laservenster (ingedrukt houden); niet op hardware bewezen. |
| M157 Move Laser to Selection ▸ Jog Laser Up | Arrange | zelfde als M154 | `holding_a_key_jogs_and_losing_focus_releases_it` (burnburn-presentation/tests/laser_window.rs) | deels | W2-L1-laser | Pijltoetsen in het laservenster (ingedrukt houden); niet op hardware bewezen. |
| M158 Grid / Array | Arrange ▸ Grid/Array | `burnburn-cam/src/array.rs`, `burnburn-document/src/structure/modifier.rs`; `burnburn-cam/src/array.rs`; `burnburn-document/src/structure/modifier.rs`; `burnburn-geometry` (pad-array) | `grid_array_modifier_outputs_every_copy_and_follows_source_edits` (document/tests/array_modifiers.rs); `grid_and_circular_arrays_generate_placements` (cam/tests/nest.rs); `a_grid_array_is_a_preview_until_apply_and_then_one_modifier_with_one_undo_step`; `a_circular_array_turns_each_copy_about_the_centre`; `a_path_array_lists_the_other_paths_and_follows_the_chosen_one` | ✔ | – | Dialoog Schikken ▸ Reeks… met raster, cirkel en langs pad; voorbeeld als ghosts; één modifier = één undo-stap (G5-cam). |
| M159 Circular Array | Arrange | `burnburn-cam/src/array.rs`, `burnburn-document/src/structure/modifier.rs`; `burnburn-cam/src/array.rs`; `burnburn-document/src/structure/modifier.rs`; `burnburn-geometry` (pad-array) | `circular_array_modifier_turns_copies_about_the_centre` (document/tests/array_modifiers.rs); `a_grid_array_is_a_preview_until_apply_and_then_one_modifier_with_one_undo_step`; `a_circular_array_turns_each_copy_about_the_centre`; `a_path_array_lists_the_other_paths_and_follows_the_chosen_one` | ✔ | – | Dialoog Schikken ▸ Reeks… met raster, cirkel en langs pad; voorbeeld als ghosts; één modifier = één undo-stap (G5-cam). |
| M160 Copy Along Path | Arrange | `burnburn-cam/src/array.rs`; `burnburn-document/src/structure/modifier.rs`; `burnburn-geometry` (pad-array) | `a_grid_array_is_a_preview_until_apply_and_then_one_modifier_with_one_undo_step`; `a_circular_array_turns_each_copy_about_the_centre`; `a_path_array_lists_the_other_paths_and_follows_the_chosen_one` | ✔ | – | Dialoog Schikken ▸ Reeks… met raster, cirkel en langs pad; voorbeeld als ghosts; één modifier = één undo-stap (G5-cam). |
| M161 Create rubber band outline from selection | Arrange | – | – | – | W3-L1 | Convexe omhullende hoort bij nesting. |
| M162 Break Apart | Arrange | – | `break_apart_makes_one_object_per_subpath_in_one_entry` (burnburn-presentation/tests/edit_arrange.rs) | deels | W2-L1-bewerken | Domein en undo getest; menu-item voor losmaken niet in de Mac-labels gevonden. |
| M163 Push Forward in Draw Order | Arrange ▸ Tekenvolgorde | – | `draw_order_defaults_to_creation_order_and_new_objects_land_on_top` (burnburn-document/tests/arrange_commands.rs), `a_push_that_changes_nothing_is_silent_and_makes_no_history` (burnburn-presentation/tests/edit_arrange.rs) | ✔ | W2-L1-bewerken | Menu Schikken ▸ Een stap naar voren. |
| M164 Push Backward in Draw Order | Arrange ▸ Tekenvolgorde | – | `draw_order_defaults_to_creation_order_and_new_objects_land_on_top` (burnburn-document/tests/arrange_commands.rs), `a_push_that_changes_nothing_is_silent_and_makes_no_history` (burnburn-presentation/tests/edit_arrange.rs) | ✔ | W2-L1-bewerken | Menu Schikken ▸ Een stap naar achteren. |
| M165 Push to Front | Arrange ▸ Tekenvolgorde | – | – | – | geen | Domein voor vooraan staat klaar; het Mac-menu-label "vooraan" is niet gevonden. |
| M166 Push to Back | Arrange ▸ Tekenvolgorde | – | `draw_order_defaults_to_creation_order_and_new_objects_land_on_top` (burnburn-document/tests/arrange_commands.rs), `a_push_that_changes_nothing_is_silent_and_makes_no_history` (burnburn-presentation/tests/edit_arrange.rs) | ✔ | W2-L1-bewerken | Menu Schikken ▸ Achteraan. |
| M167 Lock Selected Shapes | Arrange | `burnburn-document` (draw order and locks) | `lock_marks_objects_and_is_one_undoable_step`; `locked_objects_cannot_be_picked_selected_all_or_banded_but_stay_in_the_tree` | ✔ | – | Vormen vergrendelen/ontgrendelen (W2-L1-bewerken). |
| M168 Unlock Selected Shapes | Arrange | `burnburn-document` (draw order and locks) | `lock_marks_objects_and_is_one_undoable_step`; `locked_objects_cannot_be_picked_selected_all_or_banded_but_stay_in_the_tree` | ✔ | – | Vormen vergrendelen/ontgrendelen (W2-L1-bewerken). |

### A6. Laser Tools

| LightBurn-item | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| M169 Print and Cut ▸ Start Wizard | Laser Tools | – | – | – | W2-L2d | |
| M170 Print and Cut ▸ Set First Target Position | Laser Tools | `burnburn-presentation` (`schikken_registration.rs`) | `two_measured_targets_move_the_next_compile_by_the_exact_rotation_and_translation` | ✔ | – | Print and Cut als registratie op de compile (W3-UI-schikken). Echte laserlezing handmatig/hardware. |
| M171 Print and Cut ▸ Set Second Target Position | Laser Tools | `burnburn-presentation` (`schikken_registration.rs`) | `two_measured_targets_move_the_next_compile_by_the_exact_rotation_and_translation` | ✔ | – | Print and Cut als registratie op de compile (W3-UI-schikken). Echte laserlezing handmatig/hardware. |
| M172 Print and Cut ▸ Align Output to Targets | Laser Tools | `burnburn-presentation` (`schikken_registration.rs`) | `two_measured_targets_move_the_next_compile_by_the_exact_rotation_and_translation` | ✔ | – | Print and Cut als registratie op de compile (W3-UI-schikken). Echte laserlezing handmatig/hardware. |
| M173 Print and Cut ▸ Align Output to Targets (no scaling) | Laser Tools | `burnburn-presentation` (`schikken_registration.rs`) | `two_measured_targets_move_the_next_compile_by_the_exact_rotation_and_translation` | ✔ | – | Print and Cut als registratie op de compile (W3-UI-schikken). Echte laserlezing handmatig/hardware. |
| M174 Print and Cut ▸ Reset Print and Cut | Laser Tools | `burnburn-presentation` (`schikken_registration.rs`) | `two_measured_targets_move_the_next_compile_by_the_exact_rotation_and_translation` | ✔ | – | Print and Cut als registratie op de compile (W3-UI-schikken). Echte laserlezing handmatig/hardware. |
| M175 Manage Cameras | Laser Tools | `burnburn-macos/src/appkit/camera.rs`, `burnburn-macos/src/camera.rs`, `burnburn-presentation/src/camera.rs`; `burnburn-presentation` (`camera_workspace.rs`); `burnburn-macos/src/appkit/camera.rs` | `camera_starts_only_after_permission_and_shows_the_latest_frame_as_visual_placement` (presentation/tests/gui_requests.rs); `the_lens_wizard_detects_corners_fits_the_lens_and_stores_it_for_this_configuration`; native `camera_window` | ✔ | – | Cameravenster met lens- en uitlijnwizard, beheer en meerdere camera's (W3-UI-camera). Echte camerabron handmatig; kalibraties niet persistent (W04). |
| M176 Rotary Setup | Laser Tools | `burnburn-presentation` (`cam_device.rs`) | native `device_settings_create_and_change_the_profile_with_rotary` | ✔ | – | Rotary Setup in Device Settings (W3-UI-cam). Y-schaal op een echte machine hardware. |
| M177 Feeder Setup | Laser Tools | `burnburn-cam/src/feeder.rs`; `burnburn-job` | `the_ruida_backend_refuses_every_feeder_plan_by_name` | deels | – | Feeder-dialoog (Laser ▸ Feeder…, G5-cam): herhalen, voeden, pauze, tijdschatting. RD en GRBL weigeren getypeerd; hardwarebewijs ontbreekt. |
| M178 Focus Test | Laser Tools | `burnburn-presentation` (`cam_tests_gen.rs`) | `a_focus_test_with_z_cuts_one_line_per_offset`; `a_focus_test_is_greyed_with_its_reason_without_a_z_axis` | ✔ | – | Focus Test (alleen met Z; W3-UI-cam). Geen RD-vorm (`a_focus_test_has_no_rd_form_yet_and_says_so`). |
| M179 Interval Test | Laser Tools | `burnburn-presentation` (`cam_tests_gen.rs`) | `an_interval_test_scans_each_cell_at_its_own_interval` | ✔ | – | Interval Test (W3-UI-cam). |
| M180 Material Test | Laser Tools | `burnburn-presentation` (`cam_tests_gen.rs`) | `a_material_test_makes_a_grid_plan_and_leaves_the_document_alone` | ✔ | – | Material Test (W3-UI-cam). |
| M181 Center Finder | Laser Tools | `burnburn-presentation` (`schikken_registration.rs`) | `three_edge_points_give_the_centre_the_radius_and_the_residuals` | ✔ | – | Center Finder (W3-UI-schikken). |
| M182 Taper Warp | Laser Tools | – | – | – | geen | Alleen DOC-index; Ruida niet gerealiseerd (02-features §11). |
| M183 Manage Devices | Laser Tools | `burnburn-job/src/profile.rs`; `burnburn-job/src/profile.rs`; `burnburn-presentation` (`machine_hub.rs`) | `capability_matrix_gates_each_feature_with_a_typed_error` (cam/tests/process_features.rs); `g5_machine_hub::*`; `g6_profile_open::*` | deels | – | Machinehub (macOS) en Archief ▸ Machineprofiel openen…; Windows-venster ontbreekt; het actieve apparaat wordt niet tussen starts bewaard. |
| M184 Device Settings | Laser Tools | `burnburn-job/src/profile.rs`; `burnburn-job/src/profile.rs`; `burnburn-presentation` (`cam_device.rs`, `g5_grbl_device.rs`) | `machine_frame_maps_each_origin_corner_and_back` (job/tests/plan.rs); native `device_settings_create_and_change_the_profile_with_rotary`; `g6_machine_identity::*` | ✔ | – | Device Settings: rotary, Ruida/GRBL-keuze met GRBL-velden, model/firmware als gebruikersopgave (W3-UI-cam, G5-grbl-ui, G6-machine). |
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
| M189 Preview | Window ▸ Preview | `burnburn-job/src/preview.rs`; plan-paneel in `burnburn-macos/src/panels/plan.rs`; `burnburn-job/src/preview.rs`; `burnburn-presentation` (`app_preview.rs`) | `preview_moves_trace_to_source_and_estimate_time` (job/tests/preview.rs); `headless_gate_b_same_revision_preview_and_rd` (presentation/tests/gate_b.rs); `scrubbing_puts_the_head_on_the_plan_at_that_estimated_time`; native `preview_window_picks_a_move_and_plays_to_the_end` | ✔ | – | Previewvenster met afspelen, scrubben en klik-naar-bron; tijden als schatting gelabeld (W3-UI-app). |
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
| M203 Text Options | Window | `burnburn-presentation` (`text_toolbar.rs`) | `the_toolbar_follows_the_selected_text_and_only_that`; native `text_panels_work_in_the_real_window` | ✔ | – | Teksttoolbar (W3-UI-tekst). |
| M204 Modifiers | Window | `burnburn-document/src/structure/modifier.rs` | `modifier_on_modifier_output_follows_and_cycles_are_refused` (document/tests/modifiers.rs) | – | W2-L1c | Modifierstapel niet in Mac-venster. |
| M205 Arrange | Window | `burnburn-editor/src/arrange.rs` | `arrange_and_boolean_commands_follow_the_selection` (presentation/tests/im09_interaction.rs) | – | W2-L2a | |
| M206 Arrange (long) | Window | zelfde als M205 | zelfde als M205 | – | W2-L2a | |
| M207 Color Palette | Window | `burnburn-presentation/src/controller/layers.rs`; `burnburn-macos/src/panels/operations.rs` (`palette_buttons`) | `palette_click_becomes_assign_layer_and_shift_adds` (burnburn-macos/tests/layer_panels.rs); `colour_click_with_selection_creates_a_layer_and_moves_objects_between_colours` (presentation/tests/layer_palette.rs) | ✔ | – | 30 laagkleuren C00–C29; T1/T2 niet gevonden. |
| M208 Docking | Window | – | – | – | W3-L1 | Panelen zijn vast. |
| M209 Modes | Window | `burnburn-macos/src/panels/cut_settings.rs` (`SHEET_MODES`, `sheet_mode`) | `blocked_mode_is_listed_disabled_with_the_missing_capability` (burnburn-macos/tests/layer_panels.rs) | deels | W1-L3b | Line/Fill/Image in de Cut Settings-sheet; Offset Fill uitgeschakeld in het onderzochte profiel. |
| M210 Art Library | Window | `burnburn-io` (art library); `burnburn-presentation` (`files_art.rs`) | `files_art::*` (9); host `file_panels` | ✔ | – | Art Library-paneel (W3-UI-bestanden). |
| M211 Cameras | Window | `burnburn-macos/src/appkit/camera.rs`; `burnburn-macos/src/panels/camera.rs` (`camera_panel`) | `camera_starts_only_after_permission_and_shows_the_latest_frame_as_visual_placement` (presentation/tests/gui_requests.rs) | deels | W2-L5e | Cameraspaneel; geen overlay op canvas. |
| M212 Console | Window | `burnburn-gcode` (console); `burnburn-presentation` (`g5_grbl_laser.rs`) | `g5_grbl_laser::*` (25); native `grbl_window` | ✔ | – | GRBL-console met geschiedenis en transcript — alleen tegen een nagebootste controller (G5-grbl-ui). Geen seriële adapter, geen hardware. |
| M213 Cuts / Layers | Window | `burnburn-presentation/src/controller/layers.rs`; `burnburn-macos/src/panels/operations.rs` (`OperationLine`) | `layer_table_edits_power_min_passes_and_kerf_as_raw_text` (presentation/tests/layer_palette.rs); `visibility_and_output_toggle_independently` (presentation/tests/layer_palette.rs) | deels | W2-L3a | Laagtabel en Cut Settings-sheet; sublagen en tool-lagen ontbreken; Make Default ontbreekt. |
| M214 Laser | Window | `burnburn-presentation` (machinepaneel); `burnburn-machine/src/actor.rs`; `burnburn-macos/src/panels/machine.rs` (`machine_panel`) | `the_new_menu_items_round_trip_and_translate` (burnburn-macos/tests/laser_panel.rs) | deels | W2-L1-laser | Menu Laser; start/pauze/stop, frame, home, jog, bestandslijst; move/position geweigerd tot bewezen. |
| M215 File List | Window | – | `the_controller_file_list_is_kept_and_a_missing_size_reads_unknown` (burnburn-presentation/tests/laser_window.rs) | ✔ | W2-L1-laser | Bestanden op controller: lijst, metadata, uitvoeren; verwijderen vraagt eerst een sheet. |
| M216 Macros | Window | `burnburn-gcode` (macro's); `burnburn-presentation` (`g5_grbl_laser.rs`) | `g5_grbl_laser::*`; `[gcode] console::*` (17) | ✔ | – | Benoemde macro's, regel voor regel, stoppen bij `error:N` — alleen tegen een nagebootste controller. |
| M217 Material Library | Window | `burnburn-io` (materiaalbibliotheek); `burnburn-presentation` (`files_material.rs`) | `applying_an_entry_copies_its_settings_and_shows_the_link`; `library_update_does_not_change_existing_job_settings` | ✔ | – | Materiaalbibliotheek met herkomstlinks (W3-UI-bestanden, G5-cam). |
| M218 Move | Window | – | `typed_numbers_go_to_presentation_unparsed_and_a_refusal_shows_in_words` (burnburn-macos/tests/laser_panel.rs) | deels | W2-L1-laser | Numeriek verplaatsen en jogpaneel; geweigerd tot MoveAbsolute bewezen. |
| M219 Shape Properties | Window | `burnburn-presentation/src/view_model.rs` (`InspectorViewModel`) | `a_selected_basic_shape_shows_its_parameters_with_captions_in_the_window_language` (burnburn-macos/tests/shape_panels.rs) | deels | W1-L2b | Parameterpaneel voor basisvormen ✔; paden tonen geen sectie. |
| M220 Variable Text | Window | `burnburn-generators/src/rows.rs` (CSV); `burnburn-presentation/src/production.rs`; `burnburn-generators/src/variable.rs`; `burnburn-presentation` (`variable_text.rs`) | `csv_parses_quotes_escapes_crlf_and_empty_cells`, `frozen_rows_are_reproducible_and_bound_by_hash` (generators/tests/personalisation.rs); `the_counter_advances_once_on_the_chosen_event_only`; `frozen_job_keeps_chosen_data_rows` | ✔ | – | Variabele-tekstpaneel met teller, CSV, gekozen rij; tellers in het projectbestand (W3-UI-tekst, G5-UI-gaten). Auto-Advance-gebeurtenis van LightBurn onbevestigd. |

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
| M255 Generate Support Data | Help | `burnburn-presentation` (`app_support.rs`) | `export_support_bundle` | ✔ | – | Supportbundel met zichtbaar welke gegevens erin gaan (W3-UI-app). |
| M256 Camera Selection Help | Help | – | – | – | W2-L5e | |
| M257 Check for Updates | Help | – | – | – | W4 | Installer/updates. |
| M258 License Management | Help | – | – | – | geen | Licentiesysteem mag eigen ontwerp krijgen (02-features §11). |
| M259 Enable Debug Log | Help | `burnburn-presentation` (`app_support.rs`) | native `menus_follow_the_register_and_the_debug_log_switch` | ✔ | – | Debuglog aan/uit (W3-UI-app). |
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
| S108 Mode (Line/Fill/Image/Offset Fill) | Cut Settings ▸ Gedeelde kop | `burnburn-macos/src/panels/cut_settings.rs` (`sheet_mode`) | `blocked_mode_is_listed_disabled_with_the_missing_capability` (burnburn-macos/tests/layer_panels.rs) | deels | W1-L3b | Line/Fill/Image in de Cut Settings-sheet; Offset Fill uitgeschakeld in het onderzochte profiel. |
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
| S120 Perforation Cut/Skip | Cut Settings ▸ Line / Common | `burnburn-cam/src/features.rs` | `leads_overcut_and_perforation_show_their_stored_values_and_change_the_cut` (burnburn-presentation/tests/cut_settings_options.rs) | ✔ | W2-L1-cut | Cut Settings ▸ Line (perforatie, overcut, lead-in/out). |
| S121 Tabs/Bridges: handmatig of automatisch | Cut Settings ▸ Line / Common | `burnburn-cam/src/features.rs` | `tabs_by_count_and_by_spacing_are_one_command_each_and_cut_the_bridges_out` (burnburn-presentation/tests/cut_settings_options.rs) | ✔ | W2-L1-cut | Cut Settings ▸ Line: tabs (aantal of afstand, breedte). |
| S122 Tabs: min/max, spacing, size, skip inner, clear | Cut Settings ▸ Line / Common | `burnburn-cam/src/features.rs` | `tabs_by_count_and_by_spacing_are_one_command_each_and_cut_the_bridges_out` (burnburn-presentation/tests/cut_settings_options.rs) | ✔ | W2-L1-cut | Cut Settings ▸ Line: tabs (aantal of afstand, breedte). |
| S123 Tab Cut Power | Cut Settings ▸ Line / Common | `burnburn-cam/src/features.rs` | – | – | W1-L3a | |
| S124 Overcut | Cut Settings ▸ Line / Advanced | `burnburn-cam/src/features.rs` | `leads_overcut_and_perforation_show_their_stored_values_and_change_the_cut` (burnburn-presentation/tests/cut_settings_options.rs) | ✔ | W2-L1-cut | Cut Settings ▸ Line (perforatie, overcut, lead-in/out). |
| S125 Start Pause (dwell ms) | Cut Settings ▸ Line / Advanced | – | – | – | W1-L3a | |
| S126 End Pause (dwell ms) | Cut Settings ▸ Line / Advanced | – | – | – | W1-L3a | |
| S127 Cut Through met eigen power | Cut Settings ▸ Line / Advanced | – | – | – | W1-L3a | |
| S128 Lead-In/Out (lijn/boog, lengte, hoek) | Cut Settings ▸ Line / Advanced | `burnburn-cam/src/features.rs`; `burnburn-cam/src/line.rs` | `leads_overcut_and_perforation_show_their_stored_values_and_change_the_cut` (burnburn-presentation/tests/cut_settings_options.rs) | ✔ | W2-L1-cut | Cut Settings ▸ Line (perforatie, overcut, lead-in/out). |
| S129 Override PWM Frequency | Cut Settings ▸ Line / Advanced | – | – | – | W1-L3a | Ruida-optie; eenheidsconversie niet geverifieerd. |
| S130 Dot Mode | Cut Settings ▸ Line / Advanced | – | `dot_mode_excludes_the_continuous_line_options_and_greys_them_with_a_reason` (burnburn-presentation/tests/cut_settings_options.rs) | ✔ | W2-L1-cut | Cut Settings ▸ Line: dot-modus sluit de doorlopende lijnopties uit (met reden). |
| S131 U Offset | Cut Settings ▸ Line / Advanced | – | – | – | W2-L4b | |

### B5. Fill en Offset Fill

| Instelling (LightBurn) | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| S132 Bi-directional Fill | Cut Settings ▸ Fill / Common | `burnburn-cam/src/fill.rs` | – | – | W1-L3b | |
| S133 Cross-Hatch | Cut Settings ▸ Fill / Common | – | `fill_options_are_read_and_written_as_one_update_with_unchanged_input_sending_nothing` (burnburn-presentation/tests/cut_settings_options.rs) | ✔ | W2-L1-cut | Cut Settings ▸ Fill: cross-hatch, grouping, flood fill. |
| S134 Line Interval / LPI | Cut Settings ▸ Fill / Common | `burnburn-cam/src/fill.rs` | – | – | W1-L3b | Veld niet in sheet. |
| S135 Scan Angle | Cut Settings ▸ Fill / Common | `burnburn-cam/src/fill.rs` | `fill_options_the_profile_does_not_support_are_visible_greyed_and_never_replaced` (burnburn-presentation/tests/cut_settings_options.rs) | deels | W1-L3b | Scan angle: hoek ≠ 0/90 vraagt capability ScanAngle (uitgegrijsd met reden). |
| S136 Number of Passes (fill) | Cut Settings ▸ Fill / Common | `burnburn-cam/src/compile.rs` | `passes_order_constraints_and_disabled_operations_shape_the_plan` | ✔ | – | Passes-veld gedeeld met Line. |
| S137 Z Offset / Z step (fill) | Cut Settings ▸ Fill / Common | – | – | – | W2-L3d | |
| S138 Fill All Shapes at Once / Groups Together / Individually | Cut Settings ▸ Fill / Common | – | `fill_options_are_read_and_written_as_one_update_with_unchanged_input_sending_nothing` (burnburn-presentation/tests/cut_settings_options.rs) | ✔ | W2-L1-cut | Cut Settings ▸ Fill: cross-hatch, grouping, flood fill. |
| S139 Ramp Length | Cut Settings ▸ Fill / Advanced | – | `fill_options_the_profile_does_not_support_are_visible_greyed_and_never_replaced` (burnburn-presentation/tests/cut_settings_options.rs) | deels | W2-L1-cut | Ramp: uitgegrijsd zolang het profiel het niet bewijst. |
| S140 Ramp Outer Edge | Cut Settings ▸ Fill / Advanced | – | – | – | W2-L1-cut | |
| S141 Flood Fill | Cut Settings ▸ Fill / Advanced | – | `fill_options_are_read_and_written_as_one_update_with_unchanged_input_sending_nothing` (burnburn-presentation/tests/cut_settings_options.rs) | ✔ | W2-L1-cut | Cut Settings ▸ Fill: cross-hatch, grouping, flood fill. |
| S142 Override PWM Frequency (fill) | Cut Settings ▸ Fill / Advanced | – | – | – | W1-L3b | |
| S143 U Offset (fill) | Cut Settings ▸ Fill / Advanced | – | – | – | W2-L4b | |
| S144 Interval / LPI (offset fill) | Cut Settings ▸ Offset Fill / Common | `burnburn-cam/src/offset_fill.rs` | `default_offset_fill` (presentation/src/controller/layers.rs) | – | W1-L3b | |
| S145 Passes (offset fill) | Cut Settings ▸ Offset Fill / Common | `burnburn-cam/src/offset_fill.rs` | `offset_fill_cuts_concentric_rings_at_half_step_insets` (burnburn-cam/tests/process_features.rs) | deels | – | Passes-domein getest; Offset Fill is uitgeschakeld in het onderzochte profiel (C6), dus geen Mac-bediening bevestigd. |
| S146 Z-offset / Z-step (offset fill) | Cut Settings ▸ Offset Fill / Common | `burnburn-cam/src/offset_fill.rs` | – | – | W2-L3d | |
| S147 Grouping (offset fill) | Cut Settings ▸ Offset Fill / Common | `burnburn-cam/src/offset_fill.rs` | – | – | W1-L3b | |
| S148 Override PWM (offset fill) | Cut Settings ▸ Offset Fill / Advanced | – | – | – | W1-L3b | |
| S149 PPI (controllergebonden, Trocen) | Cut Settings ▸ Offset Fill / Advanced | – | – | – | geen | Niet in plan; capability `supportsPPI` (DOC). |
| S150 U (offset fill) | Cut Settings ▸ Offset Fill / Advanced | – | – | – | W2-L4b | |

### B6. Image

| Instelling (LightBurn) | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| S151 Bidirectioneel (image) | Cut Settings ▸ Image | `burnburn-cam/src/image.rs` | – | – | W1-L5a | |
| S152 Negative Image | Cut Settings ▸ Image | – | `image_options_negative_dot_width_angle_interval_direction_overscan_and_screen` (burnburn-presentation/tests/cut_settings_options.rs) | ✔ | W2-L1-cut | Cut Settings ▸ Image-sheet (negatief, dot width, screen, interval). |
| S153 Overscan (controllergebonden) | Cut Settings ▸ Image | `burnburn-cam/src/preflight.rs` | `overscan_outside_bed_is_a_laser_off_error_without_blaming_the_run` (cam/tests/preflight.rs) | – | W1-L5a | |
| S154 Interval / DPI | Cut Settings ▸ Image | `burnburn-cam/src/image.rs`; `burnburn-image/src/scale.rs` | `image_options_negative_dot_width_angle_interval_direction_overscan_and_screen` (burnburn-presentation/tests/cut_settings_options.rs) | ✔ | W1-L5a | Cut Settings ▸ Image-sheet: scan interval / dot pitch. |
| S155 Dot Width Correction | Cut Settings ▸ Image | – | `image_options_negative_dot_width_angle_interval_direction_overscan_and_screen` (burnburn-presentation/tests/cut_settings_options.rs) | ✔ | W2-L1-cut | Cut Settings ▸ Image-sheet (negatief, dot width, screen, interval). |
| S156 Scan Angle (image) | Cut Settings ▸ Image | `burnburn-cam/src/image.rs` | `grayscale_and_oblique_image_angles_need_their_capability` (burnburn-presentation/tests/cut_settings_options.rs) | deels | W2-L1-cut | Schuine hoek vraagt capability ScanAngle (uitgegrijsd met reden). |
| S157 Angle Increment | Cut Settings ▸ Image | – | – | – | W1-L5a | |
| S158 Z-offset (image) | Cut Settings ▸ Image | – | – | – | W2-L3d | |
| S159 Cells per Inch | Cut Settings ▸ Image | – | `image_options_negative_dot_width_angle_interval_direction_overscan_and_screen` (burnburn-presentation/tests/cut_settings_options.rs) | ✔ | W2-L1-cut | Cut Settings ▸ Image-sheet (negatief, dot width, screen, interval). |
| S160 Halftone Angle | Cut Settings ▸ Image | – | `image_options_negative_dot_width_angle_interval_direction_overscan_and_screen` (burnburn-presentation/tests/cut_settings_options.rs) | ✔ | W2-L1-cut | Cut Settings ▸ Image-sheet (negatief, dot width, screen, interval). |
| S161 Passes (image) | Cut Settings ▸ Image | `burnburn-cam/src/compile.rs` | – | – | W2-L1-cut | Passes-veld alleen voor Line/Fill/Offset Fill; image-operatie heeft geen sheet-veld. |
| S162 Ramp Length (image) | Cut Settings ▸ Image | – | `ramp_runs_belong_to_binary_scans_and_are_validated` (job/tests/modes.rs) | – | W2-L1-cut | Domein getest. Geen Mac-bediening. |
| S163 Pass-Through | Cut Settings ▸ Image | `burnburn-cam/src/image.rs`; `burnburn-image/src/scale.rs` | `every_image_mode_is_one_command_and_comes_back_in_the_sheet` (burnburn-presentation/tests/cut_settings_options.rs) | ✔ | W2-L1-cut | Image-sheet: Pass-Through (geen hoek). |
| S164 Beeldmodus (10 modi) | Cut Settings ▸ Image | `burnburn-cam/src/image.rs`; `burnburn-document/src/process/settings.rs` (`DitherMode`) | `an_image_sheet_lists_the_modes_and_shows_only_the_rows_of_the_chosen_one` (burnburn-macos/tests/cut_settings_panel.rs), `every_image_mode_is_one_command_and_comes_back_in_the_sheet` (burnburn-presentation/tests/cut_settings_options.rs) | ✔ | W2-L1-cut | Image-sheet met de modus-picker (alle render-modi). |
| S165 Grouping (image) | Cut Settings ▸ Image | – | – | – | W1-L5a | |
| S166 PWM override (image) | Cut Settings ▸ Image | – | – | – | W1-L5a | |
| S167 U (image) | Cut Settings ▸ Image | – | – | – | W2-L4b | |

### B7. Adjust Image en Trace

| Instelling (LightBurn) | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| S168 Contrast | Adjust Image | `burnburn-image/src/adjust.rs` | `adjust_panel_shows_the_typed_values_source_size_gray_method_and_state` (burnburn-macos/tests/image_panels.rs) | ✔ | W2-L1-beeld | Adjust-paneel. |
| S169 Brightness | Adjust Image | `burnburn-image/src/adjust.rs` | `adjust_panel_shows_the_typed_values_source_size_gray_method_and_state` (burnburn-macos/tests/image_panels.rs) | ✔ | W2-L1-beeld | Adjust-paneel. |
| S170 Gamma | Adjust Image | `burnburn-image/src/adjust.rs` | `adjust_panel_puts_the_gamma_issue_on_the_gamma_row` (burnburn-macos/tests/image_panels.rs) | ✔ | W2-L1-beeld | Adjust-paneel. |
| S171 Enhance Amount | Adjust Image | – | – (geen enhance-code) | – | W2-L1-beeld | Enhance-filter bestaat niet in de code. |
| S172 Enhance Radius | Adjust Image | – | – (geen enhance-code) | – | W2-L1-beeld | Enhance-filter bestaat niet in de code. |
| S173 EnhanceDenoise (opgeslagen beeldveld) | Adjust Image | – | – (geen enhance-code) | – | W2-L1-beeld | Enhance-filter bestaat niet in de code. |
| S174 Invert Display (alleen preview) | Adjust Image | – | – | – | W1-L5a | |
| S175 Adjust presets | Adjust Image | – | – | – | W1-L5a | |
| S176 Trace Cutoff | Trace Image | `burnburn-trace/src/recipe.rs` | `tracer_panel_mirrors_the_form_and_the_summary` (burnburn-macos/tests/image_panels.rs) | ✔ | W2-L1-beeld | Tracer-paneel (drempel). |
| S177 Trace Threshold | Trace Image | `burnburn-trace/src/recipe.rs` | `tracer_panel_mirrors_the_form_and_the_summary` (burnburn-macos/tests/image_panels.rs) | ✔ | W2-L1-beeld | Tracer-paneel (drempel). |
| S178 Ignore Less Than | Trace Image | `burnburn-trace/src/recipe.rs` | `ignore_smaller_than_applies_to_contours_only_while_smoothness_applies_to_both` (burnburn-macos/tests/image_panels.rs) | ✔ | W2-L1-beeld | Tracer-paneel. |
| S179 Smoothness | Trace Image | `burnburn-trace/src/fit.rs` | `ignore_smaller_than_applies_to_contours_only_while_smoothness_applies_to_both` (burnburn-macos/tests/image_panels.rs) | ✔ | W2-L1-beeld | Tracer-paneel. |
| S180 Optimize | Trace Image | `burnburn-trace/src/fit.rs` | ? | – | W2-L1g | |
| S181 Transparency / Sketch Trace / previewopties | Trace Image | `burnburn-trace/src/preview.rs` | ? | – | W2-L1g | |
| S182 Boundary | Trace Image | `burnburn-trace/src/contour.rs` | ? | – | W2-L1g | |

## C. Dialoogoptie (06-dialoogcatalogus.md)

### C1. Offset

| Optie | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| D01 Offset Distance | Offset | presentatie `Offset`-intent (`burnburn-presentation/src/intent.rs`); `burnburn-presentation` (`g5_edit.rs`) | `offset_panel_adds_one_offset_object_in_one_entry` (presentation/tests/edit_ops.rs); `applying_an_offset_uses_the_chosen_options` | ✔ | – | Afstand in het offsetpaneel (G5-native). |
| D02 Direction: Outward / Inward / Both | Offset | – | – | – | W1-L2a | "Both" levert twee kanten. |
| D03 Corner Style: Round / Bevel / Corner | Offset | `burnburn-presentation` (`g5_edit.rs`) | `applying_an_offset_uses_the_chosen_options` | ✔ | – | Hoekvorm Round/Miter/Bevel. |
| D04 Outer Shapes Only | Offset | – | – | – | W1-L2a | |
| D05 Select Resulting Objects | Offset | – | – | – | W1-L2a | |
| D06 Delete Original Objects | Offset | presentatie `Offset`-intent; `burnburn-presentation` (`g5_edit.rs`) | `offset_panel_adds_one_offset_object_in_one_entry` (presentation/tests/edit_ops.rs); `applying_an_offset_uses_the_chosen_options` | ✔ | – | "Bron behouden" als optie (standaard aan: aanname). |
| D07 Optimize / Simplify Results | Offset | – | – | – | W1-L2a | |
| D08 OK / Cancel | Offset | presentatie; `burnburn-presentation` (`g5_edit.rs`) | `impossible_offset_reports_location_and_keeps_document` (presentation/tests/edit_ops.rs); `an_impossible_offset_preview_names_the_problem_and_shows_nothing` | ✔ | – | Onmogelijke offset laat het document ongewijzigd en noemt het probleem; Cancel is Sluiten. |

### C2. Create Grid Array

| Optie | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| D09 X Columns | Grid Array (horizontaal) | `burnburn-document/src/structure/modifier.rs`; `burnburn-cam/src/array.rs`; `burnburn-presentation` (`g5_cam_array.rs`) | `grid_array_modifier_outputs_every_copy_and_follows_source_edits` (document/tests/array_modifiers.rs); `a_grid_array_is_a_preview_until_apply_and_then_one_modifier_with_one_undo_step` | deels | – | Reeks-dialoog (raster/cirkel/langs pad) bestaat (G5-cam); de afzonderlijke opties zijn hier niet veld voor veld gecontroleerd. |
| D10 X Spacing | Grid Array (horizontaal) | zelfde als D09; `burnburn-cam/src/array.rs`; `burnburn-presentation` (`g5_cam_array.rs`) | zelfde als D09; `a_grid_array_is_a_preview_until_apply_and_then_one_modifier_with_one_undo_step` | deels | – | Reeks-dialoog (raster/cirkel/langs pad) bestaat (G5-cam); de afzonderlijke opties zijn hier niet veld voor veld gecontroleerd. |
| D11 X Column Shift | Grid Array (horizontaal) | zelfde als D09; `burnburn-cam/src/array.rs`; `burnburn-presentation` (`g5_cam_array.rs`) | zelfde als D09; `a_grid_array_is_a_preview_until_apply_and_then_one_modifier_with_one_undo_step` | deels | – | Reeks-dialoog (raster/cirkel/langs pad) bestaat (G5-cam); de afzonderlijke opties zijn hier niet veld voor veld gecontroleerd. |
| D12 Total Width | Grid Array (horizontaal) | zelfde als D09; `burnburn-cam/src/array.rs`; `burnburn-presentation` (`g5_cam_array.rs`) | zelfde als D09; `a_grid_array_is_a_preview_until_apply_and_then_one_modifier_with_one_undo_step` | deels | – | Reeks-dialoog (raster/cirkel/langs pad) bestaat (G5-cam); de afzonderlijke opties zijn hier niet veld voor veld gecontroleerd. |
| D13 Distance Center-to-Center vs. Edge-padding | Grid Array (algemeen) | `burnburn-cam/src/array.rs` | `grid_and_circular_arrays_generate_placements` (cam/tests/nest.rs) | – | W2-L1d | |
| D14 Reverse Direction | Grid Array (horizontaal) | `burnburn-cam/src/array.rs` | – | – | W2-L1d | |
| D15 Shift by Half | Grid Array (horizontaal) | `burnburn-cam/src/array.rs` | – | – | W2-L1d | |
| D16 Mirror Alternate Columns in X/Y | Grid Array (horizontaal) | – | – | – | W2-L1d | |
| D17 Y Rows | Grid Array (verticaal) | zelfde als D09; `burnburn-cam/src/array.rs`; `burnburn-presentation` (`g5_cam_array.rs`) | zelfde als D09; `a_grid_array_is_a_preview_until_apply_and_then_one_modifier_with_one_undo_step` | deels | – | Reeks-dialoog (raster/cirkel/langs pad) bestaat (G5-cam); de afzonderlijke opties zijn hier niet veld voor veld gecontroleerd. |
| D18 Y Spacing | Grid Array (verticaal) | zelfde als D09; `burnburn-cam/src/array.rs`; `burnburn-presentation` (`g5_cam_array.rs`) | zelfde als D09; `a_grid_array_is_a_preview_until_apply_and_then_one_modifier_with_one_undo_step` | deels | – | Reeks-dialoog (raster/cirkel/langs pad) bestaat (G5-cam); de afzonderlijke opties zijn hier niet veld voor veld gecontroleerd. |
| D19 Y Row Shift | Grid Array (verticaal) | zelfde als D09; `burnburn-cam/src/array.rs`; `burnburn-presentation` (`g5_cam_array.rs`) | zelfde als D09; `a_grid_array_is_a_preview_until_apply_and_then_one_modifier_with_one_undo_step` | deels | – | Reeks-dialoog (raster/cirkel/langs pad) bestaat (G5-cam); de afzonderlijke opties zijn hier niet veld voor veld gecontroleerd. |
| D20 Total Height | Grid Array (verticaal) | zelfde als D09; `burnburn-cam/src/array.rs`; `burnburn-presentation` (`g5_cam_array.rs`) | zelfde als D09; `a_grid_array_is_a_preview_until_apply_and_then_one_modifier_with_one_undo_step` | deels | – | Reeks-dialoog (raster/cirkel/langs pad) bestaat (G5-cam); de afzonderlijke opties zijn hier niet veld voor veld gecontroleerd. |
| D21 Mirror Alternate Rows in X/Y | Grid Array (verticaal) | `burnburn-cam/src/array.rs`; `burnburn-presentation` (`g5_cam_array.rs`) | `a_grid_array_is_a_preview_until_apply_and_then_one_modifier_with_one_undo_step` | deels | – | Reeks-dialoog (raster/cirkel/langs pad) bestaat (G5-cam); de afzonderlijke opties zijn hier niet veld voor veld gecontroleerd. |
| D22 Create Virtual Array | Grid Array (algemeen) | `burnburn-document/src/structure/modifier.rs` (virtuele array = modifier); `burnburn-cam/src/array.rs`; `burnburn-presentation` (`g5_cam_array.rs`) | `grid_array_modifier_outputs_every_copy_and_follows_source_edits`; `a_grid_array_is_a_preview_until_apply_and_then_one_modifier_with_one_undo_step` | deels | – | Reeks-dialoog (raster/cirkel/langs pad) bestaat (G5-cam); de afzonderlijke opties zijn hier niet veld voor veld gecontroleerd. |
| D23 Random Orientation (+ numeriek veld) | Grid Array (algemeen) | – | – | – | W2-L1d | LightBurn-randombeleid niet getest. |
| D24 Auto-Increment Variable Text (+ increment) | Grid Array (algemeen) | – | – | – | W1-L5c | |
| D25 Select Results / Group Results | Grid Array (algemeen) | `burnburn-presentation/src/controller.rs` | – | – | W2-L1d | |

### C3. Circular Array

| Optie | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| D26 Center X / Y | Circular Array | `burnburn-document/src/structure/modifier.rs`; `burnburn-cam/src/array.rs`; `burnburn-presentation` (`g5_cam_array.rs`) | `circular_array_modifier_turns_copies_about_the_centre` (document/tests/array_modifiers.rs); `a_grid_array_is_a_preview_until_apply_and_then_one_modifier_with_one_undo_step` | deels | – | Reeks-dialoog (raster/cirkel/langs pad) bestaat (G5-cam); de afzonderlijke opties zijn hier niet veld voor veld gecontroleerd. |
| D27 Start / End (hoek) | Circular Array | `burnburn-cam/src/array.rs`; `burnburn-cam/src/array.rs`; `burnburn-presentation` (`g5_cam_array.rs`) | `a_grid_array_is_a_preview_until_apply_and_then_one_modifier_with_one_undo_step` | deels | – | Reeks-dialoog (raster/cirkel/langs pad) bestaat (G5-cam); de afzonderlijke opties zijn hier niet veld voor veld gecontroleerd. |
| D28 Step | Circular Array | `burnburn-cam/src/array.rs`; `burnburn-cam/src/array.rs`; `burnburn-presentation` (`g5_cam_array.rs`) | `a_grid_array_is_a_preview_until_apply_and_then_one_modifier_with_one_undo_step` | deels | – | Reeks-dialoog (raster/cirkel/langs pad) bestaat (G5-cam); de afzonderlijke opties zijn hier niet veld voor veld gecontroleerd. |
| D29 Copies | Circular Array | `burnburn-cam/src/array.rs`; `burnburn-cam/src/array.rs`; `burnburn-presentation` (`g5_cam_array.rs`) | `grid_and_circular_arrays_generate_placements` (cam/tests/nest.rs); `a_grid_array_is_a_preview_until_apply_and_then_one_modifier_with_one_undo_step` | deels | – | Reeks-dialoog (raster/cirkel/langs pad) bestaat (G5-cam); de afzonderlijke opties zijn hier niet veld voor veld gecontroleerd. |
| D30 Rotate Object Copies | Circular Array | `burnburn-document/src/structure/modifier.rs`; `burnburn-cam/src/array.rs`; `burnburn-presentation` (`g5_cam_array.rs`) | `circular_array_modifier_turns_copies_about_the_centre`; `a_grid_array_is_a_preview_until_apply_and_then_one_modifier_with_one_undo_step` | deels | – | Reeks-dialoog (raster/cirkel/langs pad) bestaat (G5-cam); de afzonderlijke opties zijn hier niet veld voor veld gecontroleerd. |
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
| D43 2 of 5 | Bar Code Type ▸ 2 of 5 | `burnburn-generators/src/symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Geïmplementeerd en kiesbaar in de barcodedialoog; leesproef met een scanner handmatig. |
| D44 2 of 5 IATA | Bar Code Type ▸ 2 of 5 | `burnburn-generators/src/symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Geïmplementeerd en kiesbaar in de barcodedialoog; leesproef met een scanner handmatig. |
| D45 2 of 5 Industrial | Bar Code Type ▸ 2 of 5 | `burnburn-generators/src/symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Geïmplementeerd en kiesbaar in de barcodedialoog; leesproef met een scanner handmatig. |
| D46 AprilTag 16h5 | Bar Code Type ▸ fiducials | `burnburn-generators/src/symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Geïmplementeerd en kiesbaar in de barcodedialoog; leesproef met een scanner handmatig. |
| D47 AprilTag 25h9 | Bar Code Type ▸ fiducials | `burnburn-generators/src/symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Geïmplementeerd en kiesbaar in de barcodedialoog; leesproef met een scanner handmatig. |
| D48 AprilTag 36h10 | Bar Code Type ▸ fiducials | `burnburn-generators/src/symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Geïmplementeerd en kiesbaar in de barcodedialoog; leesproef met een scanner handmatig. |
| D49 AprilTag 36h11 | Bar Code Type ▸ fiducials | `burnburn-generators/src/symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Geïmplementeerd en kiesbaar in de barcodedialoog; leesproef met een scanner handmatig. |
| D50 ArUco MIP 36h11 | Bar Code Type ▸ fiducials | `burnburn-generators/src/symbology/` | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | deels | – | Staat in de lijst met de reden dat dit type niet ondersteund is; maakt nooit een patroon. |
| D51 ArUco OpenCV 7x7 | Bar Code Type ▸ fiducials | `burnburn-generators/src/symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Geïmplementeerd en kiesbaar in de barcodedialoog; leesproef met een scanner handmatig. |
| D52 Aztec | Bar Code Type ▸ matrix | `burnburn-generators/src/symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Geïmplementeerd en kiesbaar in de barcodedialoog; leesproef met een scanner handmatig. |
| D53 Chinese Sensible Code | Bar Code Type ▸ matrix | `burnburn-generators/src/symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Geïmplementeerd en kiesbaar in de barcodedialoog; leesproef met een scanner handmatig. |
| D54 DataMatrix | Bar Code Type ▸ matrix | `burnburn-generators/src/symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Geïmplementeerd en kiesbaar in de barcodedialoog; leesproef met een scanner handmatig. |
| D55 Grid Matrix | Bar Code Type ▸ matrix | `burnburn-generators/src/symbology/` | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | deels | – | Staat in de lijst met de reden dat dit type niet ondersteund is; maakt nooit een patroon. |
| D56 CodaBar | Bar Code Type ▸ lineair | `burnburn-generators/src/symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Geïmplementeerd en kiesbaar in de barcodedialoog; leesproef met een scanner handmatig. |
| D57 Code 128 | Bar Code Type ▸ lineair | `burnburn-generators/src/barcode.rs`; `burnburn-generators/src/symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `code128_symbols_match_hand_computed_vectors` (generators/tests); `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Geïmplementeerd en kiesbaar in de barcodedialoog; leesproef met een scanner handmatig. |
| D58 Code 128 GS1 | Bar Code Type ▸ lineair | `burnburn-generators/src/barcode.rs`; `burnburn-generators/src/symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Geïmplementeerd en kiesbaar in de barcodedialoog; leesproef met een scanner handmatig. |
| D59 Code 93 | Bar Code Type ▸ lineair | `burnburn-generators/src/symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Geïmplementeerd en kiesbaar in de barcodedialoog; leesproef met een scanner handmatig. |
| D60 EAN | Bar Code Type ▸ lineair | `burnburn-generators/src/symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Geïmplementeerd en kiesbaar in de barcodedialoog; leesproef met een scanner handmatig. |
| D61 ISBN | Bar Code Type ▸ lineair | `burnburn-generators/src/symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Geïmplementeerd en kiesbaar in de barcodedialoog; leesproef met een scanner handmatig. |
| D62 Plessey | Bar Code Type ▸ lineair | `burnburn-generators/src/symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Geïmplementeerd en kiesbaar in de barcodedialoog; leesproef met een scanner handmatig. |
| D63 UPC A | Bar Code Type ▸ lineair | `burnburn-generators/src/symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Geïmplementeerd en kiesbaar in de barcodedialoog; leesproef met een scanner handmatig. |
| D64 UPC E | Bar Code Type ▸ lineair | `burnburn-generators/src/symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Geïmplementeerd en kiesbaar in de barcodedialoog; leesproef met een scanner handmatig. |
| D65 VIN | Bar Code Type ▸ lineair | `burnburn-generators/src/symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Geïmplementeerd en kiesbaar in de barcodedialoog; leesproef met een scanner handmatig. |
| D66 PDF417 | Bar Code Type ▸ PDF417 | `burnburn-generators/src/symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Geïmplementeerd en kiesbaar in de barcodedialoog; leesproef met een scanner handmatig. |
| D67 PDF417 Compact | Bar Code Type ▸ PDF417 | `burnburn-generators/src/symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Geïmplementeerd en kiesbaar in de barcodedialoog; leesproef met een scanner handmatig. |
| D68 PDF417 Micro | Bar Code Type ▸ PDF417 | `burnburn-generators/src/symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Geïmplementeerd en kiesbaar in de barcodedialoog; leesproef met een scanner handmatig. |
| D69 QR Code | Bar Code Type ▸ QR | `burnburn-generators/src/qr.rs` (`qr_encode`); `burnburn-generators/src/symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `qr_geometry_is_one_square_per_dark_module_inside_the_quiet_zone` (generators/tests); `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Geïmplementeerd en kiesbaar in de barcodedialoog; leesproef met een scanner handmatig. |
| D70 QR Micro | Bar Code Type ▸ QR | `burnburn-generators/src/symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Geïmplementeerd en kiesbaar in de barcodedialoog; leesproef met een scanner handmatig. |
| D71 QR Micro Rectangular | Bar Code Type ▸ QR | `burnburn-generators/src/symbology/`; `burnburn-presentation` (`barcode_dialog.rs`) | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | Geïmplementeerd en kiesbaar in de barcodedialoog; leesproef met een scanner handmatig. |

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
| D78 Material Test: presetbeheer en titel | Material Test | `burnburn-presentation` (`cam_tests_gen.rs`) | `a_material_test_makes_a_grid_plan_and_leaves_the_document_alone` | ✔ | – | Material Test (W3-UI-cam); opties per veld niet afzonderlijk gecontroleerd. |
| D79 Material Test: Vertical/Rows (parameter, min, max, count, maat) | Material Test | `burnburn-presentation` (`cam_tests_gen.rs`) | `a_material_test_makes_a_grid_plan_and_leaves_the_document_alone` | ✔ | – | Material Test (W3-UI-cam); opties per veld niet afzonderlijk gecontroleerd. |
| D80 Material Test: Horizontal/Columns | Material Test | `burnburn-presentation` (`cam_tests_gen.rs`) | `a_material_test_makes_a_grid_plan_and_leaves_the_document_alone` | ✔ | – | Material Test (W3-UI-cam); opties per veld niet afzonderlijk gecontroleerd. |
| D81 Material Test: X/Y Center | Material Test | `burnburn-presentation` (`cam_tests_gen.rs`) | `a_material_test_makes_a_grid_plan_and_leaves_the_document_alone` | ✔ | – | Material Test (W3-UI-cam); opties per veld niet afzonderlijk gecontroleerd. |
| D82 Material Test: Enable Text / Border | Material Test | `burnburn-presentation` (`cam_tests_gen.rs`) | `a_material_test_makes_a_grid_plan_and_leaves_the_document_alone` | ✔ | – | Material Test (W3-UI-cam); opties per veld niet afzonderlijk gecontroleerd. |
| D83 Material Test: Edit Material/Text/Border Settings | Material Test | `burnburn-presentation` (`cam_tests_gen.rs`) | `a_material_test_makes_a_grid_plan_and_leaves_the_document_alone` | ✔ | – | Material Test (W3-UI-cam); opties per veld niet afzonderlijk gecontroleerd. |
| D84 Interval Test: Simple Fill / Dithered Image | Interval Test | `burnburn-presentation` (`cam_tests_gen.rs`) | `an_interval_test_scans_each_cell_at_its_own_interval` | ✔ | – | Interval Test (W3-UI-cam); opties per veld niet afzonderlijk gecontroleerd. |
| D85 Interval Test: steps, min/max interval | Interval Test | `burnburn-presentation` (`cam_tests_gen.rs`) | `an_interval_test_scans_each_cell_at_its_own_interval` | ✔ | – | Interval Test (W3-UI-cam); opties per veld niet afzonderlijk gecontroleerd. |
| D86 Interval Test: maat, snelheid, power | Interval Test | `burnburn-presentation` (`cam_tests_gen.rs`) | `an_interval_test_scans_each_cell_at_its_own_interval` | ✔ | – | Interval Test (W3-UI-cam); opties per veld niet afzonderlijk gecontroleerd. |
| D87 Interval Test: Preview / Frame / Start / Send / Save | Interval Test | – | – | – | W2-L3e | |
| D88 Camera manager: cameralijst | Manage Cameras | `burnburn-macos/src/appkit/camera.rs`; `burnburn-presentation` (`camera_workspace.rs`) | `two_cameras_share_one_bed_and_the_better_view_wins_where_they_overlap` | ✔ | – | Cameralijst met meerdere camera's in het cameravenster (W3-UI-camera). |

## D. Featurefamilies (02-features.md, familietabel in lightburn-pariteit.md)

| Familie | menu/dialoog | BurnBurn-crate en bestand | bestaande test(namen) | UI (Mac) | pakket | opmerking |
|---|---|---|---|---|---|---|
| F01 Werkruimte: zoom/pan/fit, wireframe/filled, grid, rulers, guides | Window / Display | `burnburn-editor/src/view.rs`, `burnburn-editor/src/snap.rs` | `zoom_keeps_anchor_world_point_fixed` (editor/src/view.rs) | deels | W2-L1b | Zoom/pan via scroll; fit, linialen, hulplijnen ontbreken. |
| F02 Selectie: gebied, omkeren, per laag, open paden, ingesloten, lock/hide | Edit / Tools | `burnburn-editor/src/selection.rs`, `tool/select.rs` | `rubber_band_selects_only_objects_fully_inside` (editor/src/selection.rs) | deels | W2-L2a + W1-L1 | Gebied en Alt-klik werken; selectiefilters, lock en hide ontbreken. |
| F03 Transform: X/Y/B/H/rotatie, %-schaal, aspectlock, shear, 3×3-referentie | Window ▸ Numerieke bewerkingen | `burnburn-presentation/src/controller.rs`; `burnburn-geometry/src/affine.rs` | `the_inspector_lists_position_size_and_the_transform_actions_in_that_order` (burnburn-macos/tests/inspector_controls.rs), `the_aspect_lock_button_toggles_the_view_model_state` (burnburn-macos/tests/inspector_controls.rs), `the_reference_grid_has_nine_labelled_points_and_a_click_sends_that_point` (burnburn-macos/tests/inspector_controls.rs), `a_scale_commit_forwards_the_raw_text_and_the_neutral_value_is_not_a_commit` (burnburn-macos/tests/inspector_controls.rs) | deels | W2-L1-bewerken | Inspector: X/Y/B/H, rotatie, %-schaal, aspectlock, referentiepunt (9) en spiegelen; shear ontbreekt; 90°-commando's ontbreken. |
| F04 Groepen, break apart, auto-group, undo-historie | Arrange / Edit | `burnburn-document/src/edit.rs`; `burnburn-presentation` (`app_history.rs`) | `nested_groups_and_ungroup_restore_membership`; `a_click_on_an_earlier_step_goes_back_and_the_later_steps_stay_listed_as_undone` | deels | – | Geschiedenispaneel ✔ (W3-UI-app); groeperen en break apart als commando's (W2-L1-bewerken); auto-group niet gecontroleerd. |
| F05 Basisvormen (driehoek, veelhoek, ster, …) parametrisch | Tools ▸ Draw Shape | `burnburn-geometry/src/shapes.rs`; `burnburn-editor/src/tool/parametric.rs`; `burnburn-document` (parametric shapes) | `a_dragged_star_is_a_named_shape_with_parameters_in_the_inspector` (burnburn-presentation/tests/draw_tools.rs), `the_polygon_constructions_and_the_rounded_rectangle_make_editable_shapes` (burnburn-presentation/tests/draw_tools.rs) | ✔ | W1-L2b | Vormtools in de toolstrip (catalogus); parameterpaneel; Convert to Path. |
| F06 Lijnen, pen, nodes: break, join, close, smooth/corner, curve↔line, trim/extend | Tools | `burnburn-editor/src/tool/node.rs`, `burnburn-geometry/src/node.rs` | `delete_and_break_work_on_the_selected_nodes_in_one_entry_each` (burnburn-presentation/tests/node_tools.rs), `close_and_join_ends_close_an_open_path` (burnburn-presentation/tests/node_tools.rs) | deels | W2-L1-tekenen | Knooppunt-werkbalk (smooth/corner, lijn↔kromme, sluiten, verbinden, verwijderen, breken, startpunt); node-contextmenu en trim/extend niet bevestigd. |
| F07 Offset, weld/booleans, cut shapes, round corners, optimize shapes, start point, tangent circle, warp/deform, resize slots, measure | Tools / Edit | `burnburn-geometry/src/boolean.rs`, `repair.rs`, `burnburn-presentation/src/task.rs` (measure) | `cut_shapes_cuts_the_first_selected_by_the_last_and_undo_restores_both` (burnburn-presentation/tests/draw_tools.rs), `join_asks_for_a_tolerance_and_joins_the_selected_paths_in_one_entry` (burnburn-presentation/tests/draw_tools.rs), `optimize_removes_a_node_that_is_on_the_line_and_says_so_when_nothing_is_left_to_do` (burnburn-presentation/tests/draw_tools.rs) | deels | W2-L1-tekenen, W2-L2c | Cut Shapes, Join met tolerantie en Optimize Shapes in de Mac-host; booleans/weld (W2-L2c) en offset-randgevallen nog niet. |
| F08 Align/distribute/dock/move together | Arrange | `burnburn-geometry/src/align.rs`; `burnburn-presentation` (`schikken_dock.rs`) | `align_and_distribute_of_many_objects_is_one_entry`; `dock_left_slides_the_other_objects_against_the_first_selected_in_one_undo_entry` | ✔ | – | Uitlijnen, verdelen, dock en move together bedienbaar (W2-L1-bewerken, W3-UI-schikken). LightBurn-gedrag van Dock/Move Together onbevestigd. |
| F09 Grid/circular array, copy along path, virtuele arrays | Arrange | `burnburn-cam/src/array.rs`; `burnburn-document/src/structure/modifier.rs` | `a_grid_array_is_a_preview_until_apply_and_then_one_modifier_with_one_undo_step` | deels | – | Reeks-dialoog met raster, cirkel en langs pad (G5-cam). Opties per veld niet gecontroleerd. |
| F10 Nest selected / quick nest | Arrange | `burnburn-cam/src/nest.rs`; `burnburn-presentation` (`schikken_nest.rs`) | `quick_nest_places_every_part_inside_the_bed_with_the_padding_in_one_undo_entry` | ✔ | – | Quick Nest en Nest Selected als taak (W3-UI-schikken); spiegelen, platen, rest en orderstatus (G6-werk). |
| F11 Tekst: font, bold/italic, uitlijning, spatiëring, welded, op pad, convert to path, SHX/RTL | Tools ▸ Edit Text / Window ▸ Text Options | `burnburn-text/src/`; `burnburn-generators/src/path_text.rs`; `burnburn-presentation` (`text_toolbar.rs`) | `the_toolbar_follows_the_selected_text_and_only_that`; `a_hebrew_word_reads_from_right_to_left` | deels | – | Teksttoolbar, convert to path, tekst op pad en RTL (W3-UI-tekst, G5-tekst-api). Geen SHX; Arabisch niet getest; tekst op pad is gebakken. |
| F12 Variabele tekst: serial, datum, CSV, advance, bake | Window ▸ Variable Text | `burnburn-generators/src/rows.rs`; `burnburn-presentation` (`variable_text.rs`) | `the_counter_advances_once_on_the_chosen_event_only`; `a_failed_upload_never_consumes_a_number` | ✔ | – | Paneel, ProductionRecord-binding en tellers in het projectbestand (W3-UI-tekst, G5-UI-gaten). |
| F13 Barcodes/QR (29 symbologieën) | Tools ▸ Create Bar Code | `burnburn-generators/src/barcode.rs`, `symbology/` | `all_29_types_are_listed_and_the_unsupported_ones_say_why` | ✔ | – | 27 van 29 symbologieën; ArUco MIP 36h11 en Grid Matrix niet. Streepjescode als generatorobject (G6-editor). |
| F14 Beeld: adjust, crop, mask, replace, save processed | Tools / Edit | `burnburn-image/src/adjust.rs`, `mask.rs`, `prepare.rs` | `adjust_panel_shows_the_typed_values_source_size_gray_method_and_state` (burnburn-macos/tests/image_panels.rs), `typing_in_the_four_crop_fields_is_one_set_image_adjust_with_every_field` (burnburn-macos/tests/image_panels.rs), `the_save_panel_suggests_the_document_title_as_png` (burnburn-macos/tests/image_panels.rs) | deels | W2-L1-beeld | Adjust, crop, replace/refresh en bewaren als bitmap in de Mac-host; Enhance-filter bestaat niet; mask-bediening niet bevestigd. |
| F15 Image modes: threshold, ordered, Atkinson, dither, Stucki, Jarvis, newsprint, halftone, sketch, grayscale; negative, dot width, pass-through | Cut Settings ▸ Image | `burnburn-cam/src/image.rs`; `burnburn-document/src/process/settings.rs` (`DitherMode`); `burnburn-image/src/threshold.rs`, `gray.rs` | `an_image_sheet_lists_the_modes_and_shows_only_the_rows_of_the_chosen_one` (burnburn-macos/tests/cut_settings_panel.rs), `every_image_mode_is_one_command_and_comes_back_in_the_sheet` (burnburn-presentation/tests/cut_settings_options.rs) | deels | W2-L1-cut | Alle modi in het Cut Settings-sheet; capability-gated opties (grijswaarden, schuine hoek) uitgegrijsd met reden. |
| F16 Trace image (+ multi-file) | Tools ▸ Trace Image | `burnburn-trace/src/` | `tracer_panel_mirrors_the_form_and_the_summary` (burnburn-macos/tests/image_panels.rs), `the_image_menu_items_ask_for_the_right_panels` (burnburn-macos/tests/image_panels.rs) | deels | W2-L1-beeld | Tracer-paneel met recepten; multi-file en eigen recepten-bibliotheek ontbreken. |
| F17 Lagen C00–C29, T1/T2, sublagen, defaults per device | Window ▸ Cuts/Layers / Color Palette | `burnburn-presentation/src/controller/layers.rs` | `colour_click_with_selection_creates_a_layer_and_moves_objects_between_colours` (presentation/tests/layer_palette.rs) | deels | W2-L3a + W2-L1 | C00–C29 ✔; T1/T2, sublagen, Make Default ontbreken. |
| F18 Line-mode: passes, kerf, tabs/bridges, perforation, lead-in/out, overcut, dot/dwell | Cut Settings ▸ Line | `burnburn-cam/src/features.rs`, `line.rs` | `leads_overcut_and_perforation_show_their_stored_values_and_change_the_cut` (burnburn-presentation/tests/cut_settings_options.rs), `tabs_by_count_and_by_spacing_are_one_command_each_and_cut_the_bridges_out` (burnburn-presentation/tests/cut_settings_options.rs), `dot_mode_excludes_the_continuous_line_options_and_greys_them_with_a_reason` (burnburn-presentation/tests/cut_settings_options.rs) | deels | W2-L1-cut | Tabs, leads, overcut, perforatie en dot in Cut Settings ▸ Line; RD-lowering voor dot/ramp wordt geweigerd (alleen preview). |
| F19 Fill/offset fill: grouping, flood fill, cross-hatch, ramp, scan angle, bidirectional, overscan | Cut Settings ▸ Fill | `burnburn-cam/src/fill.rs`, `offset_fill.rs` | `fill_options_are_read_and_written_as_one_update_with_unchanged_input_sending_nothing` (burnburn-presentation/tests/cut_settings_options.rs), `fill_options_the_profile_does_not_support_are_visible_greyed_and_never_replaced` (burnburn-presentation/tests/cut_settings_options.rs) | deels | W2-L1-cut | Cut Settings ▸ Fill: cross-hatch, grouping, flood fill; ramp en scan angle uitgegrijsd zolang het profiel ze niet bewijst. Offset Fill (C6) uit. |
| F20 Optimalisatie: order by layer/group/priority, inner-first, startpunt, richting, remove overlapping, backlash | Optimization Settings | `burnburn-cam/src/compile.rs`; `burnburn-presentation` (`cam_optimization.rs`) | `inner_contours_are_cut_before_the_contour_that_contains_them`; `apply_is_one_set_optimization_with_the_order_of_the_list` | ✔ | – | Optimalisatie-instellingen en per vorm startpunt/richting vergrendelen (W3-UI-cam, G6-machine). |
| F21 Preview: simulatie, tijdschatting, travel tonen, scrubben | Window ▸ Preview | `burnburn-job/src/preview.rs`; `burnburn-presentation` (`app_preview.rs`) | `scrubbing_puts_the_head_on_the_plan_at_that_estimated_time` | ✔ | – | Previewvenster met afspelen en scrubben (W3-UI-app). Tijd is een schatting. |
| F22 Laservenster: start/pause/stop, frame, home, go to origin, start from, 3×3 job origin, cut selected, selection origin | Window ▸ Laser | `burnburn-presentation`; `burnburn-machine/src/laser.rs`; `burnburn-machine/src/placement.rs`; `burnburn-machine/src/frame.rs` | `holding_a_key_jogs_and_losing_focus_releases_it` (burnburn-presentation/tests/laser_window.rs), `the_controller_file_list_is_kept_and_a_missing_size_reads_unknown` (burnburn-presentation/tests/laser_window.rs), `exporting_diagnostics_writes_the_rendered_replay_and_touches_no_machine` (burnburn-presentation/tests/laser_window.rs) | deels | W2-L1-laser | Laservenster: start/pauze/stop, frame (rechthoek + rubber band), home, jog, bestandslijst, diagnose-export. Positie/move geweigerd tot MoveAbsolute op hardware bewezen. |
| F23 Move/jog, position laser, save/run RD, file list (DSP) | Window ▸ Move / Laser | `burnburn-macos/src/appkit/app.rs` (`ExportRd`); `burnburn-machine/src/actor/live.rs`; `burnburn-ruida/src/live.rs` | `typed_numbers_go_to_presentation_unparsed_and_a_refusal_shows_in_words` (burnburn-macos/tests/laser_panel.rs), `the_position_reads_onbekend_never_zero_and_off_controls_explain_themselves` (burnburn-macos/tests/laser_panel.rs) | deels | W2-L1-laser | Jog, positie-uitlezing (onbekend ≠ 0) en Save/Run RD in het venster; move/go-to-origin geweigerd tot bewezen. |
| F24 Machinebewijs: verified wire-dialect op echte Ruida | Laser | `burnburn-ruida/src/`; `docs/hardware/` | `upload_ack_is_not_shown_as_stored`, `identity_mismatch_uploads_nothing` (presentation/tests/gate_c.rs) | – | W1-HW | Handmatig met hardware; nog niet gedaan. Upload wordt geweigerd tot bewijs. |
| F25 Device settings, rotary (chuck/roller), auto-feed, Z/focus | Laser Tools / Device Settings | `burnburn-job/src/profile.rs`; `burnburn-presentation` (`cam_device.rs`) | native `device_settings_create_and_change_the_profile_with_rotary` | deels | – | Device Settings met rotary (W3-UI-cam); feeder alleen als planning (RD/GRBL geweigerd). |
| F26 Material test, interval test, focus test | Laser Tools | `burnburn-presentation` (`cam_tests_gen.rs`) | `a_material_test_makes_a_grid_plan_and_leaves_the_document_alone`; `an_interval_test_scans_each_cell_at_its_own_interval`; `a_focus_test_with_z_cuts_one_line_per_offset` | ✔ | – | Material, Interval en Focus Test (W3-UI-cam). |
| F27 Material library, art library, bundles, presets | File / Window | `burnburn-io`; `burnburn-presentation` (`files_material.rs`, `files_art.rs`, `files_bundle.rs`) | `applying_an_entry_copies_its_settings_and_shows_the_link`; `package_import_shows_profile_diff_before_replace` | ✔ | – | Materiaalbibliotheek, art library en bundels (W3-UI-bestanden). |
| F28 Camera: lenskalibratie, workspace alignment, overlay, opacity, capture, meerdere camera's | Laser Tools / Window | `burnburn-presentation` (`camera_workspace.rs`); `burnburn-macos/src/appkit/camera.rs` | `two_cameras_share_one_bed_and_the_better_view_wins_where_they_overlap`; `calibration_is_invalid_after_camera_resolution_changes` | deels | – | Overlay, lens- en uitlijnwizard, beheer, meerdere camera's (W3-UI-camera). Kalibraties niet persistent; echte camerabron handmatig. |
| F29 Print and cut, center finder | Laser Tools | `burnburn-presentation` (`schikken_registration.rs`) | `two_measured_targets_move_the_next_compile_by_the_exact_rotation_and_translation` | ✔ | – | Print and Cut en Center Finder (W3-UI-schikken). Echte laserlezing hardware. |
| F30 Import: SVG, DXF, AI/PDF, HPGL/PLT, bitmaps, LBRN2, templates | File ▸ Import | `burnburn-io/src/import/`; `burnburn-presentation` (`files_import.rs`) | `svg_import_reports_skipped_elements_and_commits_in_one_entry`; `pdf_import_lists_the_pages_and_the_choice_decides_what_is_committed` | ✔ | – | SVG, DXF, PDF/AI (paginakeuze), HPGL/PLT, bitmaps incl. BMP/TIFF, LBRN2, templates (W3-UI-bestanden, W4-UI-gaten). |
| F31 Export: SVG, DXF, RD, processed bitmap | File ▸ Export / Production ▸ Exporteer RD | `burnburn-io/src/export.rs`; `burnburn-presentation` (`files_export.rs`) | `export_lists_lost_constraints_before_writing` | ✔ | – | SVG/DXF-exportdialoog met verliesrapport, RD-export, verwerkte bitmap (W3-UI-bestanden). |
| F32 Voorkeuren, sneltoetsen-editor, taal, help, supportbundel, debuglog | File ▸ Voorkeuren / Help | `burnburn-presentation` (`app_prefs.rs`, `app_shortcuts.rs`, `app_support.rs`) | `preferences_show_every_row_and_send_the_text_the_id_or_on_off_as_the_mac_does`; `export_support_bundle` | ✔ | – | Voorkeuren, sneltoetseneditor, supportbundel en debuglog (W3-UI-app). Alleen NL/EN. |
| F33 G-code-backend en galvo | Window ▸ Console / Macros | `burnburn-gcode/`; `burnburn-presentation` (`g5_grbl_*.rs`) | `g5_grbl_device::*`; `g5_grbl_export::*`; `g5_grbl_laser::*` | deels | – | GRBL-backend (W4-L4) met Device Settings, Exporteer G-code en GRBL-venster met console en macro's (G5-grbl-ui) — alleen tegen een nagebootste controller; start/upload geweigerd; geen seriële adapter. Galvo niet gebouwd. |
| F34 Windows-host op pariteit | – | `burnburn-windows/` | `wheel_pans_and_ctrl_wheel_zooms_around_the_cursor`; `menus.rs`; `g6_forms.rs` | deels | – | Windows-host op pariteit met menu's, formulieren, camera-port en recente lijsten (W4-L8a/b, G5-windows) — headless bewezen; Win32-laag alleen type-check + clippy; nooit gedraaid. |
| F35 BurnBurn-extra: sketches en constraints | Window / sketch | `burnburn-sketch/src/`; `burnburn-document/src/sketch/`; `burnburn-macos/src/appkit/panels/sketch.rs`; Schets-menu in `burnburn-macos/src/appkit/app.rs` | `the_schets_menu_commands_all_have_tags_and_titles_that_follow_the_selection`, `the_panel_lists_each_constraint_with_its_kind_and_status_in_words` (macos/tests/sketch_panel.rs); `a_drawn_rectangle_becomes_a_sketch_in_one_undo_entry_and_keeps_its_place` (presentation/tests/sketch_mode.rs); `freedom_shows_as_a_hollow_ring_a_filled_dot_and_a_pinned_triangle` (render/tests/sketch_overlays.rs) | ✔ | – | BurnBurn-extra: Schets-menu, sketch-paneel (beperkingen, maten, conflicten) en toolstrip-panelen Beperkingen en Patroon (patroon deferred). |

## Telling en items zonder pakket

### Telling per kolomwaarde: UI (Mac-venster)

| Tabel | ✔ | deels | – | ? | totaal |
|---|---:|---:|---:|---:|---:|
| A. Menu-items (M001–M262) | 116 | 22 | 124 | 0 | 262 |
| B. Instellingen (S01–S182) | 30 | 7 | 144 | 1 | 182 |
| C. Dialoogopties (D01–D88) | 41 | 17 | 30 | 0 | 88 |
| D. Featurefamilies (F01–F35) | 14 | 20 | 1 | 0 | 35 |
| **Totaal** | **201** | **66** | **299** | **1** | **567** |

Verschil met de vorige stand (20-10-2026, main 147f7ff: 91 / 53 / 422 / 1): zie de tabel hierboven; de toename komt uit golf 3–6. Tabel B (instellingen) is in deze ronde niet opnieuw gecontroleerd; van de tabellen A, C en D zijn de rijen die bij golf 3–6 horen bijgewerkt (136 rijen), de overige rijen staan nog zoals op 147f7ff.

### Telling per kolomwaarde: pakket

| Waarde | Aantal rows | Toelichting |
|---|---:|---|
| pakket-ID | 495 | Eén of meer pakketten uit `lightburn-pariteit.md`; bij meerdere pakketten telt de eerste. |
| – | 26 | Afgerond (UI ✔) of geen pakket nodig. |
| geen | 46 | Komt in het plan niet voor; zie onderstaande lijst. |

Verdeling eerste pakket-ID (rijen): W1-HW 2, W1-L2a 13, W1-L2b 3, W1-L3a 6, W1-L3b 15, W1-L4a 18, W1-L5a 9, W1-L5b 6, W1-L5c 6, W1-L6a 14, W2-L1 3, W2-L1-beeld 17, W2-L1-bewerken 23, W2-L1-cut 24, W2-L1-laser 12, W2-L1-tekenen 13, W2-L1b 24, W2-L1c 16, W2-L1d 27, W2-L1e 2, W2-L1f 3, W2-L1g 4, W2-L1h 10, W2-L1i 3, W2-L2 1, W2-L2a 24, W2-L2c 7, W2-L2d 8, W2-L3a 7, W2-L3c 1, W2-L3d 9, W2-L3e 14, W2-L4b 22, W2-L5d 38, W2-L5e 11, W2-L6 1, W2-L6b 5, W3-L1 67, W3–W4-L8 1, W4 6.

Rijen die na golf 2 in de Mac-host staan, hebben het golf-2-pakket (`W2-L1-bewerken`, `W2-L1-beeld`, `W2-L1-cut`, `W2-L1-laser`, `W2-L1-tekenen`) in plaats van het golf-1-domeinpakket.

### Items zonder pakket (46) — moeten nog in het plan

Deze items hebben in `lightburn-pariteit.md` geen pakket. De plannende sessie beslist per item of het een pakket krijgt, buiten scope valt of naar W1-HW gaat.

- **Menu (36):** M001 About LightBurn; M002 Services ▸ Activity Monitor; M003 Services ▸ Allocations & Leaks; M004 Services ▸ File Activity; M005 Services ▸ System Trace; M006 Services ▸ Time Profile Active Application; M007 Services ▸ Time Profile App Under Mouse; M008 Services ▸ Time Profile Entire System; M009 Services ▸ Toggle Instruments Recording; M010 Services ▸ Services Settings…; M011 Hide LightBurn; M012 Hide Others; M013 Show All; M016 New Window; M018 Recent Templates; M021 Show Notes; M024 Save As Template; M034 Print (Black Only); M035 Print (Keep Colors); M047 Paste in Place; M050 Convert to Bitmap; M068 AutoFill ▸ Contact…; M069 AutoFill ▸ Passwords…; M070 AutoFill ▸ Credit Card…; M071 Start Dictation…; M072 Emoji & Symbols; M165 Push to Front; M182 Taper Warp; M185 Machine Settings; M186 Get Controller Info; M187 Cuttle; M248 End User License Agreement; M258 License Management; M260 Debug Drawing; M261 Convert to Cut (Debug); M262 Real-Time Statistics.
- **Instellingen (10):** S14 Use System Clipboard; S61 Output print: Curve Tolerance (mm); S62 Output print: line width (mm); S76 OMTech Polar; S92 Laser 2 Enable Controls; S93 Laser 2 naam; S94 Only allow one laser at a time; S95 Laser 2 RF tube / Fiber; S96 Enable Laser 2 Offset (X/Y); S149 PPI (controllergebonden, Trocen).
- **Dialoogopties (0):** –.
- **Featurefamilies (0):** –.

### Bevindingen (stand main 147f7ff, golf 1 + SolidCraft + golf 2 gemerged)

1. **Menu's.** In `burnburn-macos` bestaan nu de menu's Wijzig (klembord, dupliceren, verwijder dubbelen, groepen, knopen, richting), Schikken (groeperen, spiegelen, uitlijnen, tekenvolgorde), Beeld (Adjust, Trace, Verwerkte bitmap), Weergave, Recente projecten, Laser en Productie (`l10n.rs`: `MenuEdit`, `MenuArrange`, `ImageMenu`, `MenuView`, `RecentProjects`, `MenuLaser`). Items die hier `–` blijven, ontbreken in de labels of zijn niet getest.
2. **Cut Settings.** Het sheet toont Line (tabs, leads, overcut, perforatie, dot), Fill (cross-hatch, grouping, flood fill) en Image (alle modi, negatief, dot width, screen, pass-through). Velden die het profiel niet bewijst (schuine hoek, ramp, scan angle) staan uitgegrijsd met reden: die blijven `deels`.
3. **Beeld en tracer.** Adjust Image met gamma/helderheid/contrast en crop-velden, het tracer-paneel met drempel, ignore-smaller-than en smoothness, en Verwerkte bitmap bewaren zijn getest. Enhance (S171–S173) bestaat niet in de code en blijft `–`. Multi-file trace, recepten-bibliotheek en crop-handles op het canvas ontbreken.
4. **Laservenster.** Start, pauze/hervat, stop, frame (rechthoek en rubber band), home, jog, bestandslijst, Save/Run RD en diagnose-export staan in het venster. Positie, verplaatsen en go-to-origin blijven geweigerd (`NotVerified`) tot `MoveAbsolute` op echte hardware bewezen is (W1-HW). Die knoppen staan daarom op `deels`, niet op ✔.
5. **Parametrische vormen en knopen.** Ster, driehoek, vijf-/achthoek en dubbele ster zijn in de catalogus met parameterpaneel; Convert to Path en de knopen-werkbalk staan erin. Trim, offset-randgevallen en booleans zijn niet opnieuw in de Mac-host bevestigd.
6. **Testnamen gecontroleerd, niet gedraaid.** Elke ✔-testnaam is als `fn` in de repo gevonden. De tests zijn in deze update niet uitgevoerd; groen-status per story staat in `docs/user-flows/story-coverage.md`.

### Stand na golf 6 (22-10-2026)

- Bedienbaar geworden sinds 147f7ff (met test): export- en importdialogen, bundels, materiaal- en art library (W3-UI-bestanden);
  dock, move together, warp/deform, resize slots, tangent circle, print and cut, center finder, nest (W3-UI-schikken);
  optimalisatie, material/interval/focus test, Device Settings met rotary (W3-UI-cam); teksttoolbar, variabele tekst,
  barcodedialoog (W3-UI-tekst); cameravenster (W3-UI-camera); voorkeuren, sneltoetsen, geschiedenis, preview, RD-inspectie,
  supportbundel (W3-UI-app); reeksen, toollagen, sublagen, preflight, feeder-planning (G5-cam); GRBL-venster en G-code-export
  (G5-grbl-ui); offset/reparatie, centerline-beslissingen, structuurpaneel, notities (G5-native); machinehub (G5-machine-rest);
  generatorpaneel en zoeken (G6-editor); beeld voorbereiden, restplaten, operatorweergave (G6-werk); profielbestand openen en
  vergrendelen (G6-machine).
- Nog niet gebouwd of niet gecontroleerd: Preferences import/export/backup (M027–M030), Replace to Fit (M066), de selectiefilters
  M058–M060/M062/M063, View Style (M194–M198), Zoom to Page en Frame Selection (M190, M193), de items Move to… en
  Move Laser to Selection… (M135–M153), Services/Hide/Show All (M002–M013, OS-diensten), talen anders dan NL/EN (M221–M247),
  externe links (M249–M254), Check for Updates (M257, besluit B-9), Offset-opties D02/D04/D05/D07, Offset Fill (D72–D77,
  uitgeschakeld in het onderzochte profiel).
- Windows: de matrix meet alleen het Mac-venster. De Windows-pariteit staat in `integratie-wachtrij.md` en is nooit gedraaid.

### Vervolg

- Dit bestand hoort bij W1-L7, W2-L7 en W3-L7. Bijwerken na elke golf (Haiku), of wanneer een pakket een menu-item of instelling raakt.
- Items zonder pakket: de plannende sessie beslist of ze een pakket krijgen, buiten scope vallen, of naar W1-HW gaan.
- Golf 3 (W3-UI-*): de `deels`-rijen van golf 2 (laservenster-positie, tracer multi-file, shape-properties, schuine hoeken) horen bij de UI-lanes van golf 3; herzien na die merges.
