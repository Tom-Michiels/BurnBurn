# Trace-corpus

Herkomst-audit W3-robuust (2026-10-20). Het corpus is **synthetisch en vooraf bepaald**: de PNG's zijn gerasterd uit de
vectorreferentie (`*.json`) ernaast door `cargo xtask gen-trace-corpus` (`xtask/src/trace_corpus.rs`); de verwachte
topologie (`components`, `holes`, hoeken) staat in `manifest.json` en is geschreven voordat er een tracer bestond.
Een tracer wordt beoordeeld tegen de vectorreferentie (afstand, gaten, graafstructuur), niet tegen pixeloverlap.

| Map | Herkomst | Controle |
|---|---|---|
| `synthetic/` (8 cases: ring, separate-components, square-sharp-corners, letter-a, stroke-t/x/y, strokes-near-touching) | Eigen generator uit vectorbeschrijvingen; geen tracer-uitvoer | `fixtures_match_manifests` (grootte, sha256, `burnburn-test-support`) en `committed_corpus_matches_generator` (xtask: bytes = generatoruitvoer) |
| `holdout/` (2 cases) | Zelfde generator, andere cases | Niet gebruiken om de tracer te ontwikkelen of af te stellen; alleen de release-evaluatie leest ze (`crates/burnburn-scenarios/tests/trace_holdout.rs`) |

Beleid: nooit hergenereren om een test groen te krijgen; een gewijzigd beeld vraagt een beoordeelde uitleg van de rasterizer
of de casewijziging. Beperking: de PNG's zijn door onze eigen rasterizer geschreven; onafhankelijk is de *vorm* (vectordefinitie
met de hand gekozen), niet de rasterisatie zelf.
