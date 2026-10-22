# Lettertypefixtures

Herkomst-audit W3-robuust (2026-10-20). Geen bestand hier is door BurnBurn berekend; er staan geen verwachtingen in
dit mapje, alleen invoer. Niet automatisch vervangen: een andere fontversie wijzigt outlines en shaping.

| Bestand | Herkomst | Onafhankelijk van de implementatie? |
|---|---|---|
| `LiberationSans-Regular.ttf` | Ongewijzigde kopie van Liberation Sans 2.x (SIL OFL 1.1) uit het zusterproject `solidcraft`; sha256 in `manifest.json` | Ja, derde partij |
| `BIZUDPGothic-Regular.ttf` | Ongewijzigde kopie van BIZ UDPGothic (SIL OFL 1.1) uit `solidcraft`; sha256 in `manifest.json` | Ja, derde partij |
| `DejaVuSans.ttf` | Ongewijzigde kopie van DejaVu Sans (Bitstream Vera-licentie met publiek-domein-wijzigingen; permissief) uit het matplotlib-pakket dat al op deze machine staat (`mpl-data/fonts/ttf/`); niet gedownload; sha256 in `manifest.json`. Arabisch testfont (G7-trace): GSUB-vormen isol/init/medi/fina en lam-alef-ligatuur | Ja, derde partij |
| `MuktaMahee.ttc` | Ongewijzigde kopie van Mukta Mahee (Ek Type, SIL OFL 1.1) uit `/System/Library/Fonts/` op deze machine (macOS levert het mee; de name table noemt OFL 1.1 en copyright 2017 Ek Type); niet gedownload; sha256 in `manifest.json`. Fontcollectie van 7 gewichten, eerste gezicht = Regular. Indisch testfont (G8-trace): Devanagari en Gurmukhi met de Indic-regels (matra vóór de medeklinker, conjuncten, reph) | Ja, derde partij |
| `MuktaMahee-OFL.txt` | Licentietekst, samengesteld: copyright en licentieverklaring uit de name table (ID 0, 13, 14) plus de standaard OFL 1.1-tekst zoals in `LiberationSans-OFL.txt`; de originele `OFL.txt` van Ek Type staat niet op deze machine | n.v.t. |
| `DejaVuSans-LICENSE.txt` | Licentietekst (`LICENSE_DEJAVU` van matplotlib) bij DejaVu Sans | n.v.t. |
| `*-OFL.txt` | Licentieteksten bij de fonts | n.v.t. |
| `reference_metrics.py` | Onderzoekshulpmiddel: leest `head`/`cmap`/`hmtx`/`glyf` met alleen `struct` (geen fontTools, geen skrifa). Geen runtime-dependency | Ja: oracle voor de metrics in `crates/burnburn-text/tests/` |

Gebruik: `python3 -I reference_metrics.py LiberationSans-Regular.ttf "HAVfio"` geeft units-per-em, glyph-id,
advance en bounding box per teken; de verwachte waarden in de teksttests komen daaruit of uit de fontspecificatie.
