# Plan: SolidCraft-look-and-feel en volledige sketch-tekenset (18-10-2026)

Doel: de BurnBurn-werkbalk en tekentools werken zoals in SolidCraft (`../solidcraft`): een werkbalk met **iconen per
commando**, gegroepeerd in panelen (MAKEN, WIJZIGEN, PATROON, BEPERKINGEN, INVOEGEN, SELECTEREN), varianten via een
dropdown op hetzelfde icoon (zoals de rechthoekvarianten), tooltips met sneltoets, licht/donker-thema, en **alle
tekenelementen die SolidCraft voor sketches heeft**. BurnBurn blijft native AppKit: geen egui, wel dezelfde aanpak
(eigen SVG-masters met kleurrollen, gerasterd in Rust met tiny-skia, één commandoregister dat werkbalk, menu en sneltoetsen voedt).

## Wat we van SolidCraft overnemen (MIT OR Apache-2.0, zelfde eigenaar; herkomst in `assets/icons/ATTRIBUTION.md`)

| SolidCraft | BurnBurn |
|---|---|
| `assets/icons/ui/design/svg/model/*.svg` (sketch-iconen) + `flat/*.svg` | `assets/icons/ui/svg/<id>.svg` (ongewijzigd gekopieerd) |
| `assets/icons/ui/design/palette.toml` (kleurrollen licht/donker) | `assets/icons/ui/palette.toml` |
| `crates/ui-egui/src/icon_svg.rs` (SVG-subset + tiny-skia, rolrecolouring) | `crates/burnburn-icons` (aangepast, zonder egui) |
| `CommandSpec` (id, label, tab/panel, icon, key, enabled) | `burnburn-presentation::catalog::ToolSpec` |
| `toolbar.rs` panelen per tab, `promoted` | AppKit-werkbalk met panelen en variant-dropdowns |

## Sketch-tekenset (SolidCraft-ids → BurnBurn-tool-id)

**Maken:** `sketch.line`, `sketch.line.midpoint`, `sketch.centerline`, `sketch.rectangle.{two_point,center,three_point,rounded}`,
`sketch.circle.{center,two_point,three_point,two_tangent,three_tangent}`, `sketch.arc.{three_point,center_point,tangent}`,
`sketch.ellipse`, `sketch.polygon.{inscribed,circumscribed,edge}`, `sketch.slot.{center_to_center,overall,center_point,arc_three_point,arc_center}`,
`sketch.spline.{fit_point,control_point}`, `sketch.conic`, `sketch.point`, `sketch.text`, `sketch.dimension`.
**Wijzigen:** `sketch.fillet`, `sketch.chamfer.{equal_distance,two_distance,distance_angle}`, `sketch.trim`, `sketch.extend`,
`sketch.break`, `sketch.offset`, `sketch.mirror`, `sketch.move`, `sketch.scale`, `sketch.construction`.
**Patroon:** `sketch.pattern.{rectangular,circular}`.
**Beperkingen:** `sketch.constraint.{coincident,collinear,concentric,equal,fix,horizontal_vertical,midpoint,parallel,perpendicular,symmetry,tangent}`, `sketch.auto_constrain`.
**Invoegen:** `sketch.insert_svg`, `sketch.insert_dxf` (bestaande importroute).
**Selecteren/weergave (flat):** select, node, pan, fit, undo/redo, save.

Niet overgenomen (3D-specifiek): `line3d`, `spline3d`, `project`, `include_3d`, `intersection_curve`, `isoparametric_curve`, `spun_profile`.
Tools waarvan het domein nog ontbreekt, staan wel in de catalogus maar **uitgeschakeld met reden** ("nog niet beschikbaar") — nooit een knop die stil iets anders doet.

## Lanes (deze golf)

| Lane | Model | Eigen paden | Levert |
|---|---|---|---|
| **S-icons** | Sonnet | `crates/burnburn-icons/` (nieuw), `docs/design/layers.toml` (1 regel), root `Cargo.toml` (deps-regels) | SVG-subset-parser + tiny-skia-rasterizer, rolrecolouring licht/donker, `render(id, px, scale, Theme) -> Option<RgbaImage>`, build.rs die alle `assets/icons/ui/svg/*.svg` embedt; tests: elk icoon parseert, niet-leeg, donker ≠ licht |
| **H-assets** | Haiku | `assets/icons/`, `assets/l10n/tools.toml` | SVG's + palette kopiëren, `ATTRIBUTION.md`, `assets/icons/ui/icons.toml` (id → bestand, categorie), NL/EN-labels en tooltips voor alle tool-ids |
| **S-draw** | Sonnet | `crates/burnburn-editor/src/tool/{line,rect,circle,arc,ellipse,polygon,slot,point}.rs`, `crates/burnburn-geometry` (nieuwe constructiefuncties) | State machines voor alle **Maken**-tools behalve spline/conic/text/dimension; elk één `EditCommit` per gebaar, Esc annuleert |
| **S-modify** | Sonnet | `crates/burnburn-editor/src/tool/{spline,text,fillet,chamfer,trim,extend,offset,mirror,pattern,construction}.rs`, `crates/burnburn-sketch` | Spline/conic/text/dimension-tools, alle **Wijzigen**/**Patroon**-tools, constraint-tools via bestaande sketch-API |
| **S-gui** | Sonnet | `crates/burnburn-presentation/src/catalog.rs` (+ `lib.rs`-regel, `intent.rs`/`view_model.rs` kleine edits), `crates/burnburn-macos/` | `ToolCatalog` (één register: id, panel, icon, sneltoets, enabled+reden), werkbalk met panelen, iconknoppen, variant-dropdown, tooltips, licht/donker (NSAppearance-wissel → iconen opnieuw renderen), NSImage uit RGBA |

Gedeeld: `crates/burnburn-editor/src/tool/mod.rs` (`ToolKind`-enum en dispatch) — S-draw en S-modify voegen elk alleen hun
eigen varianten toe met kleine gerichte edits; bij "file modified" opnieuw lezen. S-gui verwijst naar tools via de catalogus-id;
een id zonder `ToolKind` is uitgeschakeld met reden tot de tool bestaat.

## Regels
- TDD volgens AGENTS.md; rode fase loggen in `docs/dev-notes/solidcraft-look-<lane>.md`. Tests op publiek gedrag
  (geometrie van een gecommit object, catalogus-enablement, gerenderde pixels), niet op interne formules.
- Eigen `CARGO_TARGET_DIR=target/<lane>`; tijdens het werk `cargo test -p <crate>`; eind: fmt, clippy -D warnings zonder -A op eigen crates.
- Werk in de hoofdcheckout zonder commits; raak de `.claude/worktrees/*`-lanes niet aan.
- Overlap met het pariteitsplan: W1-L2b (vormen) en L1 (UI) werken in dezelfde crates op aparte branches; die branches
  moeten bij integratie op deze golf worden gerebased (genoteerd in `integratie-wachtrij.md`).
