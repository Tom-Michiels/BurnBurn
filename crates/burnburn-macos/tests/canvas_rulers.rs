//! LF9: the macOS canvas draws the rulers above and left of the drawing, in the theme of the
//! window's appearance. Headless on the packet; the look is checked with the offscreen snapshots.

#![cfg(target_os = "macos")]
#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use burnburn_macos::appkit::canvas_packet_themed;
use burnburn_presentation::{CommandKey, HeadlessEditorHarness};
use burnburn_render::{Primitive, RULER_THICKNESS_PT, Theme};

#[test]
fn the_canvas_packet_has_a_ruler_band_on_top_and_left_in_the_theme_colour() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (60.0, 40.0));
    let scene = h.scene(doc).unwrap();
    let scale = scene.viewport.backing_scale() as f32;
    for (name, dark, theme) in [("light", false, Theme::light()), ("dark", true, Theme::dark())] {
        let packet = canvas_packet_themed(&scene, false, 0.25, dark);
        let bands: Vec<_> = packet
            .primitives
            .iter()
            .filter_map(|p| match p {
                Primitive::Fill { contours_px, color } if *color == theme.ruler_background => Some(contours_px),
                _ => None,
            })
            .collect();
        assert_eq!(bands.len(), 2, "{name}: a band above and one left");
        let thick = RULER_THICKNESS_PT as f32 * scale;
        let top = bands[0][0].iter().map(|p| p[1]).fold(f32::MIN, f32::max);
        let left = bands[1][0].iter().map(|p| p[0]).fold(f32::MIN, f32::max);
        assert!((top - thick).abs() < 0.01 && (left - thick).abs() < 0.01, "{name}: {top} / {left}");
    }
}
