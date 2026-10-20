# Ruida RD-fixtures

Ongewijzigde kopieën uit `research/lightburn/fixtures/` (LightBurn Core 2.1.04, native export) met hun
bronbestanden in `sources/`. Herkomst, grootte, SHA-256 en exportcondities staan in `manifest.json`;
`burnburn-test-support` (test `ruida_fixture_hashes_match_manifest`) controleert ze, en
`ruida_fixture_manifest_agrees_with_research_evidence` vergelijkt met `research/lightburn/evidence/fixtures-manifest.json`.

`oracle/` bevat de uitvoer van de **onafhankelijke Python-onderzoeksdecoder** (`research/lightburn/tools/rd_inspect.py --json`).
Dat is een ontwikkeloracle om de Rust-decoder tegen te vergelijken, **geen runtime-dependency**: BurnBurn bouwt en draait
zonder Python. De recordsplitsing is een onderzoekshulpmiddel, geen volledige grammatica.

Gebruik in tests: `burnburn_test_support::fixtures::read_ruida_fixture("rectangle-default.rd")` (controleert de hash vóór
teruggeven). Bytes of hashes nooit automatisch bijwerken; een gewijzigde verwachting krijgt een inhoudelijke review.
`rectangle-fill_backup.lbrn2` uit het onderzoek is bewust niet overgenomen (automatische LightBurn-backup, geen exportbron).

## Herkomst (audit W3-robuust, 2026-10-20)
| Bestand | Herkomst | Onafhankelijk? |
|---|---|---|
| `rectangle-default.rd`, `rectangle-fill.rd`, `gray-ramp-grayscale.rd` | Native RD-export van LightBurn Core 2.1.04 (`research/lightburn/fixtures/`); bytes identiek aan de onderzoekskopie (`cmp`) | Ja: uitvoer van de referentietoepassing, niet van BurnBurn |
| `sources/*` | Bronbestanden van die exports (SVG, PNG, LBRN2); exportcondities in `manifest.json` | Ja |
| `oracle/*-decoded.json` | Uitvoer van `research/lightburn/tools/rd_inspect.py --json` (Python, geen runtime-dependency) | Ja: aparte decoder. Op 2026-10-20 opnieuw gedraaid op de drie `.rd`-bestanden; de JSON werd reproduceerbaar teruggevonden (zonder padvelden) |
Beperking: de Python-decoder is een onderzoekshulpmiddel en geen volledige grammatica; onbekende records blijven `UNKNOWN`.
Er is hier geen golden die uit de Rust-encoder komt: `crates/burnburn-ruida/tests/lower.rs` vergelijkt de lowering met deze
LightBurn-bestanden en meldt verschillen in `docs/dev-notes/IM15.md`.
