//! G7-trace, T03: pointing happens on the canvas. While corners or a brush are asked for, the photo
//! is laid on the sheet (at the place applying the trace would put it, one pixel = one pixel of the
//! declared scale) and the pointer of the canvas is the pointer in the picture: a click is a corner,
//! a drag with the brush is a stroke. Escape stops. Everything is in millimetres on the sheet and the
//! controller turns it into pixels of the picture; the host needs nothing of its own.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod g7_support;

use burnburn_editor::{Modifiers, Overlay};
use burnburn_presentation::{MaskMode, PickKind, TracePick};
use g7_support::*;

/// A black trapezoid photographed on white (64 × 48 px, 0.1 mm per pixel): corners (10,8), (54,8),
/// (60,40), (4,40) in pixels.
fn trapezoid() -> Vec<u8> {
    image(64, 48, |x, y| {
        let (x, y) = (x as f64 + 0.5, y as f64 + 0.5);
        if !(8.0..=40.0).contains(&y) {
            return 255;
        }
        let t = (y - 8.0) / 32.0;
        let (left, right) = (10.0 - 6.0 * t, 54.0 + 6.0 * t);
        if (left..=right).contains(&x) { 0 } else { 255 }
    })
}

/// Pixel (x, y) of the 64 × 48 photo in millimetres on the sheet (y up, origin at the bottom left).
fn mm(x: f64, y: f64) -> (f64, f64) {
    (x * 0.1, (48.0 - y) * 0.1)
}

#[test]
fn while_corners_are_asked_for_the_photo_lies_on_the_sheet_and_four_clicks_apply_the_correction() {
    let (mut h, doc) = opened_with(&trapezoid());
    assert!(!h.scene(doc).unwrap().overlays.iter().any(|o| matches!(o, Overlay::BedImage { .. })));
    pick(&mut h, doc, TracePick::BeginCorners);
    let scene = h.scene(doc).unwrap();
    let Some(Overlay::BedImage { picture, quads, .. }) =
        scene.overlays.iter().find(|o| matches!(o, Overlay::BedImage { .. }))
    else {
        panic!("the photo to point at is on the sheet");
    };
    assert_eq!((picture.width, picture.height), (64, 48));
    let quad = &quads[0];
    let (min, max) = (quad.bed_mm[3], quad.bed_mm[1]);
    assert!((min.x - 0.0).abs() < 1e-9 && (min.y - 0.0).abs() < 1e-9, "{min:?}");
    assert!((max.x - 6.4).abs() < 1e-9 && (max.y - 4.8).abs() < 1e-9, "6.4 × 4.8 mm: {max:?}");

    // Click the corners, bottom-right first. The canvas pointer is the pointer in the picture.
    for (x, y) in [(60.0, 40.0), (10.0, 8.0), (4.0, 40.0)] {
        h.click(doc, mm(x, y), Modifiers::default());
        assert!(prep(&h, doc).pick.is_some() && !prep(&h, doc).perspective.on);
    }
    // The corners picked so far are marked on the canvas.
    let marks = h.scene(doc).unwrap().overlays.iter().filter(|o| matches!(o, Overlay::SnapCandidate { .. })).count();
    assert_eq!(marks, 3);
    h.click(doc, mm(54.0, 8.0), Modifiers::default());
    h.run_tasks();
    let p = prep(&h, doc);
    assert_eq!(p.issue, None);
    assert!(p.pick.is_none() && p.perspective.on);
    let t = p.perspective.transform.unwrap();
    assert!(t.residual_px < 1e-6);
    let near = |text: &str, v: f64| (text.parse::<f64>().unwrap() - v).abs() < 1e-6;
    let c = &p.perspective.corners;
    assert!(near(&c[0][0], 10.0) && near(&c[0][1], 8.0), "top left: {c:?}");
    assert!(near(&c[1][0], 54.0) && near(&c[1][1], 8.0), "top right: {c:?}");
    assert!(near(&c[2][0], 60.0) && near(&c[2][1], 40.0), "bottom right: {c:?}");
    assert!(near(&c[3][0], 4.0) && near(&c[3][1], 40.0), "bottom left: {c:?}");
    assert!(!h.scene(doc).unwrap().overlays.iter().any(|o| matches!(o, Overlay::BedImage { .. })), "done pointing");
    // Same correction as typing the corners: a 56 × 33 px plate.
    assert_eq!((p.perspective.width.as_str(), p.perspective.height.as_str()), ("56", "33"));
}

