//! Finger joints, tab/slot and the finger-jointed box against analytical oracles.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use burnburn_generators::{
    BoxParams, EdgeStart, FingerJoint, GenError, Lid, PanelRole, SegmentKind, TabSlot, finger_box, tab_and_slot,
};
use proptest::prelude::*;

const EPS: f64 = 1e-9;

fn joint(thickness: f64) -> FingerJoint {
    FingerJoint { length: 100.0, thickness, finger_width: 10.0, fit: 0.0 }
}

fn widths(p: &burnburn_generators::EdgeProfile, kind: SegmentKind) -> Vec<f64> {
    p.segments.iter().filter(|s| s.kind == kind).map(|s| s.to - s.from).collect()
}

/// IM22 first test. Kerf is CAM's (OperationSettings), applied once there: the generator has no
/// kerf input, so its slots are nominal. Thickness only changes the slot *depth*; `fit` is a
/// separate design clearance that widens slots by exactly its value.
#[test]
fn material_thickness_changes_slot_but_does_not_apply_kerf_twice() {
    // 100 mm / 10 mm = 10 → largest odd count 9; every segment 100/9 mm.
    let seg = 100.0 / 9.0;
    for t in [3.0, 6.0] {
        let p = joint(t).profile(EdgeStart::Finger).unwrap();
        assert_eq!(p.depth, t, "slot depth is the material thickness");
        assert_eq!(p.segments.len(), 9);
        for w in widths(&p, SegmentKind::Slot).into_iter().chain(widths(&p, SegmentKind::Finger)) {
            assert!((w - seg).abs() < EPS, "nominal width {w} (no kerf subtracted or added)");
        }
    }
    let mut fitted = joint(3.0);
    fitted.fit = 0.1;
    let p = fitted.profile(EdgeStart::Finger).unwrap();
    for w in widths(&p, SegmentKind::Slot) {
        assert!((w - (seg + 0.1)).abs() < EPS, "fit widens a slot by exactly fit: {w}");
    }
    // Interior fingers lose fit/2 on each side; the end fingers only on their inner side.
    let f = widths(&p, SegmentKind::Finger);
    assert!((f[0] - (seg - 0.05)).abs() < EPS && (f[2] - (seg - 0.1)).abs() < EPS && (f[4] - (seg - 0.05)).abs() < EPS);
    assert!((p.segments.last().unwrap().to - 100.0).abs() < EPS);
}

#[test]
fn mating_edges_are_complementary() {
    let j = joint(3.0);
    let a = j.profile(EdgeStart::Finger).unwrap();
    let b = j.profile(EdgeStart::Slot).unwrap();
    assert_eq!((a.count(SegmentKind::Finger), a.count(SegmentKind::Slot)), (5, 4));
    assert_eq!((b.count(SegmentKind::Finger), b.count(SegmentKind::Slot)), (4, 5));
    for (x, y) in a.segments.iter().zip(&b.segments) {
        assert!((x.from - y.from).abs() < EPS && (x.to - y.to).abs() < EPS);
        assert_ne!(x.kind, y.kind, "a finger meets a slot");
    }
}

#[test]
fn invalid_joint_parameters_are_concrete_errors() {
    let bad = |f: fn(&mut FingerJoint)| {
        let mut j = joint(3.0);
        f(&mut j);
        j.profile(EdgeStart::Finger).unwrap_err()
    };
    assert_eq!(bad(|j| j.thickness = -1.0), GenError::NotPositive { param: "thickness" });
    assert_eq!(bad(|j| j.finger_width = f64::NAN), GenError::NotFinite { param: "finger_width" });
    assert_eq!(bad(|j| j.fit = -0.1), GenError::Negative { param: "fit" });
    // Fewer than three segments fit.
    assert_eq!(
        bad(|j| j.finger_width = 40.0),
        GenError::Count { param: "fingers", min: 3, max: u32::MAX as u64, value: 2 }
    );
    // Fit that eats the whole finger.
    assert!(matches!(bad(|j| j.fit = 11.2), GenError::TooSmall { param: "fit", .. }));
}

