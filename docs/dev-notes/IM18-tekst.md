# IM18 Tekst (tekstdeel) — Generators/tekst

Status: groen op macOS arm64, headless. Windows: **onbevestigd** (geen runner). Rasterdeel van IM18 is CAM/Trace.

## Opgeleverd
- `burnburn-text` (L3): `FontData::from_bytes` (skrifa + harfrust over dezelfde bytes, hash = asset-domein),
  `FontSource` + `MemoryFontSource` (asset-hash wint; een ontbrekend ingebed asset wordt nooit vervangen door een
  font met dezelfde familienaam), `shape` (harfrust: kern/liga schakelbaar; regels, uitlijning, regel- en
  letterafstand; outlines in mm via skrifa, quad → exacte cubic; fallbackketen per teken; ontbrekende glyphs in
  `missing`, nooit tofu; `glyph_outlines` per glyph in tekstruimte), `TextMode::SingleLine` (eigen enkellijnsset
  `0-9 A-Z -.+/:`, open paden, cap = 0,7 em; geen Hershey-data → geen licentievraag), `TextEvaluator`
  (`document::Evaluator`, `DerivedOwner::Text`, één output per niet-lege regel, key = regelindex), `inputs_hash`
  (canonieke bron + hash van de daadwerkelijk geladen fontbytes + evaluatorversie).
- DR-22: tekstdata in `burnburn-document::text` (`TextObject`, `FontRef`, `FontSize`, `TextLayout`, `TextId`,
  `MissingGlyph`, `TextLayer`, `TextCommand::{Create, Set, Delete}`, canonieke vorm). Haken in Kern-bestanden
  (klein, gericht): `DerivedOwner::Text`, `DerivedOwnerKind::Text`, `EvalDiagnosis::FontMissing`,
  `OwnerSolution::Text { missing }`, `CommandPayload::Text`, `UndoLabel::Text`, `ChangeSet.texts`,
  `CommandError::Text`, snapshot `texts` + `with_texts`/`text_layer`/`text`, session (txn, evaluatie in dezelfde
  transactie, `apply_text_patch`, undo/redo/dirty). io: `text_dto.rs`, `text_map.rs`, `Source::DerivedText`,
  `Document.texts` (additief, weggelaten als leeg), projecthash `tag("texts")` alleen indien aanwezig.
- Fixtures `test-data/fonts/` (manifest met licentie en herkomst; OFL-fonts gekopieerd uit `../solidcraft/assets/fonts/`,
  niets gedownload) + `reference_metrics.py` (struct-only referentie, onafhankelijk van skrifa).

## Cycli
Commando's met `export PATH="$HOME/.cargo/bin:$PATH" CARGO_TARGET_DIR=target/generators`.

| Test | Verwachte fout (red) | Red-commando en waargenomen resultaat | Green-commando en resultaat | Refactor |
|---|---|---|---|---|
| `shaping::*` (14) | stub-`shape` geeft lege `ShapedText` | `cargo test -p burnburn-text --test shaping` → `FAILED. 2 passed; 12 failed` | shaping/outlines/layout/single-line → `13 passed; 1 failed` (liga, zie volgende rij) | — |
| ↳ `font_loads_from_bytes_and_garbage_is_a_typed_error`, `shaping_is_deterministic` | — | **geen rode fase**: fontladen was al echt in de stub; determinisme triviaal | groen | — |
| ↳ `fi_ligature_*` | aanname "Liberation Sans heeft liga fi" bleek fout | test faalde ook ná implementatie; struct-parser over GSUB: latn = ccmp/subs/sups, dlig alleen hebr | test herschreven op BIZ UDPGothic (latn `liga`, glyph 12864, hmtx 1491) → `14 passed` | `dlig`-veld weer verwijderd |
| `shaping::glyph_outlines_are_per_glyph_in_text_space` | veld leeg | `cargo test -p burnburn-text --test shaping glyph_outlines` → `FAILED … left: 0` | `15 passed` | — |
| `evaluate::*` (eerste versie, losse `evaluate()`) | stub `Failed`/leeg | `cargo test -p burnburn-text --test evaluate` → `FAILED. 1 passed; 4 failed` (`invalid_layout…` slaagde toevallig op de stub) | `5 passed` | vervangen door DR-22-variant |
| `evaluate::*` (DR-22, 6: bron in document, outputs in zelfde transactie, ids stabiel + undo/redo/delete, FontMissing houdt laatste outlines + `production_blocked`, missing glyphs = Valid + lijst, ongeldige layout geweigerd, inputs-hash) | — | **implementatie vóór test**; rood via mutatie (`apply_text_patch` → `let _ = patch`): `FAILED. 2 passed; 4 failed` | hersteld → `6 passed` | — |
| `io::text_roundtrip::{text_object_survives_save_and_reopen_with_record_and_outlines, project_hash_covers_text_sources}` | io bewaarde geen tekst; afgeleide objecten werden stil `Drawn` | `cargo test -p burnburn-io --test text_roundtrip` → `FAILED. 0 passed; 2 failed` (`TextLayer { texts: {} … }`, gelijke hashes) | DTO + mapping + hash → `2 passed`; overige io-tests groen behalve `lbrn2` (Kern, IM28 in aanbouw, rood vóór mijn wijziging-onafhankelijk) | — |
| `single_line::tests::every_listed_character_has_strokes_within_the_grid` | — | **geen rode fase** (samen met de tabel geschreven) | groen | — |

