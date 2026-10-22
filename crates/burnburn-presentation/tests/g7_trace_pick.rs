//! G7-trace, T03: correcting the lens of a photo and pointing in the image instead of typing. The
//! four corners of the plane are clicked (in any order), masks are painted with a brush, and the
//! radial lens distortion of the photo is corrected before the plane. All of it is part of the
//! recipe; the source pixels are never touched.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod g7_support;

use burnburn_presentation::{MaskMode, PickKind, PrepField, PrepIssue, TracePick, TracePrepIntent};
use g7_support::*;

// ---------------------------------------------------------------------------------------------
// Lens
// ---------------------------------------------------------------------------------------------

/// A 201 × 151 px photo of one straight vertical line (1.5 px wide, at x = 40.5 of the undistorted
/// scene) seen through a barrel lens with `k1`: for each photo pixel the position in the scene is
/// found by bisection on the radial law (independent of the code under test).
fn bowed_line(k1: f64) -> Vec<u8> {
    let (w, h) = (201usize, 151usize);
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    let half_diag = cx.hypot(cy);
    image(w, h, |x, y| {
        let (dx, dy) = ((x as f64 + 0.5 - cx) / half_diag, (y as f64 + 0.5 - cy) / half_diag);
        let rd = dx.hypot(dy);
        let (mut lo, mut hi) = (0.0f64, 2.0f64);
        for _ in 0..60 {
            let mid = (lo + hi) / 2.0;
            if mid * (1.0 + k1 * mid * mid) < rd { lo = mid } else { hi = mid }
        }
        let ru = (lo + hi) / 2.0;
        let scale = if rd > 0.0 { ru / rd } else { 1.0 };
        let xu = cx + dx * scale * half_diag;
        if (xu - 40.5).abs() < 0.75 { 0 } else { 255 }
    })
}

#[test]
fn the_lens_coefficients_straighten_a_line_that_the_lens_bowed() {
    let (mut h, doc) = opened_with(&bowed_line(-0.25));
    assert!(!prep(&h, doc).lens.on);
    let bowed = tracer(&h, doc).result.expect("a trace").width_mm;
    assert!(bowed > 0.45, "the fixture is bowed: {bowed} mm wide for a 0.15 mm line");

    send(&mut h, doc, TracePrepIntent::UseLens { k1: s("-0.25"), k2: s("0") });
    let p = prep(&h, doc);
    assert_eq!(p.issue, None);
    assert!(p.lens.on);
    assert_eq!((p.lens.k1.as_str(), p.lens.k2.as_str()), ("-0.25", "0"));
    let straight = tracer(&h, doc).result.expect("a trace").width_mm;
    assert!(straight < 0.35, "straight again: {straight} mm");

    send(&mut h, doc, TracePrepIntent::ClearLens);
    let p = prep(&h, doc);
    assert!(!p.lens.on);
    assert!((tracer(&h, doc).result.unwrap().width_mm - bowed).abs() < 1e-9, "the photo as it was");
}

#[test]
fn a_lens_that_cannot_be_read_or_is_no_lens_is_refused_where_it_was_typed() {
    let (mut h, doc) = opened_with(&bowed_line(-0.25));
    let before = tracer(&h, doc).result.unwrap().width_mm;
    for (k1, k2, want) in [
        ("abc", "0", PrepIssue::InvalidNumber(PrepField::LensK1)),
        ("-0.1", "x", PrepIssue::InvalidNumber(PrepField::LensK2)),
        ("5", "0", PrepIssue::OutOfRange(PrepField::LensK1)),
        ("0", "-3", PrepIssue::OutOfRange(PrepField::LensK2)),
        // 1 + 3 k1 r² is zero inside the image: the mapping folds over itself.
        ("-0.5", "0", PrepIssue::OutOfRange(PrepField::LensK1)),
    ] {
        send(&mut h, doc, TracePrepIntent::UseLens { k1: s(k1), k2: s(k2) });
        let p = prep(&h, doc);
        assert_eq!(p.issue, Some(want), "{k1:?} {k2:?}");
        assert!(!p.lens.on);
    }
    assert!((tracer(&h, doc).result.unwrap().width_mm - before).abs() < 1e-9);
}