#[test]
fn tab_and_slot_slots_match_tabs_plus_fit() {
    let r = tab_and_slot(&TabSlot { length: 120.0, tabs: 3, tab_width: 10.0, thickness: 4.0, fit: 0.2 }).unwrap();
    assert_eq!(r.centres, [20.0, 60.0, 100.0]);
    assert_eq!(r.tab_edge.count(SegmentKind::Finger), 3);
    assert_eq!(r.tab_edge.depth, 4.0);
    assert_eq!(r.slots.len(), 3);
    for (slot, c) in r.slots.iter().zip(&r.centres) {
        let b = slot.bounds().unwrap();
        assert!((b.width() - 10.2).abs() < EPS && (b.height() - 4.2).abs() < EPS);
        assert!((b.center().x - c).abs() < EPS && b.center().y.abs() < EPS);
        assert!(slot.subpaths().iter().all(|s| s.is_closed()));
    }
    for s in r.tab_edge.segments.iter().filter(|s| s.kind == SegmentKind::Finger) {
        assert!((s.to - s.from - 10.0).abs() < EPS, "tabs are nominal");
    }
    let err = |p: TabSlot| tab_and_slot(&p).unwrap_err();
    assert_eq!(
        err(TabSlot { length: 120.0, tabs: 0, tab_width: 10.0, thickness: 4.0, fit: 0.0 }),
        GenError::Count { param: "tabs", min: 1, max: 6, value: 0 }
    );
    assert!(matches!(
        err(TabSlot { length: 30.0, tabs: 3, tab_width: 10.0, thickness: 4.0, fit: 0.0 }),
        GenError::Count { param: "tabs", .. }
    ));
}

fn params(lid: Lid) -> BoxParams {
    BoxParams { width: 100.0, depth: 60.0, height: 40.0, thickness: 3.0, finger_width: 10.0, lid, spacing: 5.0 }
}

/// Analytic area of a panel a×b: every edge with joint segments n (odd), slots recessed by t.
/// An edge of kind `Finger` start has (n-1)/2 slots, `Slot` start (n+1)/2; where two slot
/// segments meet at a corner, the t×t square is counted twice and added back.
fn analytic_area(a: f64, b: f64, t: f64, edges: [(usize, bool, f64); 4]) -> f64 {
    let mut area = a * b;
    for (n, starts_with_slot, len) in edges {
        if n == 0 {
            continue; // flat edge
        }
        let slots = if starts_with_slot { n.div_ceil(2) } else { (n - 1) / 2 };
        area -= t * slots as f64 * len / n as f64;
    }
    for i in 0..4 {
        let (e, f) = (edges[i], edges[(i + 1) % 4]);
        if e.0 > 0 && f.0 > 0 && e.1 && f.1 {
            area += t * t;
        }
    }
    area
}

