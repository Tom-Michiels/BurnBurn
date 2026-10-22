//! G9-cam-werk, W06: recognition of a mirrored product, of a product seen at an angle and of a
//! product partly outside the camera picture. Fake camera, rendered frames with analytic ground truth
//! (the frame is made by evaluating the product at the position the forward model sends back), the
//! real controller and task port. The operator still decides about every candidate, and a partly
//! visible one says so in the view model.
//!
//! Forward model: see `crates/burnburn-image/tests/g9_locate_pose.rs`.

#![allow(clippy::unwrap_used, clippy::panic, clippy::expect_used)] // test helpers

mod common;
#[path = "common/camera.rs"]
mod rig;

use std::sync::Arc;

use burnburn_geometry::ProductionTolerance;
use burnburn_image::SourceImage;
use burnburn_presentation::{
    CandidateDecision as D, CommandKey, HeadlessEditorHarness, RecognitionIntent as R, RecognitionProblem as P,
    RecognitionViewModel, SelectMode, UiIntent,
};
use burnburn_types::{DocumentId, GeometryId, Length};
use rig::*;

const A: &str = "cam-A";
const W: u32 = 160;
const H: u32 = 120;
const MAP: Mapping = Mapping { mm_per_px: 0.5, origin_mm: [100.0, 300.0] };
/// Half the diagonal of the 29 x 29 template.
const RADIUS: f64 = 20.506_096_654_409_877;

fn speckle(x: i64, y: i64) -> f64 {
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 29;
    h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 32;
    (h % 1000) as f64 / 1000.0 * 6.0 - 3.0
}

/// A blob with three features of different strength, size and distance: handed, so that it differs
/// from its mirror image by more than a turn.
fn product(u: f64, v: f64) -> f64 {
    90.0 * (-(u * u + v * v) / 32.0).exp()
        - 45.0 * (-((u + 10.0).powi(2) + (v - 6.0).powi(2)) / 18.0).exp()
        + 60.0 * (-((u - 11.0).powi(2) + (v + 8.0).powi(2)) / 12.5).exp()
        + 50.0 * (-((u - 4.0).powi(2) + (v - 12.0).powi(2)) / 8.0).exp()
}

#[derive(Clone, Copy)]
struct Posed {
    cx: f64,
    cy: f64,
    angle: f64,
    mirrored: bool,
    tilt: (f64, f64),
}

fn upright(cx: f64, cy: f64) -> Posed {
    Posed { cx, cy, angle: 0.0, mirrored: false, tilt: (0.0, 0.0) }
}

impl Posed {
    /// The template offset that lands on the pixel offset `(dx, dy)` from the centre, if any.
    fn template_offset(&self, dx: f64, dy: f64) -> Option<(f64, f64)> {
        let a = self.angle.to_radians();
        let (c, s) = (a.cos(), a.sin());
        let (tx, ty) = (self.tilt.0 / RADIUS, self.tilt.1 / RADIUS);
        let (m11, m12, m21, m22) = (c - dx * tx, -s - dx * ty, s - dy * tx, c - dy * ty);
        let det = m11 * m22 - m12 * m21;
        if det.abs() < 1e-12 {
            return None;
        }
        let (mx, my) = ((m22 * dx - m12 * dy) / det, (-m21 * dx + m11 * dy) / det);
        Some((if self.mirrored { -mx } else { mx }, my))
    }
}

fn photo(at: &[Posed]) -> SourceImage {
    let mut rgba = Vec::with_capacity((W * H * 4) as usize);
    for y in 0..H {
        for x in 0..W {
            let mut value = 128.0 + speckle(i64::from(x), i64::from(y));
            for p in at {
                if let Some((u, v)) = p.template_offset(f64::from(x) - p.cx, f64::from(y) - p.cy)
                    && u.abs() <= 24.0
                    && v.abs() <= 24.0
                {
                    value += product(u, v);
                }
            }
            let g = value.clamp(0.0, 255.0).round() as u8;
            rgba.extend_from_slice(&[g, g, g, 255]);
        }
    }
    SourceImage::from_rgba8(W, H, rgba).unwrap()
}

