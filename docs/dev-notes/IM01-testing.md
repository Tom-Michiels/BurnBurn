# IM01 (Testing) testinfrastructuur, fixtures, laagcheck en CI

Status: gereed op macOS arm64 (lokaal). CI-workflow is **niet uitgevoerd** (geen runner vanuit deze omgeving);
Windows en Linux: onbevestigd.

Commando's met `export PATH="$HOME/.cargo/bin:$PATH"` en `CARGO_TARGET_DIR=target/testing`.

## Opgeleverd
- `crates/burnburn-test-support`: `FakeClock` (implementeert `burnburn_types::Clock`, handmatig `advance`, clones en
  `Arc<dyn Clock>` delen één tijd, verzadigt bij overflow), `ScriptedPeer<Req, Resp>` + `Step` (verwachtingen in
  volgorde, `reply`, `reply_after` tot fake-tijd, `reply_duplicated`, `lose_reply`, `then_disconnect`, `disconnect`/
  `reconnect`, `sent`/`sent_log` met tijd en uitkomst, `lost`, `verify_complete`), `fixtures` (manifestgecontroleerd
  lezen). Geen dependency op `burnburn-machine`. Crate-doc: alleen vanuit `tests/` gebruiken.
- Workspace-dependencies: `proptest`, `serde_json`, `sha2`, `toml`, `burnburn-test-support`.
- `test-data/ruida/`: 3 RD-fixtures, 4 bronbestanden (`sources/`), 3 oracle-decodes (`oracle/`), `manifest.json`, `README.md`.
- `xtask/src/layers.rs` + `cargo xtask layers` (regels R1–R8 uit `docs/design/layers.toml` van Design, R6 per triple
  via `cargo metadata --filter-platform`); stap in `cargo xtask verify` direct na `fmt --check`.
  Structuur overgenomen van `../solidcraft/xtask/src/layers.rs` (MIT OR Apache-2.0), vermeld in de module-doc.
- `.github/workflows/ci.yml`: kernsuite op macOS/Windows/Linux, aparte `macos-native`-job (placeholder).
- `crates/burnburn-geometry/tests/properties.rs` (7 propertytests) + `proptest` dev-dependency in geometry.

## Cycli

| Test | Verwachte fout (red) | Red-commando en waargenomen resultaat | Green-commando en resultaat | Refactor |
|---|---|---|---|---|
| `fake_clock::fake_clock_stands_still_until_advanced`, `fake_clock_clones_and_arc_dyn_clock_share_time`, `fake_clock_saturates_instead_of_wrapping` | stub: `now()` altijd `ZERO`, `advance` doet niets | `cargo test -p burnburn-test-support` → 3 FAILED, o.a. `left: MonotonicTime(0ns) right: MonotonicTime(250ms)`; saturatietest faalt op `max >= 10s` | gedeelde `Arc<Mutex<MonotonicTime>>`, `checked_add` met verzadiging → 3 passed | geen |
| `scripted_peer::*` (11 tests: volgorde, afwijzing, na einde script, vertraging tot fake-tijd, volgorde op due-tijd, duplicaat, verloren antwoord, disconnect na request + reconnect, handmatige disconnect, tijdstempels, thread) | stub: `script`/`send` negeren alles, `recv` geeft `None` | `cargo test -p burnburn-test-support --no-fail-fast --test scripted_peer` → 11 FAILED (`test result: FAILED. 0 passed; 11 failed`) | implementatie in `peer.rs` → 11 passed | geen |
| `fixtures::sha256_matches_published_test_vector` | stub `sha256_hex` geeft `""` | `cargo test -p burnburn-test-support --test fixtures` → FAILED `left: "" right: "ba7816bf…"` | `sha2::Sha256` hex → passed | — |
| `fixtures::ruida_fixture_hashes_match_manifest` | stub `verify_manifest` geeft lege lijst (fixtures nog niet gekopieerd) | zelfde run → FAILED `left: [] right: ["rectangle-default.rd", …]` | kopie + manifestcontrole → passed | — |
| `fixtures::ruida_fixture_manifest_agrees_with_research_evidence` | stub-hash `""` | zelfde run → FAILED `left: "" right: "392e319b…"` | → passed (7 researchbestanden gekruist) | — |
| `fixtures::tampered_fixture_byte_is_reported`, `truncated_fixture_is_reported_as_size_mismatch`, `missing_fixture_is_an_io_error` | stub geeft altijd `Ok` | zelfde run → 3 FAILED (`unwrap_err()` op `Ok`) | grootte- vóór hashcontrole, IO-fout getypeerd → passed | — |
| `fixtures::read_ruida_fixture_returns_checked_bytes` | stub geeft lege bytes | zelfde run → FAILED `left: 0 right: 525` | daarna **testfout** gevonden: `left: Some(96) right: Some(215)`. De test verwachtte de *ontscramblede* EOF-byte D7 uit 04-exportanalyse.md op het gescramblede bestand. Oracle gecorrigeerd naar 0x60 (= scramble(D7), met afleiding in commentaar) → 7 passed | — |
| `layers::tests::*` (17 tests: parse, onbekende kind, schone graaf, R1, opwaarts, zelfde laag, niet in `allow`, dev-regels, hosts/tools nooit dependency, R4, R5-prefixpatronen, R6 per triple, R7, tool vrijgesteld, onbekende namen in tabel, metadata-model, closure zonder dev-randen) | stub: `parse_table` leeg, `check` geeft niets, metadata-parsers leeg | `cargo test -p xtask` → 15 FAILED, 2 passed (`clean_downward_graph_passes`, `tool_is_exempt_from_dependency_rules`: negatieve gevallen, slagen terecht tegen een lege checker) | regelengine → 17 passed | geen |