#[test]
fn closed_box_panels_have_outer_sizes_counts_and_areas() {
    let panels = finger_box(&params(Lid::Closed)).unwrap();
    let roles: Vec<_> = panels.iter().map(|p| p.role).collect();
    assert_eq!(
        roles,
        [PanelRole::Bottom, PanelRole::Top, PanelRole::Front, PanelRole::Back, PanelRole::Left, PanelRole::Right]
    );
    // Counts: W 100/10 → 9, D 60/10 → 5, H 40/10 → 3.
    let (nw, nd, nh, t) = (9, 5, 3, 3.0);
    for p in &panels {
        let b = p.path.bounds().unwrap();
        let (w, h) = match p.role {
            PanelRole::Bottom | PanelRole::Top => (100.0, 60.0),
            PanelRole::Front | PanelRole::Back => (100.0, 40.0),
            PanelRole::Left | PanelRole::Right => (60.0, 40.0),
        };
        assert!((b.width() - w).abs() < EPS && (b.height() - h).abs() < EPS, "{:?}", p.role);
        assert_eq!((p.width, p.height), (w, h));
        assert_eq!(p.path.subpaths().len(), 1);
        assert!(p.path.subpaths()[0].is_closed());
        let expected = match p.role {
            PanelRole::Bottom | PanelRole::Top => {
                analytic_area(w, h, t, [(nw, true, w), (nd, true, h), (nw, true, w), (nd, true, h)])
            }
            PanelRole::Front | PanelRole::Back => {
                analytic_area(w, h, t, [(nw, false, w), (nh, false, h), (nw, false, w), (nh, false, h)])
            }
            PanelRole::Left | PanelRole::Right => {
                analytic_area(w, h, t, [(nd, false, w), (nh, true, h), (nd, false, w), (nh, true, h)])
            }
        };
        assert!(
            (p.path.signed_area().abs() - expected).abs() < 1e-6,
            "{:?}: {} vs {expected}",
            p.role,
            p.path.signed_area()
        );
    }
    // Panels do not overlap in the flat layout.
    for (i, a) in panels.iter().enumerate() {
        for b in &panels[i + 1..] {
            assert!(!a.path.bounds().unwrap().intersects(&b.path.bounds().unwrap()), "{:?} {:?}", a.role, b.role);
        }
    }
}

#[test]
fn shared_box_edges_are_complementary() {
    let panels = finger_box(&params(Lid::Closed)).unwrap();
    let get = |r| panels.iter().find(|p| p.role == r).unwrap();
    let (bottom, front, left) = (get(PanelRole::Bottom), get(PanelRole::Front), get(PanelRole::Left));
    let pairs = [
        (&front.edges[0], &bottom.edges[0]), // front bottom edge ↔ bottom front edge (W)
        (&front.edges[1], &left.edges[3]),   // front side ↔ left side (H)
        (&left.edges[0], &bottom.edges[3]),  // left bottom ↔ bottom left edge (D)
    ];
    for (a, b) in pairs {
        let (a, b) = (a.as_ref().unwrap(), b.as_ref().unwrap());
        assert_eq!(a.segments.len(), b.segments.len());
        for (x, y) in a.segments.iter().zip(&b.segments) {
            assert!((x.from - y.from).abs() < EPS && (x.to - y.to).abs() < EPS);
            assert_ne!(x.kind, y.kind);
        }
    }
}

#[test]
fn open_box_has_five_panels_with_flat_top_edges() {
    let panels = finger_box(&params(Lid::Open)).unwrap();
    assert_eq!(panels.len(), 5);
    assert!(panels.iter().all(|p| p.role != PanelRole::Top));
    for p in panels.iter().filter(|p| p.role != PanelRole::Bottom) {
        assert!(p.edges[2].is_none(), "{:?} top edge is flat", p.role);
    }
}

#[test]
fn box_rejects_fingers_not_wider_than_the_material() {
    let mut p = params(Lid::Closed);
    p.finger_width = 3.0;
    assert!(matches!(finger_box(&p).unwrap_err(), GenError::TooSmall { param: "finger_width", .. }));
    p = params(Lid::Closed);
    p.height = 5.0;
    assert!(finger_box(&p).is_err());
}

proptest! {
    /// Thickness never changes the panel's outer size, only the joint depth.
    #[test]
    fn box_outer_size_is_independent_of_thickness(t in 1.0f64..4.0, w in 80.0f64..200.0) {
        let mut p = params(Lid::Closed);
        p.thickness = t;
        p.width = w;
        let panels = finger_box(&p).unwrap();
        let front = panels.iter().find(|x| x.role == PanelRole::Front).unwrap();
        let b = front.path.bounds().unwrap();
        prop_assert!((b.width() - w).abs() < 1e-9 && (b.height() - 40.0).abs() < 1e-9);
    }
}
