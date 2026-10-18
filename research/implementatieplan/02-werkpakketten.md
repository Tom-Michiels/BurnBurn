# Werkpakketten voor de Rust implementatie

De implementatie gebeurt in kleine verticale resultaten. Ieder pakket begint met de genoemde falende gedragstest en volgt het [test-driven design](01-test-driven-design.md). Testnamen zijn geplande contracten, geen al aanwezige of geslaagde tests. De [74 stories](../applicatieplan/03-user-stories.md) blijven de productspecificatie.

De package-ID's IM01–IM32 geven werkvolgorde en afhankelijkheden. Een pakket kan meer dan één kleine red/green/refactor-cyclus bevatten. Een story met brede scope is pas gereed wanneer al haar acceptatiecriteria in de bedoelde release zijn gehaald.

## Eerste fundament en native bewijs

| Pakket | Afhankelijkheden | Eerste falende test | Op te leveren |
|---|---|---|---|
| IM01 Rust-workspace en testharness | Geen | `core_contracts_run_without_native_gui` | Exacte toolchain, Cargo.lock, kerncrate met echte eerste unitcontracttest, Rust xtask, CI-selectie voor platformen en TDD-logformaat |
| IM02 Eenheden en geometrie | IM01 | `inch_rectangle_has_expected_millimeter_bounds` | IDs, eenheden, finite values, `f64` primitives, transforms, bounds en afzonderlijke toleranties |
| IM03 Documentcommands en undo | IM02 | `resize_then_undo_restores_geometry_and_identity` | Transacties, revisions, stabiele IDs, bronnenreferenties, undo/redo en mislukte-commandatomiciteit |
| IM04 Projectopslag en recovery | IM03 | `failed_replace_keeps_last_readable_project` | Container v1, assethashes, atomair bewaartraject, tijdelijke mappen, recovery en eerste migratiefixture |
| IM05 Gedeelde presentatie en input | IM03 | `drag_cancel_leaves_document_and_history_unchanged` | AppController, commands, viewmodels, genormaliseerde pointerinput en HeadlessEditorHarness |
| IM06 AppKit-host in Rust | IM04, IM05 | `native_dimension_commit_dispatches_one_document_command` | Native venster, menu, splitview, maatveld, open/save/cancel, documentfocus en lifecycle-adapters |
| IM07 Gedeeld GPU-canvas | IM02, IM05, IM06 | `scene_projects_selected_rectangle_at_retina_scale` | SceneSnapshot, wgpu-surface op Mac, paden, grid, selecties, zoom/pan en offscreen-test |
| IM08 Vroege native Windows-proef | IM04, IM05, IM07 | `windows_host_replays_dimension_scenario_with_same_document_result` | Eén native venster, veld, lijst, file picker en gedeeld canvas; versiegebonden WinUI/Rust-proef en eventueel gedocumenteerde Win32-route |

**Integratiepoort A:** teken 120 × 80 mm, verander naar 150 × 80 mm, undo, save en heropen op Mac. De headless route bereikt hetzelfde resultaat. De Windows-host hoeft nog geen volledige editor te zijn, maar toont dat kern, commands en canvas geen Mac-widgettypen bevatten. Werkende Metal- en Windows-surface-integratie zijn apart bewezen.

IM01 richt geen netwerkhardware in en installeert niet ongemerkt globale toolchains. De huidige shell heeft bij de plancontrole geen `cargo`/`rustc` op PATH; het eerste uitvoeringspakket moet daarom eerst de Rust-toolchain beschikbaar maken en vastleggen voordat red/green-resultaten kunnen worden gerapporteerd.

## Betere editor en eerste offline uitvoer

