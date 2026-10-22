//! G7-trace, T05 and T06: an area retrace that leaves chosen contours alone and aims at a number of
//! nodes, and pieces that "ignore less than" dropped that come back one by one (or are protected
//! from the filter beforehand).

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)] // test helpers

mod g7_support;

use burnburn_presentation::{CommandKey, PrepField, PrepIssue, SelectMode, TracePrepIntent, UiIntent};
use g7_support::*;

const REGION: [&str; 4] = ["30", "40", "90", "70"];

fn hole_id(h: &burnburn_presentation::HeadlessEditorHarness, doc: burnburn_types::DocumentId) -> u64 {
    let rows = prep(h, doc).contours;
    let holes: Vec<_> = rows.iter().filter(|c| c.hole).collect();
    assert_eq!(holes.len(), 1, "the counter of the ring: {rows:?}");
    holes[0].id
}

// ---------------------------------------------------------------------------------------------
// T05, protected contours
// ---------------------------------------------------------------------------------------------

#[test]
fn the_panel_lists_the_contours_of_the_result_with_kind_area_and_place() {
    let (h, doc) = opened_scene();
    let rows = prep(&h, doc).contours;
    assert_eq!(rows.len(), 4, "ring, its counter, rectangle, far disc");
    assert_eq!(rows.iter().filter(|c| c.hole).count(), 1);
    // Areas are analytic: ring π·3.5², counter π·1.5², rectangle 7.0 × 8.0, far disc π·0.8² (mm²).
    let want_outer = [std::f64::consts::PI * 12.25, 56.0, std::f64::consts::PI * 0.64];
    for w in want_outer {
        assert!(rows.iter().any(|c| !c.hole && (c.area_mm2 - w).abs() < 0.04 * w), "{w}: {rows:?}");
    }
    let hole = rows.iter().find(|c| c.hole).unwrap();
    assert!((hole.area_mm2 - std::f64::consts::PI * 2.25).abs() < 0.3);
    // The counter is in the middle of the ring: (5, 6) mm from the left bottom corner (y up).
    assert!((hole.x_mm - 5.0).abs() < 0.2 && (hole.y_mm - 6.0).abs() < 0.2, "{hole:?}");
    assert!(rows.iter().all(|c| !c.protected && !c.kept));
}

#[test]
fn a_protected_contour_stays_as_it_is_inside_the_retraced_area() {
    let (mut h, doc) = opened_scene();
    let hole = hole_id(&h, doc);
    let before = subpaths(&h, doc);
    // Without protection a threshold of 200 fills the counter (150) and picks up the speck (170).
    retrace(&mut h, doc, REGION, "200", "");
    assert_eq!(counts(&h, doc), (4, 0));
    send(&mut h, doc, TracePrepIntent::RemoveCorrection { index: 0 });
    assert_eq!(counts(&h, doc), (3, 1));

    send(&mut h, doc, TracePrepIntent::ProtectContour { id: hole, on: true });
    assert!(prep(&h, doc).contours.iter().any(|c| c.id == hole && c.protected));
    retrace(&mut h, doc, REGION, "200", "");
    assert_eq!(prep(&h, doc).issue, None);
    assert_eq!(counts(&h, doc), (4, 1), "the speck is in, the counter is still a hole");
    let hole_path = |paths: &[burnburn_geometry::Subpath]| {
        let rows = prep(&h, doc).contours;
        let c = rows.iter().find(|c| c.id == hole).expect("the same contour id");
        let near = |s: &&burnburn_geometry::Subpath| {
            burnburn_geometry::Path::from_subpaths(vec![(*s).clone()]).bounds().is_some_and(|b| {
                // The ring's outer contour has the same middle; the counter is the narrow one.
                b.width() < 4.0
                    && ((b.min().x + b.max().x) / 2.0 - c.x_mm).abs() < 0.05
                    && ((b.min().y + b.max().y) / 2.0 - c.y_mm).abs() < 0.05
            })
        };
        paths.iter().filter(near).cloned().collect::<Vec<_>>()
    };
    let after = subpaths(&h, doc);
    assert_eq!(hole_path(&after).len(), 1);
    assert!(before.iter().any(|b| hole_path(&after).contains(b)), "bit-identical to the contour before the retrace");
    let rows = prep(&h, doc).corrections;
    assert_eq!((rows.len(), rows[0].protected), (1, 1));
    // The protection was for that retrace only.
    assert!(prep(&h, doc).contours.iter().all(|c| !c.protected));
}

