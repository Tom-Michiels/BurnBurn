//! G8-trace, T05 and T06: protecting by clicking on the canvas. While the tracer asks for contours or
//! corners, the photo lies on the sheet with the outlines of the result in force; a click on an
//! outline (or inside a part or hole) protects that contour from the next area retrace, a click near
//! a corner protects the corner so the fit keeps a node exactly there. A second click takes it back.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod g7_support;

use burnburn_editor::{Modifiers, Overlay};
use burnburn_geometry::{DisplayTolerance, Point};
use burnburn_presentation::{
    HeadlessEditorHarness, PickKind, PrepIssue, TraceMode, TracePick, TracePrepIntent, TracePrepViewModel,
};
use burnburn_types::{DocumentId, Length};
use g7_support::*;

const REACH: f64 = 1.5;

fn overlays(h: &HeadlessEditorHarness, doc: DocumentId) -> Vec<Overlay> {
    h.scene(doc).expect("a scene").overlays.clone()
}

fn count(h: &HeadlessEditorHarness, doc: DocumentId, f: impl Fn(&Overlay) -> bool) -> usize {
    overlays(h, doc).iter().filter(|o| f(o)).count()
}

fn protected_ids(p: &TracePrepViewModel) -> Vec<u64> {
    p.contours.iter().filter(|c| c.protected).map(|c| c.id).collect()
}

fn hole_id(p: &TracePrepViewModel) -> u64 {
    p.contours.iter().find(|c| c.hole).expect("the counter").id
}

// ---------------------------------------------------------------------------------------------
// Contours
// ---------------------------------------------------------------------------------------------

#[test]
fn beginning_to_protect_contours_lays_the_photo_on_the_sheet_with_every_outline() {
    let (mut h, doc) = opened_scene();
    assert_eq!(count(&h, doc, |o| matches!(o, Overlay::BedImage { .. })), 0);
    pick(&mut h, doc, TracePick::BeginProtectContours);
    let p = prep(&h, doc);
    assert_eq!(p.issue, None);
    assert!(matches!(p.pick.as_ref().map(|k| &k.kind), Some(PickKind::ProtectContours)));
    assert_eq!(count(&h, doc, |o| matches!(o, Overlay::BedImage { .. })), 1);
    // Ring, its counter, the rectangle and the far disc: four outlines, none protected yet.
    assert_eq!(count(&h, doc, |o| matches!(o, Overlay::ResultPreview { .. })), 4);
    assert_eq!(count(&h, doc, |o| matches!(o, Overlay::ProtectedOutline { .. })), 0);
}

#[test]
fn a_click_on_an_outline_protects_that_contour_and_a_second_click_takes_it_back() {
    let (mut h, doc) = opened_scene();
    pick(&mut h, doc, TracePick::BeginProtectContours);
    let hole = hole_id(&prep(&h, doc));
    // The counter is the circle of r 15 px around (50, 60): its right-most point is (65, 60).
    pick(&mut h, doc, TracePick::ProtectAt { x: 65.4, y: 60.0, reach_px: REACH });
    let p = prep(&h, doc);
    assert_eq!(p.issue, None);
    assert_eq!(protected_ids(&p), [hole]);
    assert_eq!(count(&h, doc, |o| matches!(o, Overlay::ProtectedOutline { .. })), 1);
    assert_eq!(count(&h, doc, |o| matches!(o, Overlay::ResultPreview { .. })), 3);
    assert!(p.pick.is_some(), "pointing goes on: more can be clicked");

    pick(&mut h, doc, TracePick::ProtectAt { x: 65.4, y: 60.0, reach_px: REACH });
    assert!(protected_ids(&prep(&h, doc)).is_empty());
    assert_eq!(count(&h, doc, |o| matches!(o, Overlay::ProtectedOutline { .. })), 0);
}

#[test]
fn a_click_inside_a_part_or_a_hole_protects_the_smallest_contour_around_it() {
    let (mut h, doc) = opened_scene();
    pick(&mut h, doc, TracePick::BeginProtectContours);
    let p = prep(&h, doc);
    let hole = hole_id(&p);
    // Inside the counter (centre of the ring): the hole, not the ring that encloses it.
    pick(&mut h, doc, TracePick::ProtectAt { x: 50.0, y: 60.0, reach_px: REACH });
    assert_eq!(protected_ids(&prep(&h, doc)), [hole]);
    // In the body of the ring (x 20 px from the centre, between r 15 and r 35): the ring's outer contour.
    pick(&mut h, doc, TracePick::ProtectAt { x: 70.0, y: 60.0, reach_px: REACH });
    let p = prep(&h, doc);
    let ring =
        p.contours.iter().find(|c| !c.hole && (c.area_mm2 - std::f64::consts::PI * 12.25).abs() < 1.5).unwrap().id;
    let mut want = vec![hole, ring];
    want.sort_unstable();
    let mut got = protected_ids(&p);
    got.sort_unstable();
    assert_eq!(got, want);
}

