//! LF2: the canvas follows the window's light/dark appearance. The host's theme choice is tested
//! headless on the packet; which appearance a view has is native (see the dark gui_snapshots).

#![cfg(target_os = "macos")]
#![allow(clippy::unwrap_used, clippy::panic)] // test helpers outside #[test] fns

use burnburn_macos::appkit::{canvas_packet_themed, canvas_theme};
use burnburn_presentation::{CommandKey, HeadlessEditorHarness};
use burnburn_render::{Primitive, Theme};

#[test]
fn the_dark_appearance_draws_the_dark_canvas_and_the_light_one_the_light_canvas() {
    let mut h = HeadlessEditorHarness::new();
    let doc = h.new_document().unwrap();
    h.invoke(doc, CommandKey::ToolRectangle);
    h.drag(doc, (10.0, 10.0), (60.0, 40.0));
    let scene = h.scene(doc).unwrap();

    let light = canvas_packet_themed(&scene, false, 0.25, false);
    let dark = canvas_packet_themed(&scene, false, 0.25, true);
    assert_eq!(light.clear, Theme::light().background);
    assert_eq!(dark.clear, Theme::dark().background);
    assert_ne!(light.clear, dark.clear);
    // The ink of the drawn rectangle is the theme's own (a layer or the design ink, never the other theme's).
    let dark_strokes: Vec<_> = dark
        .primitives
        .iter()
        .filter_map(|p| match p {
            Primitive::Stroke { color, .. } => Some(*color),
            _ => None,
        })
        .collect();
    assert!(!dark_strokes.is_empty());
    assert!(!dark_strokes.contains(&Theme::light().design.stroke.unwrap()), "no light-theme ink on the dark canvas");
}

#[test]
fn wireframe_and_display_quality_apply_in_both_appearances() {
    for dark in [false, true] {
        let t = canvas_theme(dark, true, 0.5);
        assert!(t.design.fill.is_none() && t.selected.fill.is_none(), "wireframe has no fills (dark: {dark})");
        assert_eq!(t.tolerance_px, 0.5);
        let base = if dark { Theme::dark() } else { Theme::light() };
        assert_eq!(t.background, base.background);
    }
}