#[test]
fn the_lens_is_stored_with_the_applied_trace_and_comes_back_when_it_is_reopened() {
    use burnburn_presentation::CommandKey;
    let (mut h, doc) = opened_with(&bowed_line(-0.25));
    send(&mut h, doc, TracePrepIntent::UseLens { k1: s("-0.25"), k2: s("0.01") });
    let straight = tracer(&h, doc).result.unwrap().width_mm;
    h.invoke(doc, CommandKey::ApplyTraceLinked);
    let id = vm(&h, doc).objects.last().unwrap().id;
    h.invoke(doc, CommandKey::CloseTracer);
    h.send(burnburn_presentation::UiIntent::Select {
        document: doc,
        ids: vec![id],
        mode: burnburn_presentation::SelectMode::Replace,
    });
    h.invoke(doc, CommandKey::TraceEdit);
    h.run_tasks();
    let p = prep(&h, doc);
    assert!(p.lens.on);
    assert_eq!((p.lens.k1.as_str(), p.lens.k2.as_str()), ("-0.25", "0.01"));
    assert!((tracer(&h, doc).result.unwrap().width_mm - straight).abs() < 1e-9);
}

// ---------------------------------------------------------------------------------------------
// Corners clicked in the photo
// ---------------------------------------------------------------------------------------------

/// A black trapezoid photographed on white: corners (10,8), (54,8), (60,40), (4,40).
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

#[test]
fn four_clicks_in_any_order_are_the_same_correction_as_the_typed_corners() {
    // Typed.
    let (mut typed, tdoc) = opened_with(&trapezoid());
    send(
        &mut typed,
        tdoc,
        TracePrepIntent::ApplyPerspective {
            corners: Box::new([[s("10"), s("8")], [s("54"), s("8")], [s("60"), s("40")], [s("4"), s("40")]]),
            width: s(""),
            height: s(""),
        },
    );
    let want = prep(&typed, tdoc).perspective;

    // Clicked: bottom-right, top-left, bottom-left, top-right.
    let (mut h, doc) = opened_with(&trapezoid());
    pick(&mut h, doc, TracePick::BeginCorners);
    let p = prep(&h, doc).pick.expect("waiting for corners");
    assert_eq!(p.kind, PickKind::Corners { picked: vec![] });
    assert_eq!(p.surface_px, (64, 48));
    for (n, (x, y)) in [(60.2, 40.1), (9.8, 7.9), (4.0, 40.0), (54.0, 8.0)].into_iter().enumerate() {
        assert!(!prep(&h, doc).perspective.on, "nothing is applied before the fourth click");
        pick(&mut h, doc, TracePick::Point { x, y });
        if n < 3 {
            let PickKind::Corners { picked } = prep(&h, doc).pick.unwrap().kind else { panic!("corners") };
            assert_eq!(picked.len(), n + 1);
        }
    }
    let p = prep(&h, doc);
    assert_eq!(p.issue, None);
    assert!(p.pick.is_none(), "the fourth click ends the pointing");
    assert!(p.perspective.on);
    let t = p.perspective.transform.expect("the transform is shown");
    assert!(t.residual_px < 1e-6);
    // Corners are stored top-left, top-right, bottom-right, bottom-left, as clicked (decimals included).
    let near = |text: &str, v: f64| (text.parse::<f64>().unwrap() - v).abs() < 1e-9;
    let c = &p.perspective.corners;
    assert!(near(&c[0][0], 9.8) && near(&c[0][1], 7.9), "{c:?}");
    assert!(near(&c[1][0], 54.0) && near(&c[1][1], 8.0), "{c:?}");
    assert!(near(&c[2][0], 60.2) && near(&c[2][1], 40.1), "{c:?}");
    assert!(near(&c[3][0], 4.0) && near(&c[3][1], 40.0), "{c:?}");
    // The sizes follow from the clicked corners the way typed corners get theirs.
    let edge = |a: [f64; 2], b: [f64; 2]| (b[0] - a[0]).hypot(b[1] - a[1]);
    let (tl, tr, br, bl) = ([9.8, 7.9], [54.0, 8.0], [60.2, 40.1], [4.0, 40.0]);
    let w = edge(tl, tr).max(edge(bl, br)).round();
    let hh = edge(tl, bl).max(edge(tr, br)).round();
    assert_eq!(p.perspective.width, format!("{w}"));
    assert_eq!(p.perspective.height, format!("{hh}"));
    // The typed correction of the exact corners lands within a pixel of the clicked one.
    let (wt, ht): (f64, f64) = (want.width.parse().unwrap(), want.height.parse().unwrap());
    assert!((w - wt).abs() <= 1.0 && (hh - ht).abs() <= 1.0, "{w} x {hh} against {wt} x {ht}");
    let (comp, width, height) = {
        let r = tracer(&h, doc).result.unwrap();
        (r.components, r.width_mm, r.height_mm)
    };
    assert_eq!(comp, 1);
    assert!((width - 5.6).abs() < 0.35 && (height - 3.3).abs() < 0.35, "the corrected plate: {width} x {height}");
}

