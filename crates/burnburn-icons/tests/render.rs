//! Behaviour of the icon renderer, checked on analytic pixel positions of the 64-unit masters.

#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)] // test helpers outside #[test] fns

use burnburn_icons::{IconError, Palette, RgbaImage, Theme, has_icon, icon_ids, render, render_svg};

const PALETTE: &str = include_str!("data/palette.toml");
const CIRCLE: &str = include_str!("data/circle.svg");
const RECT: &str = include_str!("data/rect.svg");
const UNDO: &str = include_str!("data/undo-flat.svg");

// Expected colours are written out from palette.toml (an independent oracle, not read back).
const ACTIVE_3_LIGHT: [u8; 3] = [0x23, 0x6e, 0xad];
const ACTIVE_3_DARK: [u8; 3] = [0x29, 0x76, 0xb8];
const ACTIVE_1_LIGHT: [u8; 3] = [0xb9, 0xdd, 0xf8];
const LINE_LIGHT: [u8; 3] = [0x34, 0x46, 0x5b];
const LINE_DARK: [u8; 3] = [0xdb, 0xe4, 0xee];

fn palette() -> Palette {
    Palette::parse(PALETTE).expect("test palette parses")
}

fn px(img: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * img.width + x) * 4) as usize;
    [img.pixels[i], img.pixels[i + 1], img.pixels[i + 2], img.pixels[i + 3]]
}

#[test]
fn rect_icon_has_stroke_pixels_on_its_edges() {
    // Path M10 16 L54 16 L54 48 L10 48 Z, stroke 4.25 in active-3, face in active-1; at 64 px 1 unit = 1 px.
    let img = render_svg(RECT, &palette(), 64, 1.0, Theme::Light).expect("renders");
    assert_eq!((img.width, img.height), (64, 64));
    for (x, y) in [(32, 16), (32, 47), (54, 32), (20, 16)] {
        let p = px(&img, x, y);
        assert_eq!(p, [ACTIVE_3_LIGHT[0], ACTIVE_3_LIGHT[1], ACTIVE_3_LIGHT[2], 255], "edge pixel ({x},{y})");
    }
    let face = px(&img, 32, 32);
    assert_eq!(face, [ACTIVE_1_LIGHT[0], ACTIVE_1_LIGHT[1], ACTIVE_1_LIGHT[2], 255], "face");
    assert_eq!(px(&img, 2, 2)[3], 0, "outside is transparent");
    assert_eq!(px(&img, 32, 8)[3], 0, "above the top edge is transparent");
}

#[test]
fn corner_handles_knock_out_the_stroke() {
    // `#ffffff` is the `gap` role: it erases to transparency instead of painting white.
    // The knockout disc r=5.8 at (10,16) is refilled by the r=3.4 handle in active-3, ring between is clear.
    let img = render_svg(RECT, &palette(), 64, 1.0, Theme::Light).expect("renders");
    let ring = px(&img, 10 + 4, 16); // distance 4.5 from centre: inside the knockout, outside the handle
    assert_eq!(ring[3], 0, "ring between handle and knockout edge is clear");
    assert_eq!(px(&img, 10, 16)[..3], ACTIVE_3_LIGHT);
}

#[test]
fn edge_pixels_are_straight_not_premultiplied() {
    // Outer edge of the circle master: centre (32,32), stroke 4.25 around radius 22, so r = 24.125.
    // Pixels on that rim are partly covered stroke only; straight alpha keeps the stroke colour.
    let img = render_svg(CIRCLE, &palette(), 64, 1.0, Theme::Light).expect("renders");
    let mut partial = 0;
    for y in 0..64u32 {
        for x in 0..64u32 {
            let d = ((f64::from(x) + 0.5 - 32.0).powi(2) + (f64::from(y) + 0.5 - 32.0).powi(2)).sqrt();
            let p = px(&img, x, y);
            if (23.6..24.6).contains(&d) && (40..230).contains(&p[3]) {
                partial += 1;
                for c in 0..3 {
                    assert!(
                        p[c].abs_diff(ACTIVE_3_LIGHT[c]) <= 8,
                        "({x},{y}) channel {c} is {} but the straight colour is {} (alpha {})",
                        p[c],
                        ACTIVE_3_LIGHT[c],
                        p[3]
                    );
                }
            }
        }
    }
    assert!(partial >= 20, "only {partial} anti-aliased rim pixels found");
}

#[test]
fn dark_theme_recolours_line_role() {
    // `currentColor` is the `line` role. The undo shaft runs along y = 24.5 from x = 14 to 29.
    let light = render_svg(UNDO, &palette(), 64, 1.0, Theme::Light).expect("renders");
    let dark = render_svg(UNDO, &palette(), 64, 1.0, Theme::Dark).expect("renders");
    let (l, d) = (px(&light, 20, 24), px(&dark, 20, 24));
    assert_eq!(l, [LINE_LIGHT[0], LINE_LIGHT[1], LINE_LIGHT[2], 255]);
    assert_eq!(d, [LINE_DARK[0], LINE_DARK[1], LINE_DARK[2], 255]);
    assert_ne!(l, d);
}