`cargo xtask layers` op de echte workspace → `layers: 9 crates, 2 triples, no violations` (geen rode fase: er is
momenteel geen overtreding; het onderscheidend vermogen is bewezen met de unittests hierboven).

## Propertytests geometry (geen rode fase)
`crates/burnburn-geometry/tests/properties.rs`: `transform_inverse_restores_every_point`,
`transform_then_inverse_is_identity`, `inverse_preserves_orientation_flag`, `collapsed_axis_is_singular`,
`bounds_follow_translation`, `bounds_lie_between_endpoints_and_control_hull`, `signed_area_scales_with_determinant`.
`cargo test -p burnburn-geometry --test properties` → 7 passed bij de eerste run. **Geen rode fase**: dit zijn
invarianten op reeds geïmplementeerde code (karakterisering), geen nieuw gedrag; ze zijn niet tegen een kapotte
implementatie gedraaid. Oracles zijn wiskundige identiteiten, geen kopie van de productieformule.

## Verificatie
- `cargo fmt --all` → uitgevoerd (formatteert ook bestanden van andere rollen; geen inhoudelijke wijziging).
- `cargo clippy -p burnburn-test-support -p xtask -p burnburn-geometry --all-targets -- -D warnings` → schoon.
- `cargo test -p burnburn-test-support -p xtask` → 3 + 7 + 11 + 17 passed.
- Volledige `cargo xtask verify` is **niet** gedraaid (andere rollen bouwden tegelijk; tussentijds ontbrekende
  manifests van nieuwe crates braken `cargo metadata`).

## Onbevestigd
- GitHub Actions-workflow: niet uitgevoerd. Windows/Linux-kernsuite en de macOS-native job zijn onbevestigd.
- R6 (transitief) is alleen op de huidige, kleine graaf gedraaid; aarch64-apple-darwin en x86_64-pc-windows-msvc
  worden via metadata-resolutie gecontroleerd, niet door te bouwen.

---

# Fase B (Testing) — fuzz, corpora, fixturecontrole, workspace-deps, reviews (18-10-2026)

Commando's met `export PATH="$HOME/.cargo/bin:$PATH"` en `CARGO_TARGET_DIR=target/testing`. Toen andere rollen
tijdelijk onvolledige crates (`burnburn-image`, `burnburn-trace`) hadden en de workspace niet laadde, is de rode run
van xtask gedraaid in een scratch-workspace met alleen `xtask` (zelfde `src/` via symlink, zelfde `Cargo.lock`,
`--offline`); de groene runs in de echte workspace.