#[test]
fn a_protected_contour_can_be_released_again_and_an_unknown_one_is_refused() {
    let (mut h, doc) = opened_scene();
    let hole = hole_id(&h, doc);
    send(&mut h, doc, TracePrepIntent::ProtectContour { id: hole, on: true });
    send(&mut h, doc, TracePrepIntent::ProtectContour { id: hole, on: false });
    assert!(prep(&h, doc).contours.iter().all(|c| !c.protected));
    retrace(&mut h, doc, REGION, "200", "");
    assert_eq!(counts(&h, doc), (4, 0), "nothing was protected any more");

    send(&mut h, doc, TracePrepIntent::ProtectContour { id: 999, on: true });
    assert_eq!(prep(&h, doc).issue, Some(PrepIssue::UnknownContour));
}

#[test]
fn protected_contours_are_stored_with_the_correction_and_applied_again_on_reopening() {
    let (mut h, doc) = opened_scene();
    let hole = hole_id(&h, doc);
    send(&mut h, doc, TracePrepIntent::ProtectContour { id: hole, on: true });
    retrace(&mut h, doc, REGION, "200", "");
    let corrected = counts(&h, doc);
    h.invoke(doc, CommandKey::ApplyTraceLinked);
    let ids: Vec<_> = vm(&h, doc).objects.iter().map(|o| o.id).collect();
    h.invoke(doc, CommandKey::CloseTracer);
    h.send(UiIntent::Select { document: doc, ids: vec![ids[0]], mode: SelectMode::Replace });
    h.invoke(doc, CommandKey::TraceEdit);
    h.run_tasks();
    assert_eq!(counts(&h, doc), corrected, "the protection is part of the stored correction");
    assert_eq!(prep(&h, doc).corrections[0].protected, 1);
}

// ---------------------------------------------------------------------------------------------
// T05, node goal
// ---------------------------------------------------------------------------------------------

#[test]
fn a_node_goal_is_reached_by_a_coarser_tolerance_and_the_correction_row_says_so() {
    let (mut h, doc) = opened_scene();
    let base_tolerance = 0.05; // the default fit budget of the tracer, mm

    // A goal that is met at once changes nothing about the tolerance: this is how many nodes the
    // whole result has.
    send(&mut h, doc, TracePrepIntent::SetNodeTarget { max_nodes: s("1000"), max_tolerance_mm: s("2") });
    let t = prep(&h, doc).node_target.expect("the goal is shown");
    assert_eq!((t.max_nodes.as_str(), t.max_tolerance_mm.as_str()), ("1000", "2"));
    retrace(&mut h, doc, REGION, "200", "");
    assert_eq!(prep(&h, doc).issue, None);
    let relaxed = prep(&h, doc).corrections[0].node_goal.expect("the correction has a goal");
    assert!(relaxed.met && (relaxed.tolerance_mm - base_tolerance).abs() < 1e-12, "{relaxed:?}");
    assert!(prep(&h, doc).node_target.is_none(), "the goal was for that retrace only");
    send(&mut h, doc, TracePrepIntent::RemoveCorrection { index: 0 });

    // A few nodes fewer than that needs a coarser fit of the retraced parts.
    let goal = relaxed.nodes - 3;
    send(&mut h, doc, TracePrepIntent::SetNodeTarget { max_nodes: goal.to_string(), max_tolerance_mm: s("2") });
    retrace(&mut h, doc, REGION, "200", "");
    let tight = prep(&h, doc).corrections[0].node_goal.expect("a goal");
    assert_eq!(tight.max_nodes, goal);
    assert!(tight.met && tight.nodes <= goal, "{tight:?}");
    assert!(tight.tolerance_mm > base_tolerance && tight.tolerance_mm <= 2.0, "{tight:?}");
}

#[test]
fn a_goal_that_cannot_be_met_is_said_not_hidden() {
    let (mut h, doc) = opened_scene();
    send(&mut h, doc, TracePrepIntent::SetNodeTarget { max_nodes: s("1"), max_tolerance_mm: s("0.06") });
    retrace(&mut h, doc, REGION, "200", "");
    let goal = prep(&h, doc).corrections[0].node_goal.expect("a goal");
    assert!(!goal.met, "one node in the whole scene within 0.06 mm cannot be: {goal:?}");
    assert!(goal.nodes > 1);
}