struct Rig {
    h: HeadlessEditorHarness,
    doc: DocumentId,
    cam: Arc<FakeCam>,
}

fn rig() -> Rig {
    let cam = FakeCam::new(&[(A, &[mode(W, H)])]);
    cam.authorize();
    let mut h = HeadlessEditorHarness::new();
    h.controller().attach_camera(cam.clone());
    let doc = h.new_document().unwrap();
    h.run_tasks();
    Rig { h, doc, cam }
}

fn view(r: &Rig) -> RecognitionViewModel {
    r.h.document(r.doc).unwrap().recognition
}

fn send(r: &mut Rig, intent: R) {
    r.h.send(UiIntent::Recognition(intent));
    r.h.run_tasks();
}

fn show(r: &mut Rig, at: &[Posed]) {
    r.cam.show(A, mode(W, H), photo(at));
    r.h.send(UiIntent::CameraWorkspace(burnburn_presentation::CameraWorkspaceIntent::UseCamera {
        device: dev(A),
        mode: mode(W, H),
        lens: "standard".into(),
    }));
}

/// An upright product at (60, 60); the operator marks the 29 x 29 window around it.
fn learned() -> Rig {
    let mut r = rig();
    show(&mut r, &[upright(60.0, 60.0)]);
    send(
        &mut r,
        R::Learn {
            device: dev(A),
            x: "46".into(),
            y: "46".into(),
            width: "29".into(),
            height: "29".into(),
            name: "Kaartje".into(),
        },
    );
    assert_eq!(view(&r).problem, None);
    r
}

fn find(r: &mut Rig, min_percent: &str) {
    send(r, R::Find { min_percent: min_percent.into() });
}

fn extras(r: &mut Rig, mirror: bool, perspective: bool, partial: bool) {
    send(r, R::SetSearchExtras { mirror, perspective, partial });
}

fn calibrate(r: &mut Rig) {
    let mut store = burnburn_image::calib::CalibrationStore::default();
    store.insert_alignment(aligned(A, mode(W, H), 0.0, MAP));
    r.h.controller().attach_calibrations(store);
}

