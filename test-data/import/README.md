# Importfixtures (IM10)

Handgeschreven SVG- en DXF-bestanden met analytisch bekende inhoud. De verwachtingen staan in
`crates/burnburn-io/tests/import.rs` en zijn met de hand afgeleid; `manifest.json` bevat grootte en SHA-256.
De bestanden zijn ook seeds voor `crates/burnburn-io/tests/parser_robustness.rs` (mutaties mogen nooit panieken).
Niet automatisch regenereren.

## W2-L6
- `square-circle.plt` — HP-GL (plotter-eenheden 0,025 mm): vierkant, cirkel, lijn + boog; verwachtingen in `tests/hpgl.rs`.
- `plate-layers.pdf` — A4-PDF met optionele-inhoudslagen (Cut aan, Guide uit), rechthoek + cirkel (vier Béziers) en tekst; verwachtingen in `tests/pdf.rs`.
Beide staan ook in `manifest.json` en zijn seeds voor `tests/robustness_w2.rs` en de fuzzdoelen `io_parse_hpgl` / `io_parse_pdf`.
