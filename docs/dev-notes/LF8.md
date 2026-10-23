# LF8: iconen (look-and-feel)

Basis `cb2166b`. Commando's met `CARGO_TARGET_DIR=/Users/tom/Sandbox/BurnBurn/target/lf8 CARGO_INCREMENTAL=0
BURNBURN_NATIVE_QUIET=1`. Screenshots/contactsheets alleen offscreen, in de scratchpad (`lf-lf8/before`, `lf-lf8/after`,
`lf-lf8/light-nl-10-laser-laservenster.png`).

## Beslissingen
- Streekset van de SVG-masters: **2,55** (detail, hulplijnen, gap-knip) en **4,25** (actie), altijd ronde caps en joins,
  viewBox 64, stippatronen `0.1 5` en `2.4 4.2`, coordinaten met hooguit twee decimalen op het doek. Gedeeld door de
  Windows-werkbalk (LF14): de SVG's zijn ongewijzigd platformonafhankelijk.
- Geharmoniseerd (waarden 1,2/1,5/2/2,2/2,8/3/3,6 -> 2,55; 3,2/3,4/3,5/5,6 -> 4,25): arc_center, arc_slot_center,
  auto_constrain, bb_clockwise, bb_counter_clockwise, bb_reverse, bb_start_point, delete, dxf, extend (stippen
  `0.1 5.6` -> `0.1 5`), fit, new, pan, pattern_circ, pattern_rect (0,92x geschaald: raakte de rand bij 16 pt @1x),
  polygon, polygon_circumscribed, slot_center, spline_cv, svg, zoom. `bb_double_star` was 0,9x geschaald (afgekapt bij 16 pt).
- `appkit/symbols.rs`: tabel `Symbol` (40 acties) met SF Symbol-naam, NL/EN-beschrijving en optionele terugval op een eigen
  master (`fallback_icon`); `apply` zet beeld, toegankelijke naam en tooltip, valt terug op de masters en dan op de
  beschrijving als tekst; `apply_with_title` voor knop met woord en pijl; `without_glyphs`; `set_symbol` voor twee-standen.
- Vervangen glyphs: laser (stap-pijlen, jogknoppen met woord + pijl, jobanker cirkel/gevuld), voorbeeldvenster (vorige/volgende),
  CAM-volgorde (omhoog/omlaag). De gedeelde host-models-teksten (`JogLeft` ... "◀ X−") zijn niet aangeraakt (Windows gebruikt ze
  nog); macOS filtert de glyph eruit.

## Red/green
| Test | Verwachte fout (red) | Red | Green |
|---|---|---|---|
| `burnburn-icons/tests/style.rs::every_master_uses_only_the_agreed_stroke_set` | 24 bestanden met andere breedtes | `cargo test -p burnburn-icons --locked --test style`: FAILED, 4 van 6 rood (strekenset, elementen/attributen door `height` in mask, coordinaten door mask `-32..96`, rendering `bb_double_star 16@1 touches the edge`); de twee laatste waren testfouten (mask hoort erbuiten) en zijn in de test gecorrigeerd | na SVG-wijzigingen 6/6 ok |
| `...::every_icon_renders_at_all_sizes_and_scales_with_the_right_pixel_size_and_content` | `circle_tt`/`pattern_rect 16@1` raken de rand (randpixel alpha 120 > 96) | idem; na de eerste fix bleef `pattern_rect` over (4 meldingen light+dark) | `pattern_rect` geschaald: ok |
| overig in `style.rs`: `every_master_uses_only_agreed_elements_and_attributes`, `stroked_shapes_always_use_round_caps_and_joins`, `path_coordinates_have_at_most_two_decimals_and_stay_on_the_canvas` | – (regressiewachters, waren al groen na de testcorrectie) | – | ok |
| native `symbols` (`cargo test -p burnburn-macos --test symbols --locked`) | stub `Symbol` met lege naam: "SF Symbol `` does not exist", ontbrekende beschrijving, 1 symbool; glyph-titels in laser (13 knoppen), voorbeeld (‹ ›), CAM-volgorde (↑ ↓), document (↔ ↕ ■ ▾), icoonknop zonder toegankelijke naam (bundelsheet) | FAILED, 2x (eerst 23 meldingen, na de tabel 6, na de checkbox-correctie in de test 4 = alleen LF7) | `test symbols ... ok` |
| `appkit::symbols::tests::*` (4) | geschreven bij de implementatie (niet eerst rood) | – | `cargo test -p burnburn-macos --lib --locked symbols`: 4 ok |

Eerlijk: de unit-tests in `symbols.rs` zijn naast de implementatie geschreven; de rode fase zit in de native test. De
terugvalroute (symbool ontbreekt) is niet met een echt ontbrekend symbool getest; alleen dat elke terugval-master bestaat.

## Open / onbevestigd
- `LF7_EXCEPTIONS` in `tests/native/macos/symbols.rs` bevat nog 4 paren (document: ↔ ↕ ■ ▾) in LF7-bestanden
  (`inspector.rs`, `operations.rs`, `toolstrip.rs`); wachtrij-regel staat erin; bij de integratie moet de lijst leeg.
- Template-image-kleur (accent/geselecteerd) is niet apart getest; handmatig te bekijken. De toolstrip-SVG's blijven gekleurde RGBA.
- Windows: geen wijziging in `burnburn-windows`; Segoe-koppeling hoort bij LF14.
