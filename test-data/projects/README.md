# Projectcontainer-fixtures

Eén of meer `.burnburn`-bestanden per uitgebracht schema (ADR 0004). Ze worden **nooit** herschreven: zij zijn de
invoer waartegen migraties (`v1 → v2 → …`) getest worden. Herkomst, grootte, SHA-256 en inhoud staan in
`manifest.json`; `crates/burnburn-io/tests/migration_fixtures.rs` controleert de hash en de met de hand
uitgeschreven verwachtingen (geometrie, ID's, operations).

- `v1/rectangle-120x80.burnburn` — poort A-rechthoek (120 × 80 mm) met één placeholder-operation.
  Operations zijn tot IM13 een placeholder (`id`, `name`, `targets`).

Een nieuwe fixture: schrijf hem met een `#[ignore]`-test, controleer `manifest.json`/`document.json` inhoudelijk
(bijv. `unzip -p bestand document.json`), noteer de SHA-256 en schrijf de verwachtingen met de hand in de test.

## Herkomst (audit W3-robuust, 2026-10-20)
- `v1/rectangle-120x80.burnburn` en `v2/plate-operations.burnburn` zijn **door BurnBurn zelf geschreven** (ignored schrijftests, zie
  `manifest.json`) en daarna met de hand nagelezen. Het zijn bevroren *migratie-invoer*, geen onafhankelijk orakel van de
  schrijfroute: ze bewijzen dat oude bestanden blijven laden. De verwachtingen (geometrie, ID's, operations) staan met de hand
  uitgeschreven in `migration_fixtures.rs` en `crates/burnburn-scenarios/tests/project_fixtures.rs`.
- Hash-goldens van projecten: zie `docs/dev-notes/IM01-testing.md` (W3-robuust) voor welke met de hand berekend zijn en welke opgenomen.