Eindrun: `cargo test -p burnburn-text` → 22 groen (1 + 6 + 15); `-p burnburn-document` groen; io-tekst/sketch/roundtrip/hash groen.

## Verificatie
- `cargo fmt -p burnburn-text -p burnburn-generators --check` schoon; eigen nieuwe bestanden in document/io met rustfmt.
  Bestaande Kern-bestanden niet herformatteerd (rustfmt-diffs daar zijn Kern-regels in aanbouw).
- `cargo clippy -p burnburn-text -p burnburn-generators --all-targets -- -D warnings` schoon (geen `-A`).
  `cargo clippy -p burnburn-io --lib --test text_roundtrip` en `-p burnburn-document --lib` schoon; document-tests
  `modifiers/parts_and_plates/relations` (Kern) geven clippy-fouten, niet van mij.
- `cargo run -p xtask -- layers` → `23 crates, 2 triples, no violations`.
- Afhankelijkheden: harfrust 0.14.0 (`default-features = false, ["std"]`), skrifa 0.48.0 (idem); transitief
  read-fonts 0.45.1, font-types, bytemuck, bitflags, smallvec, foldhash, once_cell, yoke (+ proc-macros); geen `cc`.

## Beperkingen / Onbevestigd
- Geen bidi/RTL (runs worden LTR gevormd); variabele fonts op de standaardlocatie; geen hinting (niet nodig voor mm-geometrie).
- `missing`-lijst wordt niet in het projectbestand bewaard (afgeleid; na heropenen opnieuw berekend).
- Windows: onbevestigd.

---

# W1-L5b/c (uitgevoerd in golf 2, lane W2-L5) — tekstopties en variabele tekst

Status: groen op macOS arm64, headless. Windows: **onbevestigd**. Geen UI: per feature een regel in
`docs/plan/integratie-wachtrij.md` (pakket `W2-L5`). Commando's met `CARGO_TARGET_DIR=target/l5-text`
(sessie-eigen). Rode fase = waargenomen gedragsmatige fout tegen een minimale stub (compileerbaar, verkeerde
waarden); compilatiefouten tellen niet.

## Opgeleverd
- **Document** (`burnburn-document::text`, additief): `TextLayout.{uppercase, valign: VAlign, v_space_mm, welded}`,
  `FontStyle::{from_flags, is_bold, is_italic}`, `TextObject.variable: Option<VariableText{offset}>`,
  `TextCommand::ConvertToPath{text}` (+ `TextLabel::ConvertToPath`, `TextError::NothingToConvert`). Canonieke vorm
  schrijft de nieuwe opties **alleen als ze afwijken van de standaard** (bestaande projecthashes blijven gelijk).
  Session: `detach_outputs` zet de afgeleide objecten van de tekst op `GeometrySource::Drawn` (zelfde id's, zelfde
  contouren), verwijdert tekst + record; kleine Kern-haken in `session.rs` en `object.rs`.
- **io** (`text_dto.rs`/`text_map.rs`, bestandseigenaar Tekst): optionele velden `options` en `variable_offset`
  (afwezig = standaard), roundtrip getest.
- **Tekst** (`burnburn-text`): bold/italic-face-keuze (`switch_style`, `FontSource::available_styles`; een ontbrekende
  stijl is `FontMissing`, nooit een stille vervanging; een ingebed asset van het oude face vervalt bij het
  omschakelen), uppercase (Unicode-mapping, `ß`→`SS`, clusters en `missing`-offsets blijven bronbytes),
  verticale uitlijning Top/Middle/Bottom/Baseline uit hhea-ascent/descent van het eerste font (enkellijn: kap =
  0,7 em), VSpace (mm bovenop `line_spacing·em`; HSpace = bestaande `letter_spacing_mm`), Welded (union per regel,
  non-zero, tellers blijven open; veelhoekig binnen em/2000). Multiline en fallback bestonden al.
