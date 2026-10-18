# SolidCraft-look — lane S-modify (18-10-2026)

Status: headless groen op macOS arm64. Windows: onbevestigd (geen runner). Native AppKit-koppeling (catalogus →
`ToolKind`) is van S-gui; hier niet gedaan. Commando's met
`export PATH="$HOME/.cargo/bin:$PATH" CARGO_TARGET_DIR=target/modify` vanuit de repo-root.
Gedrag nagebouwd naar `solidcraft/crates/engine/src/cmd/sketch_{modify,create,constraints}.rs` (zelfde eigenaar); geen code
gekopieerd, wel de semantiek (conic `rho`, chamfer afstand+hoek via sinusregel, constructie als vlag).

## Opgeleverd
- **burnburn-geometry** (nieuw): `Path::cut_corner(pick, reach, CornerCut::{Fillet, ChamferEqual, ChamferTwo, ChamferAngle})`,
  `Path::trim_at` (lijn, boog, Bézier; gesloten contour heeft twee snijpunten nodig), `Path::extend_end` (lijn én cirkel-/ellipsboog),
  `Path::break_at`, `Path::spline_fit`, `Path::spline_control` (geklemde uniforme kubische B-spline; 4 punten = exact één Bézier;
  gesloten = periodiek), `Path::conic(start, end, apex, rho)` (acht kubische stukken), `GeometryError::InvalidParameter`,
  `ExtendError::NotExtendable`. Bestanden `corner.rs`, `cutting.rs`, `spline.rs`, `ring.rs`.
- **burnburn-document** (kleine, gerichte edits): `GeometryCommand::{CutCorner, TrimAt, ExtendEnd, BreakAt}` (+ `EditLabel::{CutCorner, Break}`,
  `EditError::{Corner, Break}`), `SketchCommand::SetConstruction` (+ `SketchLabel`, `Sketch::set_construction`).
- **burnburn-editor**: `tool/{modify,spline,text,dimension,fillet,chamfer,trim,extend,break_tool,offset,mirror,move_scale,construction,pattern,constraint,sketch_pick}.rs`;
  31 `ToolKind`-varianten, `ModifyTool` (één `Active`-variant), `ToolStep::{Measured, Rejected}`, `Rejection`/`Unavailable`,
  `ToolParam::Modify(ModifyParam)`, nieuwe `EditCommit`-varianten (`CutCorner`, `Trim`, `Extend`, `Break`, `Mirror`, `AddArray`, `Sketch`, `PlaceText`).
- **burnburn-presentation** `commit.rs`: `payload()` mapt `CutCorner/Trim/Extend/Break/Mirror(in place)/Sketch(1 commando)/AddArray(1 doel)`.

## Cycli (rood waargenomen)
| Test(s) | Rode fase | Groen |
|---|---|---|
| `burnburn-geometry/tests/modify.rs` (21: fillet-raakpunten en middelpunt (5,5), draairichting, scherpe hoek, rechthoek-oppervlak per hoek incl. startpunt, chamfer-driehoeken/sinusregel, typed errors, proptest oppervlak+insluiting, trim cirkel/boog/Bézier/lijn-gelijk-aan-`trim_line`, extend boog (126,87°-snijpunt, start-uiteinde), break open/gesloten, spline door punten + C1, Cox–de Boor-referentie, periodieke knoopwaarden, conic parabool/cirkel) | stubs `Err(...)`: `cargo test -p burnburn-geometry --test modify` → **21 failed, 0 passed** (gedragsmatig: verkeerde `Err`/`unwrap` op stub) | zelfde commando → 21 passed. Mutaties: draairichting omgedraaid → 4 FAILED; `last - m` → `first - m` bij trim → 1 FAILED; teruggezet |
| `burnburn-document/tests/modify_commands.rs` (6: één undo-stap per bewerking, geweigerde bewerking verandert niets, constructievlag + undo, onbekende entiteit) | stubs `NothingChanged`/`InvalidValue`: `cargo test -p burnburn-document --test modify_commands` → **6 failed, 0 passed** | 6 passed; hele `-p burnburn-document` groen |
| `burnburn-editor` tool-tests (70 gedrag: spline/conic, tekst, dimensie, fillet/chamfer, trim/extend/break, offset, mirror, move/scale, constructie, patroon, 11 constraints + auto, sketch-pick) | `ModifyTool` met `input → Nothing`: `cargo test -p burnburn-editor` → **70 failed, 127 passed** | 197 passed. Mutaties: horizontaal/verticaal omgekeerd → FAILED; trim snijdt zichzelf → 2 FAILED; spiegelmatrix `cos 2θ` teken → 2 FAILED; punt/kromme-voorkeur omgedraaid → 4 FAILED; teruggezet |
| `modify::tests::{every_tool_id_maps_to_its_own_kind, switching_away_…}` | **geen rode fase** (types/id-tabel stonden er al, gesplitst van de stub) | groen |
| `presentation::commit::tests` (3 mapping-tests) | `payload → None`: 3 FAILED | 3 passed |
| `presentation::commit::tests::end_to_end::{fillet…, trim…, construction…}` | **geen rode fase** (na de mapping geschreven; echte `Editor` + `DocumentSession` + `SketchEvaluator`) | 3 passed; fillet-oppervlak `100 − (4−π)`, undo herstelt 100 |

