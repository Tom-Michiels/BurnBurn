# Barcode-fixtures

Herkomst-audit W3-robuust (2026-10-20). Alle bestanden zijn **uitvoer van een andere implementatie dan de encoder die ze
toetst** (of van de officiële bron); BurnBurn rekent er niets van uit. Niet automatisch regenereren: een wijziging vraagt
een beoordeelde uitleg.

| Bestand | Herkomst | Gebruikt door | Orakel |
|---|---|---|---|
| `qr-coreimage.txt` | Apple CoreImage `CIQRCodeGenerator` (macOS), gegenereerd door `research/qr-reference/reference.swift` | `tests/qr.rs` | Onafhankelijke encoder van het besturingssysteem |
| `barcode-bwipp.txt` | `bwipp-rs` 0.1.1 (Rust-port van BWIPP), gegenereerd door `research/barcode-reference` (eigen `[workspace]`, niet in de app) | `tests/symbology_reference.rs` | Tweede encoder (anyd is de productie-encoder); 2 of 5 Industrial/IATA, Plessey, PDF417 (+Compact), MicroPDF417 |
| `fiducial-reference.txt` | Officiële AprilTag C-bronnen (`apriltag-sys` 0.4.0) en OpenCV 7x7-woordenboek zoals `locus-core` 0.10.0 het uitpakt | `tests/symbology_reference.rs` | AprilTag: officiële bron. ArUco 7x7: tweede extractie van dezelfde OpenCV-bron (zwakker, zie IM22.md) |
| `zint-vectors.txt` | Testsuite van zint (libzint, BSD-3-Clause), `backend/tests/test_hanxin.c` en `test_2of5.c`, commit `f5fd981`; verwachte modules ongewijzigd overgenomen, kop in het bestand | `tests/symbology_zint.rs` | Han Xin versie 1-3 (inclusief het ISO/IEC 20830-figuur 1) en Standaard/Matrix 2 of 5; zint is een andere codebasis dan anyd |

Bekende beperkingen staan in `docs/dev-notes/IM22.md` (o.a. geen fysieke leesproef; Han Xin alleen versie 1-3 met drie vectoren;
Grid Matrix en ArUco MIP 36h11 niet geleverd).
