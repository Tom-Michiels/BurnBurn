# Lettertypen in de app

- `LiberationSans-Regular.ttf`: ongewijzigde kopie van Liberation Sans 2.x (SIL OFL 1.1, tekst in
  `LiberationSans-OFL.txt`), afkomstig uit `test-data/fonts/` (zelfde bytes, sha256 in `test-data/fonts/manifest.json`).
  Gebundeld met `include_bytes!` in `crates/burnburn-render/src/scene/text.rs` voor canvastekst (maatlabels,
  linialen, snaplabels). Niets is gedownload.
