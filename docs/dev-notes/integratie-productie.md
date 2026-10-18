# Integratie-productie (User-flow, 18-10-2026)

Losse eindjes na IM01–IM32: historie aan presentation, poort-D-scenario's, mutatieproeven uit de Testing-review,
GUI-verzoeken aan presentation, storydekking. macOS arm64; Windows en native hosts niet uitgevoerd.
Commando's steeds met `export PATH="$HOME/.cargo/bin:$PATH"; CARGO_TARGET_DIR=target/integration-b`.
Mutaties zijn met een script toegepast, gedraaid en teruggezet (inhoudscontrole: niets van anderen overschreven).

## 1. `burnburn-history` achter de `HistoryStore`-poort

Ontwerp: de poort blijft in presentation met eigen leesmodellen (`UploadRecord`, `OutcomeRecord`, `Recorded`,
`RecoveredMachine/Execution`, `StoredOrder(s)`); de adapter `burnburn_runtime::JournalHistoryStore` (laag 6, mag
presentation én history noemen; presentation noemt geen historytypes, layers.toml). `MachineService::new` leest bij
start de historie: de laatste onafgeronde execution wordt `Unknown(InterruptedByRestart)` in de actor
(`ExecutionActor::resume_unknown`), alleen statusquery of een operatorverklaring volgt. `ExecutionId` komt uit de
historie. Actie-ids → stabiele hash (soort, onderwerp, generatie); generaties worden na herstart uit de historie
geteld, dus een oude knop is nooit opnieuw "vers". Uitkomst registreren = `MachineAction::RecordOutcome(Outcome)`,
alleen bij `Unknown` én laatste statusantwoord ≠ Running; duurzaam via `reconcile_by_operator`, daarna
`ExecutionActor::settle_unknown`. Orders: nieuwe journalrecords `order-recorded`/`order-linked` (history) en
`AppController::attach_history`.

| Test | Verwachte fout (red) | Red: commando → waargenomen | Green | Refactor |
|---|---|---|---|---|
| `gate_c::restart_marks_unfinished_execution_unknown_without_resending_start` (`#[ignore]` weg) | nieuwe service kent geen execution | `cargo test -p burnburn-presentation --test gate_c restart` → FAILED `left: Idle right: Unknown` (gate_c.rs:273) | herstel in `MachineService::new` → 11 passed | — |
| machine `resumed_unknown_execution_allows_only_a_status_query`, `resume_refuses_known_states_and_a_busy_actor`, `settling_an_unknown_execution_is_explicit_and_terminal` | stubs `resume_unknown`/`settle_unknown` = `Ok(())` | `cargo test -p burnburn-machine --test execution` → 3 FAILED: `unwrap()` op `None` (execution.rs:120), `left: Ok(()) right: Err(InvalidState(Stored))`, `left: Ok(()) right: Err(InvalidState(Running))` | implementatie → 17 passed | — |
| history `orders_and_their_executions_survive_a_restart` | stubs schrijven niets | `cargo test -p burnburn-history --test executions orders` → FAILED `left: Ok(RecordSeq(0)) right: Err(UnknownOrder(9))` | append + index → alle history-tests groen | — |
| `gate_c::restarted_unknown_execution_is_reconciled_by_the_operator_after_a_status_query` | geen uitkomstknop | `--test gate_c` → FAILED gate_c.rs:326 "after an idle status the operator may state the outcome" | knoppen `RecordOutcome` in `machine_view_of` → groen | — |
| `im31_operator::orders_and_their_counts_survive_a_restart` | `attach_history` stub | `--test im31_operator` → FAILED `left: 0 right: 1` "the order is in the history" | `store_order` + laden → groen | `Order.chosen` als planhash |
| runtime `history_store.rs` (4): `restart_marks_unfinished…`, `status_query_after_restart_is_journaled_and_the_operator_statement_settles_it`, `an_action_from_before_the_restart_is_never_fresh_again`, `orders_and_their_executions_are_durable` | adapter-stub `Err("not implemented")` | `cargo test -p burnburn-runtime --test history_store` → 4 FAILED, allen `NotRecorded("not implemented")` bij upload (history_store.rs:186, gezamenlijke oorzaak) | adapter → 3 passed, 1 FAILED **testfout** (`NotReconcilable`: in dezelfde sessie is StartRequested geen Unknown) → test laat de job via Running/Finished voltooien → 4 passed | — |