#[test]
fn pointing_can_be_stopped_and_a_click_without_pointing_is_refused() {
    let (mut h, doc) = opened_with(&trapezoid());
    pick(&mut h, doc, TracePick::Point { x: 10.0, y: 8.0 });
    assert_eq!(prep(&h, doc).issue, Some(PrepIssue::PickNotStarted));
    pick(&mut h, doc, TracePick::BeginCorners);
    pick(&mut h, doc, TracePick::Point { x: 10.0, y: 8.0 });
    pick(&mut h, doc, TracePick::End);
    let p = prep(&h, doc);
    assert!(p.pick.is_none() && !p.perspective.on, "stopped: nothing from the clicks is kept");
    pick(&mut h, doc, TracePick::Point { x: 10.0, y: 8.0 });
    assert_eq!(prep(&h, doc).issue, Some(PrepIssue::PickNotStarted));
}

#[test]
fn a_click_on_a_corner_that_is_already_picked_is_refused_and_four_collinear_clicks_apply_nothing() {
    let (mut h, doc) = opened_with(&trapezoid());
    pick(&mut h, doc, TracePick::BeginCorners);
    pick(&mut h, doc, TracePick::Point { x: 10.0, y: 8.0 });
    pick(&mut h, doc, TracePick::Point { x: 10.4, y: 8.3 });
    assert_eq!(prep(&h, doc).issue, Some(PrepIssue::CornersTooClose));
    let PickKind::Corners { picked } = prep(&h, doc).pick.unwrap().kind else { panic!("corners") };
    assert_eq!(picked.len(), 1, "the second click on the same place is not a corner");
    for x in [20.0, 30.0, 40.0] {
        pick(&mut h, doc, TracePick::Point { x, y: 8.0 });
    }
    let p = prep(&h, doc);
    assert_eq!(p.issue, Some(PrepIssue::DegenerateCorners), "four points on one line span no plane");
    assert!(!p.perspective.on);
    assert!(p.pick.is_none(), "the pointing ended; the user starts again");
}

#[test]
fn the_picture_to_point_at_shows_the_photo_as_the_tracer_gets_it() {
    let (mut h, doc) = opened_with(&trapezoid());
    pick(&mut h, doc, TracePick::BeginCorners);
    let picture = prep(&h, doc).pick.unwrap().picture.expect("a picture to point at");
    assert_eq!((picture.width, picture.height), (64, 48), "small images are shown as they are");
    assert_eq!(picture.gray8.len(), 64 * 48);
    assert_eq!(picture.gray8[0], 255, "paper");
    assert_eq!(picture.gray8[24 * 64 + 32], 0, "the middle of the plate");
}

// ---------------------------------------------------------------------------------------------
// The brush
// ---------------------------------------------------------------------------------------------

/// White with a dark object at x 8…17 and a dark shadow blob at x 40…55.
fn object_and_shadow() -> Vec<u8> {
    image(64, 32, |x, y| {
        let object = (8..18).contains(&x) && (10..22).contains(&y);
        let shadow = (40..56).contains(&x) && (4..28).contains(&y);
        if object || shadow { 0 } else { 255 }
    })
}

#[test]
fn a_brush_stroke_over_the_shadow_removes_it_and_is_one_mask() {
    let (mut h, doc) = opened_with(&object_and_shadow());
    assert_eq!(counts(&h, doc).0, 2, "the object and its shadow");
    pick(&mut h, doc, TracePick::BeginBrush { mode: MaskMode::Remove, radius_px: 9.0 });
    let p = prep(&h, doc).pick.expect("brushing");
    assert_eq!(p.kind, PickKind::Brush { mode: MaskMode::Remove, radius_px: 9.0 });
    pick(&mut h, doc, TracePick::Stroke { points: vec![[48.0, 4.0], [48.0, 16.0], [47.0, 27.0]] });
    let p = prep(&h, doc);
    assert_eq!(p.issue, None);
    assert!(p.pick.is_some(), "the brush stays in hand for the next stroke");
    assert_eq!(p.masks.len(), 1);
    assert_eq!((p.masks[0].mode, p.masks[0].brush_px), (MaskMode::Remove, Some(9.0)));
    assert_eq!(counts(&h, doc).0, 1, "only the object is left");

    // Removing the mask brings the shadow back; the mask list is the one of rectangles.
    send(&mut h, doc, TracePrepIntent::RemoveMask { index: 0 });
    assert_eq!(counts(&h, doc).0, 2);
}

