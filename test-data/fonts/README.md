# Lettertypefixtures

Herkomst-audit W3-robuust (2026-10-20). Geen bestand hier is door BurnBurn berekend; er staan geen verwachtingen in
dit mapje, alleen invoer. Niet automatisch vervangen: een andere fontversie wijzigt outlines en shaping.

| Bestand | Herkomst | Onafhankelijk van de implementatie? |
|---|---|---|
| `LiberationSans-Regular.ttf` | Ongewijzigde kopie van Liberation Sans 2.x (SIL OFL 1.1) uit het zusterproject `solidcraft`; sha256 in `manifest.json` | Ja, derde partij |
| `BIZUDPGothic-Regular.ttf` | Ongewijzigde kopie van BIZ UDPGothic (SIL OFL 1.1) uit `solidcraft`; sha256 in `manifest.json` | Ja, derde partij |
| `*-OFL.txt` | Licentieteksten bij de fonts | n.v.t. |
| `reference_metrics.py` | Onderzoekshulpmiddel: leest `head`/`cmap`/`hmtx`/`glyf` met alleen `struct` (geen fontTools, geen skrifa). Geen runtime-dependency | Ja: oracle voor de metrics in `crates/burnburn-text/tests/` |

Gebruik: `python3 -I reference_metrics.py LiberationSans-Regular.ttf "HAVfio"` geeft units-per-em, glyph-id,
advance en bounding box per teken; de verwachte waarden in de teksttests komen daaruit of uit de fontspecificatie.
