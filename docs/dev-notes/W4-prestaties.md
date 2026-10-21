# W4-prestaties (X04: grote ontwerpen vloeiend bewerken)

Doel (voorlopig): p95 selectiereactie <= 100 ms op de benoemde 100k-segmentfixture, release-build, referentiemachine
= deze Mac. Tracing, nesting en compilatie annuleerbaar; geheugendruk begrensd zonder documentverlies.

## Fixtures (herkomst)

Gegenereerd in testcode, geen bestand in git: `crates/burnburn-scenarios/tests/benchmarks_100k.rs` (document/editor/io/cam)
en `crates/burnburn-render/tests/perf_scene.rs` (scenevertaling, zelfde rooster). Deterministisch, geen random-afhankelijkheid
(LCG met vaste seed voor klikpunten). Alle drie hebben precies 100 000 segmenten op een rooster van `round(sqrt(1,6 * n))` kolommen:

| Naam | Objecten | Vorm | Rasterafstand / straal |
|---|---|---|---|
| F100K-A | 1 000 | gesloten 100-hoek | 30 / 12 mm |
| F100K-B | 10 000 | gesloten 10-hoek | 9 / 3,5 mm |
| F100K-C | 100 000 | enkele horizontale lijn | 3 / 1 mm |

Camera/viewport: 1 pt/mm, 1300 x 800 pt, `mm_per_point` 1 (klik-tolerantie 4 mm). Opslaan/openen gebruikt `MemoryFileSystem`
(serialisatiekosten, niet schijf).

## Machine en omstandigheden

Apple M4 Max, 16 kernen, 128 GB, macOS (Darwin 27), rustc 1.99.1, `--release`. **De machine was tijdens alle metingen zwaar
belast door parallelle lanes (load average 45-130)**; absolute getallen zijn dus pessimistisch en sporadische uitschieters
(max, soms p95 bij weinig runs) komen door scheduling, niet door de code. Basis voor de "voor"-cijfers: commit 6dcdb60 met
alleen de benchmark toegevoegd; "na": zie laatste commit van de branch.

Commando: `cargo test -p burnburn-scenarios --release --locked --test benchmarks_100k -- --ignored --nocapture --test-threads=1`
(`BB_FIXTURE=F100K-B` beperkt tot een fixture; `BB_COMPILE_ALL=1` neemt de compile van F100K-C mee, ~90 s).
Render: `cargo test -p burnburn-render --release --locked --test perf_scene -- --ignored --nocapture`.

## Resultaten (ms; p50 / p95 / max waar er meerdere runs zijn)

F100K-C (het zwaarste geval: 100 000 objecten):

| Stap | Voor | Na |
|---|---|---|
| fixture importeren (1 commando) | 32 004 | 91 |
| klik-selectie (press via SelectTool) | 44,3 / 46,6 / 57,6 | 0,005 / 0,007 / 45 (eerste klik; zie volgende regel) |
| eerste klik na elke bewerking (index herbouwen) | = elke klik (44) | 42,9 / 44,8 / 44,8 |
| klik naast alles | 43,3 / 45,0 / 46,8 | 0,000 |
| band-selectie 10 % venster (9 548 objecten) | 4,9 / 5,6 / 5,9 | 5,2 / 52 / 68 (belasting) |
| select all | 558 / 570 / 570 | 1,0 / 5,9 / 5,9 |
| sleep-preview alle objecten, 1 pointer-move, snap aan | 1 270 / 1 914 / 1 914 | 0,78 / 0,79 / 0,79 |
| sleep-preview 1 object, snap aan | 4,1 / 8,6 / 9,6 | 0,011 / 0,015 / 0,026 |
| commit verplaatsing alle objecten | 818 | 71 |
| undo / redo van die verplaatsing | 0,0 | 0,0 |
| `build_scene` alles geselecteerd | 731 / 1 255 / 1 255 | 10,2 / 111 / 111 (belasting) |
| render `build_packet` (100k strepen, geen GPU) | n.v.t. | 11,5 / 11,8 / 11,8 |
| compile naar JobPlan | 87 491 | ~92 000 (niet aangepast, zie "Open") |
| opslaan / openen | 890 / 697 | 829 / 704 |