#[test]
fn a_keep_brush_traces_only_what_was_painted_and_the_strokes_together_are_the_union() {
    let (mut h, doc) = opened_with(&object_and_shadow());
    pick(&mut h, doc, TracePick::BeginBrush { mode: MaskMode::Keep, radius_px: 7.0 });
    pick(&mut h, doc, TracePick::Stroke { points: vec![[13.0, 16.0]] });
    assert_eq!(counts(&h, doc).0, 1, "a single click paints a disc over the object");
    pick(&mut h, doc, TracePick::Stroke { points: vec![[48.0, 10.0], [48.0, 22.0]] });
    assert_eq!(counts(&h, doc).0, 2, "painting the shadow too keeps both");
    assert_eq!(prep(&h, doc).masks.len(), 2);
    // The brush works on the corrected image once there is a correction; here: still the photo.
    pick(&mut h, doc, TracePick::End);
    assert!(prep(&h, doc).pick.is_none());
}

#[test]
fn a_stroke_without_points_or_outside_the_image_is_refused_and_changes_nothing() {
    let (mut h, doc) = opened_with(&object_and_shadow());
    pick(&mut h, doc, TracePick::BeginBrush { mode: MaskMode::Remove, radius_px: 3.0 });
    pick(&mut h, doc, TracePick::Stroke { points: vec![] });
    assert_eq!(prep(&h, doc).issue, Some(PrepIssue::InvalidStroke));
    pick(&mut h, doc, TracePick::Stroke { points: vec![[f64::NAN, 3.0]] });
    assert_eq!(prep(&h, doc).issue, Some(PrepIssue::InvalidStroke));
    pick(&mut h, doc, TracePick::Stroke { points: vec![[500.0, 500.0], [510.0, 500.0]] });
    assert_eq!(prep(&h, doc).issue, Some(PrepIssue::MaskOutsideImage));
    assert!(prep(&h, doc).masks.is_empty());
    assert_eq!(counts(&h, doc).0, 2);
    // A radius that is no radius is refused when the brush is picked up.
    for radius_px in [0.0, -1.0, f64::NAN, 5000.0] {
        pick(&mut h, doc, TracePick::BeginBrush { mode: MaskMode::Keep, radius_px });
        assert_eq!(prep(&h, doc).issue, Some(PrepIssue::OutOfRange(PrepField::BrushRadius)), "{radius_px}");
    }
    // A stroke with no brush in hand.
    pick(&mut h, doc, TracePick::End);
    pick(&mut h, doc, TracePick::Stroke { points: vec![[13.0, 16.0]] });
    assert_eq!(prep(&h, doc).issue, Some(PrepIssue::PickNotStarted));
}

#[test]
fn the_brush_radius_can_be_typed_as_text_and_a_bad_text_is_refused_where_it_was_typed() {
    let (mut h, doc) = opened_with(&object_and_shadow());
    pick(&mut h, doc, TracePick::BeginBrushTyped { mode: MaskMode::Remove, radius: s("12,5") });
    assert_eq!(prep(&h, doc).issue, None);
    assert_eq!(prep(&h, doc).pick.unwrap().kind, PickKind::Brush { mode: MaskMode::Remove, radius_px: 12.5 });
    for (text, want) in [
        ("abc", PrepIssue::InvalidNumber(PrepField::BrushRadius)),
        ("", PrepIssue::InvalidNumber(PrepField::BrushRadius)),
        ("0", PrepIssue::OutOfRange(PrepField::BrushRadius)),
        ("-3", PrepIssue::OutOfRange(PrepField::BrushRadius)),
        ("99999", PrepIssue::OutOfRange(PrepField::BrushRadius)),
    ] {
        pick(&mut h, doc, TracePick::End);
        pick(&mut h, doc, TracePick::BeginBrushTyped { mode: MaskMode::Keep, radius: s(text) });
        assert_eq!(prep(&h, doc).issue, Some(want), "{text:?}");
        assert!(prep(&h, doc).pick.is_none());
    }
}
