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
