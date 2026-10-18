# SolidCraft-look, lane S-gui (18-10-2026)

Catalogus, werkbalk, intents; alles met `CARGO_TARGET_DIR=target/gui`.

## Cycli (rood → groen)
| Test | Rood (waargenomen) | Groen |
|---|---|---|
| `presentation catalog::tests::*` (15) | `--lib catalog`: 9 failed, 6 passed (stubs `enablement`/`build`; datatests direct groen, geen rode fase) | 15 passed |
| `presentation tests/toolbar.rs` | `--test toolbar`: 7 failed, 1 passed (intents genegeerd) | 8 passed (testfout `"ellipse"` → `"sketch.ellipse"` hersteld) |
| `macos tests/toolbar.rs` (teksten) | 8 failed, 1 passed (stubs) | 9 passed (orakel "Hoekafronding" uit tools.toml) |
| native `tool_strip` | FAILED "select has no visible image" (knoppen zonder beeld) | passed: beeld per id, tooltips, klik, variant 'center' via menu, donker/licht-wissel |
| `presentation tests/tool_steps.rs` | 3 failed (Rejected, Measured, PlaceText genegeerd), 1 vacuüm groen | 4 passed |
| `macos l10n all_message_keys_have_nl_and_en` | FAILED (ToolRejected zonder tekst) | passed |

## Eind
fmt, clippy `-D warnings` (presentation, macos, `--no-deps --all-targets`), tests presentation+macos groen,
`cargo check --target x86_64-pc-windows-msvc -p burnburn-windows` schoon. `finder_open` en `native_object_list`
faalden eenmaal tijdens een parallelle zware run (andere lanes bouwen mee) en slaagden apart en bij herhaling.
Handmatig: screenshot donker (systeem staat donker; licht niet gemaakt, thema-wissel alleen native getest).

## Open
- `sketch.centerline`, `sketch.point`: uitgeschakeld (document heeft geen construction-vlag/puntobject; `payload()` = None).
- `Deferred` ("Volgt"): mirror, patronen, collinear, auto-constrain. Constraint-/construction-tools: `NeedsSketch` (geen sketch-bewerkmodus in presentation, `in_sketch` = false).
- H-assets: `sketch.pen`, `sketch.polyline`, `sketch.ellipse.box` ontbreken in tools.toml/icons.toml (labels vallen terug op commandonamen); `node` heeft geen icoon (tekstknop).
- Multi-window: de strip volgt alleen het actieve document.