#[test]
fn a_click_on_nothing_is_said_and_protects_nothing() {
    let (mut h, doc) = opened_scene();
    pick(&mut h, doc, TracePick::BeginProtectContours);
    pick(&mut h, doc, TracePick::ProtectAt { x: 195.0, y: 5.0, reach_px: REACH });
    let p = prep(&h, doc);
    assert_eq!(p.issue, Some(PrepIssue::NothingToProtect));
    assert!(protected_ids(&p).is_empty());
    // Not started: refused where it was asked.
    pick(&mut h, doc, TracePick::End);
    pick(&mut h, doc, TracePick::ProtectAt { x: 65.4, y: 60.0, reach_px: REACH });
    assert_eq!(prep(&h, doc).issue, Some(PrepIssue::PickNotStarted));
}

#[test]
fn the_canvas_pointer_is_the_pointer_and_the_protection_holds_for_the_next_retrace() {
    let (mut h, doc) = opened_scene();
    pick(&mut h, doc, TracePick::BeginProtectContours);
    let hole = hole_id(&prep(&h, doc));
    // (65, 60) px in millimetres on the sheet: 0.1 mm per pixel, y up from a 12 mm high picture.
    h.click(doc, (6.5, 6.0), Modifiers::default());
    assert_eq!(protected_ids(&prep(&h, doc)), [hole]);
    // The area retrace of the g7 tests: unprotected, a threshold of 200 would fill the counter (150).
    pick(&mut h, doc, TracePick::End);
    retrace(&mut h, doc, ["30", "40", "90", "70"], "200", "");
    assert_eq!(counts(&h, doc), (4, 1), "the speck is in, the counter is still a hole");
}

#[test]
fn contours_can_only_be_protected_in_a_contour_trace() {
    let (mut h, doc) = opened_scene();
    edit(&mut h, doc, |f| f.mode = TraceMode::Centerline);
    pick(&mut h, doc, TracePick::BeginProtectContours);
    assert_eq!(prep(&h, doc).issue, Some(PrepIssue::NothingToCorrect));
    assert!(prep(&h, doc).pick.is_none());
}

// ---------------------------------------------------------------------------------------------
// Corners
// ---------------------------------------------------------------------------------------------

/// A house outline (160 × 130 px at 0.1 mm per pixel) whose roof apex at (80, 44) px is a crease of
/// only 11.4°: with a generous fit budget the apex is rounded off unless it is protected.
fn house() -> Vec<u8> {
    // Polygon in mm (y up): (2,2) (14,2) (14,8) (8,8.6) (2,8); pixel (x, y down) = (mm.x * 10, 130 - mm.y * 10).
    let poly = [(2.0, 2.0), (14.0, 2.0), (14.0, 8.0), (8.0, 8.6), (2.0, 8.0)];
    // 8 × 8 samples per pixel give the edges the grey levels of an anti-aliased photo, so the sub-pixel
    // outline follows the polygon, not the staircase of a hard threshold.
    let covered = |x: f64, y: f64| {
        (0..poly.len()).all(|i| {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            (b.0 - a.0) * (y - a.1) - (b.1 - a.1) * (x - a.0) >= 0.0
        })
    };
    image(160, 130, |px, py| {
        let mut hits = 0u32;
        for sy in 0..8 {
            for sx in 0..8 {
                let x = (px as f64 + (f64::from(sx) + 0.5) / 8.0) / 10.0;
                let y = (130.0 - py as f64 - (f64::from(sy) + 0.5) / 8.0) / 10.0;
                hits += u32::from(covered(x, y));
            }
        }
        (255.0 * (1.0 - f64::from(hits) / 64.0)).round() as u8
    })
}

/// Distance from `p` to the segment `a`–`b`.
fn dist_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let (ab, ap) = (b - a, p - a);
    let len2 = ab.dot(ab);
    let t = if len2 == 0.0 { 0.0 } else { (ap.dot(ab) / len2).clamp(0.0, 1.0) };
    p.distance(a + ab * t)
}