/// White with a dark object at x 8…17 and a dark shadow blob at x 40…55 (64 × 32 px).
fn object_and_shadow() -> Vec<u8> {
    image(64, 32, |x, y| {
        let object = (8..18).contains(&x) && (10..22).contains(&y);
        let shadow = (40..56).contains(&x) && (4..28).contains(&y);
        if object || shadow { 0 } else { 255 }
    })
}

fn mm32(x: f64, y: f64) -> (f64, f64) {
    (x * 0.1, (32.0 - y) * 0.1)
}

#[test]
fn dragging_the_brush_over_the_shadow_paints_one_stroke_and_the_trace_follows() {
    let (mut h, doc) = opened_with(&object_and_shadow());
    assert_eq!(counts(&h, doc).0, 2);
    pick(&mut h, doc, TracePick::BeginBrush { mode: MaskMode::Remove, radius_px: 9.0 });
    // The picture is on the sheet; the stroke in progress is drawn while the button is down.
    h.begin_drag(doc, mm32(48.0, 4.0), mm32(48.0, 16.0));
    let drawing = h.scene(doc).unwrap().overlays.iter().filter(|o| matches!(o, Overlay::ResultPreview { .. })).count();
    assert_eq!(drawing, 1, "the stroke so far is drawn");
    assert!(prep(&h, doc).masks.is_empty(), "nothing is a mask before the button is released");
    h.pointer(doc, burnburn_editor::PointerPhase::Move, mm32(47.0, 27.0), Modifiers::default());
    h.pointer(doc, burnburn_editor::PointerPhase::Up, mm32(47.0, 27.0), Modifiers::default());
    h.run_tasks();
    let p = prep(&h, doc);
    assert_eq!(p.issue, None);
    assert_eq!(p.masks.len(), 1);
    assert_eq!((p.masks[0].mode, p.masks[0].brush_px), (MaskMode::Remove, Some(9.0)));
    assert_eq!(counts(&h, doc).0, 1, "only the object is left");
    assert!(p.pick.is_some(), "the brush stays in hand");
    assert!(
        !h.scene(doc).unwrap().overlays.iter().any(|o| matches!(o, Overlay::ResultPreview { .. })),
        "the finished stroke is no longer drawn as one in progress"
    );
}

#[test]
fn escape_puts_the_brush_down_and_a_pointer_that_is_not_pointing_selects_as_before() {
    let (mut h, doc) = opened_with(&object_and_shadow());
    pick(&mut h, doc, TracePick::BeginBrush { mode: MaskMode::Keep, radius_px: 4.0 });
    h.escape(doc);
    let p = prep(&h, doc);
    assert!(p.pick.is_none());
    // Clicking on the sheet now is an ordinary click: it paints nothing.
    h.click(doc, mm32(13.0, 16.0), Modifiers::default());
    assert!(prep(&h, doc).masks.is_empty());
    // And a brush with a different kind of pick is shown as what it is.
    pick(&mut h, doc, TracePick::BeginCorners);
    assert!(matches!(prep(&h, doc).pick.unwrap().kind, PickKind::Corners { .. }));
}

#[test]
fn a_click_in_brush_mode_paints_a_disc_and_a_click_outside_the_photo_is_refused() {
    let (mut h, doc) = opened_with(&object_and_shadow());
    pick(&mut h, doc, TracePick::BeginBrush { mode: MaskMode::Keep, radius_px: 7.0 });
    h.click(doc, mm32(13.0, 16.0), Modifiers::default());
    h.run_tasks();
    assert_eq!(prep(&h, doc).masks.len(), 1);
    assert_eq!(counts(&h, doc).0, 1, "a disc over the object keeps only the object");
    // 100 px to the right of a 64 px wide photo, further than the brush reaches.
    h.click(doc, mm32(164.0, 16.0), Modifiers::default());
    assert_eq!(prep(&h, doc).issue, Some(burnburn_presentation::PrepIssue::MaskOutsideImage));
    assert_eq!(prep(&h, doc).masks.len(), 1);
}