#[test]
fn dark_theme_uses_dark_value_of_role() {
    let img = render_svg(RECT, &palette(), 64, 1.0, Theme::Dark).expect("renders");
    let p = px(&img, 32, 16);
    assert_eq!(p, [ACTIVE_3_DARK[0], ACTIVE_3_DARK[1], ACTIVE_3_DARK[2], 255]);
}

#[test]
fn retina_scale_doubles_pixel_size() {
    let one = render_svg(RECT, &palette(), 32, 1.0, Theme::Light).expect("renders");
    let two = render_svg(RECT, &palette(), 32, 2.0, Theme::Light).expect("renders");
    assert_eq!((one.width, one.height), (32, 32));
    assert_eq!((two.width, two.height), (64, 64));
    assert_eq!(two.pixels.len(), 64 * 64 * 4);
    // The same artwork: the edge at the same relative position is stroke-coloured at both scales.
    assert_eq!(px(&one, 16, 8)[..3], ACTIVE_3_LIGHT);
    assert_eq!(px(&two, 32, 16)[..3], ACTIVE_3_LIGHT);
}

#[test]
fn fractional_scale_rounds_to_whole_pixels() {
    let img = render_svg(RECT, &palette(), 24, 1.5, Theme::Light).expect("renders");
    assert_eq!((img.width, img.height), (36, 36));
}

#[test]
fn invalid_sizes_are_typed_errors() {
    let p = palette();
    assert_eq!(render_svg(RECT, &p, 0, 1.0, Theme::Light), Err(IconError::InvalidSize));
    assert_eq!(render_svg(RECT, &p, 16, 0.0, Theme::Light), Err(IconError::InvalidSize));
    assert_eq!(render_svg(RECT, &p, 16, f32::NAN, Theme::Light), Err(IconError::InvalidSize));
    assert!(matches!(render_svg(RECT, &p, 100_000, 1.0, Theme::Light), Err(IconError::SizeTooLarge { .. })));
    assert!(matches!(render_svg(RECT, &p, 1024, 8.0, Theme::Light), Err(IconError::SizeTooLarge { .. })));
}

#[test]
fn malformed_or_foreign_artwork_is_a_parse_error() {
    let p = palette();
    for bad in [
        "<svg><rect x=\"0\"/></svg>",
        "<svg><path d=\"M0 0 L10\" stroke=\"currentColor\"/></svg>",
        "<svg><path d=\"M0 0 L10 10\" stroke=\"#123456\"/></svg>",
        "<svg><g></svg>",
        "<svg",
    ] {
        assert!(matches!(render_svg(bad, &p, 16, 1.0, Theme::Light), Err(IconError::Parse(_))), "{bad}");
    }
}

#[test]
fn palette_without_required_roles_is_rejected() {
    assert!(matches!(
        Palette::parse("[roles.ink]\nsource=\"#000000\"\nlight=\"#000000\"\ndark=\"#ffffff\"\n"),
        Err(IconError::Palette(_))
    ));
    assert!(matches!(Palette::parse("not toml ["), Err(IconError::Palette(_))));
}

#[test]
fn unknown_id_is_typed_error() {
    assert!(!has_icon("no.such.icon"));
    assert_eq!(render("no.such.icon", 32, 1.0, Theme::Light), Err(IconError::UnknownIcon("no.such.icon".into())));
}

#[test]
fn every_registered_icon_parses_and_is_not_blank() {
    let ids: Vec<&str> = icon_ids().collect();
    for id in &ids {
        assert!(has_icon(id), "{id}");
        for theme in [Theme::Light, Theme::Dark] {
            let img = render(id, 32, 1.0, theme).unwrap_or_else(|e| panic!("{id} ({theme:?}): {e}"));
            assert_eq!((img.width, img.height), (32, 32));
            assert!(img.pixels.as_chunks::<4>().0.iter().any(|p| p[3] > 0), "{id} ({theme:?}) is blank");
        }
    }
}

#[test]
fn registry_is_not_empty_once_assets_are_present() {
    // The assets lane delivers `assets/icons/ui/svg`; an empty registry means the build embedded nothing.
    let svg_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/icons/ui/svg");
    let on_disk = std::fs::read_dir(&svg_dir)
        .map(|d| d.filter_map(Result::ok).filter(|e| e.path().extension().is_some_and(|x| x == "svg")).count())
        .unwrap_or(0);
    assert!(icon_ids().count() >= on_disk, "{} registered but {on_disk} svg files on disk", icon_ids().count());
}

#[test]
fn icon_ids_are_sorted_and_unique() {
    let ids: Vec<&str> = icon_ids().collect();
    assert!(ids.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn tool_ids_from_icons_toml_resolve_and_missing_artwork_is_unregistered() {
    // `icons.toml` maps tool ids to files; unlisted files stay reachable under their stem.
    assert!(has_icon("sketch.rectangle.two_point"));
    assert!(has_icon("rect"));
    let by_tool = render("sketch.rectangle.two_point", 32, 1.0, Theme::Light).expect("renders");
    let by_stem = render("rect", 32, 1.0, Theme::Light).expect("renders");
    assert_eq!(by_tool, by_stem);
    // `node` is marked `missing = true` (no artwork): not registered, so the host shows a label.
    assert!(!has_icon("node"));
    assert_eq!(render("node", 32, 1.0, Theme::Light), Err(IconError::UnknownIcon("node".into())));
}
