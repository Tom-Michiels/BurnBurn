//! Common-line cutting, ordering rule of contract IM32: a shared stretch is cut as part of the *last*
//! cut shape, after the inner features of both shapes. Oracles are analytic (rectangle perimeters)
//! and structural (the position of the shared cut among the cuts of the holes).

#![allow(clippy::unwrap_used)]

mod common;

use burnburn_document::process::{AdvancedOptions, CommonLine, ProcessCommand};
use burnburn_document::{CommandPayload, DocumentSession};
use burnburn_job::{Feature, PlanEvent};
use burnburn_types::GeometryId;
use common::*;

type Cut = ((f64, f64), (f64, f64));

/// Every `Cut` of the plan as `(from, to)`, in plan order.
fn cuts(s: &DocumentSession) -> Vec<Cut> {
    let plan = plan_from_outside(s, &supporting(&[Feature::CommonLine])).unwrap();
    let mut at = plan.start();
    let mut out = Vec::new();
    for e in plan.events() {
        match *e {
            PlanEvent::Travel { to } | PlanEvent::CutPower { to, .. } => at = to,
            PlanEvent::Cut { to, .. } => {
                out.push(((at.x_mm(), at.y_mm()), (to.x_mm(), to.y_mm())));
                at = to;
            }
            _ => {}
        }
    }
    out
}

/// `count` 20 x 10 plates in a row, 0.03 mm apart, each with a 4 x 4 hole; one Line operation with common-line.
fn row_with_holes(count: u32) -> DocumentSession {
    let mut s = session();
    let mut ids: Vec<GeometryId> = Vec::new();
    for i in 0..count {
        let x = f64::from(i) * 20.03;
        ids.push(rectangle(&mut s, x, 0.0, 20.0, 10.0));
        ids.push(rectangle(&mut s, x + 8.0, 3.0, 4.0, 4.0));
    }
    let op = add_op(&mut s, line_op("Snijden", 10.0, 50.0), ids);
    let advanced =
        AdvancedOptions { common_line: Some(CommonLine::new(mm(0.05), mm(2.0)).unwrap()), ..Default::default() };
    run(&mut s, CommandPayload::Process(ProcessCommand::SetAdvanced { operation: op, advanced }));
    s
}

/// The hole of plate `i` (a 4 x 4 square in the middle of the 20 x 10 plate).
fn in_hole_of(p: (f64, f64), i: u32) -> bool {
    let x = f64::from(i) * 20.03;
    (x + 8.0 - 1e-6..=x + 12.0 + 1e-6).contains(&p.0) && (3.0 - 1e-6..=7.0 + 1e-6).contains(&p.1)
}

fn in_hole(p: (f64, f64), count: u32) -> bool {
    (0..count).any(|i| in_hole_of(p, i))
}

/// The seam `k` (between plate `k - 1` and plate `k`) of a cut that runs along it, if it does.
fn seam_of(c: &Cut, count: u32) -> Option<u32> {
    (1..count).find(|k| {
        let x = f64::from(*k) * 20.03;
        [x - 0.03, x].iter().any(|edge| (c.0.0 - edge).abs() < 1e-6 && (c.1.0 - edge).abs() < 1e-6)
    })
}

fn is_shared(c: &Cut, count: u32) -> bool {
    seam_of(c, count).is_some()
}

fn assert_shared_after_the_holes_of_both(count: u32) {
    let s = row_with_holes(count);
    let cuts = cuts(&s);
    let seams: Vec<(usize, u32)> = cuts.iter().enumerate().filter_map(|(i, c)| Some((i, seam_of(c, count)?))).collect();
    assert_eq!(seams.len(), (count - 1) as usize, "one shared stretch per seam: {cuts:?}");
    for (at, k) in seams {
        for plate in [k - 1, k] {
            let last_hole = cuts.iter().rposition(|c| in_hole_of(c.0, plate) && in_hole_of(c.1, plate)).unwrap();
            assert!(
                at > last_hole,
                "seam {k} (cut {at}) follows the hole of plate {plate} (cut {last_hole}): {cuts:?}"
            );
        }
    }
}

#[test]
fn the_shared_stretch_of_two_plates_is_cut_after_the_holes_of_both() {
    assert_shared_after_the_holes_of_both(2);
}

#[test]
fn every_seam_of_a_row_of_plates_is_cut_after_the_holes() {
    assert_shared_after_the_holes_of_both(3);
}

#[test]
fn the_total_length_is_unchanged_by_the_ordering_rule() {
    // Perimeter 60 + hole 16 per plate, minus 10 mm per seam.
    for count in [2u32, 3] {
        let s = row_with_holes(count);
        let total: f64 = cuts(&s).iter().map(|c| ((c.1.0 - c.0.0).powi(2) + (c.1.1 - c.0.1).powi(2)).sqrt()).sum();
        let expected = f64::from(count) * 76.0 - f64::from(count - 1) * 10.0;
        assert!((total - expected).abs() < 1e-6, "{total} vs {expected}");
    }
}

#[test]
fn no_inner_feature_is_cut_after_the_first_shared_stretch() {
    let s = row_with_holes(2);
    let cuts = cuts(&s);
    let shared = cuts.iter().position(|c| is_shared(c, 2)).unwrap();
    let holes_after = cuts.iter().enumerate().filter(|(i, c)| *i > shared && in_hole(c.0, 2)).count();
    assert_eq!(holes_after, 0, "{cuts:?}");
}