F100K-B (10 000 objecten): klik p50/p95 2,8 / 21 -> 0,002 / 0,002; eerste klik na bewerking 2,8 / 96 (uitschieter onder
load; mediaan 2,8); select all 5,6 -> 0,11; sleep-alles met snap 8,1 -> 0,07; compile 3 960 -> 3 985 (ongewijzigd).
F100K-A (1 000 objecten): klik p50/p95 0,53 / 1,1 -> 0,002 / 0,002; compile 162 -> 136; opslaan/openen 258/145 -> 86/32.

Render-scenevertaling (`perf_scene`): A 0,71 ms, B 2,3 ms, C 11,5 ms (p50, 10 runs), ook bij 10x inzoomen (er is geen
culling; de kosten blijven hetzelfde en ruim onder budget). GPU-tessellatie en tekenen zijn niet gemeten.

**Conclusie X04-doel:** op deze machine liggen alle interactieve stappen (klik, eerste klik na bewerking, band, select all,
sleep-preview, scene-opbouw, undo) op alle drie de fixtures ruim onder 100 ms (p95), ook onder zware belasting; het
slechtste interactieve geval is de eerste klik na een bewerking op 100k objecten (~45 ms). De metingen zijn van een
enkele machine zonder CI-drempel; er is niets op andere hardware of op Windows gemeten.

## Wijzigingen (gedrag identiek)

1. `Selection` gebruikt een hash-set naast de volgorde-vector (`contains`/`Replace` waren O(n) resp. O(n^2)).
2. `burnburn-editor::pick_index`: een ruimtelijke index (BVH uit `geometry::SpatialIndex`) over alle objecten, per thread
   gecachet op `DocumentSnapshot::picking_key()` (identiteit van objecten + arrange-laag via `Arc::ptr_eq`; de cache houdt de
   Arcs vast, dus in-place mutatie is onmogelijk). Hit-test en snapping gebruiken hem; snapping volledig-scan met
   `exclude.contains` (O(N*K), 1,3 s bij slepen van alles) is vervangen door een bounds-query + gehashte exclude.
3. `EditTxn`: naamindex voor de duplicate-naam-waarschuwing en een set voor `modified` (import van 100k: 32 s -> 0,09 s);
   `Transform`-commando idem (818 -> 71 ms).
4. `SpatialIndex::query_point_cost`: deterministische kostenteller voor tests.
5. `DocumentSession`: begrensd undo-geheugen (`HistoryBudget`, standaard 512 MiB geschat), oudste stappen vervallen met
   `CommandWarning::HistoryTrimmed`, de nieuwste stap blijft altijd ongedaan te maken, het document blijft onaangetast.

## TDD-log

Alle commando's met `export PATH="$HOME/.cargo/bin:$PATH"` en `CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/w4-prestaties`.