| Pakket | Afhankelijkheden | Eerste falende test | Op te leveren |
|---|---|---|---|
| IM09 Direct tekenen en repareren | IM05, IM07 | `editing_one_bezier_span_preserves_other_spans` | Tools, nodes, selectiecyclen, snapping, align, booleans, trim/extend en geometrische foutlocaties; bibliotheekadapters op randgevallen toetsen |
| IM10 Import en geometrie-export | IM02, IM04, IM09 | `unitless_dxf_needs_explicit_scale_before_import_commit` | SVG/DXF/PNG/JPEG, importverslag, fysieke schaal, beperkte SVG/DXF-export en parserlimieten |
| IM11 Parametrische sketches | IM03, IM09 | `changing_plate_width_preserves_symmetric_hole_offsets` | Rust-solveradapter, parameterparser met eenheden, constraints, driving/driven, diagnose en last-valid-result |
| IM12 Bronbeelden en contourtracing | IM09, IM10 | `traced_ring_preserves_hole_at_declared_physical_scale` | Originele pixels, uitsnede/schaal, maskers, contourboom, curvefitting, receptopslag en vergelijkingspreview |
| IM13 Operations en compiler | IM03, IM09, IM11 | `two_operations_share_one_geometry_without_duplicate_source` | Ontwerplaag/proceslaag apart, OperationRefs, line/fillplanning, volgordeconstraints, materiaalrecepten en eerste immutable JobPlan |
| IM14 Zuivere Rust Ruida-codec | IM13 | `reference_line_fixture_decodes_expected_bounds_and_power` | Decoder vóór encoder, known vectors, scrambling, veldgrenzen, checksums, fixturemanifest en onafhankelijke decodevergelijking |
| IM15 Preflight en planpreview | IM07, IM13, IM14 | `outside_bed_operation_links_error_to_source_geometry` | Dezelfde planrevisie in preview/export, tracebare moves, kerf, nominale maat, capabilitydata, origins en betrouwbare boundscontrole |

**Integratiepoort B:** de montageplaat blijft correct bij maatwijziging; een vrij logo krijgt nette curves; een ringtrace bewaart het gat. Een JobPlan voor dezelfde documentrevisie levert inspecteerbare line/fill-RD. Offline export is nog geen geslaagde fysieke Ruida-job.

De solverproef in IM11 begint met constraints die nodig zijn voor de montageplaat. Een Rust-dependency krijgt niet automatisch alle geclaimde mogelijkheden. Onvoldoende dekking leidt tot een afgebakend Rust-solverwerkpakket; de taalkeuze wordt niet via een C++-wrapper omzeild.

## Eerste dagelijkse productie op macOS

| Pakket | Afhankelijkheden | Eerste falende test | Op te leveren |
|---|---|---|---|
| IM16 Transport en machineactor | IM14, IM15 | `lost_start_reply_does_not_cause_second_start_after_reconnect` | Fake clock/controller, Ethernetadapter, upload/stored-bewijs, machine-identiteit, begrensde retries, laser-uit-framing en state machine |
| IM17 Bevroren jobs en historie | IM04, IM13, IM16 | `editing_project_does_not_modify_prepared_job_revision` | CompiledJob/Execution apart, snapshots, queue, aantallen, Unknown-reconciliatie en geschiedenisdatabase |
| IM18 Tekst en rasterproductie | IM10, IM13, IM15 | `dither_runs_are_regenerated_from_source_after_physical_resize` | Rust-tekstshaping, glyphcontouren, fontfallback, eenvoudige teksteditor, tooncorrectie, dither en binaire image-uitvoer |
| IM19 Lokale tracecorrectie | IM12, IM15 | `retrace_region_preserves_protected_hole_and_untouched_segments` | Region masks, vaste ankers, afwijkingskaart, protected details, knooptelling en correcties met bronidentiteit |
| IM20 Native afwerking en release | IM16, IM17, IM18, IM19 | `closing_dirty_native_document_then_cancel_keeps_session_open` | Offline-checks, focus/IME, toegankelijkheid, NL/EN, DPI, recovery, `.app`-bundle en installatie-/updatecontracten |

**Integratiepoort C:** de gekozen Ruida-configuratie doorstaat offline protocol- en fouttests en vervolgens hardwarevalidatie. Gewone productie werkt via expliciete preview/upload/start. Upload telt geen gemaakt werkstuk; een verloren startantwoord blijft Unknown. De native Mac-app heeft geen webview als hoofdinterface.

IM16 wordt opgesplitst in codecscripts, fake transport en pas daarna live IO. USB volgt later met een eigen matrix. IM18 belooft geen grayscale-modulatie vóór de afzonderlijke semantiekproef. IM20 bevat vanaf de eerste publieke release native smokechecks; packaging wordt niet tot het einde van de volledige roadmap uitgesteld.