fn angle_gap(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

// --- the panel choices ---------------------------------------------------------------------------------------------

#[test]
fn the_new_search_choices_are_off_until_the_operator_asks_and_independent_of_rotation_and_scale() {
    let mut r = learned();
    assert_eq!(view(&r).search, Default::default());
    extras(&mut r, true, false, true);
    let s = view(&r).search;
    assert!(s.mirror && !s.perspective && s.partial && !s.rotation && !s.scale);
    send(&mut r, R::SetSearch { rotation: true, scale: false });
    let s = view(&r).search;
    assert!(s.rotation && s.mirror && s.partial, "setting rotation keeps the extras: {s:?}");
    extras(&mut r, false, true, false);
    let s = view(&r).search;
    assert!(s.rotation && !s.mirror && s.perspective && !s.partial, "setting the extras keeps rotation: {s:?}");
}

// --- mirrored ---------------------------------------------------------------------------------------------------------

#[test]
fn a_mirrored_product_is_a_pending_candidate_marked_mirrored_only_when_mirroring_is_switched_on() {
    let mut r = learned();
    show(&mut r, &[Posed { angle: 35.0, mirrored: true, ..upright(100.0, 70.0) }]);
    send(&mut r, R::SetSearch { rotation: true, scale: false });
    find(&mut r, "93");
    assert_eq!(view(&r).problem, Some(P::NothingFound), "the original is not that alike to its mirror image");

    extras(&mut r, true, false, false);
    find(&mut r, "93");
    let v = view(&r);
    assert_eq!(v.problem, None);
    assert_eq!(v.candidates.len(), 1, "{:?}", v.candidates);
    let c = &v.candidates[0];
    assert!(c.mirrored, "{c:?}");
    assert!((c.x_px - 100.5).abs() < 0.75 && (c.y_px - 70.5).abs() < 0.75, "{c:?}");
    assert!(angle_gap(c.angle_deg, 35.0) < 3.0, "{c:?}");
    assert_eq!((c.decision, c.visible_percent), (D::Pending, 100.0), "the operator decides; it is wholly visible");
}

#[test]
fn an_original_product_is_not_marked_mirrored_when_mirroring_is_allowed() {
    let mut r = learned();
    show(&mut r, &[Posed { angle: 20.0, ..upright(100.0, 70.0) }]);
    send(&mut r, R::SetSearch { rotation: true, scale: false });
    extras(&mut r, true, false, false);
    find(&mut r, "90");
    let v = view(&r);
    assert_eq!(v.candidates.len(), 1, "{:?} {:?}", v.candidates, v.problem);
    assert!(!v.candidates[0].mirrored, "{:?}", v.candidates[0]);
}

/// Draws a rectangle and returns the id of the one new object.
fn rectangle(r: &mut Rig, from: (f64, f64), to: (f64, f64)) -> GeometryId {
    let before: Vec<GeometryId> = r.h.document(r.doc).unwrap().objects.iter().map(|o| o.id).collect();
    r.h.invoke(r.doc, CommandKey::ToolRectangle);
    r.h.drag(r.doc, from, to);
    r.h.invoke(r.doc, CommandKey::ToolSelect);
    r.h.document(r.doc).unwrap().objects.iter().map(|o| o.id).find(|id| !before.contains(id)).unwrap()
}

fn corners_of(r: &Rig, ids: &[GeometryId], keep: bool) -> Vec<Vec<[f64; 2]>> {
    let tolerance = ProductionTolerance::new(Length::from_mm(0.01).unwrap()).unwrap();
    r.h.scene(r.doc)
        .unwrap()
        .items
        .iter()
        .filter(|i| ids.contains(&i.source) == keep)
        .map(|i| {
            let mut points: Vec<[f64; 2]> = i.path.flatten(&tolerance)[0].points.iter().map(|p| [p.x, p.y]).collect();
            if points.len() > 1 && points.first() == points.last() {
                points.pop();
            }
            points
        })
        .collect()
}

fn same_corners(found: &[[f64; 2]], want: &[[f64; 2]], tolerance: f64) -> bool {
    found.len() == want.len()
        && want.iter().all(|w| found.iter().any(|f| (f[0] - w[0]).hypot(f[1] - w[1]) <= tolerance))
}

#[test]
fn a_copy_put_on_a_mirrored_product_is_mirrored_about_the_centre_of_the_selection() {
    let mut r = learned();
    calibrate(&mut r);
    // A bar (10..20 x 10..14) and a small block beside it (22..26 x 10..13): the selection is 16 wide
    // with its centre at x = 18; mirrored, the block lies to the left of the bar.
    let square = rectangle(&mut r, (22.0, 10.0), (26.0, 13.0));
    let bar = rectangle(&mut r, (10.0, 10.0), (20.0, 14.0));
    r.h.send(UiIntent::Select { document: r.doc, ids: vec![bar, square], mode: SelectMode::Replace });
    show(&mut r, &[Posed { mirrored: true, ..upright(100.0, 70.0) }]);
    extras(&mut r, true, false, false);
    find(&mut r, "93");
    assert_eq!(view(&r).candidates.len(), 1, "{:?}", view(&r));
    assert!(view(&r).candidates[0].mirrored);
    send(&mut r, R::Decide { index: 0, decision: D::Accepted });
    let doc = r.doc;
    send(&mut r, R::Place { document: doc });
    assert_eq!(view(&r).problem, None);

    let target = MAP.bed([100.5, 70.5]);
    // The originals as drawn (the drag does not promise exact sizes): the selection centre is the
    // centre of their bounds; mirrored in x about it, the centre put on the target.
    let originals = corners_of(&r, &[bar, square], true);
    assert_eq!(originals.len(), 2);
    let all: Vec<[f64; 2]> = originals.iter().flatten().copied().collect();
    let (lo_x, hi_x) = all.iter().fold((f64::MAX, f64::MIN), |a, c| (a.0.min(c[0]), a.1.max(c[0])));
    let (lo_y, hi_y) = all.iter().fold((f64::MAX, f64::MIN), |a, c| (a.0.min(c[1]), a.1.max(c[1])));
    let (cx, cy) = ((lo_x + hi_x) / 2.0, (lo_y + hi_y) / 2.0);
    let mirrored = |corners: &[[f64; 2]]| -> Vec<[f64; 2]> {
        corners.iter().map(|c| [target[0] - (c[0] - cx), target[1] + (c[1] - cy)]).collect()
    };
    let copies = corners_of(&r, &[bar, square], false);
    assert_eq!(copies.len(), 2, "one copy of each object");
    for original in &originals {
        let want = mirrored(original);
        assert!(copies.iter().any(|c| same_corners(c, &want, 0.6)), "{copies:?} vs {want:?}");
    }
    // And it is really the mirror image: not the plain translation of the selection either.
    let plain: Vec<Vec<[f64; 2]>> = originals
        .iter()
        .map(|o| o.iter().map(|c| [target[0] + (c[0] - cx), target[1] + (c[1] - cy)]).collect())
        .collect();
    assert!(!plain.iter().all(|w| copies.iter().any(|c| same_corners(c, w, 0.6))), "not just moved");
}

// --- perspective ------------------------------------------------------------------------------------------------------

#[test]
fn a_product_seen_at_an_angle_is_found_when_perspective_is_switched_on() {
    let mut r = learned();
    show(&mut r, &[Posed { tilt: (0.45, 0.0), ..upright(100.0, 70.0) }]);
    find(&mut r, "98");
    assert_eq!(view(&r).problem, Some(P::NothingFound), "a flat search does not reach that similarity");
    extras(&mut r, false, true, false);
    find(&mut r, "98");
    let v = view(&r);
    assert_eq!(v.candidates.len(), 1, "{:?} {:?}", v.candidates, v.problem);
    let c = &v.candidates[0];
    assert!((c.x_px - 100.5).abs() < 1.0 && (c.y_px - 70.5).abs() < 1.0, "{c:?}");
    assert!(c.similarity_percent > 98.0, "{c:?}");
    assert_eq!(c.decision, D::Pending);
}

// --- partly visible ---------------------------------------------------------------------------------------------------

#[test]
fn a_product_at_the_edge_of_the_picture_is_found_and_said_to_be_partly_visible() {
    let mut r = learned();
    show(&mut r, &[upright(6.0, 70.0)]);
    find(&mut r, "90");
    assert_eq!(view(&r).problem, Some(P::NothingFound), "wholly inside the picture only, unless asked");

    extras(&mut r, false, false, true);
    find(&mut r, "90");
    let v = view(&r);
    assert_eq!(v.candidates.len(), 1, "{:?} {:?}", v.candidates, v.problem);
    let c = &v.candidates[0];
    assert!((c.x_px - 6.5).abs() < 1.0 && (c.y_px - 70.5).abs() < 1.0, "{c:?}");
    assert!(c.visible_percent > 65.0 && c.visible_percent < 78.0, "{c:?}");
    assert_eq!(c.decision, D::Pending, "a partly visible match waits for the operator like any other");
    // It cannot be placed before it is decided.
    let doc = r.doc;
    send(&mut r, R::Place { document: doc });
    assert_eq!(view(&r).problem, Some(P::Undecided));
}

#[test]
fn a_product_wholly_in_the_picture_reports_one_hundred_percent_with_partial_allowed() {
    let mut r = learned();
    show(&mut r, &[upright(100.0, 70.0)]);
    extras(&mut r, false, false, true);
    find(&mut r, "90");
    let v = view(&r);
    assert_eq!(v.candidates.len(), 1, "{:?}", v.candidates);
    assert_eq!(v.candidates[0].visible_percent, 100.0);
}

#[test]
fn a_search_without_the_extras_reports_an_unmirrored_wholly_visible_candidate() {
    let mut r = learned();
    show(&mut r, &[upright(100.0, 70.0)]);
    find(&mut r, "80");
    let c = &view(&r).candidates[0];
    assert!(!c.mirrored && c.visible_percent == 100.0, "{c:?}");
}