Mutaties: (a) `reconcilable` negeert Running → `restarted_unknown…` FAILED gate_c.rs:319; (b) geen `resume_unknown`
bij start → 2 gate_c-tests FAILED `left: Idle`; (c) adapter schrijft evidence vanaf de historielengte i.p.v. de
sessiepositie → `status_query_after_restart…` FAILED "evidence after the restart is journaled" (echte bug in
`History::observe` voor geadopteerde executions, daarom eigen boekhouding in de adapter); (d) geen generatietelling
uit de historie → eerst **groen** (de geweigerde herhaling verhoogde de teller zelf); test aangescherpt (volgende id
vóór de herhaling bekijken) → FAILED `assert_ne` "the next upload button has a new id".

## 2. Poort D (`crates/burnburn-scenarios/tests/gate_d.rs`)

Elk scenario decodeert de RD met `burnburn-ruida` en controleert per laag (part) snelheid, vermogen, werkmodus en de
snijgeometrie tegen handafgeleide waarden. **Geen rode fase**: het gedrag bestond al; eerste run 1 passed, 3 FAILED
door testfouten (vertex-gemiddelde i.p.v. bbox-midden; doos op x = 0 → kerf buiten het bed = `OutsideBed`; sortering
van afgeronde maten; SVG-boog met eindpunt naast de cirkel vergrootte de straal). Na correctie 4 passed. Daarom per
scenario een mutatieproef:

| Scenario | Mutatie | Resultaat |
|---|---|---|
| `headless_gate_d_mounting_plate_variants_nested_on_two_plates_give_one_rd_per_plate` | nesting zonder `spacing`-groei (cam/nest.rs) | FAILED `left: 16 right: 15` (andere verdeling over de platen) |
| `headless_gate_d_box_thickness_change_no_double_kerf` | kerf-offset `width` i.p.v. `width / 2` (cam/compile.rs) | FAILED "kerf once: (60.4, 40.4) vs (60.2, 40.2)" |
| `headless_gate_d_customer_logo_repaired_union_with_fill_and_line` | fill-werkmodus 0 i.p.v. 2 (ruida/lower.rs) | FAILED `(300000, 4915, 0)` vs `(…, 2)` |
| `headless_gate_d_handwriting_centerline_open_paths` | elke contour gesloten (cam/line.rs) | FAILED "stroke-t: centerlines stay open" |

API-gaten (glue in de test, verzoek in `docs/coordination/userflow.md`): geen command dat nesting toepast vanuit
`nest()` (alleen `[document] applying_a_nest…` met eigen invoer), geen compile-optie per plaat (`plate_view`),
`Dim` noemt één parameter (gatposities via extra variantparameters), geen `trace_command` voor centerline.

## 3. Testing-review: mutatieproeven (IM09-interactie, IM05)