Eind: `cargo fmt` (mijn bestanden + `--check` op de vijf crates) schoon; `cargo clippy -p burnburn-editor -p burnburn-geometry -p burnburn-sketch -p burnburn-document -p burnburn-presentation --all-targets -- -D warnings` zonder `-A` schoon; `cargo test` van dezelfde vijf crates groen;
`cargo run -p xtask -- layers` → `25 crates, 2 triples, no violations`. `burnburn-macos`/`-windows` niet gebouwd of getest door deze lane.

## Gedrag en bewuste keuzes
- Eén commit per gebaar; Esc, pointer-cancel en toolwissel annuleren zonder commit. Een geweigerd gebaar geeft `ToolStep::Rejected(Rejection)` met typed reden, laat document en tool schoon achter.
- **Niet beschikbaar (typed, geen stille fallback):** fillet/chamfer op een lijn–boog-hoek (`Unavailable::CurvedCorner`), extend van een Bézier-uiteinde
  (`Unavailable::BezierExtend`), constructie op getekende objecten (`Unavailable::ConstructionOutsideSketch`: de vlag bestaat alleen voor sketch-entiteiten).
- Trim/extend/break rekenen in de editor (preview = commit-`path`), het document past dezelfde `GeometryCommand` opnieuw toe (zoals `EditNodes`); het preview-pad is geen vertrouwde invoer.
- Offset maakt een nieuw object (`CreatePath`, Miter, 0,01 mm tolerantie); zijde door de wijzer, afstand door de wijzer of getypt (`exact_size`).
- Dimensie: lijn → lengte, cirkel → diameter, boog → straal, twee punten → afstand (`"<waarde> mm"`, `Driving`); buiten een sketch (geen entiteit onder de wijzer) twee klikken → `ToolStep::Measured`.
  Hoekdimensie tussen twee lijnen ontbreekt nog (niet getypeerd aangeboden: gewoon geen gebaar).
- Constraint-tools kiezen over alle sketches van het document (geen "actieve sketch" in `ToolContext`); picks uit verschillende sketches → `InvalidSelection`.
  Collinear = `Parallel` + `PointOn(begin van lijn 2, lijn 1)`. Auto-constrain: horizontaal/verticaal binnen 1°, coïncident binnen 0,05 mm, wat de sketch al heeft wordt overgeslagen.
- Instellingen via `Editor::set_param(ToolParam::Modify(..))` (fillet-straal, chamferafstanden/-hoek, conic-rho, patroonaantallen, schaalfactor, behoud origineel). Standaardwaarden: straal/afstand 2 mm, hoek 45°, rho 0,5, raster 3×3, cirkel 6 stuks 360°.

## Open / onbevestigd
- **Atomiciteit:** `EditCommit::Sketch` met meerdere commando's (collinear, auto-constrain) en `AddArray` met meerdere doelen hebben geen enkel document-commando; `commit.rs` past ze niet toe (geeft `None`) i.p.v. meerdere undo-stappen te maken. Nodig: `CommandPayload`-batch of `SketchCommand::AddConstraints` en een ketting met meerdere bronnen (Kern).
- **Gespiegelde kopieën** (`keep_original: true`) wachten op een duplicate-commando (Kern).
- **Tekst:** `PlaceText { at }` moet door presentation onthouden worden als anker voor de eerstvolgende `UiIntent::InsertText`; dat is nog niet gekoppeld (S-gui/Presentation).
- Catalogus (`catalog.rs`) heeft deze ids nog als `Unavailable`; mapping staat in `docs/coordination/userflow.md`. Het bestaande `trim_mode` blijft een apart commando tot S-gui kiest.
- Native AppKit-gedrag (toetsen, tekstinvoer in het canvas, focus) is hier niet geraakt en dus niet getest; geen handmatige acceptatie uitgevoerd.