#[test]
fn a_goal_is_cleared_with_two_empty_fields_and_a_bad_goal_is_refused_where_it_was_typed() {
    let (mut h, doc) = opened_scene();
    send(&mut h, doc, TracePrepIntent::SetNodeTarget { max_nodes: s("40"), max_tolerance_mm: s("2") });
    send(&mut h, doc, TracePrepIntent::SetNodeTarget { max_nodes: s(""), max_tolerance_mm: s("") });
    assert!(prep(&h, doc).node_target.is_none() && prep(&h, doc).issue.is_none());
    for (nodes, tol, want) in [
        ("abc", "2", PrepIssue::InvalidNumber(PrepField::NodeLimit)),
        ("0", "2", PrepIssue::OutOfRange(PrepField::NodeLimit)),
        ("2.5", "2", PrepIssue::OutOfRange(PrepField::NodeLimit)),
        ("40", "x", PrepIssue::InvalidNumber(PrepField::NodeTolerance)),
        // Coarser than the tolerance of the trace (0.05 mm by default) is required.
        ("40", "0.01", PrepIssue::OutOfRange(PrepField::NodeTolerance)),
        ("40", "", PrepIssue::InvalidNumber(PrepField::NodeTolerance)),
    ] {
        send(&mut h, doc, TracePrepIntent::SetNodeTarget { max_nodes: s(nodes), max_tolerance_mm: s(tol) });
        assert_eq!(prep(&h, doc).issue, Some(want), "{nodes:?} {tol:?}");
        assert!(prep(&h, doc).node_target.is_none());
    }
}

// ---------------------------------------------------------------------------------------------
// T06, restore and protect against "ignore less than"
// ---------------------------------------------------------------------------------------------

#[test]
fn a_piece_the_filter_dropped_comes_back_by_itself_from_the_report() {
    let (mut h, doc) = opened_scene();
    // The far disc is π·0.8² = 2.0 mm²: below 3 mm² it is dropped (the counter, 7.1 mm², is not).
    edit(&mut h, doc, |f| f.ignore_below_mm2 = "3".to_owned());
    assert_eq!(counts(&h, doc), (2, 1));
    let report = prep(&h, doc).report.expect("a report");
    assert_eq!(report.removed.len(), 1);

    send(&mut h, doc, TracePrepIntent::RestoreRemoved { index: 0 });
    assert_eq!(prep(&h, doc).issue, None);
    assert_eq!(counts(&h, doc), (3, 1), "the disc is back; the filter is still 3 mm²");
    assert!(prep(&h, doc).report.unwrap().removed.is_empty());
    assert!(prep(&h, doc).contours.iter().any(|c| c.kept), "and it is listed as kept");

    // The filter keeps working for everything else: raising it drops nothing that is kept.
    edit(&mut h, doc, |f| f.ignore_below_mm2 = "5".to_owned());
    assert_eq!(counts(&h, doc), (3, 1), "the kept disc survives a stricter filter");
    send(&mut h, doc, TracePrepIntent::RestoreRemoved { index: 7 });
    assert_eq!(prep(&h, doc).issue, Some(PrepIssue::UnknownRemoved));
}

#[test]
fn a_hole_protected_beforehand_survives_a_stricter_filter_and_can_be_released() {
    let (mut h, doc) = opened_scene();
    let hole = hole_id(&h, doc);
    send(&mut h, doc, TracePrepIntent::KeepContour { id: hole, keep: true });
    assert!(prep(&h, doc).contours.iter().any(|c| c.id == hole && c.kept));
    // 10 mm² drops the far disc (2.0) and would drop the counter (7.1) too.
    edit(&mut h, doc, |f| f.ignore_below_mm2 = "10".to_owned());
    assert_eq!(counts(&h, doc), (2, 1), "ring with its protected counter, and the rectangle");
    let removed = prep(&h, doc).report.unwrap().removed;
    assert_eq!(removed.len(), 1);
    assert!(!removed[0].hole);

    let id = prep(&h, doc).contours.iter().find(|c| c.hole).unwrap().id;
    send(&mut h, doc, TracePrepIntent::KeepContour { id, keep: false });
    assert_eq!(counts(&h, doc), (2, 0), "released: the filter takes the counter");
    send(&mut h, doc, TracePrepIntent::KeepContour { id: 4242, keep: true });
    assert_eq!(prep(&h, doc).issue, Some(PrepIssue::UnknownContour));
}

#[test]
fn kept_pieces_are_stored_with_the_trace_and_found_again_on_reopening() {
    let (mut h, doc) = opened_scene();
    edit(&mut h, doc, |f| f.ignore_below_mm2 = "3".to_owned());
    send(&mut h, doc, TracePrepIntent::RestoreRemoved { index: 0 });
    h.invoke(doc, CommandKey::ApplyTraceLinked);
    let ids: Vec<_> = vm(&h, doc).objects.iter().map(|o| o.id).collect();
    assert_eq!(ids.len(), 3, "ring with its counter, rectangle and the restored disc");
    h.invoke(doc, CommandKey::CloseTracer);
    h.send(UiIntent::Select { document: doc, ids: vec![ids[0]], mode: SelectMode::Replace });
    h.invoke(doc, CommandKey::TraceEdit);
    h.run_tasks();
    assert_eq!(counts(&h, doc), (3, 1));
    assert!(prep(&h, doc).contours.iter().any(|c| c.kept));
}