## Onderscheidende ontwerpvoorbereiding

| Pakket | Afhankelijkheden | Eerste falende test | Op te leveren |
|---|---|---|---|
| IM21 Modifiers, componenten en platen | IM11, IM13, IM17 | `broken_reference_after_boolean_is_reported_without_rebinding_neighbour` | Niet-destructieve ketens, associatieve referenties, tangentie/symmetrie, onderdeeldefinities, varianten en meerdere platen |
| IM22 Lasergenerators en personalisatie | IM18, IM21 | `material_thickness_changes_slot_but_does_not_apply_kerf_twice` | Finger joints, tab/slot, doos/scharnier, stencilbruggen, padtekst, single-line tekst en bevroren datarijen/barcodes |
| IM23 Centerline en technische reconstructie | IM11, IM19 | `t_junction_retains_three_branches_and_marks_ambiguous_connections` | Rust-centerlinegraaf, spurcontrole, lokale split/join, lijn-/boog-/cirkelkandidaten met residu en bevestigde vervanging |
| IM24 Geavanceerde tracerecepten | IM19, IM23 | `old_region_patch_is_not_applied_to_new_unrelated_component` | Lokale belichting, perspectief, A/B, batchrecepten en volwassen corpus/holdoutbenchmark |
| IM25 Uitgebreide laserprocessen | IM15, IM16, IM18 | `unsupported_modulation_mode_is_rejected_instead_of_replaced` | Grayscale na bewijs, Offset Fill, scanhoeken, tabs/leads, passes, perforatie/overcut en subbewerkingen volgens featurematrix |
| IM26 Arrays en mixed-part nesting | IM21, IM25 | `nesting_keeps_order_counts_and_respects_grain_rotation` | Onderdeel/stockmodel, toegestane rotaties, plaatgaten, arrays, orderaantallen en benuttingsnoemer |
| IM27 Camera en registratie | IM15, IM20 | `calibration_is_invalid_after_camera_resolution_changes` | Cameraports per OS, overlay, markregistratie, onafhankelijke controlepunten en residuen |
| IM28 Migratie en werkplaatspakketten | IM17, IM18, IM21 | `import_reports_unknown_lbrn2_options_without_silent_drop` | PDF, expliciete LBRN2-subset, assets/fontsituatie, profielsnapshots, projectzoekfunctie en herhaalrevisies |
| IM29 Machinebeheer en calibratie | IM16, IM17, IM25 | `scan_calibration_update_invalidates_dependent_prepared_plan` | Controllerfiles, instellingenbackup/diff, pointer-/scan-/kerfproeven, USB en tests voor geselecteerde speciale capabilities |
| IM30 CLI, prestaties en uitbreidingsgrens | IM17, IM21, IM24 | `cli_and_gui_compile_same_revision_to_same_normalized_plan` | CLI, taskcancellation, 100k-segmentbench, plugincontractversies en gescheiden compile/upload/start-intenties |

**Integratiepoort D:** montageplaat, doos, klantlogo en handschrift halen de productdoelen uit het applicatieplan op een benoemd corpus. Platformhergebruik blijft via commands, domein en renderer lopen. Mac-schermen worden niet gekopieerd naar generieke shared widgettypes om werk op Windows te ontwijken.

## Uitgebreide werkplaats en Windows-product

| Pakket | Afhankelijkheden | Eerste falende test | Op te leveren |
|---|---|---|---|
| IM31 Orders, herkenning en operator | IM17, IM26, IM27, IM29, IM30 | `reconnected_operator_view_does_not_repeat_last_machine_action` | Restmateriaal, orders, matchambiguïteit, compacte operator, rechten, één machineservice en gevalideerde extra backends/assen |
| IM32 Kleurtracing en gecontroleerde automatisering | IM21, IM23, IM24, IM26 | `shared_colour_boundary_is_not_scheduled_twice_without_explicit_policy` | Kleurregio's, gedeelde grensstructuur, beoordeelbare AutoConstrain en afzonderlijk gevalideerde common-line cutting |

