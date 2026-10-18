# Generators/tekst — coördinatie (fase C, 18-10-2026)

Logs: `docs/dev-notes/IM18-tekst.md`, `docs/dev-notes/IM22.md`.

## Aan Design
1. **DR-22 uitgevoerd.** Tekstdata in `burnburn-document::text` (`TextObject`, `FontRef`, `FontSize`, `TextLayout`,
   `TextId`, `MissingGlyph`, `TextLayer`, `TextCommand::{Create, Set, Delete}`); `burnburn-text` re-exporteert ze en
   implementeert `document::Evaluator` (`TextEvaluator<F: FontSource>`). Statussen: alles getekend of glyphs ontbreken
   → `EvalStatus::Valid` + `OwnerSolution::Text { missing }` (in `TextLayer::missing(id)`); font ontbreekt →
   `Failed { FontMissing { font } }`; ongeldige layout → document weigert het command (`TextError::InvalidLayout`).
   Afwijking van contract IM18: `ShapedText.missing` is `Vec<MissingGlyph { ch, byte, line }>` i.p.v. `Vec<char>`
   (locatie voor de rijpreview), extra `lines`, `line_outlines`, `glyph_outlines`. `FontSize` i.p.v. `size: Length`
   (`from_mm`/`from_points`, expliciet). Graag in het contract overnemen of afwijzen.
2. **Register:** harfrust 0.14.0 en skrifa 0.48.0 nu **in gebruik** (beide `default-features = false, ["std"]`);
   transitief read-fonts 0.45.1, font-types 0.12.7, bytemuck(+derive), bitflags, smallvec, foldhash, once_cell,
   yoke(+derive, zerofrom, stable_deref_trait), synstructure. Geen `cc`/buildscript met C.
3. **QR:** geen pure-Rust QR-crate in het register, dus niet gebouwd. Verzoek: `qrcode` (MIT/Apache-2.0, pure Rust;
   versie/transitieven graag door Design controleren) als kandidaat, of besluit "eigen encoder" (Reed-Solomon +
   maskers; groter werkpakket). Code 128 B/C is zelf gebouwd.
4. **Hashdomein:** `FrozenRows` hasht nu in `HashDomain::Asset` (opaque). Voorstel: eigen domein `"frozen-rows"`
   (Kern voegt toe) — dan wijzigen bestaande hashes van bevroren rijen (nog nergens opgeslagen).
5. **Fixtures:** `test-data/fonts/` (Liberation Sans, BIZ UDPGothic, OFL, gekopieerd uit `../solidcraft/assets/fonts/`;
   BIZ is 4,6 MB — enige lokale OFL-font met Latijnse `liga` én CJK). Graag bevestigen of een kleiner subset-font later moet.

## Aan Kern
1. **Kleine gerichte edits in jullie bestanden** (DR-22, één variant per rol): `patch.rs` (`DerivedOwner::Text`,
   `DerivedOwnerKind::Text`, `EvalDiagnosis::FontMissing`, `OwnerSolution::Text`), `command.rs`
   (`CommandPayload::Text`, `UndoLabel::Text`, `ChangeSet.texts`, `CommandError::Text`), `canonical.rs` (tag `"text"`),
   `snapshot.rs` (`texts`, `with_texts`, `text_layer`, `text`, `derived_record`, `same_content`, extra parameter
   `from_txn`), `session.rs` (txn-veld, `snapshot_at`, evaluatielus voor tekst, `apply_text_patch`, saved/dirty).
   io: `text_dto.rs`/`text_map.rs` (nieuw, van mij), `dto.rs` (`texts`, `Source::DerivedText`), `map.rs`, `hash.rs`
   (`tag("texts")` alleen als aanwezig → bestaande hashes gelijk). Alle document-tests en io-tests groen, behalve
   `io/tests/lbrn2.rs` (jullie IM28-rood). Graag reviewen.
2. **`map.rs` slaat afgeleide objecten van onbekende owners stil op als `Drawn`** (`_ => v1::Source::Drawn`). Dat
   verloor mijn tekstoutputs tot ik `DerivedText` toevoegde; voor `Modifier`/`Part` gebeurt het nu nog. Voorstel:
   `SaveError` i.p.v. stille omzetting (regressietest per owner).
3. **Volgende stap C-2:** `document::generator` (generatordefinities) + `DerivedOwner::Generator(GeneratorId)`. De
   generatorfuncties leveren al `GeneratedPart { local, name, path }` + `to_outputs(owner, parts)`; ik voeg de
   datamodule toe zodra jullie `session.rs` rustiger is (zelfde patroon als tekst), tenzij jullie het anders willen.

## Aan User-flow / GUI
- Tekst bewerken = `CommandPayload::Text(TextCommand::Set { text, source })` met `apply_with(&[&TextEvaluator])`;
  alleen gecommitte tekst (IME blijft in de host). Rijpreview: `generators::resolve` levert `text`, `sequence`, `missing`.
- `TextLayer::missing(id)` en `EvalDiagnosis::FontMissing { font }` tonen; outlines van een ontbrekend font zijn
  `production_blocked` (geen stille OS-fallback).

## Aan Testing
- Nieuwe fixtureset `test-data/fonts/` (manifestformaat `files[]` met `licence`/`origin`); graag opnemen in de
  fixturecontrole. `reference_metrics.py` is een referentietool (struct-only), geen runtime.

## Van Integratie-datamodel (18-10-2026)
- **QR (orchestratorbesluit: eigen encoder):** `burnburn_generators::{qr, qr_encode, qr_encode_with, QrEcc, QrCode,
  QrParams, QrSymbol, qr_reed_solomon, qr_format_bits, qr_version_bits}` — bytemodus, L/M/Q/H, v1–10. Oracles: RS-vector
  "HELLO WORLD" 1-M, gepubliceerde format-/versietabellen, 10 CoreImage-matrices (`tests/fixtures/qr-coreimage.txt`,
  hulpmiddel `research/qr-reference/reference.swift`; Vision decodeert ze 10/10). Geen externe crate.
- **`document::generator` (jullie stap 3) gebouwd:** `GeneratorId`, `GeneratorDefinition { name, generator, recipe:
  EngineRecipe, placement }`, `GeneratorCommand`, `generator_inputs` (domein `eval-generator`), `DerivedOwner::Generator`,
  io-DTO. `GeneratorEvaluator` (`qr`, `code128`, `finger-box`) staat in jullie crate (`src/evaluator.rs`); de overige
  generatoren (tab/slot, hinge, stencil, path-text, data-rows) graag door jullie toevoegen — parameters als `ParamValue`.
- Ik heb `GenError` niet gewijzigd; parameterfouten worden `EvalDiagnosis::InvalidParameters { parameter }`.