## Opgeleverd
- `fuzz/` (cargo-fuzz, eigen `[workspace]`, root `exclude = ["fuzz"]`): targets `ruida_decode`, `ruida_rd_file`,
  `ruida_udp_frame`, `io_container`, `io_dto_json`; draaiinstructie en crashbeleid in `fuzz/README.md`.
  `parse_svg`/`parse_dxf` bestaan nog niet (IM10) → nog geen target.
- `crates/burnburn-test-support/tests/untrusted_bytes.rs`: stabiele tegenhanger (proptest, 512 gevallen per property):
  willekeurige bytes én beschadigde echte fixtures (bitflip, overschrijven, afkappen, invoegen, wegknippen).
  Dev-dependencies `burnburn-io`, `burnburn-ruida`, `proptest` op test-support (alleen `tests/`; geen cyclus in de
  lib-graaf).
- `fixtures`: twee manifestvormen (`files[].path` en Kern's `fixtures[].file`), `fixture_dir`, `read_fixture(set, path)`,
  `unlisted_files`; `read_ruida_fixture` delegeert. Eén test `fixtures_match_manifests` voor alle `test-data/*`.
- `test-data/trace/` (8 cases, 16 bestanden + manifest) via `cargo xtask gen-trace-corpus` (`xtask/src/trace_corpus.rs`):
  eigen deterministische rasterizer (4×4 samples, alleen `+ − × ÷`), eigen PNG-schrijver (stored deflate), vector-
  referentie als JSON naast elke PNG, verwachte topologie vooraf met de hand in het manifest.
- `cargo xtask workspace-deps` (DR-07): elke dependency in elke crate-manifest moet `workspace = true` hebben;
  stap in `cargo xtask verify` direct na `layers`. `sha2` toegevoegd aan xtask.

## Cycli

| Test | Verwachte fout (red) | Red-commando en waargenomen resultaat | Green-commando en resultaat | Refactor |
|---|---|---|---|---|
| `fixtures::project_fixture_hashes_match_manifest` | `load_manifest` kent alleen `files` | `cargo test -p burnburn-test-support --test fixtures` → FAILED `manifest: no `files` array` | beide manifestvormen → 11 passed | `read_ruida_fixture` → `read_fixture("ruida", …)` |
| `fixtures::read_fixture_checks_any_set` | stub `read_fixture` geeft altijd `NotInManifest("")` | zelfde run → FAILED `called Result::unwrap() on an Err value: NotInManifest("")` | idem | — |
| `fixtures::unlisted_fixture_is_reported` | stub `unlisted_files` geeft `[]` | zelfde run → FAILED `left: [] right: ["oracle/stray.json"]` | recursieve lijst minus manifest/README/`.DS_Store` | — |
| `fixtures::fixtures_match_manifests` | projects-manifest niet leesbaar | zelfde run → FAILED `test-data/projects: manifest: no `files` array` (4 failed, 7 passed) | idem; later uitgebreid met verplichte set `trace` → 11 passed | — |
| `workspace_deps::tests::{inline_version_is_reported, version_table_without_workspace_is_reported, path_dependency_is_reported, dev_build_and_target_sections_are_checked, invalid_toml_is_an_error}` | stub `check_manifest` geeft `Ok([])` | scratch-workspace `cargo test --offline -p xtask` → 14 failed, 19 passed; o.a. `left: [] right: ["demo: [dependencies] objc2"]`, `assertion failed: check_manifest("demo", "[dependencies\n").is_err()` | `toml`-parse, secties + `target.<cfg>.*` → `cargo test -p xtask` 33 passed | — |
| `workspace_deps::tests::workspace_inherited_dependencies_pass` | — | **geen rode fase** (negatief geval, slaagt terecht tegen de lege stub) | idem | — |
| `trace_corpus::tests::crc32_and_adler32_match_published_check_values` (oracle: CRC-32-checkwaarde `cbf43926`, Adler-32 "Wikipedia" `11e60398`) | stubs geven 0 | zelfde scratch-run → FAILED `left: 0 right: 3421780262` | bitgewijze CRC-32, Adler-32 → passed | — |
| `trace_corpus::tests::png_has_signature_header_and_trailer`, `png_idat_holds_filtered_rows_in_stored_zlib` (bytes met de hand uit PNG-/RFC 1950/1951-structuur) | stub-PNG leeg | zelfde run → FAILED `range end index 8 out of range for slice of length 0` / `range start index 33 …` | chunks + stored blocks → passed | — |
| `trace_corpus::tests::pixel_aligned_square_is_solid_inside_and_paper_outside` | stub-raster alles 255 | zelfde run → FAILED `pixel (2, 2) left: 255 right: 0` | even-odd crossing, 4×4 samples → passed | — |
| `trace_corpus::tests::half_and_quarter_pixel_edges_give_partial_coverage` (8, 4, 2 van 16 samples → 127, 191, 223) | idem | zelfde run → FAILED `left: 255 right: 127` | idem | — |
| `trace_corpus::tests::circle_and_ring_ink_approximate_their_area` (oracle πr², π(R²−r²), < 0,5 %) | idem | zelfde run → FAILED `disc ink 0 vs 1256.6370614359173` | idem | — |
| `trace_corpus::tests::stroke_covers_exactly_the_points_within_half_width` | idem | zelfde run → FAILED `row 8 left: 255 right: 0` | afstand² tot segment ≤ (w/2)² | — |
| `trace_corpus::tests::every_case_raster_has_the_hand_written_topology` (eigen CCL: inkt 8-, papier 4-samenhangend) | idem | zelfde run → FAILED `case ring left: (0, 0) right: (1, 1)` | → passed voor alle 8 cases | — |
| `trace_corpus::tests::committed_corpus_matches_generator` | corpus nog niet geschreven | zelfde run → FAILED `manifest.json: No such file or directory` — **geen gedragsrode fase** (ontbrekend bestand) | `cargo xtask gen-trace-corpus` → 17 bestanden; test passed | — |
| `trace_corpus::tests::rasterization_is_deterministic_and_names_are_unique` | — | **geen rode fase** (slaagt ook tegen de stub) | passed | — |
| `untrusted_bytes::*` (8: RD-bestand willekeurig/beschadigd, UDP-frame-contract, frame→parse, container willekeurig/beschadigd, DTO-JSON, onbeschadigde fixtures decoderen) | — | **geen rode fase**: invarianten over bestaande code; er is geen paniek gevonden, dus geen rood. Niet tegen een kapotte implementatie gedraaid | `cargo test -p burnburn-test-support --test untrusted_bytes` → 8 passed (0,28 s) | — |

Onafhankelijke controle van de PNG's (ontwikkelcheck, niet geautomatiseerd): `sips -g pixelWidth -g pixelHeight
-g bitsPerSample` → 64×64, 8 bit voor alle 8; Python `zlib.decompress` + `zlib.crc32` per chunk (`python3 -I`, scratch)
→ 4160 bytes, CRC's kloppen, ring-centrum 255, ringband 0. **Handmatig**: vergrote `letter-a` en `stroke-y` visueel
bekeken (A met driehoekig gat; Y met ronde kappen).

## Fuzzen: wat wel en niet gedraaid heeft
- `cargo-fuzz` is niet geïnstalleerd; **er is geen fuzz-run gedraaid** en `fuzz/` is **niet gecompileerd**
  (`libfuzzer-sys` staat niet in de lokale registry-cache; downloaden is niet gedaan).
- Bij het controleren met `cargo +nightly fuzz --version` installeerde rustup automatisch de nightly-toolchain
  (auto-install). Dat was niet gevraagd; de toolchain is direct weer verwijderd (`rustup toolchain uninstall nightly`).
  Huidige toolchains: stable, 1.99.1.
- De fuzz-targets roepen dezelfde publieke functies aan als `untrusted_bytes.rs`, die wel compileren en draaien.

## QueuedTaskPort / FakeFileSystem (opdrachtpunt 3): bewust niet verplaatst
Zie `docs/coordination/testing.md` § Aan User-flow en § Aan Design.

## Verificatie (fase B)
- `cargo fmt -p xtask -p burnburn-test-support --check` → alleen `xtask/src/bundle.rs` (GUI) wijkt af; mijn bestanden
  schoon. Fuzz-targets met `rustfmt --edition 2024` geformatteerd.
- `cargo clippy -p xtask -p burnburn-test-support --all-targets --locked --no-deps -- -D warnings` → schoon
  (`--no-deps`: `burnburn-document` van Kern/CAM gaf tussentijds dead-code-waarschuwingen).
- `cargo test -p xtask -p burnburn-test-support` → xtask 33, test-support 3 + 11 + 11 + 8, alles groen.
- `cargo xtask layers` → 15 crates, no violations; `cargo xtask workspace-deps` → geen overtredingen.
- Volledige `cargo xtask verify` → **faalt** op `fmt --check` door crates van andere rollen in aanbouw; details en
  per-stap-uitkomsten in `docs/coordination/testing.md` § Stand `cargo xtask verify`.

## Onbevestigd
- `fuzz/` niet gecompileerd en niet gedraaid (geen cargo-fuzz/nightly; geen download).
- PNG-determinisme is alleen op macOS arm64 gecontroleerd (de generator gebruikt geen transcendente functies; Windows/
  Linux-CI moet `committed_corpus_matches_generator` nog laten zien).
- Geen periodieke fuzz-job in CI; volgt zodra de targets eenmaal lokaal met nightly zijn gecompileerd.

---

# Fase C (Testing) — scenariocrate, fixtures, holdout, fuzz-targets, benchmarks, review (18-10-2026)

Commando's met `export PATH="$HOME/.cargo/bin:$PATH"`, `CARGO_TARGET_DIR=target/testing`. Andere rollen bouwden
tegelijk (document/cam/trace/generators compileerden tussentijds niet). Mutatieproeven en benchmarks zijn daarom
gedraaid op een **scratch-kopie** van de workspace (rsync om 01:59, toen alles compileerde; eigen target-dir), zodat
geen andermans bestand in de echte boom is gewijzigd. Mutaties: `scratchpad/mutate.py` (vervang één fragment, draai de
scenariotest, zet terug).

## Opgeleverd
- **`crates/burnburn-scenarios`** (nieuw, alleen tests; lege lib; alle domeincrates als dev-dependency). In
  `docs/design/layers.toml` als `kind = "testkit"`, laag 8, `allow = []` (zie coordination § Aan Design).
  Scenario's in `tests/`: `gate_b_plate.rs` (a), `gate_b_ring_trace.rs` (b), `sketch_save_reopen.rs` (c),
  `import_to_rd.rs` (d), `same_revision_determinism.rs` (e); helpers in `tests/common/mod.rs` (RD-bewegingslezer:
  absolute/relatieve cut/move → segmenten in mm).
- Schema-2-projectfixture `test-data/projects/v2/plate-operations.burnburn` (verzoek CAM) + manifest + test
  `project_fixtures.rs` (ignored writer, handgeschreven verwachtingen).
- `fixtures_match_manifests` eist nu ook `geometry` en `import`; `project_fixture_hashes_match_manifest` kent v2.
- Holdout-split trace-corpus: `xtask/src/trace_corpus.rs::holdout_cases` (2 cases, `test-data/trace/holdout/`,
  manifestsleutel `holdout` + `holdout_policy`, niet onder `cases`); release-evaluatie `trace_holdout.rs` (ignored).
- Fuzz-targets `io_parse_svg`, `io_parse_dxf` (ook `to_draft(Some(inch))`), `image_decode` (+ `inspect_raster`).
- Benchmarks als ignored tests (`benchmarks.rs`); resultaten in `docs/dev-notes/benchmarks.md`.

## Cycli

| Test | Verwachte fout (red) | Red-commando en waargenomen resultaat | Green-commando en resultaat | Refactor |
|---|---|---|---|---|
| `trace_corpus::tests::holdout_is_listed_separately_and_disjoint_from_dev_cases` | stub `holdout_cases()` geeft `vec![]` | `cargo test -p xtask holdout` → FAILED `holdout split has 0 cases` | 2 cases + manifest `holdout` → passed; `cargo xtask gen-trace-corpus` → 21 bestanden; `diff -rq` met kopie vooraf: alleen `holdout/` nieuw en `manifest.json` gewijzigd, de 16 dev-bestanden bitgelijk; `cargo test -p xtask trace_corpus` 11 passed | geen |
| `project_fixtures::v2_plate_fixture_loads_with_operations_constraint_and_recipe` | fixture bestaat nog niet | `cargo test -p burnburn-scenarios --test project_fixtures` → FAILED (panic in `read_fixture`, regel 89: bestand/manifest ontbreekt) — **geen gedragsrood** (ontbrekend bestand) | `-- --ignored write_v2_plate_fixture` → geschreven; `unzip -p … document.json` met de hand nagelopen (ids 1–7, next_id 8, recept rev 1 gekopieerd in operation 6, constraint 5→6); manifest bijgewerkt → passed | geen |
| `fixtures::fixtures_match_manifests` (eist `geometry`, `import`) | — | **geen rode fase** (sets bestonden al). Mutatie in scratch: `test-data/import` weggezet → FAILED `test-data/import missing: ["geometry", "projects", "ruida", "trace"]` | passed in scratch; in de echte boom FAILED door `test-data/fonts/reference_metrics.py` (niet in manifest, Generators/tekst) — terecht | — |
| scenario (a) `gate_b_plate_width_150_reaches_the_rd_file_with_holes_in_place` | — | **geen rode fase**: karakterisering van een bestaande keten, eerste run groen. Mutaties: **M-a1** cam `compile` neemt alleen het eerste target → FAILED `outline + 4 holes as separate closed loops left: 1 right: 5`; **M-a2** ruida `abs_move` +1 mm in x (header niet) → FAILED `cut motion spans the header bounds exactly left: (1.0, 0.0)…(151.0, 80.0)` | 1 passed | — |
| scenario (b) `gate_b_ring_trace_keeps_its_hole_in_the_rd_file` | — | geen rode fase; **M-b1** cam: alleen het eerste subpad per object flattenen → FAILED `outer and hole are two closed cut loops left: 1 right: 2` | 1 passed | — |
| scenario (c) `sketch_survives_save_and_reopen_and_stays_editable_with_the_real_solver` | — | geen rode fase; **M-c1** io `sketch_map`: evaluatie-`inputs` niet teruggelezen → FAILED `left: None right: Some(ContentHash(edf99869…))` | 1 passed | — |
| scenario (d) `svg_viewbox_units_reach_the_rd_file_in_millimetres`, `unitless_dxf_scale_decision_sets_the_machine_extent` | — | geen rode fase; **M-d1** io DXF: expliciete schaal genegeerd (factor 1) → FAILED `4 × 2 in left: (0,0)…(4.0, 2.0) right: …(101.6, 50.8)`; **M-d2** io SVG: geometrie 2× geschaald, eenheidsrapport ongewijzigd → FAILED in beide SVG-tests op `!preflight(..).has_errors()` (buiten bed), niet op de extent-assertie | 2 passed | — |
| **bug** `rd_header_bounds_include_a_cut_that_starts_at_the_job_start` | header-bounds = extent van de snijbeweging | eerste run (in `unitless_dxf_…`) → FAILED `RD header bounds = cut motion extent left: (101.6, 0.0)…(101.6, 50.8) right: (0.0, 0.0)…(101.6, 50.8)`. Oorzaak: de L begint in (0, 0) = jobstart, er is geen travel ernaartoe en `JobPlan::bounds` sluit de startpositie uit → `E7 03` min x te groot. Afgesplitst als eigen regressietest; **blijft rood tot CAM het oplost** | — (eigenaar CAM) | — |
| scenario (e) `same_revision_compiles_to_identical_plan_hash_and_rd_bytes` | — | geen rode fase; **M-e1** job: sessierevisie in de canonieke planbytes → FAILED `content-addressed: revision is not part of the id left: JobPlanId(610d8f6f…) right: JobPlanId(29639d3c…)` | 1 passed | — |
| `trace_holdout::holdout_cases_meet_the_release_criteria` (ignored) | — | geen rode fase, geen mutatie (evaluatie, geen gedragstest) | `-- --ignored` → `plate-two-holes` (1, 2), max 0,0395 mm bij 0,25 mm/px; `ring-offcentre-fine` (1, 1), max 0,0394 mm bij 0,05 mm/px → passed | — |
| `benchmarks::*` (3, ignored) | — | geen rode fase; invarianten (100 000 cuts; 100/100 componenten/gaten; analytisch oppervlak) | zie `benchmarks.md` | — |

Opmerking bij (a): de bestaande `presentation/tests/gate_b.rs` (User-flow) toetst de breedtewijziging in de scène en
exporteert een RD van een getekende rechthoek, maar decodeert alleen `commands().count() > 0`. Deze scenariocrate
toetst de RD-geometrie zelf (bounds, gaten op hun plaats) zonder presentatielaag.

## Verificatie (fase C)
- `cargo fmt -p burnburn-scenarios -p xtask -p burnburn-test-support` uitgevoerd; fuzz-targets met
  `rustfmt --edition 2024 --check` → schoon.
- `cargo clippy -p burnburn-scenarios -p burnburn-test-support --all-targets --locked --no-deps -- -D warnings` →
  schoon. `cargo clippy -p xtask …` → faalt alleen op `xtask/src/bundle.rs:202` (GUI, `needless_range_loop`).
- `cargo test -p burnburn-scenarios -p burnburn-test-support -p xtask --no-fail-fast` → alles groen behalve de
  bewuste CAM-regressietest en `fixtures_match_manifests` (fonts-manifest, Generators/tekst).
- Volledige `cargo xtask verify`: zie `docs/coordination/testing.md` § Stand verify fase C.

## Onbevestigd
- Fuzz-targets niet gecompileerd en niet gedraaid (geen nightly/cargo-fuzz, geen `libfuzzer-sys` in de cache).
- Benchmarks en holdout-evaluatie alleen macOS arm64, release, scratch-kopie.
- De mutaties zijn per scenario één of twee; ze tonen dat de keten bij die breuk faalt, niet dat elke breuk gevangen wordt.

## W3-robuust: herkomst van de goldens (2026-10-20)

Opdracht: elk golden-bestand en elke vastgelegde verwachting van golf 1–2 nalopen op *herkomst* (onafhankelijk berekend of uit
de implementatie overgenomen). Er is niets gewijzigd aan een golden; per fixturemap staat de herkomst nu in een README ernaast
(`test-data/{fonts,trace,geometry,import,projects,ruida}/README.md`, `crates/burnburn-generators/tests/fixtures/README.md`,
`crates/burnburn-icons/tests/data/README.md`). Dit is een leesaudit plus herberekeningen waar een script bestond; het is
geen nieuwe test en geen red/green-cyclus.

### Wat opnieuw is nagerekend
| Golden | Herberekend met | Resultaat |
|---|---|---|
| Image-mode-patronen (Floyd–Steinberg, Jarvis, Stucki, Atkinson, Bayer 4, halftone ×4 hoeken/grijswaarden, sketch ×3) | `python3 -I docs/dev-notes/IM18-image-modes-reference.py` (eigen transcriptie van de kernels uit de gepubliceerde definitie; halftone via numerieke oppervlakte-integratie) | Alle 12 uitvoerregels van het script staan woordelijk als literal in `burnburn-image/tests/modes.rs` of `burnburn-cam/tests/image_modes.rs` (scriptcontrole op de bronbestanden). De overige patroonliteralen (`#####`, `##.##`, `##..`, de maskerrij `###....###`) zijn in commentaar met de hand afgeleid (bv. unsharp van `[0,0,90,0,0]` geeft centrum 150) |
| RD-oracle-JSON (`test-data/ruida/oracle/*`) | `python3 -I research/lightburn/tools/rd_inspect.py --json` op de drie `.rd`-bestanden | Alle drie reproduceren de JSON (zonder padvelden) |
| RD-fixtures | `cmp` tegen `research/lightburn/fixtures/*.rd` | Byte-identiek aan de onderzoekskopie (LightBurn Core 2.1.04-export) |
| Barcode-vectoren | n.v.t. (extern) | `zint-vectors.txt` nieuw, zie IM22.md; de andere bestanden zijn externe uitvoer (CoreImage, BWIPP-port, AprilTag-C) |

### Overzicht per categorie
| Categorie | Fixture / golden | Herkomst | Onafhankelijk? |
|---|---|---|---|
| Image modes | literals in `burnburn-image/tests/modes.rs`, `burnburn-cam/tests/image_modes.rs` | Python-referentie + handafleiding (zie boven) | **Ja** |
| Image modes | `burnburn-image/tests/raster.rs`, `pipeline.rs` | Handberekend (commentaar) of analytische grenzen | Ja |
| Render | geen golden-PNG's; `burnburn-render/tests/offscreen.rs` (Metal, per pixel), `scene_projection.rs`, `sketch_overlays.rs`, `burnburn-icons/tests/render.rs` | Analytische pixelposities en uitgeschreven kleurconstanten; ADR 0003 sluit golden-PNG als enig orakel uit | Ja. **Onbevestigd:** de GPU-test slaat zonder adapter over (`SKIPPED` is geen pass); Windows/DX12 niet gedraaid |
| RD | `test-data/ruida/*.rd` + `oracle/` + `sources/` | Echte LightBurn-export en Python-decoder | **Ja** |
| RD | `burnburn-ruida/tests/lower*.rs`, `fixtures.rs`, scenario-tests `*_rd.rs` | Verwachtingen uit `04-exportanalyse.md`, de oracle-JSON of uit de tekening afgeleid; byte-gelijkheid alleen voor records die de exportcondities exact herhalen | Ja |
| Import | `test-data/import/*` | Met de hand geschreven; verwachtingen uit de bestandstekst | Ja |
| Geometrie | `test-data/geometry/boolean-cases.txt` | Oppervlakten met de hand afgeleid | Ja |
| Trace | `test-data/trace/*` | Gegenereerd uit vectorreferenties; topologie vooraf geschreven | Ja voor de vorm, niet voor de rasterisatie |
| Hash | `burnburn-io/tests/canonical_hash.rs::project_hash_matches_hand_encoded_bytes`, `test-support` SHA-256 van "abc" | Met de hand gecodeerde bytes, gepubliceerde testvector | Ja |
| Fonts | `test-data/fonts/*` + `reference_metrics.py` | Derde-partij fonts en een `struct`-only parser | Ja |
| **Vastgelegd (niet onafhankelijk)** | `burnburn-job/tests/modes.rs` (twee `MachineProfile`-hashes, "Values computed with the matrix of IM25 … before the new entries existed") | Uitvoer van de implementatie vóór een wijziging; beschermt hash-stabiliteit | **Nee:** regressiebewaker, bewijst geen juistheid |
| **Vastgelegd (niet onafhankelijk)** | `burnburn-geometry/tests/determinism.rs::GOLDEN` (bits van `arc_to_cubic`) | `libm`-uitvoer op macOS arm64 (IM09, DR-13) | **Nee:** bewijst platformstabiliteit; Windows niet gedraaid |
| **Vastgelegd (niet onafhankelijk)** | `test-data/projects/v1`, `v2` | Door BurnBurn geschreven, met de hand nagelezen, daarna bevroren | **Nee** voor de schrijfroute; wel een echte invoer voor migratietests (verwachtingen met de hand in de tests) |

Conclusie: geen golden gevonden waarvan de verwachting uit de te toetsen implementatie is gekopieerd *zonder* dat dit zo is
gedocumenteerd; de drie vastgelegde categorieën staan hierboven als regressie- of stabiliteitsbewaker en horen zo gelezen te worden.
Niet nagelopen omdat ze geen golden zijn maar rekenkundige tests: proptest-regressiebestanden (`*.proptest-regressions`).

## W3-robuust: fuzzen
Zie `docs/dev-notes/IM10.md` (§ W3-robuust: fuzzdoelen) voor looptijden, aantallen en bevindingen. De regel
"Fuzz-targets niet gecompileerd en niet gedraaid" in § Onbevestigd hierboven is daarmee voor de genoemde doelen achterhaald.
