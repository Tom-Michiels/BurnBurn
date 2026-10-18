# Integratiewachtrij (domeinlanes → UI-lane L1)

Eén regel per feature die domeinmatig klaar is en een UI-route nodig heeft. Alleen toevoegen (append), nooit
andermans regels wijzigen. L1 zet de status op `opgepakt` / `klaar` met de testnaam.

| Pakket | Feature | Domein-API (crate::pad) | Gewenste intent / viewmodel | Story/test | Status |
|---|---|---|---|---|---|

## Let op: golf "SolidCraft-look" (18-10-2026)
Plan `docs/plan/solidcraft-look.md` werkt in de hoofdcheckout in `burnburn-editor/src/tool/`, `burnburn-geometry`,
`burnburn-sketch`, `burnburn-presentation` (nieuw `catalog.rs`), `burnburn-macos` (werkbalk) en een nieuwe crate
`burnburn-icons`. Lane-branches L1 en W1-L2b (vormen) overlappen en moeten daarop gerebased worden vóór merge.
