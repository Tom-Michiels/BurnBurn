# Geometriefixtures (IM09)

`boolean-cases.txt`: booleaanse gevallen met met de hand afgeleide oppervlaktes (formaat in de kop van het
bestand), gebruikt door `crates/burnburn-geometry/tests/boolean_fixtures.rs`. Manifest: `manifest.json`.

## Herkomst (audit W3-robuust, 2026-10-20)
- `boolean-cases.txt`: handgeschreven polygonen; de oppervlakten (mm²) zijn met de hand afgeleid uit de coordinaten
  (rechthoekoverlappen, uitsparingen) en niet uit `i_overlay` of de implementatie gelezen. Onafhankelijk: ja.
- `manifest.json`: grootte en sha256, gecontroleerd door `burnburn-test-support`. Niet automatisch bijwerken.
- Naast deze fixture bestaat in `crates/burnburn-geometry/tests/determinism.rs` een **opgenomen** golden (`GOLDEN`, bits van
  `arc_to_cubic`): die is op macOS arm64 vastgelegd en bewijst bitgelijkheid tussen platforms, niet juistheid
  (zie `docs/dev-notes/IM09.md`, DR-13).