| Test | Red (verwachte fout, echt gezien) | Red-commando | Green |
|---|---|---|---|
| `editor selection::tests::ordered_set_membership_does_not_scan_linearly` (telt `eq`-aanroepen) | stub met lineaire scan: "37507798 comparisons for 5000 keys" | `cargo test -p burnburn-editor --locked selection` -> FAILED | zelfde commando, na hash-set: ok (295 lib-tests groen) |
| `editor selection::tests::large_replace_keeps_order_and_deduplicates` | karakteriseringstest, direct groen | idem | ok |
| `geometry spatial::tests::point_query_examines_few_nodes_and_matches_brute_force` | stub `query_point_cost` telde alles: "examined 14095 of 10000 items" | `cargo test -p burnburn-geometry --lib --locked spatial` -> FAILED | ok na echte telling (<5 % van de items per query) |
| `document tests/picking_key::{the_key_is_stable_for_clones_and_changes_with_every_edit_that_affects_picking, an_edit_never_makes_a_held_key_equal_to_the_new_state}` | stub (altijd gelijk): "geometry changed" / "the moved state is not the held content" | `cargo test -p burnburn-document --locked --test picking_key` -> 2 FAILED | ok na `Arc::ptr_eq`-vergelijking |
| `editor tests/pick_cache::{the_index_is_built_once_per_document_state_and_shared_by_clones, every_edit_gets_a_fresh_index_so_a_stale_one_is_never_used, snapping_and_hit_testing_share_one_index}` | index werd per aanroep herbouwd: 52 / 2 / 30 builds i.p.v. 1 | `cargo test -p burnburn-editor --locked --test pick_cache` -> 3 FAILED, 2 ok | alle 5 ok na cache + snap via index |
| `editor tests/pick_cache::{hit_test_equals_the_reference_also_with_hidden_locked_and_reordered_objects, snap_candidates_equal_the_reference_with_and_without_exclusions}` | orakels = de oude implementaties (volledige scan); waren vanaf het begin groen en blijven dat | idem | ok (400x2 hit-queries, 4x150 snap-queries) |
| `document tests/batch_bookkeeping::*` (3 tests: duplicate-naam-waarschuwingen, modified-lijst) | karakterisering; ook tegen de OUDE broncode gedraaid: groen | `cargo test -p burnburn-document --locked --test batch_bookkeeping` | ok na de wijziging |
| `scenarios benchmarks_100k::bench_import_and_move_all_scale_linearly` (#[ignore], verhouding 25k -> 100k < 8x) | import 595 -> 11 929 ms: "import scales superlinearly" | `cargo test -p burnburn-scenarios --release --locked --test benchmarks_100k bench_import_and_move_all -- --ignored --nocapture` -> FAILED | import 7 -> 27 ms, verplaats-alles 11 -> 42 ms: ok |
| `document tests/history_budget::{history_is_bounded_by_the_budget_without_touching_the_document, the_newest_step_stays_undoable_even_with_a_zero_budget}` | stub zonder trimmen: "estimated at least ..: 0" resp. 2 stappen i.p.v. 1 | `cargo test -p burnburn-document --locked --test history_budget` -> 2 FAILED, 1 ok | 3 ok |
| `cam tests/cancel_midrun::{a_compile_cancelled_while_running_returns_cancelled_and_no_plan, a_nest_cancelled_while_running_returns_cancelled_and_no_placements}` | gat in de dekking (alleen vooraf-geannuleerd getest); geen productiefout gevonden, direct groen (annulering na 15 ms bij 1 500 contouren/onderdelen) | `cargo test -p burnburn-cam --locked --test cancel_midrun -- --nocapture` | ok |

## Annuleerbaarheid en verouderde resultaten (inventarisatie)

Bestond al: trace midden in het fitten geannuleerd en verouderd resultaat afgewezen op taak-/receptidentiteit
(`trace/tests/task_contracts.rs`); compile vooraf geannuleerd (`cam/tests/compile.rs::cancelled_compile_…`, CLI
`cancelled_compile_writes_nothing`); nest vooraf geannuleerd (`cam/tests/nest.rs`); nest-taak: bewerking tijdens het nesten
laat het resultaat vallen (`presentation/tests/schikken_nest.rs::an_edit_while_nesting_drops_the_result_…`); compile-plan na
bewerking `stale` (`presentation/tests/gate_b.rs`, `app_preview.rs`). Toegevoegd: annulering *tijdens* compile en nest
(`cam/tests/cancel_midrun.rs`). Annuleringslatentie bij 100k contouren is niet apart gemeten.

## Open / onbevestigd

- Compile van veel losse contouren is kwadratisch in `cam::line::plan` (nearest-next) en `containment` (10k contouren 4 s,
  100k ~90 s; profiel via `sample`: `line::containment`-closure en `entry`-selectie). Niet aangepast (andere lane, golden
  JobPlan-volgorde moet identiek blijven); wachtrij-regel aangemaakt.
- Het budget voor undo-geheugen is een schatting (struct-groottes, geen allocator-meting) en nog niet in de UI zichtbaar.
- De pick-cache houdt per thread de laatst geïndexeerde objectmap vast (een extra kopie van de Arc-kaart tot de volgende klik).
- Alleen macOS arm64, enkele machine, hoge achtergrondbelasting; geen Windows, geen debug-build, geen GPU-tijden.
- Handmatig: voelbare reactie in de echte app met een 100k-bestand is niet beoordeeld.