| Test | Mutatie | Resultaat |
|---|---|---|
| `scene_follows_document_selection_and_gesture_preview`, `tree_and_canvas_share_selection` | sceneroll altijd `Design` (editor/scene.rs) | beide FAILED (`[(1, Design), (2, Design)]`; "tree → canvas" `left: []`) |
| `move_drag_is_one_history_entry_and_escape_restores` | Esc laat de drag bestaan (select.rs `take` → `clone`) | eerst **groen**; test aangescherpt (`preview == None` na Esc) → FAILED |
| `scale_and_rotate_handles_commit_one_entry_each` | schalen om het midden i.p.v. de overliggende hoek | FAILED `(-40, -10, 120, 40)` |
| `grid_snapping_is_a_window_toggle` | rastersnap uit (snap.rs) | FAILED `(0.4, 0.3, 12.2, 7.4)` (eerdere mutatie op de toggle faalde alleen op `checked`) |
| `numeric_position_in_the_inspector_is_one_command` | gelijke waarde stuurt toch command | FAILED "unchanged value: no command" |
| `typed_size_while_drawing_creates_an_exact_rectangle` | ongeldige breedte valt terug op hoogte | FAILED "error, still drawing" |
| `align_left_moves_to_the_reference_in_one_entry_and_keeps_sizes` | referentie = laatst geselecteerde | FAILED `Some(2)` vs `Some(1)` |
| `arrange_and_boolean_commands_follow_the_selection` | verdelen bij 2 objecten | FAILED "distribute needs three" |
| `node_drag_previews_only_the_dragged_span_and_escape_restores` | Esc beëindigt de knoopdrag niet | FAILED (preview blijft) |
| `invalid_dimension_text_shows_field_error_and_sends_no_command` + `field::tests::invalid_text_is_rejected_with_a_reason` | `parse_length` zonder positiviteitscontrole | beide FAILED (`"0"` geen fout; `Ok(0.0)`) |
| `field::tests::units_and_decimal_separators_give_the_same_physical_value` | cm-factor 1 | FAILED `Ok(15.0)` |
| `field::tests::positions_accept_zero_and_negative_values_with_units` | positie via `parse_length` | FAILED `Err(NotPositive)` |
| `field::tests::formats_millimetres_without_noise` | `-0` niet genormaliseerd | FAILED `"-0"` |
| `headless_gate_a_rectangle_resize_undo_redo` | Redo voert Undo uit | FAILED (inspector weg na tweede undo) |
| `two_windows_do_not_share_selection` | pointer naar het actieve venster | FAILED, maar **in de opbouw** (scenarios.rs:113), niet op de kernassertie: onderscheidend vermogen voor selectielek niet aangetoond |
| `cancelled_save_panel_and_close_sheet_change_nothing` | Annuleer op sluitblad = Verwerp | FAILED (venster weg) |
| `view::tests::rejects_non_finite_or_non_positive_view_values` | niet-eindig → `NotPositive` | FAILED `Err(NotPositive)` vs `Err(NotFinite)` (eerdere mutatie "∞ toegestaan" ving alleen een andere test) |
| editor `primary_modifier_suppresses_snapping` / `excluded_objects_offer_no_targets` / node `release_without_movement_commits_nothing` (slaagden tegen stub) | onderdrukking weg / uitsluiting weg / geen sleepdrempel | alle drie FAILED |

## 4. GUI-verzoeken aan presentation (`tests/gui_requests.rs`, `im09_interaction.rs`)

Stubs (API zonder gedrag) → `cargo test -p burnburn-presentation --test gui_requests` → 5 FAILED: "camera panel"
(`None`), geen `NoFontAvailable`, `operation_error` `None` i.p.v. `InvalidNumber(Speed)`, recovery `left: 0 right: 1`
(2×). Rotatie: `rotation_field_turns_the_selection_about_its_centre_in_one_command` → FAILED "rotation field".
Green: alle 5 + rotatie; tussendoor 1 FAILED "removed after a successful save" (harness draaide vervolgtaken niet) →
`run_tasks` loopt nu tot de wachtrij leeg is (begrensd 16 rondes); alle presentation-tests bleven groen.
Mutaties: tekst vervangt i.p.v. aanvullen; gelijk vermogen stuurt command; camera start zonder toestemming;
autosave zonder "sinds laatste autosave"; hersteld document schoon; rotatie om de oorsprong → elk FAILED op de
bedoelde assertie.

## Verificatie (eindstand)
- `cargo fmt -p burnburn-{presentation,scenarios,history,runtime,machine,editor} --check` → ok.
- `cargo clippy -p burnburn-{presentation,scenarios,history,runtime,machine,editor} --all-targets -- -D warnings` → ok (geen `-A`).
- `cargo test --workspace --exclude burnburn-macos --exclude burnburn-windows --no-fail-fast` → 796 ok, 0 FAILED.
- `cargo check -p burnburn-macos -p burnburn-windows -p burnburn-cli --all-targets` → ok (hosts ongewijzigd).
- `cargo xtask layers` → 24 crates, 2 triples, no violations.

## Onbevestigd / open
- Geen echte laser, geen echte schijf-crash: alleen `MemoryFileSystem` en de fake controller.
- Goedkeuring van een job (`approved`) en de jobnaam na herstart zijn niet duurzaam (naam = bestandsnaam uit de historie).
- Meer dan één onafgeronde execution per machine na herstart: alleen de laatste is via de UI te verklaren.
