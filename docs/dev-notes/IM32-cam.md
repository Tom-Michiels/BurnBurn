# IM32 CAM-deel: gedeelde grenzen (rol CAM/job)

Status: gereed op macOS arm64. Common-line cutting zelf (C12, kerf/volgorde-correctie) is **niet** gebouwd: alleen
`Feature::CommonLine` (standaard `Unverified`).

## Contract (vastgelegd, ter bevestiging bij Design in `cam.md`)
- `AdvancedOptions.shared_edges: SharedEdgePolicy::{CutOnce (default), CutPerRegion}` per Line-operation.
- `CutOnce`: een segment dat binnen dezelfde pass al door een **andere** target-geometrie is gesneden (exact dezelfde
  eindpunten, bitgelijk, beide richtingen) wordt laser-uit travel. Geen tolerantie: bijna-samenvallende randen worden
  niet samengevoegd. Preflight geeft dan `Warning SharedBoundaryCutOnce { operation, geometries }` (zichtbaar default).
- `CutPerRegion`: expliciete keuze, beide snijden, geen waarschuwing.
- Keuze t.o.v. de opdracht ("waarschuwing en beide snijden, of één"): **één keer snijden + waarschuwing**, omdat
  contracts-phase-c (default `CutOnce`) en de testnaam dat eisen; twee keer snijden verbrandt de grens.

## Cycli

| Test | Verwachte fout (red) | Red-commando en waargenomen resultaat | Green-commando en resultaat | Refactor |
|---|---|---|---|---|
| `cam::common_line::shared_colour_boundary_is_not_scheduled_twice_without_explicit_policy` | stub: dedupe uit, finding-variant zonder detectie | `--test common_line` → FAILED `cut length 80` (verwacht 70 = 40 + 40 − 10) | dedupe + `shared_boundaries` in preflight → 3 passed. De finding-assertie is niet apart rood gezien (eerste assertie faalde eerder) | geen |
| `…::explicit_cut_per_region_policy_cuts_the_shared_boundary_for_each_region`, `…::nearly_coincident_edges_are_not_merged` | — | **geen rode fase** (toetsen het ongewijzigde pad; slaagden tegen de stub) | 3 passed | geen |

## Open
- Preflight vergelijkt lijnen en kubische segmenten exact; bogen niet (compile dedupliceert wel na flattening).
- Regio's uit `burnburn-trace::colour` (RegionGraph) komen nog niet als targets binnen; de test gebruikt twee
  aangrenzende rechthoeken als stand-in.