/// The distance in mm from the roof apex (8.0, 8.6) to the outline in the result in force.
fn apex_distance(h: &HeadlessEditorHarness, doc: DocumentId) -> f64 {
    let path = tracer(h, doc).result.expect("a trace").preview;
    let tolerance = DisplayTolerance::new(Length::from_mm(0.005).unwrap()).unwrap();
    let apex = Point::new(8.0, 8.6);
    let mut best = f64::INFINITY;
    for line in path.flatten(&tolerance) {
        for w in line.points.windows(2) {
            best = best.min(dist_to_segment(apex, w[0], w[1]));
        }
        if let (true, Some(first), Some(last)) = (line.closed, line.points.first(), line.points.last()) {
            best = best.min(dist_to_segment(apex, *last, *first));
        }
    }
    best
}

fn open_house() -> (HeadlessEditorHarness, DocumentId) {
    let (mut h, doc) = opened_with(&house());
    edit(&mut h, doc, |f| f.smoothness_mm = s("0.6"));
    (h, doc)
}

#[test]
fn a_click_near_a_corner_protects_it_and_the_trace_keeps_a_node_on_it() {
    let (mut h, doc) = open_house();
    assert!(apex_distance(&h, doc) > 0.15, "the fixture loses the apex: {}", apex_distance(&h, doc));
    pick(&mut h, doc, TracePick::BeginProtectCorners);
    assert!(matches!(prep(&h, doc).pick.as_ref().map(|k| &k.kind), Some(PickKind::ProtectCorners)));
    // The click is nearly two pixels beside the apex, on the roof.
    pick(&mut h, doc, TracePick::ProtectAt { x: 81.0, y: 45.5, reach_px: 3.0 });
    h.run_tasks();
    let p = prep(&h, doc);
    assert_eq!(p.issue, None);
    assert_eq!(p.corners.len(), 1);
    let c = &p.corners[0];
    assert!((c.x_mm - 8.0).abs() < 0.1 && (c.y_mm - 8.6).abs() < 0.1, "the corner is the apex: {c:?}");
    assert!(apex_distance(&h, doc) < 0.1, "the outline passes through the apex: {}", apex_distance(&h, doc));
    // The corner is marked on the canvas, as a node.
    assert!(count(&h, doc, |o| matches!(o, Overlay::Node { selected: true, .. })) >= 1);
}

#[test]
fn a_second_click_or_the_remove_intent_takes_a_protected_corner_back() {
    let (mut h, doc) = open_house();
    pick(&mut h, doc, TracePick::BeginProtectCorners);
    pick(&mut h, doc, TracePick::ProtectAt { x: 81.0, y: 45.5, reach_px: 3.0 });
    h.run_tasks();
    assert_eq!(prep(&h, doc).corners.len(), 1);
    pick(&mut h, doc, TracePick::ProtectAt { x: 80.0, y: 44.0, reach_px: 3.0 });
    h.run_tasks();
    assert!(prep(&h, doc).corners.is_empty());
    assert!(apex_distance(&h, doc) > 0.15, "rounded off again");

    pick(&mut h, doc, TracePick::ProtectAt { x: 81.0, y: 45.5, reach_px: 3.0 });
    h.run_tasks();
    send(&mut h, doc, TracePrepIntent::RemoveProtectedCorner { index: 0 });
    h.run_tasks();
    assert!(prep(&h, doc).corners.is_empty());
    send(&mut h, doc, TracePrepIntent::RemoveProtectedCorner { index: 3 });
    assert_eq!(prep(&h, doc).issue, Some(PrepIssue::UnknownCorner));
}

#[test]
fn a_click_far_from_every_outline_protects_no_corner() {
    let (mut h, doc) = open_house();
    pick(&mut h, doc, TracePick::BeginProtectCorners);
    pick(&mut h, doc, TracePick::ProtectAt { x: 5.0, y: 5.0, reach_px: 3.0 });
    let p = prep(&h, doc);
    assert_eq!(p.issue, Some(PrepIssue::NothingToProtect));
    assert!(p.corners.is_empty());
}

#[test]
fn protected_corners_are_stored_with_the_applied_trace_and_come_back_when_it_is_reopened() {
    use burnburn_presentation::CommandKey;
    let (mut h, doc) = open_house();
    pick(&mut h, doc, TracePick::BeginProtectCorners);
    pick(&mut h, doc, TracePick::ProtectAt { x: 81.0, y: 45.5, reach_px: 3.0 });
    h.run_tasks();
    pick(&mut h, doc, TracePick::End);
    let before = prep(&h, doc).corners;
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
    assert_eq!(prep(&h, doc).corners, before);
    assert!(apex_distance(&h, doc) < 0.1);
}
