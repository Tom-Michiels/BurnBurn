# IM32 Kleurtracing (trace-deel)

Status: groen op macOS arm64, headless. Windows: onbevestigd. CAM-deel (niet dubbel snijden, common-line) en
AutoConstrain zijn van CAM resp. Sketch.
Rol: Implementatie (beeld/trace). Crate: `burnburn-trace`, module `colour`.

## Opgeleverd
- `quantize(&SourceImage, k, seed, cancel) -> Palette`: histogram van RGB (over wit gecomponeerd), k-means++-
  initialisatie met SplitMix64 uit `seed` (rejection sampling, u128-gewichten), Lloyd met integer-sommen en
  afgeronde integer-gemiddelden, gelijke afstand → laagste index, max. 100 iteraties; palet gesorteerd + ontdubbeld.
  ≤ k unieke kleuren → precies die kleuren. `k = 0` → `InvalidRecipe`.
- `colour_regions(&ColourRequest, cancel) -> RegionGraph`: pixels → dichtstbijzijnde paletkleur → 4-samenhangende
  regio's (raster-volgorde ids). Grenzen = crack-edges van het pixelrooster tussen verschillende regio's (of regio en
  buitenkant); knooppunten = roosterpunten met ≥ 3 grensranden. Elke maximale keten is **één** `BoundaryEdge
  { id, left, right: Option, path }` (left links in beeldweergave; buitenkant altijd `right = None`), één keer
  gefit (open: eindpunten exact op het roosterpunt, middens van de cracks als data; gesloten: hoekbewuste
  `fit_closed`). Regio's verwijzen met `BoundaryUse { boundary, reversed }` per ring (`rings`); `region_path`
  stelt de omtrek samen uit dezelfde segmenten (omgekeerd voor de buur).
- `SharedEdgePolicy { CutOnce (default), CutPerRegion }` als overdrachtstype voor CAM; `ColourRequest::key()`
  (engine `burnburn-trace/colour-1`, bron, schaal, k, seed, tolerantie).

## Cycli
Commando's met `export PATH="$HOME/.cargo/bin:$PATH" CARGO_TARGET_DIR=target/trace`, `--offline`;
runs `cargo test -p burnburn-trace --offline --test colour`.

| Test | Verwachte fout (red) | Red-resultaat | Green-resultaat | Refactor |
|---|---|---|---|---|
| `shared_colour_boundary_is_stored_once_in_the_region_topology` | stub: leeg palet, geen regio's | FAILED `exact colours of a hard-edged image left: {} right: {[30, 40, 200], …}` | passed: rood\|blauw precies één grens (x = 6 mm, 1–7 mm), beide regio's gebruiken die id in tegengestelde richting en hun omtrek bevat exact de knopen ervan; eiland één gesloten grens; één buitengrens | clippy-opruiming (`is_multiple_of`, iterator) — groen |
| `quantization_is_deterministic_for_a_fixed_seed_and_regions_are_reproducible` | stub | FAILED `left: 0 right: 3` (paletgrootte) | passed (AA-beeld: palet tweemaal gelijk en nabij wit/rood/blauw, graaf tweemaal gelijk, annulering, `k = 0`) | — |

## Verificatie (alle vier pakketten samen, eindstand)
- `cargo fmt -p burnburn-image -p burnburn-trace -- --check` → schoon.
- `cargo clippy -p burnburn-image -p burnburn-trace --all-targets --offline -- -D warnings` → schoon (ook zonder
  `--no-deps`; het geometry-probleem uit IM12 is weg). Geen `-A` op de commandoregel; testbestanden gebruiken het
  bestaande `#![allow(clippy::unwrap_used)]`.
- `cargo test -p burnburn-image -p burnburn-trace --offline` → image decode 5, pipeline 9, source 4 (+ `raster.rs` 9, niet van mij);
  trace lib 1, centerline 8, colour 2, contour 5, corpus 1, correction 5, recipes 5, task 7 (+1 ignored), holdout 0 (+1 ignored).
- `cargo test -p burnburn-presentation --offline` → groen (afnemer van de gewijzigde typen).
- `cargo xtask layers` → `23 crates, 2 triples, no violations`.
- Benchmark (release, ignored): `2000×2000 px: 313 contours, 626 segments in 78.35925ms` (IM12: 74 ms; één meting).

## Onbevestigd / open
- Windows. Uitsnede/rotatie voor kleurtracing (nu het hele bronbeeld met expliciete schaal). Hoekbehoud op open
  grensketens (rechte hoeken worden binnen de tolerantie afgerond; gesloten grenzen zijn hoekbewust).
- Antialiasranden kunnen 1-px-regio's geven; een minimale regiogrootte met samenvoegen naar de buur volgt.