- **Generators**: `PathTextParams.distort: Distort::{Rigid, Warp}` — Warp buigt de contouren mee (punt op hoogte h
  → afstand h + offset links van het pad op booglengte `start + x`), randen worden onderverdeeld tot de
  koordefout ≤ tolerantie, gaten blijven gaten. Variabele tekst: `SerialSpec/SerialCounter` (start/end/advance by,
  breedte, Wrap/Stop, Previous/Next/Reset, objectoffset), `JobClock` (UTC-seconden + vaste zone, `%Y %y %m %d %H %M
  %S %j %z`), `Template` (`{serial}`, `{date[:fmt]}`, `{csv:kolom}`, `{cut:laag.veld}`, `{{`/`}}`),
  `ProductionRecord` (immutable, content-id), `ProductionCounters` (+ `AdvancePolicy`, `JobEvent`, `commit`),
  `resolve_text`, `bake_command` (= `TextCommand::Set`, dus één undo-stap), `DataTable::from_utf8` (BOM, UTF-8).
- SHX: **niet aanwezig in de codebase** en daarom niet gebouwd; RTL/bidi blijft open (zie hierboven).

## Cycli (W1-L5b/c)

| Test | Verwachte fout (red) | Red-commando en waargenomen resultaat | Green-resultaat | Opmerking |
|---|---|---|---|---|
| `text/tests/options.rs` (8: flags, switch_style, bold-face/missing style, uppercase, valign, single-line kap, VSpace, welded) | stubs: `from_flags`→Regular, `switch_style` clone, opties genegeerd | `cargo test -p burnburn-text --test options` → `8 failed` | `7 passed; 1 failed` → welded-area-oracle te streng (0,02 mm² tegen veelhoekige benadering, **testfout**, 0,1) → `8 passed` | Geen bold-fixture in de repo: Bold-face = BIZ UDPGothic-bytes onder `(familie, Bold)`; test toetst de keuze/het ontbreken, geen typografie |
| `text/tests/evaluate.rs::{convert_to_path_*, every_layout_option_is_part_of_the_inputs_hash}` (3) | `ConvertToPath` was een no-op; hash kende de opties niet | `cargo test -p burnburn-text --test evaluate` → `6 passed; 3 failed` | `9 passed` | Oracle gaten: buitencontour en tegenwinding blijven, pad ongewijzigd, undo herstelt tekst + afgeleide objecten |
| `io/tests/text_roundtrip.rs::text_object_survives_save_and_reopen…` (uitgebreid met alle nieuwe velden) | io liet opties/`variable` vallen (`uppercase: false`…) | `cargo test -p burnburn-io --test text_roundtrip` → `1 failed` (twee keer: opties, daarna `variable`) | `2 passed` | Compile-fout (ontbrekend veld) eerst opgelost met `..Default` zodat de rode fase gedragsmatig was |
| `generators/tests/path_text_distort.rs` (4) | `Distort::Warp` genegeerd | `--test path_text_distort` → `2 failed; 2 passed` (rechte-lijn- en gat-test slagen triviaal op de stub) | `3 passed; 1 failed` → oracle gebruikte de O-overshoot i.p.v. de H-kaphoogte (**testfout**) → `4 passed` | Oracle: op cirkel R ligt elk punt op afstand `R − offset − h` van het middelpunt |
| `generators/tests/variable_text.rs` (17: serial, klok, template, record, ontbrekende waarden, E11 `frozen_job_keeps_chosen_data_rows`, commit, bake) | stub gaf overal `Unsupported` | `cargo test -p burnburn-generators --test variable_text` → `17 failed` | eerste green-run: 16 passed, 1 failed — `valid_up_to` van mijn UTF-8-test was verkeerd geteld (8 → 7, **testfout**) → `17 passed` | Datumorakels met Python `datetime` berekend (zie testkop) |
| `generators/tests/variable_properties.rs` (2 proptests) | — | **geen rode fase**: na de implementatie geschreven | `2 passed` | rooster/stap-invariant en klok→datum→lokale seconde met de voorwaartse Hinnant-formule als onafhankelijke inverse |

## Verificatie
- `cargo clippy -p burnburn-text -p burnburn-document -p burnburn-io -p burnburn-generators --all-targets -- -D warnings` schoon;
  rustfmt op de eigen bestanden; `cargo run -p xtask -- layers` → geen overtredingen.

## Open / onbevestigd
- Welke gebeurtenis LightBurn Auto-Advance op laat verhogen is **niet differentieel getest**; standaard `AdvancePolicy::Manual`.
  Wrap versus Stop bij End: LightBurn-gedrag onbevestigd (beide gebouwd, gekozen in `SerialSpec`).
- Tellerstand, `AdvancePolicy` en de gekozen CSV-rij worden **nog niet in het projectbestand bewaard** (io, wachtrij).
- Job-compile/preview moeten `resolve_text` gebruiken met het `ProductionRecord` van de job (wachtrij, L1/L3).
- Tekst-op-pad als document-object (`BindTextToPath`) bestaat nog niet; `text_on_path` blijft een pure functie.
- RTL/bidi, SHX en variabele fonts zijn niet gebouwd. Echte bold/italic-fonts als fixture ontbreken (licentie/herkomst kiezen).
- Windows: onbevestigd.
