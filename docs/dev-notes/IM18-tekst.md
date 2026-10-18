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