De **volledige Windows-GUI** wordt na de stabiele Mac-productieroute scherm voor scherm opgebouwd bovenop IM08. Volgorde: documenten/menu's → objectboom/inspecteur → sketch/tracer → voorbereiding → productie → toegankelijkheid/packaging. Voor ieder scherm draait dezelfde headless gedragssuite en daarnaast een native Windows-test. Windows-specifieke filelocking, DPI, tekstinvoer, device discovery en installatieroute hebben eigen criteria.

Windows-productieondersteuning volgt pas na een eigen driver-/hardwarematrix op Windows. Een succesvolle Mac-job of een Windows-kernbuild bewijst dat niet. De Windows-proef wordt tot die release werkend gehouden in CI zodat de portgrens niet langzaam verdwijnt.

## Dekking van alle user stories

De mapping benoemt het primaire pakket; gedeelde contracten en eerdere pakketten leveren benodigde basis. Deelreleases volgen de productspecificatie, niet alleen deze eigenaarsmapping.

| Pakket | Primaire stories |
|---|---|
| IM01 | Basis voor alle stories; geen afzonderlijke productstory |
| IM02 | Basis voor geometrie en eenheden; geen afzonderlijke productstory |
| IM03 | P02 |
| IM04 | P01 |
| IM05 | X02 |
| IM06 | Native fundament voor X02 en X03 |
| IM07 | Canvasfundament voor E01 en E03 |
| IM08 | Windows-bewijs voor X07 |
| IM09 | E01, E02, E03, E04, E05, E09 |
| IM10 | P03, P05 |
| IM11 | S01, S02, S03, S04 |
| IM12 | T01, T02 |
| IM13 | C01, C02, C06, C08 |
| IM14 | M03 |
| IM15 | C03, C07, C09, M01, M02 |
| IM16 | M04, M05, M06, M07 |
| IM17 | W01, W02, W03 |
| IM18 | E07, T09, C05 |
| IM19 | T05, T06 |
| IM20 | X01, X03, X07 |
| IM21 | E06, S05, S06, S07, P06 |
| IM22 | E08, E10, E11 |
| IM23 | T04, T08 |
| IM24 | T03, T10 |
| IM25 | C04, C11 |
| IM26 | C10 |
| IM27 | W04, W05 |
| IM28 | P04, P07, P08 |
| IM29 | M09, W09 |
| IM30 | X04, X05, X06 |
| IM31 | M08, W06, W07, W08 |
| IM32 | S08, T07, C12 |

## Eerste acht kleine red/green-cycli

Dit is de concrete start na het beschikbaar maken van de Rust-toolchain. Elke stap voegt één bewezen gedrag toe; er wordt geen halve applicatie achter lege traits opgebouwd.

1. `rejects_non_finite_length`: lengtetype accepteert geen NaN/Infinity. Minimale typed inputgrens.
2. `inch_rectangle_has_expected_millimeter_bounds`: een analytisch bekende maat wordt correct geconverteerd. Minimale geometrie en units.
3. `create_rectangle_assigns_stable_geometry_id`: een command maakt één benoemd object. Minimale documentsessie.
4. `resize_then_undo_restores_geometry_and_identity`: commit/undo bewaart bron en ID. Minimale transactielog.
5. `cancelled_drag_creates_no_history_entry`: preview is tijdelijk. Eerste editor-state machine.
6. `project_roundtrip_preserves_geometry_and_operations`: klein project bewaart betekenis. Eerste echte container.
7. `stale_task_result_does_not_change_newer_revision`: backgroundcontract voorkomt race. Eerste taakpoort.
8. `native_dimension_commit_dispatches_one_document_command`: native Mac-veld koppelt aan dezelfde commandroute. Eerste AppKit-integratie.

Deze volgorde bewijst meteen de vier moeilijkste grenzen: numerieke correctheid, herleidbare documentmutatie, annuleerbare achtergrondtaken en een dunne native GUI.

## Uitvoeringsstatus

Alle pakketten staan bij het schrijven van dit plan op **gepland**. Er zijn nog geen Cargo-workspace, native vensters of Rust-tests opgeleverd. Het plan en de storymapping worden lokaal gecontroleerd; dat is geen applicatietest. Het eerste implementatieresultaat moet de daadwerkelijk waargenomen red-/green-resultaten rapporteren.
