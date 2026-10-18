# Ontwikkelnotitie S-icons (`burnburn-icons`), 18-10-2026

Plan: `docs/plan/solidcraft-look.md`. Herkomst parser/rasterizer: SolidCraft `ui-egui/src/icon_svg.rs` (zelfde eigenaar, MIT OR Apache-2.0), zonder egui/accenten/kleurenblind-varianten; rollen komen uit een runtime-`Palette`.

## Rode fase (stub die altijd `Err` geeft; `CARGO_TARGET_DIR=target/icons cargo test -p burnburn-icons`)
Eerst compileerde de testsuite niet (ontbrekende API: geen geldige rode fase); daarna met een minimale stub:
`test result: FAILED. 2 passed; 12 failed` — gefaald o.a. `rect_icon_has_stroke_pixels_on_its_edges`,
`dark_theme_recolours_line_role`, `retina_scale_doubles_pixel_size`, `unknown_id_is_typed_error`,
`invalid_sizes_are_typed_errors`, `malformed_or_foreign_artwork_is_a_parse_error`, `palette_without_required_roles_is_rejected`.
(`every_registered_icon...` en `icon_ids_are_sorted...` slaagden vacuous op de lege stub-registry.)

## Groene fase
Implementatie (svg.rs, palette.rs, build.rs, lib.rs): `cargo test -p burnburn-icons` -> 14 passed. Twee testfouten onderweg
zijn testaannames, geen bug: tiny-skia geeft op een horizontale rand met 12 % dekking alpha 0 (verticale 4x-supersampling),
dus de straight-alpha-test gebruikt de cirkelrand (r = 24,125) i.p.v. rij 13 van de rechthoek. Registry leest `[tools."id"]`
uit het echte `icons.toml`; `missing = true` of lege `file` wordt niet geregistreerd (UnknownIcon).
Daarna `tool_ids_from_icons_toml_resolve_and_missing_artwork_is_unregistered` toegevoegd (direct groen: gedrag bestond al,
dus geen rode fase voor deze ene test). Eind: 15 passed, 72 SVG's uit `assets/icons/ui/svg` parsen en zijn niet leeg in licht en donker.

## Controles
- `cargo fmt -p burnburn-icons -- --check`: ok. `cargo clippy -p burnburn-icons --all-targets -- -D warnings`: schoon (integratietest heeft `#![allow(expect_used, panic, indexing_slicing)]` voor helpers).
- `cargo xtask layers`: 25 crates, geen overtredingen. `cargo xtask workspace-deps`: ok.
- Niet gedaan: native weergave (NSImage) hoort bij S-gui; kleurcontrast/ogen-review is handmatig.

## Ontwerpkeuzes
- `RgbaImage` is straight (niet-premultiplied) RGBA8; tiny-skia's premultiplied pixmap wordt gedemultiplied.
- Maat = `round(px * scale)` vierkant, max 2048; `viewBox` moet `0 0 N N` zijn (standaard 64).
- Accent-/icon-accents-secties van palette.toml worden genegeerd: alle iconen in basisrollen (active-* blauw).
